//! MCP Protocol Handler with Profile-Based Tool Limiting
//!
//! ## Problem
//!
//! MCP clients have TOTAL tool limits across ALL servers:
//! - Cursor: ~40 tools total
//! - Antigravity: 100 tools total
//!
//! op-dbus has 150+ tools - categorization doesn't help because
//! the limit is TOTAL, not per-server.
//!
//! ## Solution: Profiles
//!
//! Connect to ONE endpoint with a profile that limits to ~35 tools:
//! - `/mcp/profile/rust-dev` → Rust + shell + file (35 tools)
//! - `/mcp/profile/python-dev` → Python + shell + file (35 tools)  
//! - `/mcp/profile/sysadmin` → Shell + systemd + network (35 tools)
//! - `/mcp/profile/network` → OVS + bridge + netlink (35 tools)
//!
//! Each profile includes a curated set of the most useful tools.

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
use tracing::{info, debug, warn};

use crate::state::AppState;

/// Maximum tools per profile (must stay under Cursor's 40 limit)
pub const MAX_TOOLS_PER_PROFILE: usize = 35;

/// Tool profiles - each is a curated subset under 35 tools
/// Format: (profile_name, description, included_tool_prefixes)
pub const PROFILES: &[(&str, &str, &[&str])] = &[
    // Development profiles
    ("rust-dev", "Rust development (shell, file, cargo)", &[
        "shell_", "file_", "process_", "respond_", "request_", "cannot_",
        // Core response tools always included
    ]),
    ("python-dev", "Python development (shell, file, pip)", &[
        "shell_", "file_", "process_", "respond_", "request_", "cannot_",
    ]),
    ("go-dev", "Go development (shell, file)", &[
        "shell_", "file_", "process_", "respond_", "request_", "cannot_",
    ]),
    
    // Sysadmin profiles  
    ("sysadmin", "System administration (shell, systemd, process)", &[
        "shell_", "systemd_", "process_", "file_read", "file_write", "file_list",
        "respond_", "request_", "cannot_",
    ]),
    ("network", "Network administration (OVS, netlink, bridge)", &[
        "ovs_", "bridge_", "netlink_", "network_", "shell_exec",
        "respond_", "request_", "cannot_",
    ]),
    ("dbus", "D-Bus operations (introspection, method calls)", &[
        "dbus_", "systemd_", "introspect_", 
        "respond_", "request_", "cannot_",
    ]),
    
    // Specialized profiles
    ("containers", "Container management (LXC, Docker)", &[
        "lxc_", "docker_", "container_", "shell_exec",
        "respond_", "request_", "cannot_",
    ]),
    ("packages", "Package management (apt, dnf, pacman)", &[
        "package_", "apt_", "dnf_", "shell_exec",
        "respond_", "request_", "cannot_",
    ]),
    
    // Agent profiles (select specific agent types)
    ("agent-coding", "Coding agents (language-specific)", &[
        "agent_rust", "agent_python", "agent_go", "agent_typescript",
        "shell_exec", "file_",
        "respond_", "request_", "cannot_",
    ]),
    ("agent-infra", "Infrastructure agents (cloud, k8s)", &[
        "agent_docker", "agent_kubernetes", "agent_terraform", "agent_aws",
        "shell_exec", "file_read",
        "respond_", "request_", "cannot_",
    ]),
    ("agent-analysis", "Analysis agents (debug, review, security)", &[
        "agent_code_review", "agent_debug", "agent_security", "agent_performance",
        "shell_exec", "file_read",
        "respond_", "request_", "cannot_",
    ]),
    
    // Minimal profile (just essentials)
    ("minimal", "Minimal tools (shell, file, response)", &[
        "shell_exec", "file_read", "file_write", "file_list",
        "respond_", "request_", "cannot_",
    ]),

    // Self-modification profile (chatbot's own source code)
    ("self", "Self-modification tools (read, edit, commit, deploy own code)", &[
        "self_", "file_", "shell_exec",
        "respond_", "request_", "cannot_",
    ]),
];

