//! Application State
//!
//! Central state management for the web server.
//! Integrates ALL op-* crates into a unified system.

use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn, debug};

use op_chat::{NLAdminOrchestrator, SessionManager};
use op_llm::chat::ChatManager;
use op_llm::provider::ChatMessage;
use op_tools::ToolRegistry;
use op_agents::agent_registry::AgentRegistry;

use crate::orchestrator::UnifiedOrchestrator;
use crate::sse::SseEventBroadcaster;

/// Application state shared across all handlers
pub struct AppState {
    /// Unified orchestrator for all requests
    pub orchestrator: Arc<UnifiedOrchestrator>,
    /// Tool registry (also accessible via orchestrator)
    pub tool_registry: Arc<ToolRegistry>,
    /// Agent registry
    pub agent_registry: Arc<RwLock<AgentRegistry>>,
    /// NL Admin orchestrator
    #[allow(dead_code)]
    pub nl_admin: Arc<NLAdminOrchestrator>,
    /// Session manager
    #[allow(dead_code)]
    pub session_manager: Arc<SessionManager>,
    /// Chat manager for LLM access
    pub chat_manager: Arc<ChatManager>,
    /// System prompt (loaded from file)
    #[allow(dead_code)]
    pub system_prompt: String,
    /// Default model
    pub default_model: String,
    /// Provider name
    pub provider_name: String,
    /// Broadcast channel for WebSocket messages
    pub broadcast_tx: broadcast::Sender<String>,
    /// SSE event broadcaster
    pub sse_broadcaster: Arc<SseEventBroadcaster>,
    /// Server start time
    pub start_time: std::time::Instant,
    /// Conversation history (for WebSocket sessions)
    pub conversations: Arc<RwLock<HashMap<String, Vec<ChatMessage>>>>,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Self> {
        info!("Initializing application state...");

        // Load system prompt
        let system_prompt = load_system_prompt();
        info!("System prompt loaded ({} chars)", system_prompt.len());

        // Create tool registry
        let tool_registry = Arc::new(ToolRegistry::new());

        // Register ALL tools from all sources
        register_all_tools(&tool_registry).await?;

        // Log registered tools
        let tools = tool_registry.list().await;
        info!("Registered {} tools total", tools.len());
        log_tool_categories(&tools);

        // Create agent registry
        let agent_registry = Arc::new(RwLock::new(AgentRegistry::new()));
        info!("Agent registry initialized");

        // Create session manager
        let session_manager = Arc::new(SessionManager::new());

        // Create NL Admin orchestrator
        let nl_admin = Arc::new(NLAdminOrchestrator::new(tool_registry.clone()));

        // Create chat manager for LLM access
        let chat_manager = Arc::new(ChatManager::new());

        if let Some(provider) = read_persisted_provider().await {
            match provider.parse() {
                Ok(provider_type) => {
                    if let Err(e) = chat_manager.switch_provider(provider_type).await {
                        warn!("Failed to load persisted provider '{}': {}", provider, e);
                    } else {
                        info!("Loaded persisted provider: {}", provider);
                    }
                }
                Err(e) => {
                    warn!("Invalid persisted provider '{}': {}", provider, e);
                }
            }
        }

        if let Some(model) = read_persisted_model().await {
            if let Err(e) = chat_manager.switch_model(model.clone()).await {
                warn!("Failed to load persisted model '{}': {}", model, e);
            } else {
                info!("Loaded persisted model: {}", model);
            }
        }

        // Get LLM provider info
        let provider_type = chat_manager.current_provider().await;
        let default_model = chat_manager.current_model().await;
        let provider_name = format!("{:?}", provider_type);

        info!("LLM Provider: {} ({})", provider_name, default_model);

        // Create unified orchestrator
        let orchestrator = Arc::new(UnifiedOrchestrator::new(
            tool_registry.clone(),
            nl_admin.clone(),
            session_manager.clone(),
            chat_manager.clone(),
            default_model.clone(),
        ));

        // Create broadcast channel for WebSocket
        let (broadcast_tx, _) = broadcast::channel(100);

        // Create SSE broadcaster
        let sse_broadcaster = Arc::new(SseEventBroadcaster::new());

        info!("Application state initialized successfully");

        Ok(Self {
            orchestrator,
            tool_registry,
            agent_registry,
            nl_admin,
            session_manager,
            chat_manager,
            system_prompt,
            default_model,
            provider_name,
            broadcast_tx,
            sse_broadcaster,
            start_time: std::time::Instant::now(),
            conversations: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}

const PERSISTED_MODEL_PATH: &str = "/etc/op-dbus/llm-model";
const PERSISTED_PROVIDER_PATH: &str = "/etc/op-dbus/llm-provider";

async fn read_persisted_model() -> Option<String> {
    match tokio::fs::read_to_string(PERSISTED_MODEL_PATH).await {
        Ok(contents) => {
            let model = contents.trim().to_string();
            if model.is_empty() {
                None
            } else {
                Some(model)
            }
        }
        Err(_) => None,
    }
}

async fn read_persisted_provider() -> Option<String> {
    match tokio::fs::read_to_string(PERSISTED_PROVIDER_PATH).await {
        Ok(contents) => {
            let provider = contents.trim().to_string();
            if provider.is_empty() {
                None
            } else {
                Some(provider)
            }
        }
        Err(_) => None,
    }
}

/// Load system prompt from file
fn load_system_prompt() -> String {
    const PATHS: &[&str] = &[
        "LLM-SYSTEM-PROMPT-COMPLETE.txt",
        "../LLM-SYSTEM-PROMPT-COMPLETE.txt",
        "SYSTEM-PROMPT.md",
        "../SYSTEM-PROMPT.md",
        "system-prompt.txt",
    ];

    for path in PATHS {
        if let Ok(content) = std::fs::read_to_string(path) {
            info!("Loaded system prompt from: {}", path);
            return content;
        }
    }

    warn!("No system prompt file found, using fallback");
    FALLBACK_SYSTEM_PROMPT.to_string()
}

const FALLBACK_SYSTEM_PROMPT: &str = r#"
You are an AI server administrator for op-dbus-v2.

CRITICAL RULES:
1. ALWAYS USE TOOLS for system operations - never suggest CLI commands
2. Use native protocols: D-Bus for systemd/NetworkManager, OVSDB for OVS, Netlink for kernel
3. Format tool calls as: <tool_call>tool_name({"arg": "value"})</tool_call>

Available tool categories:
- OVS: ovs_list_bridges, ovs_create_bridge, ovs_add_port, ovs_delete_bridge
- Systemd: systemd_start_unit, systemd_stop_unit, systemd_restart_unit, systemd_get_unit_status
- Network: network_list_interfaces, network_get_interface
- File: file_read, file_write, file_list
- System: system_info, system_processes

When asked to perform an action, call the appropriate tool.
"#;

/// Register all tools from all sources
async fn register_all_tools(registry: &Arc<ToolRegistry>) -> anyhow::Result<()> {
    info!("Registering tools from all sources...");
    op_tools::register_builtin_tools(registry).await?;
    register_agent_tools(registry).await?;
    Ok(())
}

async fn register_agent_tools(registry: &Arc<ToolRegistry>) -> anyhow::Result<()> {
    let mut count = 0usize;
    let descriptors = op_agents::builtin_agent_descriptors();

    for descriptor in descriptors {
        let tool = op_tools::builtin::create_agent_tool(
            &descriptor.agent_type,
            &format!("{} - {}", descriptor.name, descriptor.description),
            &descriptor.operations,
            serde_json::json!({ "agent_type": descriptor.agent_type }),
        )?;

        if registry.get_definition(tool.name()).await.is_some() {
            continue;
        }

        registry.register_tool(tool).await?;
        count += 1;
    }

    info!("Registered {} agent tools", count);
    Ok(())
}

/// Log tool categories for debugging
fn log_tool_categories(tools: &[op_tools::registry::ToolDefinition]) {
    let ovs = tools.iter().filter(|t| t.name.starts_with("ovs_")).count();
    let systemd = tools.iter().filter(|t| t.name.starts_with("systemd_")).count();
    let nm = tools.iter().filter(|t| t.name.starts_with("nm_")).count();
    let file = tools.iter().filter(|t| t.name.starts_with("file_")).count();
    let system = tools.iter().filter(|t| t.name.starts_with("system_")).count();
    let plugin = tools.iter().filter(|t| t.name.starts_with("plugin_")).count();
    let other = tools.len() - ovs - systemd - nm - file - system - plugin;

    debug!("  - OVS tools: {}", ovs);
    debug!("  - Systemd tools: {}", systemd);
    debug!("  - NetworkManager tools: {}", nm);
    debug!("  - File tools: {}", file);
    debug!("  - System tools: {}", system);
    debug!("  - Plugin tools: {}", plugin);
    if other > 0 {
        debug!("  - Other tools: {}", other);
    }
}
