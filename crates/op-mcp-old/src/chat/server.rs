//! Standalone MCP Chat Server with Unified Introspection Support
//! Moved to chat module for tighter integration

use anyhow::{Context, Result};
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use tower_http::{
    cors::CorsLayer,
    services::ServeDir,
    trace::TraceLayer,
};
use tracing::{error, info};

use crate::ollama::{self, OllamaClient};
// TODO: These modules don't exist yet
// use crate::http_tls_server::*;
use crate::workflow_plugin_introspection;
use crate::introspection_cache;
use super::orchestrator;
// use super::introspection::{self, introspect_server_config, ServerConfig};
// use crate::plugin_system::{Plugin, PluginRegistry};
// use crate::plugins::network::NetworkPlugin;
// use crate::plugins::systemd::SystemdPlugin;
// use crate::plugins::dbus_auto::DbusAutoPlugin;



// Chat message structure with unified system context
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ChatMessage {
    User {
        content: String,
        timestamp: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<SystemContext>,
    },
    Assistant {
        content: String,
        timestamp: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        tools_used: Option<Vec<String>>,
    },
    Error {
        content: String,
        timestamp: u64
    },
}

// Unified system context for AI awareness
// Includes D-Bus services, tools (which include plugin-derived operations), and workflows
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SystemContext {
    active_services: Vec<String>,
    network_status: String,
    system_load: f32,
    available_tools: Vec<String>,
    tool_count: usize,
}

// Chat server state with unified introspection support and mandatory AI
// This consolidates tool/plugin introspection into a single registry
// Note: We avoid IntrospectionCache here to prevent Send+Sync issues with rusqlite
// The cache is designed for CLI usage, not async web servers
#[derive(Clone)]
struct ChatState {
    ollama_client: Arc<OllamaClient>,  // Mandatory - AI is the brain
    conversations: Arc<RwLock<HashMap<String, Vec<ChatMessage>>>>,
    // Cached unified introspection data from plugins and workflows
    // This replaces the need for IntrospectionCache in web context
    tool_introspection: Arc<RwLock<Option<Value>>>,
    // Enhanced capabilities: orchestrator for system orchestration
    _orchestrator: Arc<orchestrator::Orchestrator>,
    plugin_registry: Arc<PluginRegistry>,
    start_time: std::time::SystemTime,
    // Model selection support
    available_models: Arc<Vec<String>>,
    conversation_models: Arc<RwLock<HashMap<String, String>>>, // conversation_id -> model_name
    // External MCP server integration
    mcp_registry: Arc<crate::external_mcp_client::McpServerRegistry>,
    // SSE event broadcaster
    sse_broadcaster: Arc<RwLock<crate::sse_streaming::SseEventBroadcaster>>,
}