/// Core tools that are ALWAYS included in every profile
const CORE_TOOLS: &[&str] = &[
    "respond_to_user",
    "request_clarification", 
    "cannot_perform",
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

/// Create MCP router with profile-based endpoints
pub fn create_mcp_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Profile-based endpoints (USE THESE!)
        .route("/profile/:profile", post(mcp_handler_profile))
        // Custom user-defined profiles (from web UI)
        .route("/custom/:profile", post(mcp_handler_custom_profile))
        // Discovery
        .route("/profiles", get(profiles_handler))
        .route("/_discover", get(discover_handler))
        .route("/_config", get(config_handler))
        // Legacy endpoints (may hit limits - warns in logs)
        .route("/", post(mcp_handler_all))
        .with_state(state)
}

/// Profile-based handler - serves only tools matching the profile (max 35)
async fn mcp_handler_profile(
    State(state): State<Arc<AppState>>,
    Path(profile_name): Path<String>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    // Find the profile
    let profile = PROFILES.iter().find(|(name, _, _)| *name == profile_name);
    
    let prefixes = match profile {
        Some((_, _, prefixes)) => prefixes.to_vec(),
        None => {
            warn!("Unknown MCP profile: {}", profile_name);
            return Json(McpResponse::error(
                request.id,
                -32602,
                format!("Unknown profile: {}. Use GET /mcp/profiles for list.", profile_name),
            ));
        }
    };
    
    let filter = move |tool_name: &str| {
        // Core tools always pass
        if CORE_TOOLS.contains(&tool_name) {
            return true;
        }
        // Check prefixes
        prefixes.iter().any(|p| tool_name.starts_with(p))
    };
    
    mcp_handler_filtered(state, request, filter, &profile_name, Some(MAX_TOOLS_PER_PROFILE)).await
}

/// Custom profile handler - serves tools selected via the web UI
async fn mcp_handler_custom_profile(
    State(state): State<Arc<AppState>>,
    Path(profile_name): Path<String>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    use crate::mcp_picker::CUSTOM_PROFILES;
    
    // Load the custom profile
    let selected_tools: std::collections::HashSet<String> = match CUSTOM_PROFILES.get_profile(&profile_name).await {
        Some(tools) => tools,
        None => {
            warn!("Unknown custom MCP profile: {}", profile_name);
            return Json(McpResponse::error(
                request.id,
                -32602,
                format!("Custom profile '{}' not found. Create one at /mcp-picker", profile_name),
            ));
        }
    };
    
    info!("Using custom MCP profile '{}' with {} tools", profile_name, selected_tools.len());
    
    let filter = move |tool_name: &str| selected_tools.contains(tool_name);
    
    mcp_handler_filtered(state, request, filter, &format!("custom-{}", profile_name), Some(MAX_TOOLS_PER_PROFILE)).await
}

/// Legacy handler (all tools - may exceed client limits)
async fn mcp_handler_all(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    warn!("Using /mcp endpoint without profile - may exceed tool limits. Use /mcp/profile/:name instead.");
    let filter = |_: &str| true;
    mcp_handler_filtered(state, request, filter, "all", None).await
}

