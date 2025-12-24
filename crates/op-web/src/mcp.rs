//! MCP Protocol Handler
//!
//! Implements MCP JSON-RPC 2.0 protocol for Claude Desktop integration.

use axum::{
    extract::State,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, debug};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Debug, Serialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl McpResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(McpError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// POST /mcp - MCP JSON-RPC endpoint
pub async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    debug!("MCP request: {} (id: {:?})", request.method, request.id);

    // Validate JSON-RPC version
    if request.jsonrpc != "2.0" {
        return Json(McpResponse::error(
            request.id,
            -32600,
            "Invalid JSON-RPC version",
        ));
    }

    let response = match request.method.as_str() {
        "initialize" => handle_initialize(request.id).await,
        "initialized" => handle_initialized(request.id).await,
        "tools/list" => handle_tools_list(&state, request.id).await,
        "tools/call" => handle_tools_call(&state, request.id, request.params).await,
        "resources/list" => handle_resources_list(request.id).await,
        "resources/read" => handle_resources_read(request.id, request.params).await,
        "prompts/list" => handle_prompts_list(request.id).await,
        "ping" => handle_ping(request.id).await,
        _ => McpResponse::error(
            request.id,
            -32601,
            format!("Method not found: {}", request.method),
        ),
    };

    Json(response)
}

async fn handle_initialize(id: Option<Value>) -> McpResponse {
    McpResponse::success(id, json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {
                "listChanged": true
            },
            "resources": {
                "subscribe": false,
                "listChanged": false
            },
            "prompts": {
                "listChanged": false
            }
        },
        "serverInfo": {
            "name": "op-dbus-mcp-server",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Unified MCP server for op-dbus-v2 with native protocol tools"
        }
    }))
}

async fn handle_initialized(id: Option<Value>) -> McpResponse {
    McpResponse::success(id, json!({}))
}

async fn handle_tools_list(state: &AppState, id: Option<Value>) -> McpResponse {
    let tools = state.tool_registry.list().await;

    let tool_list: Vec<Value> = tools
        .iter()
        .map(|t| json!({
            "name": t.name,
            "description": t.description,
            "inputSchema": t.input_schema.clone()
        }))
        .collect();

    McpResponse::success(id, json!({ "tools": tool_list }))
}

async fn handle_tools_call(
    state: &AppState,
    id: Option<Value>,
    params: Option<Value>,
) -> McpResponse {
    let params = match params {
        Some(p) => p,
        None => return McpResponse::error(id, -32602, "Missing params"),
    };

    let tool_name = match params.get("name").and_then(|n| n.as_str()) {
        Some(name) => name,
        None => return McpResponse::error(id, -32602, "Missing tool name"),
    };

    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    info!("MCP tool call: {} with args: {:?}", tool_name, arguments);

    // Get and execute tool
    let tool = match state.tool_registry.get(tool_name).await {
        Some(t) => t,
        None => return McpResponse::error(id, -32602, format!("Tool not found: {}", tool_name)),
    };

    match tool.execute(arguments).await {
        Ok(result) => McpResponse::success(id, json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            }],
            "isError": false
        })),
        Err(e) => McpResponse::success(id, json!({
            "content": [{
                "type": "text",
                "text": format!("Error: {}", e)
            }],
            "isError": true
        })),
    }
}

async fn handle_resources_list(id: Option<Value>) -> McpResponse {
    // Return embedded documentation resources
    McpResponse::success(id, json!({
        "resources": [
            {
                "uri": "docs://system-prompt",
                "name": "System Prompt",
                "description": "AI system prompt for op-dbus",
                "mimeType": "text/plain"
            },
            {
                "uri": "docs://tools-reference",
                "name": "Tools Reference",
                "description": "Documentation for all available tools",
                "mimeType": "text/markdown"
            }
        ]
    }))
}

async fn handle_resources_read(id: Option<Value>, params: Option<Value>) -> McpResponse {
    let uri = params
        .and_then(|p| p.get("uri").and_then(|u| u.as_str()).map(|s| s.to_string()))
        .unwrap_or_default();

    let content = match uri.as_str() {
        "docs://system-prompt" => include_str!("../../../op-dbus-v2-old/LLM-SYSTEM-PROMPT-COMPLETE.txt").to_string(),
        "docs://tools-reference" => "# Tools Reference\n\nSee /api/tools for available tools.".to_string(),
        _ => return McpResponse::error(id, -32602, format!("Resource not found: {}", uri)),
    };

    McpResponse::success(id, json!({
        "contents": [{
            "uri": uri,
            "mimeType": "text/plain",
            "text": content
        }]
    }))
}

async fn handle_prompts_list(id: Option<Value>) -> McpResponse {
    McpResponse::success(id, json!({ "prompts": [] }))
}

async fn handle_ping(id: Option<Value>) -> McpResponse {
    McpResponse::success(id, json!({}))
}

/// GET /api/mcp/_discover - List MCP server capabilities
pub async fn discover_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    let tools = state.tool_registry.list().await;
    Json(json!({
        "server": "op-dbus-mcp-server",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "MCP 2024-11-05",
        "tools_count": tools.len(),
        "endpoints": {
            "mcp": "/mcp",
            "rest_api": "/api",
            "websocket": "/ws",
            "sse": "/api/events"
        }
    }))
}

/// GET /api/mcp/_config - Generate client config
pub async fn config_handler(
    State(_state): State<Arc<AppState>>,
) -> Json<Value> {
    Json(json!({
        "mcpServers": {
            "op-dbus": {
                "url": "http://localhost:8080/mcp",
                "transport": "http"
            }
        }
    }))
}

/// GET /api/mcp/_config/claude - Generate Claude Desktop config
pub async fn claude_config_handler(
    State(_state): State<Arc<AppState>>,
) -> Json<Value> {
    // Claude Desktop expects this format
    Json(json!({
        "mcpServers": {
            "op-dbus": {
                "command": "curl",
                "args": ["-X", "POST", "-H", "Content-Type: application/json", "-d", "@-", "http://localhost:8080/mcp"]
            }
        }
    }))
}
