//! MCP Protocol Handler with Category-Based Endpoints
//!
//! Implements MCP JSON-RPC 2.0 protocol with support for category filtering.
//!
//! ## Problem
//!
//! MCP clients have tool limits:
//! - Antigravity: 100 tools
//! - Cursor: ~40 tools
//!
//! op-dbus has 100+ tools, so we need to split them into categories.
//!
//! ## Solution
//!
//! Serve tools at category-specific endpoints:
//! - `/mcp` - All tools (may hit limits)
//! - `/mcp/shell` - Shell/system tools only
//! - `/mcp/dbus` - D-Bus tools only
//! - `/mcp/network` - Network/OVS tools only
//! - `/mcp/file` - Filesystem tools only
//! - `/mcp/agent` - Agent tools only
//! - `/mcp/chat` - Chat/response tools only

use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, debug};

use crate::state::AppState;

/// Tool categories for MCP server splitting
/// Includes both tools AND role-based agents
pub const TOOL_CATEGORIES: &[(&str, &str)] = &[
    // Core tools
    ("shell", "Shell and system command tools"),
    ("dbus", "D-Bus protocol tools (systemd, introspection)"),
    ("network", "Network tools (OVS, rtnetlink, bridge)"),
    ("file", "Filesystem tools (read, write, list)"),
    ("agent", "Agent execution tools"),
    ("chat", "Chat and response tools"),
    ("package", "Package management tools"),
    ("mcp", "External MCP integration tools"),
    
    // Agent role categories
    ("language", "Language-specific agents (Python, Rust, Go, etc.)"),
    ("architecture", "Architecture agents (Backend, Frontend, GraphQL)"),
    ("infrastructure", "Infrastructure agents (Cloud, K8s, Terraform, Network)"),
    ("analysis", "Analysis agents (Code Review, Debug, Performance, Security)"),
    ("business", "Business agents (Analyst, Support, HR, Legal, Sales)"),
    ("content", "Content agents (Docs, API, Tutorial, Mermaid)"),
    ("database", "Database agents (Architect, Optimizer, SQL)"),
    ("operations", "Operations agents (DevOps, Incident, Testing)"),
    ("orchestration", "Orchestration agents (Context, DX, TDD)"),
    ("security", "Security coding agents (Backend, Frontend, Mobile)"),
    ("seo", "SEO agents (Content, Keywords, Meta)"),
    ("specialty", "Specialty agents (Blockchain, IoT, AR, Unity)"),
    ("aiml", "AI/ML agents (Data Science, MLOps, Prompt Engineering)"),
    ("webframeworks", "Web framework agents (Django, FastAPI, Temporal)"),
    ("mobile", "Mobile agents (Flutter, iOS, Android)"),
];

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

/// Create MCP router with category-based endpoints
pub fn create_mcp_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Main endpoint (all tools)
        .route("/", post(mcp_handler_all))
        // Category-specific endpoints
        .route("/shell", post(mcp_handler_shell))
        .route("/dbus", post(mcp_handler_dbus))
        .route("/network", post(mcp_handler_network))
        .route("/file", post(mcp_handler_file))
        .route("/agent", post(mcp_handler_agent))
        .route("/chat", post(mcp_handler_chat))
        .route("/package", post(mcp_handler_package))
        // Dynamic category endpoint
        .route("/category/:category", post(mcp_handler_category))
        // Discovery endpoints
        .route("/_discover", get(discover_handler))
        .route("/_categories", get(categories_handler))
        .route("/_config", get(config_handler))
        .with_state(state)
}

// Category filter functions
fn filter_all(_category: &str) -> bool { true }
fn filter_shell(category: &str) -> bool { 
    matches!(category, "shell" | "system")
}
fn filter_dbus(category: &str) -> bool { 
    matches!(category, "dbus" | "systemd" | "introspection")
}
fn filter_network(category: &str) -> bool { 
    matches!(category, "network" | "ovs" | "bridge" | "netlink")
}
fn filter_file(category: &str) -> bool { 
    matches!(category, "file" | "filesystem" | "procfs" | "sysfs")
}
fn filter_agent(category: &str) -> bool { 
    matches!(category, "agent" | "execution")
}
fn filter_chat(category: &str) -> bool { 
    matches!(category, "chat" | "response" | "communication")
}
fn filter_package(category: &str) -> bool { 
    matches!(category, "package" | "packagekit")
}

// Handler wrappers for each category
async fn mcp_handler_all(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    mcp_handler_filtered(state, request, filter_all, "all").await
}

async fn mcp_handler_shell(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    mcp_handler_filtered(state, request, filter_shell, "shell").await
}

async fn mcp_handler_dbus(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    mcp_handler_filtered(state, request, filter_dbus, "dbus").await
}

