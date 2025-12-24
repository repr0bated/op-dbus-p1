use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
// Rate limiting will be implemented with a simple in-memory counter
use tower_http::services::ServeDir;
use zbus::Connection;

// Orchestrator proxy will be created manually

#[derive(Clone)]
struct AppState {
    mcp_status: Arc<RwLock<McpStatus>>,
    orchestrator: Option<zbus::Proxy<'static>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpStatus {
    running: bool,
    uptime_secs: u64,
    request_count: u64,
    active_agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentInfo {
    id: String,
    agent_type: String,
    status: String,
    task: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

pub async fn run_web_server() -> Result<(), Box<dyn std::error::Error>> {
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
                eprintln!("Web server connected to orchestrator");
                Some(proxy)
            }
            Err(e) => {
                eprintln!(
                    "Warning: Web server could not connect to orchestrator: {}",
                    e
                );
                eprintln!("Agent management features will be unavailable");
                None
            }
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to D-Bus session: {}", e);
            None
        }
    };

    let state = AppState {
        mcp_status: Arc::new(RwLock::new(McpStatus {
            running: true,
            uptime_secs: 0,
            request_count: 0,
            active_agents: vec![],
        })),
        orchestrator,
    };

    let app = Router::new()
        // Web UI
        .route("/", get(dashboard_handler))
        // API endpoints
        .route("/api/status", get(api_status))
        .route("/api/agents", get(api_list_agents))
        .route("/api/agents", post(api_spawn_agent))
        .route("/api/tools", get(api_list_tools))
        .route("/api/tools/:name", post(api_execute_tool))
        // MCP Discovery endpoints
        .route("/api/discovery/run", post(api_run_discovery))
        .route("/api/discovery/services", get(api_list_services))
        // WebSocket
        .route("/ws/mcp", get(ws_mcp_handler))
        .route("/ws/events", get(ws_events_handler))
        // Static files
        .nest_service("/static", ServeDir::new("web"))
        // TODO: Add rate limiting with tower_governor once API is clarified
        // Rate limiting: 10 requests per second with burst of 20 planned
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    println!("🌐 Web interface available at: http://0.0.0.0:8080");
    println!("   Access from network at: http://<your-ip>:8080");

    axum::serve(listener, app).await?;

    Ok(())
}

