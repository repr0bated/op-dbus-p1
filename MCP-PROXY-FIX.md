# MCP Proxy Fix - Always-Running Daemon Support

## Problem

MCP clients (Claude Desktop, Cursor, etc.) expect to **spawn and control** the server process via stdio. But `op-dbus` is a **long-running daemon** that's always running.

```
MCP Client
    │
    ├─── Expects: spawn process → communicate via stdin/stdout
    │
    └─── op-dbus: Already running as daemon → can't spawn
```

## Solution: MCP Proxy Shim

Create a thin `mcp-proxy` binary that:
1. Is spawned by MCP clients (satisfies their expectation)
2. Connects to running op-dbus daemon via gRPC
3. Bridges stdio ↔ gRPC

```
MCP Client (Claude Desktop)
    │ spawns
    ▼
┌─────────────────────┐
│   mcp-proxy         │  ← Thin, stateless
│   (stdio transport) │
└─────────────────────┘
    │ gRPC
    ▼
┌─────────────────────┐
│   op-dbus daemon    │  ← Always running, has state
│   (port 50051)      │
└─────────────────────┘
```

## File Structure

```
crates/
├── mcp-proxy/              # NEW: Thin MCP shim
│   ├── Cargo.toml
│   └── src/main.rs
│
└── op-dbus/                # EXISTING: Add gRPC MCP service
    └── src/
        └── grpc/
            └── mcp_service.rs  # NEW
```

## Step 1: Create crates/mcp-proxy/Cargo.toml

```toml
[package]
name = "mcp-proxy"
version = "0.1.0"
edition = "2021"
description = "MCP proxy - bridges stdio to op-dbus gRPC"

[[bin]]
name = "mcp-proxy"
path = "src/main.rs"

[dependencies]
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
tonic = "0.11"
tracing = "0.1"
tracing-subscriber = "0.3"

op-cache = { path = "../op-cache" }  # For proto types
```

## Step 2: Create crates/mcp-proxy/src/main.rs

```rust
//! MCP Proxy - Thin shim spawned by MCP clients
//!
//! - Reads JSON-RPC from stdin
//! - Forwards to op-dbus daemon via gRPC
//! - Writes responses to stdout
//!
//! STATELESS - all state lives in daemon
//! NO LAZY - connects immediately on startup

use std::io::{BufRead, Write};
use tonic::transport::Channel;
use tracing::{debug, error, info, warn};

use op_cache::proto::mcp_service_client::McpServiceClient;
use op_cache::proto::McpRequest;

const DEFAULT_DAEMON_ADDR: &str = "http://[::1]:50051";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Log to stderr (stdout is MCP protocol)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("info")
        .init();

    let daemon_addr = std::env::var("OP_DBUS_ADDR")
        .unwrap_or_else(|_| DEFAULT_DAEMON_ADDR.to_string());

    info!("Connecting to op-dbus at {}", daemon_addr);

    // EAGER: Connect immediately, fail fast
    let channel = match Channel::from_shared(daemon_addr.clone())?
        .connect()
        .await
    {
        Ok(c) => c,
        Err(e) => {
            // Return MCP-compliant error to client
            let error_response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": -32603,
                    "message": format!("Cannot connect to op-dbus daemon at {}: {}", daemon_addr, e)
                }
            });
            println!("{}", error_response);
            std::process::exit(1);
        }
    };

    let mut client = McpServiceClient::new(channel);
    info!("Connected to op-dbus daemon");

    // Main loop: stdin → gRPC → stdout
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                error!("stdin read error: {}", e);
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        debug!("Received: {}", line);

        // Parse JSON-RPC
        let json_req: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                });
                writeln!(stdout, "{}", err)?;
                stdout.flush()?;
                continue;
            }
        };

        // Build gRPC request
        let grpc_req = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: json_req["method"].as_str().unwrap_or("").to_string(),
            id: json_req["id"].to_string(),
            params: serde_json::to_vec(&json_req["params"]).unwrap_or_default(),
        };

        // Call daemon
        let response = match client.handle_request(grpc_req).await {
            Ok(resp) => {
                let r = resp.into_inner();
                if let Some(err) = r.error {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": serde_json::from_str::<serde_json::Value>(&r.id).ok(),
                        "error": { "code": err.code, "message": err.message }
                    })
                } else {
                    let result: serde_json::Value = serde_json::from_slice(&r.result)
                        .unwrap_or(serde_json::Value::Null);
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": serde_json::from_str::<serde_json::Value>(&r.id).ok(),
                        "result": result
                    })
                }
            }
            Err(e) => {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": json_req["id"],
                    "error": { "code": -32603, "message": format!("gRPC error: {}", e) }
                })
            }
        };

        writeln!(stdout, "{}", response)?;
        stdout.flush()?;
    }

    Ok(())
}
```

## Step 3: Add MCPService to op-dbus daemon

### proto/op_cache.proto (add to existing)

```protobuf
service MCPService {
    rpc HandleRequest(MCPRequest) returns (MCPResponse);
    rpc ListTools(ListToolsRequest) returns (ListToolsResponse);
}

message MCPRequest {
    string jsonrpc = 1;
    string method = 2;
    string id = 3;
    bytes params = 4;
}

message MCPResponse {
    string jsonrpc = 1;
    string id = 2;
    bytes result = 3;
    MCPError error = 4;
}

message MCPError {
    int32 code = 1;
    string message = 2;
    bytes data = 3;
}

message ListToolsRequest {}

message ListToolsResponse {
    repeated MCPTool tools = 1;
}

message MCPTool {
    string name = 1;
    string description = 2;
    bytes input_schema = 3;
}
```