async fn mcp_handler_network(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    mcp_handler_filtered(state, request, filter_network, "network").await
}

async fn mcp_handler_file(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    mcp_handler_filtered(state, request, filter_file, "file").await
}

async fn mcp_handler_agent(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    mcp_handler_filtered(state, request, filter_agent, "agent").await
}

async fn mcp_handler_chat(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    mcp_handler_filtered(state, request, filter_chat, "chat").await
}

async fn mcp_handler_package(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    mcp_handler_filtered(state, request, filter_package, "package").await
}

async fn mcp_handler_category(
    State(state): State<Arc<AppState>>,
    Path(category): Path<String>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    let category_clone = category.clone();
    let filter = move |cat: &str| cat == category_clone.as_str();
    mcp_handler_filtered(state, request, filter, &category).await
}

/// Core MCP handler with category filtering
async fn mcp_handler_filtered<F>(
    state: Arc<AppState>,
    request: McpRequest,
    category_filter: F,
    server_name: &str,
) -> Json<McpResponse>
where
    F: Fn(&str) -> bool,
{
    debug!("MCP request ({} server): {} (id: {:?})", server_name, request.method, request.id);

    // Validate JSON-RPC version
    if request.jsonrpc != "2.0" {
        return Json(McpResponse::error(
            request.id,
            -32600,
            "Invalid JSON-RPC version",
        ));
    }

    let response = match request.method.as_str() {
        "initialize" => handle_initialize(request.id, server_name).await,
        "initialized" => handle_initialized(request.id).await,
        "tools/list" => handle_tools_list(&state, request.id, &category_filter, request.params.clone()).await,
        "tools/describe" => handle_tools_describe(&state, request.id, request.params).await,
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

async fn handle_initialize(id: Option<Value>, server_name: &str) -> McpResponse {
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
            "name": format!("op-dbus-{}", server_name),
            "version": env!("CARGO_PKG_VERSION"),
            "description": format!("op-dbus-v2 MCP server - {} tools", server_name)
        }
    }))
}

async fn handle_initialized(id: Option<Value>) -> McpResponse {
    McpResponse::success(id, json!({}))
}

async fn handle_tools_list<F>(
    state: &AppState, 
    id: Option<Value>, 
    category_filter: F,
    params: Option<Value>,
) -> McpResponse
where
    F: Fn(&str) -> bool,
{
    let tools = state.tool_registry.list().await;
    
    // Check if lite mode requested (no inputSchema - reduces context)
    let lite_mode = params
        .as_ref()
        .and_then(|p| p.get("lite"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Check for pagination
    let limit = params
        .as_ref()
        .and_then(|p| p.get("limit"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1000) as usize;
    
    let offset = params
        .as_ref()
        .and_then(|p| p.get("offset"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let filtered: Vec<_> = tools
        .iter()
        .filter(|t| category_filter(&t.category))
        .collect();
    
    let total = filtered.len();
    
    let tool_list: Vec<Value> = filtered
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|t| {
            if lite_mode {
                // Lite mode: name + description only (for discovery)
                json!({
                    "name": t.name,
                    "description": t.description,
                    "annotations": {
                        "category": t.category.clone()
                    }
                })
            } else {
                // Full mode: includes inputSchema
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema.clone(),
                    "annotations": {
                        "category": t.category.clone(),
                        "tags": t.tags.clone(),
                        "namespace": t.namespace.clone()
                    }
                })
            }
        })
        .collect();

    info!(
        "MCP tools/list: {} tools returned (lite={}, total={})",
        tool_list.len(),
        lite_mode,
        total
    );

    McpResponse::success(id, json!({ 
        "tools": tool_list,
        "_meta": {
            "total": total,
            "offset": offset,
            "limit": limit,
            "lite_mode": lite_mode
        }
    }))
}

/// Handle tools/describe - Get full schema for specific tool(s)
/// Supports lazy loading: client can use lite tools/list, then describe specific tools
async fn handle_tools_describe(
    state: &AppState,
    id: Option<Value>,
    params: Option<Value>,
) -> McpResponse {
    let params = match params {
        Some(p) => p,
        None => return McpResponse::error(id, -32602, "Missing params"),
    };

    // Support single tool or array of tools
    let tool_names: Vec<String> = if let Some(name) = params.get("name").and_then(|n| n.as_str()) {
        vec![name.to_string()]
    } else if let Some(names) = params.get("names").and_then(|n| n.as_array()) {
        names
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect()
    } else {
        return McpResponse::error(id, -32602, "Missing 'name' or 'names' parameter");
    };

    if tool_names.is_empty() {
        return McpResponse::error(id, -32602, "No tool names provided");
    }

    let all_tools = state.tool_registry.list().await;
    
    let mut tools: Vec<Value> = Vec::new();
    let mut not_found: Vec<String> = Vec::new();

    for name in &tool_names {
        if let Some(t) = all_tools.iter().find(|t| &t.name == name) {
            tools.push(json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema.clone(),
                "annotations": {
                    "category": t.category.clone(),
                    "tags": t.tags.clone(),
                    "namespace": t.namespace.clone()
                }
            }));
        } else {
            not_found.push(name.clone());
        }
    }

    info!(
        "MCP tools/describe: {} tools found, {} not found",
        tools.len(),
        not_found.len()
    );

    McpResponse::success(id, json!({
        "tools": tools,
        "not_found": not_found
    }))
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
        "docs://system-prompt" => "op-dbus System Administrator AI - Full admin access".to_string(),
        "docs://tools-reference" => "# Tools Reference\n\nSee /mcp/_categories for available tool categories.".to_string(),
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

/// GET /mcp/_discover - Discover all MCP endpoints
pub async fn discover_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    let tools = state.tool_registry.list().await;
    
    // Count tools per category
    let mut category_counts: HashMap<String, usize> = HashMap::new();
    for tool in &tools {
        *category_counts.entry(tool.category.clone()).or_insert(0) += 1;
    }

    Json(json!({
        "server": "op-dbus-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "MCP 2024-11-05",
        "total_tools": tools.len(),
        "category_counts": category_counts,
        "endpoints": {
            "all": "/mcp",
            "shell": "/mcp/shell",
            "dbus": "/mcp/dbus",
            "network": "/mcp/network",
            "file": "/mcp/file",
            "agent": "/mcp/agent",
            "chat": "/mcp/chat",
            "package": "/mcp/package"
        },
        "client_configs": {
            "antigravity": "/mcp/_config/antigravity",
            "cursor": "/mcp/_config/cursor",
            "vscode": "/mcp/_config/vscode"
        }
    }))
}

