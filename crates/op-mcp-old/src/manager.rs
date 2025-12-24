// MCP Manager - Web interface for managing multiple MCP servers and orchestrator
// Provides admin UI for monitoring, configuring, and controlling MCP infrastructure

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

use super::tool_registry::{Tool, ToolRegistry};

/// MCP Manager state
#[derive(Clone)]
pub struct McpManagerState {
    /// Active MCP servers
    servers: Arc<RwLock<HashMap<String, McpServerInfo>>>,

    /// Tool registry for managing MCP tools
    tool_registry: Arc<ToolRegistry>,

    /// Introspection database (cached results)
    introspection_db: Arc<RwLock<IntrospectionDatabase>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub id: String,
    pub name: String,
    pub status: ServerStatus,
    pub endpoint: String,
    pub tools: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    Starting,
    Running,
    Paused,
    Stopped,
    Error { message: String },
}

/// Introspection database stores cached introspection results
#[derive(Debug, Default)]
pub struct IntrospectionDatabase {
    /// Snapshots indexed by timestamp
    pub snapshots: HashMap<String, serde_json::Value>,

    /// Latest snapshot
    pub latest: Option<serde_json::Value>,

    /// Provider analysis (ISP restrictions, etc.)
    pub provider_analysis: Option<serde_json::Value>,
}

impl McpManagerState {
    pub async fn new(tool_registry: ToolRegistry) -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            tool_registry: Arc::new(tool_registry),
            introspection_db: Arc::new(RwLock::new(IntrospectionDatabase::default())),
        }
    }
}

// API Routes

/// GET / - Manager dashboard
async fn dashboard() -> Html<&'static str> {
    Html(include_str!("web/index.html"))
}

/// GET /api/servers - List all MCP servers
async fn list_servers(State(state): State<McpManagerState>) -> Json<Vec<McpServerInfo>> {
    let servers = state.servers.read().await;
    Json(servers.values().cloned().collect())
}

/// POST /api/servers - Create new MCP server
#[derive(Debug, Deserialize)]
struct CreateServerRequest {
    name: String,
    tools: Vec<String>,
}

async fn create_server(
    State(state): State<McpManagerState>,
    Json(req): Json<CreateServerRequest>,
) -> Result<Json<McpServerInfo>, (StatusCode, String)> {
    let id = uuid::Uuid::new_v4().to_string();

    let server_info = McpServerInfo {
        id: id.clone(),
        name: req.name,
        status: ServerStatus::Starting,
        endpoint: format!("http://localhost:3000/mcp/{}", id),
        tools: req.tools,
        created_at: chrono::Utc::now(),
        last_heartbeat: None,
    };

    let mut servers = state.servers.write().await;
    servers.insert(id, server_info.clone());

    Ok(Json(server_info))
}

/// GET /api/servers/:id - Get server details
async fn get_server(
    State(state): State<McpManagerState>,
    Path(id): Path<String>,
) -> Result<Json<McpServerInfo>, (StatusCode, String)> {
    let servers = state.servers.read().await;
    servers
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Server {} not found", id)))
}

/// POST /api/servers/:id/start - Start server
async fn start_server(
    State(state): State<McpManagerState>,
    Path(id): Path<String>,
) -> Result<Json<McpServerInfo>, (StatusCode, String)> {
    let mut servers = state.servers.write().await;

    if let Some(server) = servers.get_mut(&id) {
        server.status = ServerStatus::Running;
        server.last_heartbeat = Some(chrono::Utc::now());
        Ok(Json(server.clone()))
    } else {
        Err((StatusCode::NOT_FOUND, format!("Server {} not found", id)))
    }
}

/// POST /api/servers/:id/stop - Stop server
async fn stop_server(
    State(state): State<McpManagerState>,
    Path(id): Path<String>,
) -> Result<Json<McpServerInfo>, (StatusCode, String)> {
    let mut servers = state.servers.write().await;

    if let Some(server) = servers.get_mut(&id) {
        server.status = ServerStatus::Stopped;
        Ok(Json(server.clone()))
    } else {
        Err((StatusCode::NOT_FOUND, format!("Server {} not found", id)))
    }
}

/// DELETE /api/servers/:id - Delete server
async fn delete_server(
    State(state): State<McpManagerState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut servers = state.servers.write().await;

    if servers.remove(&id).is_some() {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, format!("Server {} not found", id)))
    }
}

/// GET /api/tools - List all available MCP tools
async fn list_tools(State(state): State<McpManagerState>) -> Json<Vec<super::tool_registry::ToolInfo>> {
    let tools = state.tool_registry.list_tools().await;
    Json(tools)
}