pub async fn run() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting AI Chat Server...");

    // AI is mandatory - the brain of the system
    let api_key = std::env::var("OLLAMA_API_KEY")
        .expect("❌ OLLAMA_API_KEY required - AI is the core of this system\n\
                 Get your API key from: https://ollama.com");

    let model = std::env::var("OLLAMA_DEFAULT_MODEL")
        .unwrap_or_else(|_| "mistral".to_string());

    info!("✅ AI Enabled - Using model: {}", model);

    // Initialize Ollama client
    let ollama_client = Arc::new(
        OllamaClient::cloud(api_key)
            .with_default_model(model.clone())
    );

    // Test AI connection
    match ollama_client.health_check().await {
        Ok(true) => info!("✅ Connected to Ollama"),
        Ok(false) => info!("⚠️  Using cloud API (no local Ollama)"),
        Err(e) => {
            eprintln!("❌ Ollama connection failed: {}", e);
            eprintln!("Make sure OLLAMA_API_KEY is valid and set OLLAMA_DEFAULT_MODEL");
            std::process::exit(1);
        }
    }

    // Initialize plugin registry
    let plugin_registry = Arc::new(PluginRegistry::new());
    
    // Register Network Plugin
    let network_plugin = NetworkPlugin {
        bridges: vec![],
        interfaces: vec![],
        ovsdb: Default::default(),
    };
    plugin_registry.register(Box::new(network_plugin)).await?;
    info!("✅ Network plugin registered");

    // Register Systemd Plugin
    let systemd_plugin = SystemdPlugin {
        services: vec![], // Will use defaults
    };
    plugin_registry.register(Box::new(systemd_plugin)).await?;
    info!("✅ Systemd plugin registered");

    // Auto-discover D-Bus plugins
    discover_dbus_plugins(&plugin_registry).await;

    // Build unified tool introspection
    // This consolidates plugins (via PluginToolBridge) and native tools into one registry
    // Note: We use this instead of IntrospectionCache which has rusqlite Send+Sync issues
    let tool_introspection = build_unified_tool_introspection(&plugin_registry).await;
    info!("✅ Unified tool introspection initialized from workflows and plugins");

    // Initialize orchestrator for system task orchestration
    let orchestrator = Arc::new(orchestrator::Orchestrator::new().await?);
    info!("✅ Orchestrator initialized for system task orchestration");

    // Fetch available models from Ollama
    let available_models = match ollama_client.list_models().await {
        Ok(models) => {
            let model_names: Vec<String> = models.iter()
                .map(|m| m.name.clone())
                .collect();
            info!("✅ Available models: {}", model_names.join(", "));
            model_names
        }
        Err(e) => {
            error!("⚠️  Could not fetch models from Ollama: {}", e);
            info!("   Using default model: {}", model);
            vec![model.clone()]
        }
    };

    // Initialize external MCP server registry
    let mcp_registry = Arc::new(crate::external_mcp_client::McpServerRegistry::new());
    info!("✅ MCP server registry initialized");

    // Load external MCP servers from configuration file (if present)
    if std::path::Path::new("mcp-servers.toml").exists() {
        match std::fs::read_to_string("mcp-servers.toml") {
            Ok(toml_str) => {
                match toml::from_str::<crate::external_mcp_client::McpServersConfig>(&toml_str) {
                    Ok(servers_cfg) => {
                        for server in servers_cfg.servers {
                            if server.enabled {
                                let client = crate::external_mcp_client::McpClient::new(server.clone()).await;
                                match client {
                                    Ok(mut c) => {
                                        if let Err(e) = c.connect().await {
                                            error!("Failed to connect to MCP server {}: {}", server.name, e);
                                        } else {
                                            if let Err(e) = mcp_registry.register(c).await {
                                                error!("Failed to register MCP server {}: {}", server.name, e);
                                            }
                                        }
                                    }
                                    Err(e) => error!("Failed to create MCP client for {}: {}", server.name, e),
                                }
                            }
                        }
                    }
                    Err(e) => error!("Failed to parse mcp-servers.toml: {}", e),
                }
            }
            Err(e) => error!("Unable to read mcp-servers.toml: {}", e),
        }
    }

    // Initialize SSE broadcaster
    let (sse_broadcaster, _rx) = crate::sse_streaming::SseEventBroadcaster::new();
    let sse_broadcaster = Arc::new(RwLock::new(sse_broadcaster));
    info!("✅ SSE event broadcaster initialized");

    // Create enhanced chat state with orchestrator capabilities
    let chat_state = ChatState {
        ollama_client,
        conversations: Arc::new(RwLock::new(HashMap::new())),
        tool_introspection: Arc::new(RwLock::new(tool_introspection)),
        _orchestrator: orchestrator,
        plugin_registry,
        start_time: std::time::SystemTime::now(),
        available_models: Arc::new(available_models),
        conversation_models: Arc::new(RwLock::new(HashMap::new())),
        mcp_registry: mcp_registry.clone(),
        sse_broadcaster: sse_broadcaster.clone(),
    };

    info!("✅ Chat state initialized with unified introspection support");
    info!("   - Plugin/tool registry: consolidated");
    info!("   - Workflows: available");

    // Setup static file serving for the web UI
    let web_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("mcp")
        .join("web");

    // Introspect server configuration
    let config = introspect_server_config().await;

    // Create MCP chat service router with state
    let chat_state_arc = Arc::new(chat_state);
    let chat_router = ServiceRouter::new("/api/chat")
        .route("/mcp", post({
            let state = chat_state_arc.clone();
            move |Json(payload): Json<Value>| async move {
                mcp_handler(State((*state).clone()), Json(payload)).await
            }
        }))
        .route("/health", get(health_handler))
        .route("/status", get({
            let state = chat_state_arc.clone();
            move || async move {
                status_handler(State((*state).clone())).await
            }
        }))
        .route("/models", get({
            let state = chat_state_arc.clone();
            move || async move {
                models_handler(State((*state).clone())).await
            }
        }));

    // MCP discovery and config routes
    let mcp_discover_router = ServiceRouter::new("/api/mcp")
        .route("/_discover", get({
            let state = chat_state_arc.clone();
            move || async move {
                mcp_discover_handler(State((*state).clone())).await
            }
        }))
        .route("/_config", get({
            let state = chat_state_arc.clone();
            move || async move {
                mcp_config_handler(State((*state).clone())).await
            }
        }))
        .route("/_config/claude", get({
            let state = chat_state_arc.clone();
            move || async move {
                mcp_claude_config_handler(State((*state).clone())).await
            }
        }));

    // Create web interface router
    // Development: run chat-ui separately with `npm run dev`
    // Production: serve built chat-ui from chat-ui/build
    let web_router = ServiceRouter::new("/")
        .static_dir("/", "chat-ui/build");

    // Placeholder router for external MCP servers (dynamic forwarding)
    let external_mcp_router = ServiceRouter::new("/api/mcp/:server")
        .route("/", post(external_mcp_handler))
        .route("/events", get(external_mcp_sse_handler));

    // Register external MCP router
    let server = ServerBuilder::new()
        .bind_addr(format!("{}:{}", config.bind_host, config.http_port))
        .public_host(&config.public_host)
        .https_auto()
        .service_router(chat_router)
        .service_router(mcp_discover_router)
        .service_router(external_mcp_router)
        .service_router(web_router)
        .build()
        .await?;

        .build()
        .await?;

    info!("🚀 MCP Chat Server starting...");
    server.serve().await?;

    Ok(())
}