async fn dashboard_handler() -> Html<&'static str> {
    Html(
        r#"
<!DOCTYPE html>
<html>
<head>
    <title>D-Bus MCP Control Panel</title>
    <style>
        body {
            font-family: system-ui, -apple-system, sans-serif;
            margin: 0;
            padding: 20px;
            background: #1a1a1a;
            color: #e0e0e0;
        }
        .header {
            border-bottom: 2px solid #333;
            padding-bottom: 10px;
            margin-bottom: 20px;
        }
        .status-indicator {
            display: inline-block;
            width: 12px;
            height: 12px;
            border-radius: 50%;
            background: #4ade80;
            margin-right: 8px;
        }
        .grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
            gap: 20px;
            margin-top: 20px;
        }
        .card {
            background: #2a2a2a;
            border: 1px solid #333;
            border-radius: 8px;
            padding: 20px;
        }
        .card h3 {
            margin-top: 0;
            color: #60a5fa;
        }
        .stat {
            display: flex;
            justify-content: space-between;
            padding: 8px 0;
            border-bottom: 1px solid #333;
        }
        .agent {
            background: #1e293b;
            padding: 10px;
            margin: 8px 0;
            border-radius: 4px;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }
        .agent-status {
            font-size: 12px;
            padding: 4px 8px;
            border-radius: 4px;
            background: #334155;
        }
        .agent-status.idle { background: #059669; }
        .agent-status.working { background: #d97706; }
        button {
            background: #3b82f6;
            color: white;
            border: none;
            padding: 8px 16px;
            border-radius: 4px;
            cursor: pointer;
        }
        button:hover {
            background: #2563eb;
        }
        .activity {
            font-size: 14px;
            padding: 8px;
            border-left: 3px solid #3b82f6;
            margin: 8px 0;
            background: #1e293b;
        }
        .timestamp {
            color: #94a3b8;
            font-size: 12px;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>
            <span class="status-indicator"></span>
            D-Bus MCP Control Panel
        </h1>
    </div>

    <div class="grid">
        <div class="card">
            <h3>Server Status</h3>
            <div class="stat">
                <span>Status:</span>
                <span id="server-status">Running</span>
            </div>
            <div class="stat">
                <span>Uptime:</span>
                <span id="uptime">0h 0m</span>
            </div>
            <div class="stat">
                <span>Requests:</span>
                <span id="requests">0</span>
            </div>
            <div class="stat">
                <span>CPU:</span>
                <span id="cpu">0%</span>
            </div>
        </div>

        <div class="card">
            <h3>Active Agents</h3>
            <div id="agents-list">
                <p style="color: #94a3b8;">No agents running</p>
            </div>
            <button onclick="spawnAgent()">+ Spawn Agent</button>
        </div>

        <div class="card">
            <h3>MCP Discovery</h3>
            <div id="services-list">
                <p style="color: #94a3b8;">No services discovered yet</p>
            </div>
            <button onclick="runDiscovery()">🔍 Discover D-Bus Services</button>
        </div>
    </div>

    <div class="card" style="margin-top: 20px;">
        <h3>Recent Activity</h3>
        <div id="activity-log">
            <div class="activity">
                <span class="timestamp">System started</span>
            </div>
        </div>
    </div>

    <script>
        let ws;

        function connect() {
            // Use current window location to support remote access
            const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${wsProtocol}//${window.location.host}/ws/events`;
            ws = new WebSocket(wsUrl);

            ws.onmessage = (event) => {
                const data = JSON.parse(event.data);
                console.log('Event:', data);
                updateDashboard(data);
            };

            ws.onclose = () => {
                console.log('Disconnected, reconnecting...');
                setTimeout(connect, 1000);
            };
        }

        function updateDashboard(data) {
            if (data.type === 'status_update') {
                document.getElementById('uptime').textContent =
                    Math.floor(data.uptime / 3600) + 'h ' +
                    Math.floor((data.uptime % 3600) / 60) + 'm';
                document.getElementById('requests').textContent = data.requests;
            }

            if (data.type === 'agent_update') {
                updateAgentsList(data.agents);
            }
        }

        function updateAgentsList(agents) {
            const list = document.getElementById('agents-list');
            if (agents.length === 0) {
                list.innerHTML = '<p style="color: #94a3b8;">No agents running</p>';
                return;
            }

            list.innerHTML = agents.map(agent => `
                <div class="agent">
                    <div>
                        <strong>${agent.id}</strong>
                        <div style="font-size: 12px; color: #94a3b8;">${agent.agent_type}</div>
                    </div>
                    <span class="agent-status ${agent.status}">${agent.status}</span>
                </div>
            `).join('');
        }

        async function spawnAgent() {
            const type = prompt('Agent type (executor, systemd, file, monitor, network):', 'executor');
            if (!type) return;

            const response = await fetch('/api/agents', {
                method: 'POST',
                headers: {'Content-Type': 'application/json'},
                body: JSON.stringify({agent_type: type})
            });

            const result = await response.json();
            if (result.success) {
                addActivity(`Spawned agent: ${result.data.id}`);
                fetchAgents();
            } else {
                addActivity(`Failed to spawn agent: ${result.error}`);
            }
        }

        function addActivity(message) {
            const log = document.getElementById('activity-log');
            const time = new Date().toLocaleTimeString();
            const entry = document.createElement('div');
            entry.className = 'activity';
            entry.innerHTML = `<span class="timestamp">${time}</span> ${message}`;
            log.insertBefore(entry, log.firstChild);
        }

        // Fetch initial status
        async function fetchStatus() {
            const response = await fetch('/api/status');
            const data = await response.json();
            updateDashboard({type: 'status_update', ...data.data});
        }

        // Fetch agents list
        async function fetchAgents() {
            const response = await fetch('/api/agents');
            const data = await response.json();
            if (data.success) {
                updateAgentsList(data.data);
            }
        }

        // Run MCP discovery
        async function runDiscovery() {
            addActivity('Running D-Bus discovery...');
            const response = await fetch('/api/discovery/run', {method: 'POST'});
            const result = await response.json();

            if (result.success) {
                addActivity('Discovery completed successfully');
                fetchServices();
            } else {
                addActivity(`Discovery failed: ${result.error}`);
            }
        }

        // Fetch discovered services
        async function fetchServices() {
            const response = await fetch('/api/discovery/services');
            const data = await response.json();

            if (data.success && data.data.length > 0) {
                const list = document.getElementById('services-list');
                list.innerHTML = data.data.map(service => `
                    <div class="agent">
                        <div>
                            <strong>${service.name}</strong>
                            <div style="font-size: 12px; color: #94a3b8;">MCP Server</div>
                        </div>
                        <span class="agent-status active">Ready</span>
                    </div>
                `).join('');
            }
        }

        connect();
        fetchStatus();
        fetchAgents();
        fetchServices();
        setInterval(fetchStatus, 5000);
        setInterval(fetchAgents, 3000);
        setInterval(fetchServices, 10000);
    </script>
</body>
</html>
    "#,
    )
}

async fn api_status(State(state): State<AppState>) -> Json<ApiResponse<McpStatus>> {
    let status = state.mcp_status.read().await;
    Json(ApiResponse {
        success: true,
        data: Some(status.clone()),
        error: None,
    })
}

async fn api_list_agents(State(state): State<AppState>) -> Json<ApiResponse<Vec<AgentInfo>>> {
    match &state.orchestrator {
        Some(orch) => {
            let result: Result<Vec<String>, _> = orch.call("ListAgents", &()).await;
            match result {
                Ok(agent_ids) => {
                    // Convert agent IDs to AgentInfo structures
                    let agents: Vec<AgentInfo> = agent_ids
                        .iter()
                        .map(|id| {
                            // Extract agent type from ID (format: "type-uuid")
                            let agent_type = id.split('-').next().unwrap_or("unknown").to_string();
                            AgentInfo {
                                id: id.clone(),
                                agent_type,
                                status: "running".to_string(),
                                task: None,
                            }
                        })
                        .collect();

                    Json(ApiResponse {
                        success: true,
                        data: Some(agents),
                        error: None,
                    })
                }
                Err(e) => Json(ApiResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to list agents: {}", e)),
                }),
            }
        }
        None => Json(ApiResponse {
            success: true,
            data: Some(vec![]),
            error: None,
        }),
    }
}

