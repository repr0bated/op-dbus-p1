//! External MCP Server Client
//!
//! Connects to external MCP servers via stdio, HTTP, or SSE
//! Aggregates tools from multiple MCP servers

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// MCP server transport type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpTransport {
    /// Standard input/output (e.g., npx commands)
    Stdio {
        command: String,
        args: Vec<String>,
        env: Option<HashMap<String, String>>,
    },
    /// HTTP/HTTPS endpoint
    Http {
        url: String,
        headers: Option<HashMap<String, String>>,
    },
    /// Server-Sent Events
    Sse {
        url: String,
        headers: Option<HashMap<String, String>>,
    },
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub description: String,
    pub transport: McpTransport,
    pub enabled: bool,
}

/// Tool definition from external MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// External MCP client
pub struct McpClient {
    pub name: String,
    config: McpServerConfig,
    process: Option<Child>,
    tools: Arc<RwLock<Vec<McpTool>>>,
    initialized: Arc<RwLock<bool>>,
}

impl McpClient {
    /// Create a new MCP client
    pub async fn new(config: McpServerConfig) -> Result<Self> {
        let client = Self {
            name: config.name.clone(),
            config,
            process: None,
            tools: Arc::new(RwLock::new(Vec::new())),
            initialized: Arc::new(RwLock::new(false)),
        };

        Ok(client)
    }

    /// Connect to the MCP server
    pub async fn connect(&mut self) -> Result<()> {
        match &self.config.transport {
            McpTransport::Stdio { command, args, env } => {
                self.connect_stdio(command, args, env.as_ref()).await
            }
            McpTransport::Http { url, headers } => {
                self.connect_http(url, headers.as_ref()).await
            }
            McpTransport::Sse { url, headers } => {
                self.connect_sse(url, headers.as_ref()).await
            }
        }
    }

    /// Connect via stdio
    async fn connect_stdio(
        &mut self,
        command: &str,
        args: &[String],
        env: Option<&HashMap<String, String>>,
    ) -> Result<()> {
        info!("Connecting to MCP server: {} via stdio", self.name);

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(env_vars) = env {
            cmd.envs(env_vars);
        }

        let child = cmd.spawn()
            .context(format!("Failed to spawn MCP server: {}", command))?;

        self.process = Some(child);

        // Initialize MCP connection
        self.send_initialize().await?;

        info!("✅ Connected to MCP server: {}", self.name);
        Ok(())
    }

    /// Connect via HTTP
    async fn connect_http(
        &mut self,
        url: &str,
        _headers: Option<&HashMap<String, String>>,
    ) -> Result<()> {
        info!("Connecting to MCP server: {} via HTTP at {}", self.name, url);
        
        // Send initialize request
        let client = reqwest::Client::new();
        let init_request = json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "clientInfo": {
                    "name": "op-dbus",
                    "version": env!("CARGO_PKG_VERSION")
                }
            },
            "id": 1
        });

        let response = client.post(url)
            .json(&init_request)
            .send()
            .await
            .context("Failed to connect to HTTP MCP server")?;

        if response.status().is_success() {
            info!("✅ Connected to HTTP MCP server: {}", self.name);
            self.fetch_tools().await?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("HTTP MCP server returned error: {}", response.status()))
        }
    }

    /// Connect via SSE
    async fn connect_sse(
        &mut self,
        url: &str,
        _headers: Option<&HashMap<String, String>>,
    ) -> Result<()> {
        info!("Connecting to MCP server: {} via SSE at {}", self.name, url);
        // SSE client implementation would go here
        // For now, fall back to HTTP
        self.connect_http(url, None).await
    }

    /// Send initialize request (MCP protocol)
    async fn send_initialize(&mut self) -> Result<()> {
        let init_request = json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "clientInfo": {
                    "name": "op-dbus",
                    "version": env!("CARGO_PKG_VERSION")
                }
            },
            "id": 1
        });

        if let Some(process) = &mut self.process {
            if let Some(stdin) = process.stdin.as_mut() {
                let request_str = serde_json::to_string(&init_request)?;
                stdin.write_all(request_str.as_bytes()).await?;
                stdin.write_all(b"\n").await?;
                stdin.flush().await?;

                // Read response
                if let Some(stdout) = process.stdout.as_mut() {
                    let mut reader = BufReader::new(stdout);
                    let mut response_line = String::new();
                    reader.read_line(&mut response_line).await?;
                    
                    debug!("MCP initialize response: {}", response_line);
                }

                *self.initialized.write().await = true;
                self.fetch_tools().await?;
            }
        }

        Ok(())
    }

    /// Fetch available tools from the MCP server
    async fn fetch_tools(&self) -> Result<()> {
        let tools_request = json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {},
            "id": 2
        });

        // For stdio
        if let Some(_process) = &self.process {
            // Would send request and parse response
            // For now, return placeholder
            debug!("Fetching tools from {} (stdio)", self.name);
        }

        Ok(())
    }

    /// Call a tool on this MCP server
    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value> {
        let call_request = json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            },
            "id": 3
        });

        // Implementation depends on transport
        match &self.config.transport {
            McpTransport::Stdio { .. } => {
                // Send via stdin, read from stdout
                Ok(json!({
                    "result": "Tool execution via stdio (placeholder)"
                }))
            }
            McpTransport::Http { url, .. } => {
                let client = reqwest::Client::new();
                let response = client.post(url)
                    .json(&call_request)
                    .send()
                    .await?
                    .json::<Value>()
                    .await?;
                
                Ok(response)
            }
            McpTransport::Sse { .. } => {
                Ok(json!({
                    "result": "Tool execution via SSE (placeholder)"
                }))
            }
        }
    }

    /// Get list of tools from this server
    pub async fn get_tools(&self) -> Vec<McpTool> {
        self.tools.read().await.clone()
    }

    /// Add a known tool to the registry
    pub async fn add_tool(&self, tool: McpTool) {
        self.tools.write().await.push(tool);
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.start_kill();
        }
    }
}