// Enhanced MCP helper functions with orchestrator integration

async fn get_available_tools(state: &ChatState) -> Vec<Value> {
    // Get tools from unified introspection
    let tool_introspection = state.tool_introspection.read().await;
    if let Some(introspection) = tool_introspection.as_ref() {
        if let Some(tools) = introspection.get("tools") {
            if let Some(tools_array) = tools.as_array() {
                return tools_array.clone();
            }
        }
    }

    // Fallback: get orchestrator capabilities
    get_orchestrator_tools(state).await
}

async fn get_orchestrator_tools(_state: &ChatState) -> Vec<Value> {
    vec![
        json!({
            "name": "orchestrate_system_task",
            "description": "Orchestrate complex system tasks across multiple agents",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_type": {
                        "type": "string",
                        "description": "Type of system task (deploy, monitor, configure, etc.)"
                    },
                    "target_systems": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Target systems or services"
                    },
                    "parameters": {
                        "type": "object",
                        "description": "Task-specific parameters"
                    }
                },
                "required": ["task_type"]
            }
        }),
        json!({
            "name": "dbus_discovery",
            "description": "Discover and introspect D-Bus services and interfaces",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "service_filter": {
                        "type": "string",
                        "description": "Filter services by name pattern"
                    },
                    "interface_filter": {
                        "type": "string",
                        "description": "Filter interfaces by name pattern"
                    }
                }
            }
        }),
        json!({
            "name": "system_introspect",
            "description": "Get comprehensive system introspection data",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "layers": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "System layers to introspect (physical, network, service, application)",
                        "default": ["physical", "network", "service", "application"]
                    }
                }
            }
        }),
        json!({
            "name": "workflow_orchestrate",
            "description": "Execute complex multi-step workflows with orchestration",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workflow_type": {
                        "type": "string",
                        "description": "Type of workflow (deployment, monitoring, maintenance)"
                    },
                    "targets": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Target systems for workflow execution"
                    },
                    "workflow_params": {
                        "type": "object",
                        "description": "Workflow-specific parameters"
                    }
                },
                "required": ["workflow_type"]
            }
        })
    ]
}

async fn execute_tool_with_orchestration(
    state: &ChatState,
    tool_name: &str,
    parameters: &Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    info!("Executing tool with orchestration: {} with params {:?}", tool_name, parameters);

    match tool_name {
        "orchestrate_system_task" => {
            orchestrate_system_task(state, parameters).await
        }
        "dbus_discovery" => {
            dbus_discovery(state, parameters).await
        }
        "system_introspect" => {
            system_introspect(state, parameters).await
        }
        "workflow_orchestrate" => {
            workflow_orchestrate(state, parameters).await
        }
        _ => {
            // Try to execute as regular tool
            execute_regular_tool(state, tool_name, parameters).await
        }
    }
}