#[derive(Deserialize)]
struct SpawnAgentRequest {
    agent_type: String,
}

async fn api_spawn_agent(
    State(state): State<AppState>,
    Json(req): Json<SpawnAgentRequest>,
) -> Json<ApiResponse<AgentInfo>> {
    match &state.orchestrator {
        Some(orch) => {
            let result: Result<String, _> =
                orch.call("SpawnAgent", &(req.agent_type.clone(),)).await;
            match result {
                Ok(agent_id) => {
                    let agent = AgentInfo {
                        id: agent_id.clone(),
                        agent_type: req.agent_type,
                        status: "spawning".to_string(),
                        task: None,
                    };

                    let mut status = state.mcp_status.write().await;
                    status.active_agents.push(agent.clone());

                    Json(ApiResponse {
                        success: true,
                        data: Some(agent),
                        error: None,
                    })
                }
                Err(e) => Json(ApiResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to spawn agent: {}", e)),
                }),
            }
        }
        None => Json(ApiResponse {
            success: false,
            data: None,
            error: Some("Orchestrator not available".to_string()),
        }),
    }
}

async fn api_list_tools() -> Json<ApiResponse<Vec<String>>> {
    Json(ApiResponse {
        success: true,
        data: Some(vec![
            "execute_command".to_string(),
            "manage_systemd_service".to_string(),
            "query_dbus_service".to_string(),
            "spawn_agent".to_string(),
            "list_agents".to_string(),
            "dbus_introspect".to_string(),
        ]),
        error: None,
    })
}

async fn api_execute_tool() -> Json<ApiResponse<Value>> {
    Json(ApiResponse {
        success: true,
        data: Some(serde_json::json!({"result": "Tool execution not yet implemented"})),
        error: None,
    })
}

// MCP Discovery handlers
async fn api_run_discovery() -> Json<ApiResponse<Value>> {
    // Run discovery binary
    let output = tokio::process::Command::new("./target/debug/dbus-mcp-discovery")
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Json(ApiResponse {
                success: true,
                data: Some(serde_json::json!({
                    "message": "Discovery completed successfully",
                    "output": stdout.to_string()
                })),
                error: None,
            })
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Discovery failed: {}", stderr)),
            })
        }
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            error: Some(format!("Failed to run discovery: {}", e)),
        }),
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ServiceInfo {
    name: String,
    method_count: usize,
    config_path: Option<String>,
}

async fn api_list_services() -> Json<ApiResponse<Vec<ServiceInfo>>> {
    // Check for generated MCP configs
    let config_dir = std::path::Path::new("/tmp/mcp-servers");

    let mut services = Vec::new();

    if config_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(config_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        let service_name = name.trim_end_matches(".json").to_string();
                        services.push(ServiceInfo {
                            name: service_name,
                            method_count: 0, // Would need to parse JSON to get this
                            config_path: Some(entry.path().to_string_lossy().to_string()),
                        });
                    }
                }
            }
        }
    }

    Json(ApiResponse {
        success: true,
        data: Some(services),
        error: None,
    })
}

async fn ws_mcp_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_mcp_socket)
}

async fn handle_mcp_socket(mut socket: WebSocket) {
    while let Some(msg) = socket.recv().await {
        if let Ok(msg) = msg {
            if let axum::extract::ws::Message::Text(text) = msg {
                // Forward to MCP server via stdio
                // For now, echo back
                let response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {"status": "ok"}
                });

                let _ = socket
                    .send(axum::extract::ws::Message::Text(
                        serde_json::to_string(&response).unwrap(),
                    ))
                    .await;
            }
        }
    }
}

async fn ws_events_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_events_socket(socket, state))
}

async fn handle_events_socket(mut socket: WebSocket, state: AppState) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

    loop {
        interval.tick().await;

        let status = state.mcp_status.read().await;
        let event = serde_json::json!({
            "type": "status_update",
            "uptime": status.uptime_secs,
            "requests": status.request_count,
        });

        if socket
            .send(axum::extract::ws::Message::Text(
                serde_json::to_string(&event).unwrap(),
            ))
            .await
            .is_err()
        {
            break;
        }
    }
}