/// Core MCP handler with tool name filtering and optional limit
async fn mcp_handler_filtered<F>(
    state: Arc<AppState>,
    request: McpRequest,
    tool_filter: F,
    server_name: &str,
    max_tools: Option<usize>,
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
        "tools/list" => handle_tools_list(&state, request.id, &tool_filter, request.params.clone(), max_tools).await,
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
    tool_filter: F,
    params: Option<Value>,
    max_tools: Option<usize>,
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
    let param_limit = params
        .as_ref()
        .and_then(|p| p.get("limit"))
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    
    let offset = params
        .as_ref()
        .and_then(|p| p.get("offset"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    // Filter by tool NAME (not category) for profile matching
    let filtered: Vec<_> = tools
        .iter()
        .filter(|t| tool_filter(&t.name))
        .collect();
    
    let total_matching = filtered.len();
    
    // Apply max_tools limit if set (for profile-based limiting)
    let effective_limit = match (max_tools, param_limit) {
        (Some(max), Some(param)) => max.min(param),
        (Some(max), None) => max,
        (None, Some(param)) => param,
        (None, None) => 1000,
    };
    
    let tool_list: Vec<Value> = filtered
        .into_iter()
        .skip(offset)
        .take(effective_limit)
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

    let served = tool_list.len();
    
    info!(
        "MCP tools/list: {} tools returned (limit={}, max={:?}, total_matching={}, lite={})",
        served,
        effective_limit,
        max_tools,
        total_matching,
        lite_mode
    );

    McpResponse::success(id, json!({ 
        "tools": tool_list,
        "_meta": {
            "served": served,
            "total_matching": total_matching,
            "offset": offset,
            "limit": effective_limit,
            "max_tools": max_tools,
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

/// GET /mcp/profiles - List all available profiles (both built-in and custom)
pub async fn profiles_handler(
    State(_state): State<Arc<AppState>>,
) -> Json<Value> {
    use crate::mcp_picker::CUSTOM_PROFILES;
    
    // Built-in profiles
    let builtin: Vec<Value> = PROFILES.iter().map(|(name, desc, prefixes)| {
        json!({
            "name": name,
            "description": desc,
            "type": "builtin",
            "prefixes": prefixes,
            "endpoint": format!("/mcp/profile/{}", name)
        })
    }).collect();
    
    // Custom profiles
    let custom_names: Vec<String> = CUSTOM_PROFILES.list_profiles().await;
    let mut custom: Vec<Value> = Vec::new();
    for name in custom_names {
        let name: String = name;
        if let Some(tools) = CUSTOM_PROFILES.get_profile(&name).await {
            let tools: std::collections::HashSet<String> = tools;
            let name_clone = name.clone();
            let description = format!("Custom profile with {} tools", tools.len());
            let tool_count = tools.len();
            let endpoint = format!("/mcp/custom/{}", name);
            
            custom.push(json!({
                "name": name_clone,
                "description": description,
                "type": "custom",
                "tool_count": tool_count,
                "endpoint": endpoint
            }));
        }
    }
    
    Json(json!({
        "max_tools_per_profile": MAX_TOOLS_PER_PROFILE,
        "builtin_profiles": builtin,
        "custom_profiles": custom,
        "picker_ui": "/mcp-picker",
        "usage": {
            "builtin": "POST /mcp/profile/{profile_name}",
            "custom": "POST /mcp/custom/{profile_name}"
        }
    }))
}

/// GET /mcp/_discover - Discover all MCP endpoints
pub async fn discover_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    use crate::mcp_picker::CUSTOM_PROFILES;
    
    let tools = state.tool_registry.list().await;
    
    // Count tools per category
    let mut category_counts: HashMap<String, usize> = HashMap::new();
    for tool in &tools {
        *category_counts.entry(tool.category.clone()).or_insert(0) += 1;
    }
    
    // Custom profiles
    let custom_profiles = CUSTOM_PROFILES.list_profiles().await;

    Json(json!({
        "server": "op-dbus-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "MCP 2024-11-05",
        "total_tools": tools.len(),
        "max_tools_per_profile": MAX_TOOLS_PER_PROFILE,
        "category_counts": category_counts,
        "builtin_profiles": PROFILES.iter().map(|(name, desc, _)| json!({
            "name": name,
            "description": desc,
            "endpoint": format!("/mcp/profile/{}", name)
        })).collect::<Vec<_>>(),
        "custom_profiles": custom_profiles,
        "endpoints": {
            "picker_ui": "/mcp-picker",
            "profiles": "/mcp/profiles",
            "discover": "/mcp/_discover",
            "config": "/mcp/_config"
        },
        "usage": {
            "step1": "Visit /mcp-picker to select tools (max 35)",
            "step2": "Save your custom profile",
            "step3": "Use /mcp/custom/{name} as your MCP endpoint"
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