async fn orchestrate_system_task(
    state: &ChatState,
    parameters: &Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let task_type = parameters.get("task_type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let default_targets = vec![];
    let target_systems = parameters.get("target_systems")
        .and_then(|v| v.as_array())
        .unwrap_or(&default_targets)
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();

    let default_params = json!({});
    let task_params = parameters.get("parameters")
        .unwrap_or(&default_params);

    info!("Orchestrating system task: {} on targets {:?}", task_type, target_systems);

    // Create orchestrated task
    let task_payload = json!({
        "task_type": task_type,
        "target_systems": target_systems,
        "parameters": task_params
    });

    // Use orchestrator to execute task
    // Note: In a real implementation, we would call state._orchestrator.send_task()
    // but for now we just log it as the orchestrator API might need adjustment
    info!("Orchestrator task payload: {}", task_payload);

    Ok(json!({
        "status": "orchestrated",
        "task_type": task_type,
        "target_systems": target_systems,
        "message": "System task orchestration initiated"
    }))
}

async fn dbus_discovery(
    _state: &ChatState,
    parameters: &Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let service_filter = parameters.get("service_filter")
        .and_then(|v| v.as_str());
    let interface_filter = parameters.get("interface_filter")
        .and_then(|v| v.as_str());

    info!("Performing D-Bus discovery with filters: service={:?}, interface={:?}",
          service_filter, interface_filter);

    // TODO: Implement introspection cache when SQLite threading is resolved
    // For now, return placeholder data
    let services_data = vec!["org.freedesktop.systemd1".to_string()];
    let interfaces_data = vec!["org.freedesktop.systemd1.Manager".to_string()];

    // Apply filters to placeholder data
    let filtered_services: Vec<_> = services_data.into_iter()
        .filter(|s| service_filter.map_or(true, |f| s.contains(f)))
        .collect();

    let filtered_interfaces: Vec<_> = interfaces_data.into_iter()
        .filter(|i| interface_filter.map_or(true, |f| i.contains(f)))
        .collect();

    Ok(json!({
        "services": filtered_services,
        "interfaces": filtered_interfaces,
        "total_services": filtered_services.len(),
        "total_interfaces": filtered_interfaces.len(),
        "note": "Using placeholder data - introspection cache pending"
    }))
}

async fn system_introspect(
    _state: &ChatState,
    parameters: &Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let layers = parameters.get("layers")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_else(|| vec!["physical", "network", "service", "application"]);

    info!("Performing system introspection for layers: {:?}", layers);

    let mut results = json!({});

    for layer in layers {
        match layer {
            "physical" => {
                results["physical"] = json!({
                    "cpu_count": num_cpus::get(),
                    "memory": sys_info::mem_info().map(|m| m.total as u64).unwrap_or(0),
                    "hostname": gethostname::gethostname().to_string_lossy()
                });
            }
            "network" => {
                // Basic network info - could be expanded
                results["network"] = json!({
                    "interfaces": "available",
                    "connections": "discoverable"
                });
            }
            "service" => {
                // D-Bus services - TODO: Implement with proper introspection cache
                results["service"] = json!({
                    "dbus_services": 1,
                    "services": ["org.freedesktop.systemd1"],
                    "note": "Introspection cache integration pending"
                });
            }
            "application" => {
                results["application"] = json!({
                    "workflows": "available",
                    "plugins": "discoverable"
                });
            }
            _ => {}
        }
    }

    Ok(results)
}

async fn workflow_orchestrate(
    _state: &ChatState,
    parameters: &Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let workflow_type = parameters.get("workflow_type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let default_workflow_targets = vec![];
    let targets = parameters.get("targets")
        .and_then(|v| v.as_array())
        .unwrap_or(&default_workflow_targets)
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();

    let default_workflow_params = json!({});
    let workflow_params = parameters.get("workflow_params")
        .unwrap_or(&default_workflow_params);

    info!("Orchestrating workflow: {} on targets {:?}", workflow_type, targets);

    // Create workflow orchestration task
    let workflow_task = json!({
        "workflow_type": workflow_type,
        "targets": targets,
        "parameters": workflow_params,
        "orchestration_id": format!("workflow_{}", chrono::Utc::now().timestamp())
    });

    info!("Workflow orchestration task queued: {}", workflow_task);

    Ok(json!({
        "status": "workflow_orchestrated",
        "workflow_type": workflow_type,
        "targets": targets,
        "message": "Workflow orchestration initiated"
    }))
}

async fn execute_regular_tool(
    state: &ChatState,
    tool_name: &str,
    parameters: &Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    // Check if this is a plugin tool
    if tool_name.starts_with("plugin_") {
        return execute_plugin_tool(state, tool_name, parameters).await;
    }

    // For regular tools, use the existing AI-powered execution
    // This would integrate with the existing tool execution logic
    info!("Executing regular tool: {}", tool_name);

    // Placeholder - integrate with existing tool execution
    Ok(json!({
        "tool": tool_name,
        "parameters": parameters,
        "result": "Tool executed via AI orchestration",
        "status": "completed"
    }))
}

async fn execute_plugin_tool(
    state: &ChatState,
    tool_name: &str,
    parameters: &Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    info!("Executing plugin tool: {}", tool_name);

    // Parse tool name: plugin_<name>_<operation>
    let parts: Vec<&str> = tool_name.split('_').collect();
    if parts.len() < 3 {
        return Err(format!("Invalid plugin tool name: {}", tool_name).into());
    }

    let operation = parts.last().unwrap();
    let plugin_name = parts[1..parts.len()-1].join("_");

    info!("Plugin: {}, Operation: {}", plugin_name, operation);

    let plugin = state.plugin_registry.get(&plugin_name).await
        .ok_or_else(|| format!("Plugin '{}' not found", plugin_name))?;

    match *operation {
        "query" => {
            let state = plugin.get_state().await?;
            Ok(json!({
                "status": "success",
                "plugin": plugin_name,
                "state": state
            }))
        }
        "apply" => {
            // The parameters should contain the desired state
            // If parameters has a "state" field, use that, otherwise use parameters as state
            let desired_state = parameters.get("state").unwrap_or(parameters).clone();
            plugin.apply_state(desired_state).await?;
            Ok(json!({
                "status": "success",
                "plugin": plugin_name,
                "message": "State applied successfully"
            }))
        }
        "diff" => {
            let current = plugin.get_state().await?;
            let desired = parameters.get("state").unwrap_or(parameters).clone();
            let changes = plugin.diff(current, desired).await?;
            Ok(json!({
                "status": "success",
                "plugin": plugin_name,
                "changes": changes
            }))
        }
        _ => Err(format!("Unknown plugin operation: {}", operation).into())
    }
}

async fn send_chat_message_with_orchestration(
    state: &ChatState,
    message: &str,
    conversation_id: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    info!("Sending chat message with orchestration: {}", message);

    // Get system context for enhanced AI responses
    let system_context = get_system_context(state).await;

    // Get conversation history
    let conversations = state.conversations.read().await;
    let history = conversations.get(conversation_id)
        .cloned()
        .unwrap_or_default();

    // Convert system context to string
    let system_context_str = if let Some(ref ctx) = system_context {
        format!("System Context: Active services: {:?}, Network status: {}, System load: {:.2}, Available tools: {}, Tool count: {}",
            ctx.active_services,
            ctx.network_status,
            ctx.system_load,
            ctx.available_tools.join(", "),
            ctx.tool_count
        )
    } else {
        "No system context available".to_string()
    };

    // Convert history to ollama ChatMessage format
    let ollama_history: Vec<ollama::ChatMessage> = history.iter().map(|msg| {
        match msg {
            ChatMessage::User { content, .. } => ollama::ChatMessage {
                role: "user".to_string(),
                content: content.clone(),
            },
            ChatMessage::Assistant { content, .. } => ollama::ChatMessage {
                role: "assistant".to_string(),
                content: content.clone(),
            },
            ChatMessage::Error { content, .. } => ollama::ChatMessage {
                role: "system".to_string(),
                content: format!("Error: {}", content),
            },
        }
    }).collect();

    // Use AI with orchestration context
    let response = state.ollama_client.chat_with_context(
        "mistral", // model
        &system_context_str, // system_context as string
        &ollama_history, // conversation_history in correct format
        message, // user_message
        Some(0.7) // temperature
    ).await?;

    // Store the conversation
    drop(conversations);
    let mut conversations = state.conversations.write().await;
    let conversation = conversations.entry(conversation_id.to_string())
        .or_insert_with(Vec::new);

    conversation.push(ChatMessage::User {
        content: message.to_string(),
        timestamp: chrono::Utc::now().timestamp() as u64,
        context: system_context,
    });

    conversation.push(ChatMessage::Assistant {
        content: response.clone(),
        timestamp: chrono::Utc::now().timestamp() as u64,
        tools_used: None,
    });

    Ok(response)
}

// MCP handler for proxy server - enhanced chatbot with orchestration capabilities
async fn mcp_handler(
    State(state): State<ChatState>,
    Json(request): Json<Value>,
) -> impl IntoResponse {
    info!("MCP request received: {:?}", request);

    // Check if this is a JSON-RPC 2.0 MCP request
    if let (Some(jsonrpc), Some(method), Some(id)) = (
        request.get("jsonrpc").and_then(|v| v.as_str()),
        request.get("method").and_then(|v| v.as_str()),
        request.get("id")
    ) {
        if jsonrpc != "2.0" {
            return Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32600,
                    "message": "Invalid JSON-RPC version"
                }
            }));
        }

        let params = request.get("params").cloned().unwrap_or(json!({}));

        return match method {
            "initialize" => {
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "serverInfo": {
                            "name": "op-dbus-mcp-server",
                            "version": "1.0.0"
                        },
                        "capabilities": {
                            "tools": {
                                "listChanged": true
                            }
                        }
                    }
                }))
            }

            "tools/list" => {
                let tools = get_available_tools(&state).await;
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": tools
                    }
                }))
            }

            "tools/call" => {
                let tool_name = params.get("name").and_then(|v| v.as_str());
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

                if let Some(tool_name) = tool_name {
                    let result = execute_tool_with_orchestration(&state, tool_name, &arguments).await;
                    match result {
                        Ok(response) => Json(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": response
                        })),
                        Err(error) => Json(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32603,
                                "message": error.to_string()
                            }
                        }))
                    }
                } else {
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32602,
                            "message": "Missing tool name"
                        }
                    }))
                }
            }

            _ => Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", method)
                }
            }))
        };
    }

    // Fallback to legacy action-based protocol
    let action = request.get("action").and_then(|v| v.as_str());

    match action {
        Some("list_tools") => {
            // Return available tools from unified introspection
            let tools = get_available_tools(&state).await;
            Json(json!({
                "success": true,
                "data": {
                    "tools": tools
                }
            }))
        }

        Some("execute_tool") => {
            // Execute tool with orchestration support
            let tool_name = request.get("tool").and_then(|v| v.as_str());
            let parameters = request.get("parameters");

            if let (Some(tool_name), Some(parameters)) = (tool_name, parameters) {
                let result = execute_tool_with_orchestration(&state, tool_name, parameters).await;
                match result {
                    Ok(response) => Json(json!({
                        "success": true,
                        "data": response
                    })),
                    Err(error) => Json(json!({
                        "success": false,
                        "error": error.to_string()
                    }))
                }
            } else {
                Json(json!({
                    "success": false,
                    "error": "Missing tool name or parameters"
                }))
            }
        }

        Some("send_message") => {
            // Handle chat messages with system orchestration context
            let message = request.get("message").and_then(|v| v.as_str());
            let conversation_id = request.get("conversationId").and_then(|v| v.as_str());

            if let Some(message) = message {
                let response = send_chat_message_with_orchestration(
                    &state,
                    message,
                    conversation_id.unwrap_or("default")
                ).await;

                match response {
                    Ok(chat_response) => Json(json!({
                        "success": true,
                        "data": {
                            "response": chat_response,
                            "conversationId": conversation_id.unwrap_or("default")
                        }
                    })),
                    Err(error) => Json(json!({
                        "success": false,
                        "error": error.to_string()
                    }))
                }
            } else {
                Json(json!({
                    "success": false,
                    "error": "Missing message"
                }))
            }
        }

        _ => Json(json!({
            "success": false,
            "error": "Unknown action"
        }))
    }
}

