//! MCP Manager - External MCP gateway that serves tools individually to clients
//!
//! This manager:
//! - Connects to multiple MCP backends (internal or external)
//! - Routes tool requests to appropriate backends
//! - Allows clients to subscribe to specific tool subsets
//! - Provides per-client tool filtering

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Tool backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub name: String,
    pub url: Option<String>,           // For HTTP-based backends
    pub command: Option<Vec<String>>,  // For stdio-based backends
    pub tool_filter: Option<Vec<String>>, // Only expose these tools from this backend
}

/// Client subscription
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientSubscription {
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub tool_filters: Vec<String>,     // Glob patterns for tool names
    #[serde(default)]
    pub backends: Vec<String>,         // Which backends this client can use
}

/// The MCP Manager state
#[derive(Clone)]
pub struct McpManager {
    backends: Arc<RwLock<HashMap<String, BackendConfig>>>,
    clients: Arc<RwLock<HashMap<String, ClientSubscription>>>,
    tool_cache: Arc<RwLock<HashMap<String, CachedTool>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub backend: String,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            backends: Arc::new(RwLock::new(HashMap::new())),
            clients: Arc::new(RwLock::new(HashMap::new())),
            tool_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_backend(&self, config: BackendConfig) {
        let name = config.name.clone();
        self.backends.write().await.insert(name.clone(), config);
        info!("Added backend: {}", name);
    }

    pub async fn register_client(&self, client_id: &str, subscription: ClientSubscription) {
        self.clients.write().await.insert(client_id.to_string(), subscription);
        info!("Registered client: {}", client_id);
    }

    pub async fn list_tools_for_client(&self, client_id: &str) -> Vec<CachedTool> {
        let clients = self.clients.read().await;
        let tools = self.tool_cache.read().await;

        let subscription = clients.get(client_id);

        tools.values()
            .filter(|tool| {
                if let Some(sub) = subscription {
                    // Check if tool matches client filters
                    if !sub.tool_filters.is_empty() {
                        let matches_filter = sub.tool_filters.iter().any(|pattern| {
                            glob_match(pattern, &tool.name)
                        });
                        if !matches_filter {
                            return false;
                        }
                    }
                    // Check if tool's backend is allowed
                    if !sub.backends.is_empty() && !sub.backends.contains(&tool.backend) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect()
    }

    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, String> {
        let tools = self.tool_cache.read().await;

        let tool = tools.get(tool_name)
            .ok_or_else(|| format!("Tool not found: {}", tool_name))?;

        let backend_name = tool.backend.clone();
        drop(tools);

        let backends = self.backends.read().await;
        let backend = backends.get(&backend_name)
            .ok_or_else(|| format!("Backend not found: {}", backend_name))?
            .clone();
        drop(backends);

        // Call the backend
        self.call_backend(&backend, tool_name, arguments).await
    }

    async fn call_backend(&self, backend: &BackendConfig, tool_name: &str, arguments: Value) -> Result<Value, String> {
        // If it's an HTTP backend
        if let Some(url) = &backend.url {
            let client = reqwest::Client::new();
            let request = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": tool_name,
                    "arguments": arguments
                }
            });

            let response = client.post(format!("{}/mcp", url))
                .json(&request)
                .send()
                .await
                .map_err(|e| format!("HTTP error: {}", e))?;

            let result: Value = response.json().await
                .map_err(|e| format!("JSON error: {}", e))?;

            Ok(result.get("result").cloned().unwrap_or(Value::Null))
        }
        // If it's a stdio backend
        else if let Some(command) = &backend.command {
            use std::process::Stdio;
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            use tokio::process::Command;

            let mut cmd = Command::new(&command[0]);
            cmd.args(&command[1..])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());

            let mut child = cmd.spawn()
                .map_err(|e| format!("Spawn error: {}", e))?;

            let request = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": tool_name,
                    "arguments": arguments
                }
            });

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(serde_json::to_string(&request).unwrap().as_bytes()).await
                    .map_err(|e| format!("Write error: {}", e))?;
                stdin.write_all(b"\n").await.ok();
                drop(stdin);
            }

            let stdout = child.stdout.take().unwrap();
            let mut reader = BufReader::new(stdout).lines();

            if let Some(line) = reader.next_line().await.map_err(|e| format!("Read error: {}", e))? {
                let result: Value = serde_json::from_str(&line)
                    .map_err(|e| format!("JSON parse error: {}", e))?;
                Ok(result.get("result").cloned().unwrap_or(Value::Null))
            } else {
                Err("No response from backend".to_string())
            }
        } else {
            Err("Backend has no URL or command configured".to_string())
        }
    }

    /// Discover tools from all backends
    pub async fn discover_tools(&self) {
        let backends = self.backends.read().await.clone();

        for (name, backend) in backends {
            match self.discover_backend_tools(&backend).await {
                Ok(tools) => {
                    let mut cache = self.tool_cache.write().await;
                    for tool in tools {
                        cache.insert(tool.name.clone(), tool);
                    }
                    info!("Discovered tools from backend: {}", name);
                }
                Err(e) => {
                    error!("Failed to discover tools from {}: {}", name, e);
                }
            }
        }
    }

    async fn discover_backend_tools(&self, backend: &BackendConfig) -> Result<Vec<CachedTool>, String> {
        let mut tools = Vec::new();

        // For HTTP backends
        if let Some(url) = &backend.url {
            let client = reqwest::Client::new();
            let request = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list",
                "params": {}
            });

            let response = client.post(format!("{}/mcp", url))
                .json(&request)
                .send()
                .await
                .map_err(|e| format!("HTTP error: {}", e))?;

            let result: Value = response.json().await
                .map_err(|e| format!("JSON error: {}", e))?;

            if let Some(tool_list) = result.get("result").and_then(|r| r.get("tools")).and_then(|t| t.as_array()) {
                for tool in tool_list {
                    let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or_default();

                    // Apply filter
                    if let Some(filters) = &backend.tool_filter {
                        if !filters.iter().any(|f| glob_match(f, name)) {
                            continue;
                        }
                    }

                    tools.push(CachedTool {
                        name: name.to_string(),
                        description: tool.get("description").and_then(|d| d.as_str()).unwrap_or_default().to_string(),
                        input_schema: tool.get("inputSchema").cloned().unwrap_or(json!({})),
                        backend: backend.name.clone(),
                    });
                }
            }
        }

        Ok(tools)
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/health", get(health_check))
            .route("/backends", get(list_backends).post(add_backend))
            .route("/clients/:client_id", post(register_client))
            .route("/tools", get(list_all_tools))
            .route("/tools/:client_id", get(list_tools_for_client))
            .route("/call", post(call_tool))
            .route("/discover", post(discover_tools))
            .route("/mcp", post(handle_mcp_request))
            .with_state(Arc::new(self))
    }
}