/// POST /api/tools/:name/execute - Execute a tool
async fn execute_tool(
    State(state): State<McpManagerState>,
    Path(name): Path<String>,
    Json(params): Json<HashMap<String, serde_json::Value>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let params_value = serde_json::Value::Object(params.into_iter().collect());

    match state.tool_registry.execute_tool(&name, params_value).await {
        Ok(result) => {
            let result_json = serde_json::to_value(&result)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Serialization failed: {}", e)))?;
            Ok(Json(result_json))
        },
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Tool execution failed: {}", e))),
    }
}

/// GET /api/introspection - Get latest introspection data
async fn get_introspection(State(state): State<McpManagerState>) -> Json<Option<serde_json::Value>> {
    let db = state.introspection_db.read().await;
    Json(db.latest.clone())
}

/// GET /api/introspection/snapshots - List all snapshots
async fn list_snapshots(State(state): State<McpManagerState>) -> Json<Vec<String>> {
    let db = state.introspection_db.read().await;
    Json(db.snapshots.keys().cloned().collect())
}

/// GET /api/introspection/snapshots/:timestamp - Get specific snapshot
async fn get_snapshot(
    State(state): State<McpManagerState>,
    Path(timestamp): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db = state.introspection_db.read().await;
    db.snapshots
        .get(&timestamp)
        .cloned()
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Snapshot {} not found", timestamp)))
}

/// POST /api/introspection/run - Run introspection now
async fn run_introspection(State(state): State<McpManagerState>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use crate::introspection::SystemIntrospector;

    let introspector = SystemIntrospector::new();
    match introspector.introspect_system().await {
        Ok(report) => {
            let value = serde_json::to_value(&report)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Serialization failed: {}", e)))?;

            // Store in database
            let mut db = state.introspection_db.write().await;
            let timestamp = chrono::Utc::now().to_rfc3339();
            db.snapshots.insert(timestamp.clone(), value.clone());
            db.latest = Some(value.clone());

            Ok(Json(value))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Introspection failed: {}", e))),
    }
}

/// GET /api/isp-analysis - Get ISP/provider analysis
async fn get_isp_analysis(State(state): State<McpManagerState>) -> Json<Option<serde_json::Value>> {
    let db = state.introspection_db.read().await;
    Json(db.provider_analysis.clone())
}

/// POST /api/isp-analysis/run - Run ISP analysis now
async fn run_isp_analysis(State(state): State<McpManagerState>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use crate::isp_migration::IspMigrationAnalyzer;

    let analyzer = IspMigrationAnalyzer::new();
    match analyzer.analyze() {
        Ok(report) => {
            let value = serde_json::to_value(&report)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Serialization failed: {}", e)))?;

            // Store in database
            let mut db = state.introspection_db.write().await;
            db.provider_analysis = Some(value.clone());

            Ok(Json(value))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("ISP analysis failed: {}", e))),
    }
}

/// Build MCP Manager router
pub fn create_manager_router(state: McpManagerState) -> Router {
    Router::new()
        // Dashboard
        .route("/", get(dashboard))

        // Server management
        .route("/api/servers", get(list_servers).post(create_server))
        .route("/api/servers/:id", get(get_server).delete(delete_server))
        .route("/api/servers/:id/start", post(start_server))
        .route("/api/servers/:id/stop", post(stop_server))

        // Tool management
        .route("/api/tools", get(list_tools))
        .route("/api/tools/:name/execute", post(execute_tool))

        // Introspection database
        .route("/api/introspection", get(get_introspection))
        .route("/api/introspection/run", post(run_introspection))
        .route("/api/introspection/snapshots", get(list_snapshots))
        .route("/api/introspection/snapshots/:timestamp", get(get_snapshot))

        // ISP analysis
        .route("/api/isp-analysis", get(get_isp_analysis))
        .route("/api/isp-analysis/run", post(run_isp_analysis))

        // Static files (web UI)
        .nest_service("/static", ServeDir::new("src/mcp/web"))

        .with_state(state)
}

/// Start MCP Manager server
pub async fn start_manager(
    bind_addr: &str,
) -> anyhow::Result<()> {
    // Create tool registry and register introspection tools
    let tool_registry = ToolRegistry::new();
    super::introspection_tools::register_introspection_tools(&tool_registry).await?;

    let state = McpManagerState::new(tool_registry).await;

    let app = create_manager_router(state);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    println!("🚀 MCP Manager running on http://{}", bind_addr);
    println!("   Dashboard: http://{}/", bind_addr);
    println!("   API: http://{}/api/*", bind_addr);

    axum::serve(listener, app).await?;
    Ok(())
}