### op-dbus/src/grpc/mcp_service.rs

```rust
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use op_cache::proto::{
    mcp_service_server::McpService,
    ListToolsRequest, ListToolsResponse, McpError, McpRequest, McpResponse, McpTool,
};

pub struct McpServiceImpl {
    // Reference to your existing tool registry/orchestrator
    tool_registry: Arc<op_tools::ToolRegistry>,
}

impl McpServiceImpl {
    pub fn new(tool_registry: Arc<op_tools::ToolRegistry>) -> Self {
        Self { tool_registry }
    }

    fn get_tools(&self) -> Vec<McpTool> {
        // Return your MCP tools
        vec![
            McpTool {
                name: "list_tools".to_string(),
                description: "List available tools".to_string(),
                input_schema: serde_json::to_vec(&serde_json::json!({
                    "type": "object",
                    "properties": {
                        "category": { "type": "string" },
                        "limit": { "type": "integer" }
                    }
                })).unwrap(),
            },
            McpTool {
                name: "execute_tool".to_string(),
                description: "Execute a tool".to_string(),
                input_schema: serde_json::to_vec(&serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tool_name": { "type": "string" },
                        "arguments": { "type": "object" }
                    },
                    "required": ["tool_name"]
                })).unwrap(),
            },
            // Add memory, context_manager, etc.
        ]
    }
}

#[tonic::async_trait]
impl McpService for McpServiceImpl {
    async fn handle_request(
        &self,
        request: Request<McpRequest>,
    ) -> Result<Response<McpResponse>, Status> {
        let req = request.into_inner();
        debug!("MCP request: method={}", req.method);

        let result = match req.method.as_str() {
            "initialize" => Ok(serde_json::to_vec(&serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "op-dbus", "version": "1.0.0" }
            })).unwrap()),
            
            "tools/list" => {
                let tools: Vec<_> = self.get_tools().iter().map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": serde_json::from_slice::<serde_json::Value>(&t.input_schema).ok()
                    })
                }).collect();
                Ok(serde_json::to_vec(&serde_json::json!({ "tools": tools })).unwrap())
            }
            
            "tools/call" => {
                let params: serde_json::Value = serde_json::from_slice(&req.params)
                    .unwrap_or(serde_json::Value::Null);
                let tool_name = params["name"].as_str().unwrap_or("");
                let args = params["arguments"].clone();
                
                // Execute via your tool registry
                match self.tool_registry.get(tool_name).await {
                    Some(tool) => match tool.execute(args).await {
                        Ok(result) => Ok(serde_json::to_vec(&serde_json::json!({
                            "content": [{ "type": "text", "text": result.to_string() }]
                        })).unwrap()),
                        Err(e) => Ok(serde_json::to_vec(&serde_json::json!({
                            "content": [{ "type": "text", "text": format!("Error: {}", e) }],
                            "isError": true
                        })).unwrap()),
                    },
                    None => Ok(serde_json::to_vec(&serde_json::json!({
                        "content": [{ "type": "text", "text": format!("Tool not found: {}", tool_name) }],
                        "isError": true
                    })).unwrap()),
                }
            }
            
            _ => Err(McpError {
                code: -32601,
                message: format!("Method not found: {}", req.method),
                data: Vec::new(),
            }),
        };

        match result {
            Ok(result_bytes) => Ok(Response::new(McpResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: result_bytes,
                error: None,
            })),
            Err(error) => Ok(Response::new(McpResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: Vec::new(),
                error: Some(error),
            })),
        }
    }

    async fn list_tools(
        &self,
        _request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        Ok(Response::new(ListToolsResponse {
            tools: self.get_tools(),
        }))
    }
}
```

## Step 4: Update op-dbus daemon to serve gRPC

```rust
// In op-dbus main.rs or server startup
use tonic::transport::Server;

async fn start_grpc_server(tool_registry: Arc<op_tools::ToolRegistry>) -> Result<()> {
    let addr = "[::1]:50051".parse()?;
    
    let mcp_service = McpServiceImpl::new(tool_registry);
    
    info!("Starting gRPC server on {}", addr);
    
    Server::builder()
        .add_service(McpServiceServer::new(mcp_service))
        .serve(addr)
        .await?;
    
    Ok(())
}
```

## Step 5: MCP Client Configuration

### claude_desktop_config.json

```json
{
  "mcpServers": {
    "op-dbus": {
      "command": "/usr/local/bin/mcp-proxy",
      "args": [],
      "env": {
        "OP_DBUS_ADDR": "http://[::1]:50051"
      }
    }
  }
}
```

### cursor mcp.json

```json
{
  "mcpServers": {
    "op-dbus": {
      "command": "mcp-proxy"
    }
  }
}
```

## Build & Install

```bash
# Build proxy
cargo build --release -p mcp-proxy

# Install
sudo cp target/release/mcp-proxy /usr/local/bin/

# Ensure op-dbus daemon is running with gRPC
systemctl status op-dbus

# Test
echo '{"jsonrpc":"2.0","method":"tools/list","id":1}' | mcp-proxy
```

## Key Points

1. **mcp-proxy is stateless** — just a pipe
2. **All state in daemon** — agents, cache, workstacks
3. **Multiple clients OK** — each spawns own proxy
4. **Fail fast** — proxy exits with MCP error if daemon unreachable
5. **Logs to stderr** — stdout reserved for MCP protocol