// Simple glob matching (supports * wildcard)
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern.starts_with('*') && pattern.ends_with('*') {
        let inner = &pattern[1..pattern.len()-1];
        return text.contains(inner);
    }
    if pattern.starts_with('*') {
        return text.ends_with(&pattern[1..]);
    }
    if pattern.ends_with('*') {
        return text.starts_with(&pattern[..pattern.len()-1]);
    }
    pattern == text
}

// Handlers
async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "mcp-manager",
        "version": "0.1.0"
    }))
}

async fn list_backends(State(mgr): State<Arc<McpManager>>) -> Json<Value> {
    let backends = mgr.backends.read().await;
    let list: Vec<_> = backends.keys().collect();
    Json(json!({ "backends": list }))
}

async fn add_backend(
    State(mgr): State<Arc<McpManager>>,
    Json(config): Json<BackendConfig>,
) -> StatusCode {
    mgr.add_backend(config).await;
    StatusCode::CREATED
}

async fn register_client(
    State(mgr): State<Arc<McpManager>>,
    Path(client_id): Path<String>,
    Json(subscription): Json<ClientSubscription>,
) -> StatusCode {
    let mut sub = subscription;
    sub.client_id = client_id.clone();
    mgr.register_client(&client_id, sub).await;
    StatusCode::CREATED
}

async fn list_all_tools(State(mgr): State<Arc<McpManager>>) -> Json<Value> {
    let tools = mgr.tool_cache.read().await;
    let list: Vec<_> = tools.values().cloned().collect();
    Json(json!({ "tools": list }))
}

async fn list_tools_for_client(
    State(mgr): State<Arc<McpManager>>,
    Path(client_id): Path<String>,
) -> Json<Value> {
    let tools = mgr.list_tools_for_client(&client_id).await;
    Json(json!({ "tools": tools }))
}

#[derive(Deserialize)]
struct CallToolRequest {
    name: String,
    arguments: Value,
}

async fn call_tool(
    State(mgr): State<Arc<McpManager>>,
    Json(req): Json<CallToolRequest>,
) -> Result<Json<Value>, StatusCode> {
    match mgr.call_tool(&req.name, req.arguments).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            error!("Tool call failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn discover_tools(State(mgr): State<Arc<McpManager>>) -> StatusCode {
    mgr.discover_tools().await;
    StatusCode::OK
}

#[derive(Deserialize)]
struct McpRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    params: Option<Value>,
}

#[derive(Serialize)]
struct McpResponse {
    jsonrpc: String,
    id: Value,
    result: Option<Value>,
    error: Option<Value>,
}

async fn handle_mcp_request(
    State(mgr): State<Arc<McpManager>>,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    match request.method.as_str() {
        "initialize" => {
            Json(McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "mcp-manager",
                        "version": "0.1.0"
                    }
                })),
                error: None,
            })
        }
        "tools/list" => {
            let tools = mgr.tool_cache.read().await;
            let tool_list: Vec<_> = tools.values().map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema
                })
            }).collect();

            Json(McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(json!({ "tools": tool_list })),
                error: None,
            })
        }
        "tools/call" => {
            let params = request.params.unwrap_or(json!({}));
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or_default();
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

            match mgr.call_tool(name, arguments).await {
                Ok(result) => {
                    Json(McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": serde_json::to_string(&result).unwrap_or_default()
                            }]
                        })),
                        error: None,
                    })
                }
                Err(e) => {
                    Json(McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(json!({
                            "code": -32603,
                            "message": e
                        })),
                    })
                }
            }
        }
        _ => {
            Json(McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(json!({
                    "code": -32601,
                    "message": format!("Method not found: {}", request.method)
                })),
            })
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!("Starting MCP Manager v0.1.0");

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8090);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let manager = McpManager::new();

    // Add default backend (the local MCP server)
    manager.add_backend(BackendConfig {
        name: "local".to_string(),
        url: Some("http://localhost:3000".to_string()),
        command: None,
        tool_filter: None,
    }).await;

    // Auto-discover tools on startup
    manager.discover_tools().await;

    let app = manager.router();

    info!("MCP Manager listening on {}", addr);
    info!("Endpoints:");
    info!("  GET  /health          - Health check");
    info!("  GET  /backends        - List backends");
    info!("  POST /backends        - Add backend");
    info!("  POST /clients/:id     - Register client with filters");
    info!("  GET  /tools           - List all tools");
    info!("  GET  /tools/:client   - List tools for client");
    info!("  POST /call            - Call a tool");
    info!("  POST /discover        - Discover tools from backends");
    info!("  POST /mcp             - MCP JSON-RPC endpoint");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
