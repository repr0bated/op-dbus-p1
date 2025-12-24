use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{broadcast, RwLock},
    time::interval,
};
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::{error, info, warn};
use zbus::Connection;

// Orchestrator proxy will be created manually

#[derive(Clone)]
struct AppState {
    status: Arc<RwLock<McpStatus>>,
    orchestrator: Option<zbus::Proxy<'static>>,
    tools: Arc<RwLock<Vec<ToolInfo>>>,
    services: Arc<RwLock<Vec<ServiceInfo>>>,
    broadcast: broadcast::Sender<WsMessage>,
    start_time: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpStatus {
    running: bool,
    uptime_secs: u64,
    request_count: u64,
    active_agents: Vec<AgentInfo>,
    system_info: SystemInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SystemInfo {
    hostname: String,
    os: String,
    cpu_count: usize,
    memory_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentInfo {
    id: String,
    agent_type: String,
    status: String,
    task: Option<String>,
    uptime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolInfo {
    name: String,
    description: String,
    category: String,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceInfo {
    name: String,
    path: String,
    category: String,
    interfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WsMessage {
    #[serde(rename = "type")]
    msg_type: String,
    data: Option<Value>,
    message: Option<String>,
    level: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    fn error(error: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error.into()),
        }
    }
}

pub async fn run_improved_web_server() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create broadcast channel for WebSocket messages
    let (tx, _) = broadcast::channel::<WsMessage>(100);

    // Try to connect to orchestrator
    let orchestrator = match Connection::session().await {
        Ok(conn) => match zbus::Proxy::new(
            &conn,
            "org.dbusmcp.Orchestrator",
            "/org/dbusmcp/Orchestrator",
            "org.dbusmcp.Orchestrator",
        )
        .await
        {
            Ok(proxy) => {
                info!("Web server connected to orchestrator");
                Some(proxy)
            }
            Err(e) => {
                warn!("Could not connect to orchestrator: {}", e);
                None
            }
        },
        Err(e) => {
            warn!("Could not connect to D-Bus: {}", e);
            None
        }
    };

    // Initialize system info
    let system_info = SystemInfo {
        hostname: gethostname::gethostname().to_string_lossy().to_string(),
        os: std::env::consts::OS.to_string(),
        cpu_count: num_cpus::get(),
        memory_total: 0, // Would need sys-info crate for actual memory
    };

    let state = AppState {
        status: Arc::new(RwLock::new(McpStatus {
            running: true,
            uptime_secs: 0,
            request_count: 0,
            active_agents: vec![],
            system_info,
        })),
        orchestrator,
        tools: Arc::new(RwLock::new(vec![])),
        services: Arc::new(RwLock::new(vec![])),
        broadcast: tx,
        start_time: Instant::now(),
    };

    // Start background tasks
    tokio::spawn(update_status_task(state.clone()));
    tokio::spawn(monitor_agents_task(state.clone()));

    // Load initial tools
    load_initial_tools(&state).await;

    // Setup routes
    let app = create_app(state);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("Web server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn create_app(state: AppState) -> Router {
    // Determine the web directory path
    let web_dir = PathBuf::from("src/mcp/web");
    let _index_file = web_dir.join("index.html");

    Router::new()
        // Static files and index
        .route("/", get(serve_index))
        .nest_service("/static", ServeDir::new(&web_dir))
        // API routes
        .route("/api/status", get(api_status))
        .route("/api/tools", get(api_list_tools))
        .route("/api/tools/:name", post(api_execute_tool))
        .route("/api/agents", get(api_list_agents))
        .route("/api/agents", post(api_spawn_agent))
        .route("/api/agents/:id", delete(api_kill_agent))
        .route("/api/agents/:id/task", post(api_send_task))
        .route("/api/discovery/run", post(api_run_discovery))
        .route("/api/discovery/services", get(api_list_services))
        // WebSocket endpoint
        .route("/ws", get(websocket_handler))
        // Add CORS layer
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        // Add tracing
        .layer(TraceLayer::new_for_http())
        // Add state
        .with_state(state)
}

// Static file serving
async fn serve_index() -> impl IntoResponse {
    let content = include_str!("web/index.html");
    Html(content)
}

// API Handlers
async fn api_status(State(state): State<AppState>) -> impl IntoResponse {
    let mut status = state.status.write().await;
    status.uptime_secs = state.start_time.elapsed().as_secs();
    status.request_count += 1;

    Json(ApiResponse::success(status.clone()))
}

async fn api_list_tools(State(state): State<AppState>) -> impl IntoResponse {
    let tools = state.tools.read().await;
    Json(ApiResponse::success(json!({
        "tools": tools.clone()
    })))
}

async fn api_execute_tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(params): Json<Value>,
) -> impl IntoResponse {
    // Increment request count
    {
        let mut status = state.status.write().await;
        status.request_count += 1;
    }

    // In a real implementation, this would execute the tool
    // For now, return a mock response
    let result = json!({
        "content": [{
            "type": "text",
            "text": format!("Executed tool '{}' with params: {}", name, params)
        }]
    });

    // Broadcast activity
    let _ = state.broadcast.send(WsMessage {
        msg_type: "activity".to_string(),
        data: None,
        message: Some(format!("Executed tool: {}", name)),
        level: Some("info".to_string()),
    });

    Json(ApiResponse::success(result))
}

async fn api_list_agents(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(orchestrator) = &state.orchestrator {
        match orchestrator
            .call::<&str, (), Vec<String>>("ListAgents", &())
            .await
        {
            Ok(agent_ids) => {
                let mut agents = vec![];
                for id in agent_ids {
                    let params: (String,) = (id.clone(),);
                    let result: Result<String, _> =
                        orchestrator.call("GetAgentStatus", &params).await;
                    if let Ok(status_json) = result {
                        if let Ok(status) = serde_json::from_str::<Value>(&status_json) {
                            agents.push(AgentInfo {
                                id: id,
                                agent_type: status["type"]
                                    .as_str()
                                    .unwrap_or("unknown")
                                    .to_string(),
                                status: status["status"].as_str().unwrap_or("unknown").to_string(),
                                task: status["task"].as_str().map(String::from),
                                uptime: status["uptime"].as_u64().unwrap_or(0),
                            });
                        }
                    }
                }
                Json(ApiResponse::success(agents))
            }
            Err(e) => Json(ApiResponse::error(format!("Failed to list agents: {}", e))),
        }
    } else {
        Json(ApiResponse::error("Orchestrator not available"))
    }
}

async fn api_spawn_agent(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let agent_type = payload["type"].as_str().unwrap_or("executor").to_string();
    let config = payload["config"].to_string();

    if let Some(orchestrator) = &state.orchestrator {
        let params: (String, String) = (agent_type.clone(), config);
        match orchestrator
            .call::<&str, (String, String), String>("SpawnAgent", &params)
            .await
        {
            Ok(agent_id) => {
                // Broadcast activity
                let _ = state.broadcast.send(WsMessage {
                    msg_type: "activity".to_string(),
                    data: None,
                    message: Some(format!("Spawned {} agent: {}", agent_type, agent_id)),
                    level: Some("success".to_string()),
                });

                Json(ApiResponse::success(json!({
                    "agent_id": agent_id
                })))
            }
            Err(e) => Json(ApiResponse::error(format!("Failed to spawn agent: {}", e))),
        }
    } else {
        Json(ApiResponse::error("Orchestrator not available"))
    }
}

async fn api_kill_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    if let Some(orchestrator) = &state.orchestrator {
        match orchestrator.call("KillAgent", &(agent_id.clone(),)).await {
            Ok(success) => {
                if success {
                    // Broadcast activity
                    let _ = state.broadcast.send(WsMessage {
                        msg_type: "activity".to_string(),
                        data: None,
                        message: Some(format!("Killed agent: {}", agent_id)),
                        level: Some("warning".to_string()),
                    });

                    Json(ApiResponse::success(json!({ "killed": true })))
                } else {
                    Json(ApiResponse::error("Failed to kill agent"))
                }
            }
            Err(e) => Json(ApiResponse::error(format!("Failed to kill agent: {}", e))),
        }
    } else {
        Json(ApiResponse::error("Orchestrator not available"))
    }
}

async fn api_send_task(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Json(task): Json<Value>,
) -> impl IntoResponse {
    if let Some(orchestrator) = &state.orchestrator {
        let params: (String, String) = (agent_id.clone(), task.to_string());
        match orchestrator
            .call::<&str, (String, String), String>("SendTask", &params)
            .await
        {
            Ok(result) => Json(ApiResponse::success(json!({ "result": result }))),
            Err(e) => Json(ApiResponse::error(format!("Failed to send task: {}", e))),
        }
    } else {
        Json(ApiResponse::error("Orchestrator not available"))
    }
}

async fn api_run_discovery(State(state): State<AppState>) -> impl IntoResponse {
    // In a real implementation, this would run the discovery process
    // For now, return mock data
    let services = vec![
        ServiceInfo {
            name: "org.freedesktop.systemd1".to_string(),
            path: "/org/freedesktop/systemd1".to_string(),
            category: "System".to_string(),
            interfaces: vec!["org.freedesktop.systemd1.Manager".to_string()],
        },
        ServiceInfo {
            name: "org.freedesktop.NetworkManager".to_string(),
            path: "/org/freedesktop/NetworkManager".to_string(),
            category: "Network".to_string(),
            interfaces: vec!["org.freedesktop.NetworkManager".to_string()],
        },
    ];

    let mut stored_services = state.services.write().await;
    *stored_services = services.clone();

    // Broadcast activity
    let _ = state.broadcast.send(WsMessage {
        msg_type: "activity".to_string(),
        data: None,
        message: Some(format!("Discovered {} services", services.len())),
        level: Some("success".to_string()),
    });

    Json(ApiResponse::success(json!({
        "count": services.len(),
        "services": services
    })))
}

async fn api_list_services(State(state): State<AppState>) -> impl IntoResponse {
    let services = state.services.read().await;
    Json(ApiResponse::success(services.clone()))
}

// WebSocket Handler
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_websocket(socket, state))
}