/// GET /mcp/_categories - List available categories
pub async fn categories_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    let tools = state.tool_registry.list().await;
    
    let mut categories: HashMap<String, Vec<String>> = HashMap::new();
    for tool in &tools {
        categories
            .entry(tool.category.clone())
            .or_insert_with(Vec::new)
            .push(tool.name.clone());
    }

    let category_info: Vec<Value> = categories
        .iter()
        .map(|(cat, tools)| json!({
            "category": cat,
            "tool_count": tools.len(),
            "endpoint": format!("/mcp/{}", cat),
            "tools": tools
        }))
        .collect();

    Json(json!({
        "categories": category_info,
        "total_categories": categories.len()
    }))
}

/// GET /mcp/_config - Generate multi-server config for clients
pub async fn config_handler(
    State(_state): State<Arc<AppState>>,
) -> Json<Value> {
    let base_url = "https://xray.ghostbridge.tech";
    
    Json(json!({
        "mcpServers": {
            "op-dbus-shell": {
                "serverUrl": format!("{}/mcp/shell", base_url),
                "transport": "sse",
                "description": "Shell and system command tools"
            },
            "op-dbus-dbus": {
                "serverUrl": format!("{}/mcp/dbus", base_url),
                "transport": "sse",
                "description": "D-Bus protocol tools (systemd, introspection)"
            },
            "op-dbus-network": {
                "serverUrl": format!("{}/mcp/network", base_url),
                "transport": "sse",
                "description": "Network tools (OVS, rtnetlink, bridge)"
            },
            "op-dbus-file": {
                "serverUrl": format!("{}/mcp/file", base_url),
                "transport": "sse",
                "description": "Filesystem tools (read, write, list)"
            },
            "op-dbus-agent": {
                "serverUrl": format!("{}/mcp/agent", base_url),
                "transport": "sse",
                "description": "Agent execution tools"
            },
            "op-dbus-chat": {
                "serverUrl": format!("{}/mcp/chat", base_url),
                "transport": "sse",
                "description": "Chat and response tools"
            }
        }
    }))
}

/// GET /mcp/_config/claude - Generate Claude Desktop config
pub async fn claude_config_handler(
    State(_state): State<Arc<AppState>>,
) -> Json<Value> {
    // Claude Desktop expects this format
    Json(json!({
        "mcpServers": {
            "op-dbus": {
                "command": "curl",
                "args": ["-X", "POST", "-H", "Content-Type: application/json", "-d", "@-", "https://xray.ghostbridge.tech/mcp"]
            }
        }
    }))
}

// Legacy handler for backward compatibility
pub async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    mcp_handler_all(State(state), Json(request)).await
}

// Re-export for backward compatibility
pub use mcp_handler as mcp_handler_legacy;