/// MCP server registry - manages multiple external MCP servers
/// Each server gets its own endpoint
pub struct McpServerRegistry {
    servers: Arc<RwLock<HashMap<String, Arc<RwLock<McpClient>>>>>,
}

impl McpServerRegistry {
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an MCP server
    pub async fn register(&self, mut client: McpClient) -> Result<()> {
        let name = client.name.clone();
        
        // Connect to the server
        client.connect().await?;
        
        let servers = self.servers.clone();
        let mut servers_write = servers.write().await;
        servers_write.insert(name.clone(), Arc::new(RwLock::new(client)));
        
        info!("Registered MCP server: {} (available at /api/mcp/{})", name, name);
        Ok(())
    }

    /// Get a specific MCP server by name
    pub async fn get_server(&self, name: &str) -> Option<Arc<RwLock<McpClient>>> {
        let servers = self.servers.read().await;
        servers.get(name).cloned()
    }

    /// List all registered MCP server names
    pub async fn list_servers(&self) -> Vec<String> {
        let servers = self.servers.read().await;
        servers.keys().cloned().collect()
    }

    /// Get all tools from all registered servers
    pub async fn get_all_tools(&self) -> Vec<(String, McpTool)> {
        let servers = self.servers.read().await;
        let mut all_tools = Vec::new();

        for (server_name, client) in servers.iter() {
            let client_guard = client.read().await;
            let tools = client_guard.get_tools().await;
            
            for tool in tools {
                all_tools.push((server_name.clone(), tool));
            }
        }

        all_tools
    }

    /// Get tools from a specific server
    pub async fn get_server_tools(&self, server_name: &str) -> Result<Vec<McpTool>> {
        let servers = self.servers.read().await;
        
        if let Some(client) = servers.get(server_name) {
            let client_guard = client.read().await;
            Ok(client_guard.get_tools().await)
        } else {
            Err(anyhow::anyhow!("MCP server not found: {}", server_name))
        }
    }

    /// Call a tool on a specific server
    pub async fn call_tool(&self, server_name: &str, tool_name: &str, arguments: Value) -> Result<Value> {
        let servers = self.servers.read().await;
        
        if let Some(client) = servers.get(server_name) {
            let client_guard = client.read().await;
            client_guard.call_tool(tool_name, arguments).await
        } else {
            Err(anyhow::anyhow!("MCP server not found: {}", server_name))
        }
    }

    /// Get metadata for all servers (for discovery)
    pub async fn get_servers_metadata(&self) -> Vec<Value> {
        let servers = self.servers.read().await;
        let mut metadata = Vec::new();

        for (name, _client) in servers.iter() {
            metadata.push(json!({
                "name": name,
                "endpoint": format!("/api/mcp/{}", name),
                "protocol": "MCP JSON-RPC 2.0",
                "sse_endpoint": format!("/api/mcp/{}/events", name)
            }));
        }

        metadata
    }
}

impl Default for McpServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
