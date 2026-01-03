//! Compact MCP Server
//!
//! Provides stdio-based MCP server with only 4 meta-tools:
//! - list_tools: Browse available tools with pagination
//! - search_tools: Search tools by keyword
//! - get_tool_schema: Get input schema for a specific tool
//! - execute_tool: Execute any tool by name

use anyhow::Result;
use op_tools::ToolRegistry;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tracing::{debug, info, warn};
use tracing_subscriber::prelude::*;

use crate::{McpRequest, McpResponse, McpServer, ResourceRegistry};

/// Compact MCP tool definitions
pub fn get_compact_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "list_tools",
            "description": "List all available tools. Use pagination for large tool sets. Returns tool names and descriptions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "category": {
                        "type": "string",
                        "description": "Filter by category (e.g., 'networking', 'system', 'database')"
                    },
                    "limit": {
                        "type": "integer",
                        "default": 50,
                        "description": "Maximum tools to return (default: 50, max: 100)"
                    },
                    "offset": {
                        "type": "integer",
                        "default": 0,
                        "description": "Pagination offset (default: 0)"
                    }
                },
                "required": []
            }
        }),
        json!({
            "name": "search_tools",
            "description": "Search for tools by keyword in name or description. Returns matching tools.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query (searches in tool name and description)"
                    },
                    "limit": {
                        "type": "integer",
                        "default": 20,
                        "description": "Maximum results (default: 20)"
                    }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "get_tool_schema",
            "description": "Get the full input schema for a specific tool. Use this before calling execute_tool to understand required parameters.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Name of the tool to get schema for"
                    }
                },
                "required": ["tool_name"]
            }
        }),
        json!({
            "name": "execute_tool",
            "description": "Execute any tool by name with the provided arguments. First use get_tool_schema to understand the required input format.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Name of the tool to execute"
                    },
                    "arguments": {
                        "type": "object",
                        "description": "Arguments to pass to the tool (must match tool's input schema)"
                    }
                },
                "required": ["tool_name"]
            }
        }),
    ]
}

/// Run compact MCP server over stdio
pub async fn run_compact_stdio_server() -> Result<()> {
    // Load environment
    op_core::config::load_environment();

    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "op_mcp=info,tokio=warn,warn".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    info!("Starting compact MCP server (stdio mode)");

    // Create minimal tool registry with just the meta-tools
    let tool_registry = Arc::new(ToolRegistry::new());

    // Register the compact meta-tools
    register_compact_tools(&tool_registry).await?;

    // Create ChatActor
    let config = op_chat::ChatActorConfig::default();
    let (chat_actor, chat_handle) = op_chat::ChatActor::with_registry(config, tool_registry).await?;
    tokio::spawn(chat_actor.run());

    info!("Compact MCP server ready");

    // Create MCP server
    let resource_registry = ResourceRegistry::new();
    let mcp_server = Arc::new(McpServer::new(chat_handle, resource_registry));

    // Run stdio protocol
    run_stdio_protocol(mcp_server).await
}

/// Register compact meta-tools
async fn register_compact_tools(registry: &Arc<ToolRegistry>) -> Result<()> {
    // For compact mode, we don't actually register real tools
    // The meta-tools are handled specially by the MCP server
    // This is just to satisfy the ChatActor initialization
    Ok(())
}

/// Run MCP protocol over stdio
async fn run_stdio_protocol(mcp_server: Arc<McpServer>) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let mut stdout_writer = stdout;
    let mut lines = tokio::io::BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        debug!("Received MCP request: {}", line);

        // Parse MCP request
        let request: Result<McpRequest, _> = serde_json::from_str(line);
        match request {
            Ok(mcp_request) => {
                // Handle compact mode specially
                let response = handle_compact_request(mcp_request).await;

                // Send response
                let response_json = serde_json::to_string(&response)?;
                stdout_writer.write_all(response_json.as_bytes()).await?;
                stdout_writer.write_all(b"\n").await?;
                stdout_writer.flush().await?;

                debug!("Response sent");
            }
            Err(e) => {
                warn!("Failed to parse MCP request: {}", e);

                // Send error response
                let error_response = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": "Parse error"
                    }
                });

                let error_json = serde_json::to_string(&error_response)?;
                stdout_writer.write_all(error_json.as_bytes()).await?;
                stdout_writer.write_all(b"\n").await?;
                stdout_writer.flush().await?;
            }
        }
    }

    Ok(())
}

/// Handle compact mode MCP requests
async fn handle_compact_request(request: McpRequest) -> McpResponse {
    match request.method.as_str() {
        "initialize" => McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "instructions": "Compact MCP server with 4 meta-tools: list_tools (browse tools), search_tools (find tools), get_tool_schema (get tool details), execute_tool (run any tool).",
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": "op-dbus-compact",
                    "version": "1.0.0"
                }
            })),
            error: None,
        },

        "initialized" => McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({})),
            error: None,
        },

        "tools/list" => {
            // Return the 4 meta-tools
            McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(json!({
                    "tools": get_compact_tools()
                })),
                error: None,
            }
        }

        "tools/call" => {
            // Handle meta-tool execution by forwarding to HTTP endpoint
            handle_tool_call(request).await
        }

        _ => McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(crate::protocol::McpError::new(
                -32601,
                format!("Method not found: {}", request.method),
            )),
        },
    }
}

/// Handle tool execution by forwarding to HTTP endpoint
async fn handle_tool_call(request: McpRequest) -> McpResponse {
    let params = match &request.params {
        Some(Value::Object(map)) => map,
        _ => {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(crate::protocol::McpError::new(
                    -32602,
                    "Invalid params: expected object".to_string(),
                )),
            }
        }
    };

    let tool_name = match params.get("name") {
        Some(Value::String(name)) => name.clone(),
        _ => {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(crate::protocol::McpError::new(
                    -32602,
                    "Invalid params: missing 'name' field".to_string(),
                )),
            }
        }
    };

    let empty_object = Value::Object(Default::default());
    let arguments = params.get("arguments").unwrap_or(&empty_object);

    // Forward to HTTP endpoint
    match forward_to_http(&tool_name, arguments).await {
        Ok(result) => McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(result),
            error: None,
        },
        Err(err) => McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(crate::protocol::McpError::new(
                -32603,
                format!("Tool execution failed: {}", err),
            )),
        },
    }
}

/// Forward tool execution to HTTP endpoint
async fn forward_to_http(tool_name: &str, arguments: &Value) -> Result<Value> {
    let client = reqwest::Client::new();

    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments
        }
    });

    let response = client
        .post("http://localhost:8081/mcp/compact/message")
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    let response_json: Value = response.json().await?;

    if let Some(error) = response_json.get("error") {
        return Err(anyhow::anyhow!("HTTP error: {}", error));
    }

    if let Some(result) = response_json.get("result") {
        Ok(result.clone())
    } else {
        Err(anyhow::anyhow!("No result in HTTP response"))
    }
}