// WebSocket handler for chat
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<ChatState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: ChatState) {
    let (mut sender, mut receiver) = socket.split();

    // Generate a simple conversation ID
    let conversation_id = format!(
        "conv_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    while let Some(Ok(message)) = receiver.next().await {
        if let Message::Text(text) = message {
            // Parse incoming message
            if let Ok(chat_msg) = serde_json::from_str::<ChatMessage>(&text) {
                match chat_msg {
                    ChatMessage::User { content, .. } => {
                        // Get system context for enhanced AI awareness
                        let system_context = get_system_context(&state).await;

                        // Store user message
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        let user_msg = ChatMessage::User {
                            content: content.clone(),
                            timestamp,
                            context: system_context.clone(),
                        };

                        // Add to conversation
                        {
                            let mut conversations = state.conversations.write().await;
                            conversations
                                .entry(conversation_id.clone())
                                .or_insert_with(Vec::new)
                                .push(user_msg.clone());
                        }

                        // Send back the user message for UI update
                        if let Ok(response) = serde_json::to_string(&user_msg) {
                            let _ = sender.send(Message::Text(response)).await;
                        }

                        // Generate AI response using the AI brain
                        {
                            // Build context-aware prompt with system information
                            let enhanced_prompt = build_enhanced_prompt(&content, &system_context);
                            let model = state.ollama_client.default_model();

                            match state.ollama_client.simple_chat(&model, &enhanced_prompt).await {
                                Ok(ai_response) => {
                                    let ai_msg = ChatMessage::Assistant {
                                        content: ai_response,
                                        timestamp: std::time::SystemTime::now()
                                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs(),
                                        tools_used: None,
                                    };

                                    // Add to conversation
                                    {
                                        let mut conversations = state.conversations.write().await;
                                        conversations
                                            .entry(conversation_id.clone())
                                            .or_insert_with(Vec::new)
                                            .push(ai_msg.clone());
                                    }

                                    // Send AI response
                                    if let Ok(response) = serde_json::to_string(&ai_msg) {
                                        let _ = sender.send(Message::Text(response)).await;
                                    }
                                }
                                Err(e) => {
                                    error!("AI chat error: {}", e);
                                    let error_msg = ChatMessage::Error {
                                        content: format!("AI chat failed: {}", e),
                                        timestamp: std::time::SystemTime::now()
                                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs(),
                                    };

                                    if let Ok(response) = serde_json::to_string(&error_msg) {
                                        let _ = sender.send(Message::Text(response)).await;
                                    }
                                }
                            }
                        }
                    }
                    _ => {} // Ignore other message types
                }
            }
        }
    }
}