async fn handle_websocket(socket: WebSocket, state: AppState) {
    use futures::{SinkExt, StreamExt};
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.broadcast.subscribe();

    // Send initial status
    let status = state.status.read().await;
    let initial_msg = WsMessage {
        msg_type: "status".to_string(),
        data: Some(json!(status.clone())),
        message: None,
        level: None,
    };

    if let Ok(msg) = serde_json::to_string(&initial_msg) {
        let _ = sender.send(Message::Text(msg)).await;
    }

    // Spawn task to forward broadcast messages to WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Ok(text) = serde_json::to_string(&msg) {
                if sender.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Handle incoming messages
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    // Handle incoming text messages
                    if let Ok(data) = serde_json::from_str::<Value>(&text) {
                        info!("Received WebSocket message: {:?}", data);
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

// Background Tasks
async fn update_status_task(state: AppState) {
    let mut interval = interval(Duration::from_secs(1));

    loop {
        interval.tick().await;

        let mut status = state.status.write().await;
        status.uptime_secs = state.start_time.elapsed().as_secs();

        // Broadcast status update
        let _ = state.broadcast.send(WsMessage {
            msg_type: "status".to_string(),
            data: Some(json!(status.clone())),
            message: None,
            level: None,
        });
    }
}

async fn monitor_agents_task(state: AppState) {
    let mut interval = interval(Duration::from_secs(5));

    loop {
        interval.tick().await;

        if let Some(orchestrator) = &state.orchestrator {
            if let Ok(agent_ids) = orchestrator
                .call::<&str, (), Vec<String>>("ListAgents", &())
                .await
            {
                let mut agents = vec![];

                for id in agent_ids {
                    let params: (String,) = (id.clone(),);
                    let result: Result<String, _> =
                        orchestrator.call("GetAgentStatus", &params).await;
                    if let Ok(status_json) = result {
                        if let Ok(status) = serde_json::from_str::<Value>(&status_json) {
                            agents.push(AgentInfo {
                                id: id,
                                agent_type: status["type"]
                                    .as_str()
                                    .unwrap_or("unknown")
                                    .to_string(),
                                status: status["status"].as_str().unwrap_or("unknown").to_string(),
                                task: status["task"].as_str().map(String::from),
                                uptime: status["uptime"].as_u64().unwrap_or(0),
                            });
                        }
                    }
                }

                // Update status
                let mut status = state.status.write().await;
                status.active_agents = agents.clone();

                // Broadcast agent update
                let _ = state.broadcast.send(WsMessage {
                    msg_type: "agent_update".to_string(),
                    data: Some(json!(agents)),
                    message: None,
                    level: None,
                });
            }
        }
    }
}

async fn load_initial_tools(state: &AppState) {
    // Load some example tools
    let tools = vec![
        ToolInfo {
            name: "systemd_status".to_string(),
            description: "Get systemd service status".to_string(),
            category: "System".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "service": {
                        "type": "string",
                        "description": "Service name"
                    }
                },
                "required": ["service"]
            }),
        },
        ToolInfo {
            name: "file_read".to_string(),
            description: "Read file contents".to_string(),
            category: "File".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolInfo {
            name: "network_interfaces".to_string(),
            description: "List network interfaces".to_string(),
            category: "Network".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ];

    let mut stored_tools = state.tools.write().await;
    *stored_tools = tools;
}