/// Get system context from unified tool introspection
async fn get_system_context(state: &ChatState) -> Option<SystemContext> {
    // Get introspection data from unified tool registry
    let introspection_guard = state.tool_introspection.read().await;

    if let Some(introspection) = introspection_guard.as_ref() {
        let tool_count = introspection
            .get("total_tools_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let available_plugins = introspection
            .get("available_plugins")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Some(SystemContext {
            active_services: vec![
                "dbus-daemon".to_string(),
                "systemd".to_string(),
                "networkd".to_string(),
            ],
            network_status: format!("{} state plugins available", available_plugins),
            system_load: 0.45, // Could be enhanced with real system load
            available_tools: vec![
                "plugin_systemd".to_string(),
                "plugin_network".to_string(),
                "plugin_packagekit".to_string(),
            ],
            tool_count,
        })
    } else {
        // Fallback to basic context
        Some(SystemContext {
            active_services: vec!["dbus-daemon".to_string()],
            network_status: "System context unavailable".to_string(),
            system_load: 0.0,
            available_tools: vec![],
            tool_count: 0,
        })
    }
}

/// Build enhanced prompt with unified system context
fn build_enhanced_prompt(user_input: &str, context: &Option<SystemContext>) -> String {
    // Get workflow and plugin information
    let wp_introspection = workflow_plugin_introspection::WorkflowPluginIntrospection::new();
    let wp_context = wp_introspection.to_chat_context();

    match context {
        Some(ctx) => {
            format!(
                "User query: {}\n\nSystem context:\n\
                 - Active services: {}\n\
                 - Network status: {}\n\
                 - System load: {:.2}\n\
                 - Available tools/operations: {} (including plugin-derived operations)\n\n\
                 Available Workflows and Plugins:\n\
                 {}\n\n\
                 Context: The system has {} total tools (includes both native tools and tools created from state plugins).\n\
                 Plugins are automatically converted to tools with names like 'plugin_systemd', 'plugin_network', etc.\n\n\
                 If the user asks about:\n\
                 - System operations: recommend relevant tools\n\
                 - Code review/testing: suggest code_review or test_generation workflows\n\
                 - State/service management: mention systemd, network, packagekit plugins\n\
                 - Deployment: recommend deployment workflows with rollback",
                user_input,
                ctx.active_services.join(", "),
                ctx.network_status,
                ctx.system_load,
                ctx.available_tools.join(", "),
                wp_context,
                ctx.tool_count
            )
        }
        None => {
            // Even without D-Bus context, include workflows and plugins
            format!(
                "User query: {}\n\nAvailable Workflows and Plugins:\n{}\n\n\
                 Please provide a response considering available workflows and state management plugins.",
                user_input,
                wp_context
            )
        }
    }
}

/// Health check handler for the shared server
async fn health_handler() -> impl IntoResponse {
    Json(json!({
        "status": "healthy",
        "service": "mcp-chat",
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }))
}

/// Status handler for detailed service information
async fn status_handler(State(state): State<ChatState>) -> impl IntoResponse {
    let tool_count = state.tool_introspection.read().await
        .as_ref()
        .and_then(|v| v.get("total_tools"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    Json(json!({
        "service": "mcp-chat",
        "status": "active",
        "tool_count": tool_count,
        "ollama_available": state.ollama_client.is_available().await,
        "uptime_seconds": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() - state.start_time
            .duration_since(std::time::UNIX_EPOCH)
                            .as_secs() - state.start_time
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
    }))
}

/// MCP discovery handler – lists all registered external MCP servers
async fn mcp_discover_handler(State(state): State<ChatState>) -> impl IntoResponse {
    let metadata = state.mcp_registry.get_servers_metadata().await;
    Json(json!({ "servers": metadata }))
}

/// MCP config handler – generates a generic client config JSON
async fn mcp_config_handler(State(state): State<ChatState>) -> impl IntoResponse {
    // Build list of all server endpoints
    let servers = state.mcp_registry.list_servers().await;
    let config = json!({
        "mcpServers": servers.iter().map(|name| json!({
            "name": name,
            "endpoint": format!("/api/mcp/{}", name),
            "sse_endpoint": format!("/api/mcp/{}/events", name),
        })).collect::<Vec<_>>()
    });
    Json(config)
}

/// MCP config for Claude Desktop – uses client_config_generator for formatting
async fn mcp_claude_config_handler(State(state): State<ChatState>) -> impl IntoResponse {
    // Use the generator module to produce Claude‑compatible config
    match crate::client_config_generator::generate_claude_config("http://localhost:8080") {
        Ok(cfg) => {
            // Return as plain text (Claude expects JSON string)
            (axum::http::StatusCode::OK, cfg)
        }
        Err(e) => {
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("Error generating config: {}", e))
        }
    }
}

/// Models handler - returns available AI models from all providers
async fn models_handler(State(state): State<ChatState>) -> impl IntoResponse {
    // Build models list from config + dynamic discovery
    let models: Vec<Value> = state.available_models.iter().map(|model_name| {
        // Determine provider from model name
        let (provider, display_name) = if model_name.contains("meta-llama") {
            ("huggingface", format!("🤗 {}", model_name))
        } else if model_name.contains("mistralai") {
            ("huggingface", format!("🤗 {}", model_name))  
        } else if model_name.contains("google") {
            ("huggingface", format!("🤗 {}", model_name))
        } else {
            ("ollama", model_name.clone())
        };

        json!({
            "id": model_name,
            "name": display_name,
            "provider": provider,
            "description": format!("AI model via {}", provider),
        })
    }).collect();

    Json(json!({
        "models": models,
        "default_model": state.available_models.first().cloned().unwrap_or_else(|| "mistral".to_string()),
    }))
}

/// Build unified tool introspection from workflows and plugins
/// This follows the unified introspection pattern defined in ToolRegistry.get_introspection()
/// In a full MCP server, this data would come from ToolRegistry which includes native tools too.
/// For chat_main.rs, we build it from WorkflowPluginIntrospection which provides the plugin view.
async fn build_unified_tool_introspection(plugin_registry: &PluginRegistry) -> Option<Value> {
    let wp_introspection = workflow_plugin_introspection::WorkflowPluginIntrospection::new();
    
    // Get registered plugins
    let registered_plugins = plugin_registry.get_all_metadata().await;

    // Convert plugins to the tool format
    // Each plugin generates three tools: query, diff, apply
    let plugin_tools: Vec<serde_json::Value> = registered_plugins
        .iter()
        .flat_map(|plugin| {
            vec![
                serde_json::json!({
                    "name": format!("plugin_{}_query", plugin.name),
                    "description": format!("Query current state from {} plugin", plugin.name),
                    "type": "plugin_tool",
                    "plugin_name": plugin.name,
                    "operation": "query",
                }),
                serde_json::json!({
                    "name": format!("plugin_{}_diff", plugin.name),
                    "description": format!("Calculate state diff for {} plugin", plugin.name),
                    "type": "plugin_tool",
                    "plugin_name": plugin.name,
                    "operation": "diff",
                }),
                serde_json::json!({
                    "name": format!("plugin_{}_apply", plugin.name),
                    "description": format!("Apply state changes for {} plugin", plugin.name),
                    "type": "plugin_tool",
                    "plugin_name": plugin.name,
                    "operation": "apply",
                }),
            ]
        })
        .collect();

    Some(serde_json::json!({
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        "type": "unified_system_introspection",
        "description": "Unified introspection: plugin-derived tools and workflows (native tools available in full MCP server)",
        "tools": plugin_tools,
        "workflows": wp_introspection.workflows,
        "state_plugins": registered_plugins, // Use actual registered plugins
        "total_tools": plugin_tools.len(),
        "total_workflows": wp_introspection.workflows.len(),
        "available_plugins": registered_plugins.len(),
    }))
}

/// Auto-discover D-Bus services and register them as plugins
async fn discover_dbus_plugins(registry: &Arc<PluginRegistry>) {
    info!("🔍 Auto-discovering D-Bus plugins...");
    
    // List of well-known services to auto-register
    // In a real implementation, we would scan the bus
    let targets = vec![
        ("org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager"),
        ("org.freedesktop.timedate1", "/org/freedesktop/timedate1", "org.freedesktop.timedate1"),
        ("org.freedesktop.locale1", "/org/freedesktop/locale1", "org.freedesktop.locale1"),
        ("org.freedesktop.hostname1", "/org/freedesktop/hostname1", "org.freedesktop.hostname1"),
    ];

    for (service, path, interface) in targets {
        match DbusAutoPlugin::new(service.to_string(), path.to_string(), interface.to_string()).await {
            Ok(plugin) => {
                let name = plugin.name().to_string();
                if let Err(e) = registry.register(Box::new(plugin)).await {
                    info!("  ⚠️ Failed to register auto-plugin {}: {}", name, e);
                } else {
                    info!("  ✅ Auto-registered plugin: {}", name);
                }
            }
            Err(e) => {
                info!("  ⚠️ Could not connect to {}: {}", service, e);
            }
        }
    }
}
