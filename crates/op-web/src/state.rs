//! Application State
//!
//! Central state management for the web server.
//! Simple, direct tool access - no MCP complexity.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

use op_llm::chat::ChatManager;
use op_llm::provider::ChatMessage;
use op_tools::ToolRegistry;
use op_agents::agent_registry::AgentRegistry;

use crate::orchestrator::UnifiedOrchestrator;
use crate::sse::SseEventBroadcaster;
use crate::users::UserStore;
use crate::email::{EmailConfig, EmailSender};
use crate::wireguard::WgServerConfig;

/// Application state shared across all handlers
pub struct AppState {
    /// Unified orchestrator - direct tool access
    pub orchestrator: Arc<UnifiedOrchestrator>,
    /// Tool registry
    pub tool_registry: Arc<ToolRegistry>,
    /// Agent registry
    pub agent_registry: Arc<RwLock<AgentRegistry>>,
    /// Chat manager for LLM access
    pub chat_manager: Arc<ChatManager>,
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
    /// Privacy router user store
    pub user_store: Arc<UserStore>,
    /// Email sender for magic links
    pub email_sender: Arc<EmailSender>,
    /// WireGuard server configuration
    pub server_config: WgServerConfig,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Self> {
        info!("Initializing application state...");

        // Create tool registry
        let tool_registry = Arc::new(ToolRegistry::new());

        // Register ALL tools
        register_all_tools(&tool_registry).await?;

        // Log tool count
        let tools = tool_registry.list().await;
        info!("✅ Registered {} tools", tools.len());
        log_tool_summary(&tools);

        // Create chat manager for LLM access
        let chat_manager = Arc::new(ChatManager::new());

        // Load persisted provider/model
        if let Some(provider) = read_persisted_provider().await {
            if let Ok(provider_type) = provider.parse() {
                if let Err(e) = chat_manager.switch_provider(provider_type).await {
                    warn!("Failed to load provider '{}': {}", provider, e);
                } else {
                    info!("Loaded provider: {}", provider);
                }
            }
        }

        if let Some(model) = read_persisted_model().await {
            if let Err(e) = chat_manager.switch_model(model.clone()).await {
                warn!("Failed to load model '{}': {}", model, e);
            } else {
                info!("Loaded model: {}", model);
            }
        }

        // Get LLM info
        let provider_type = chat_manager.current_provider().await;
        let default_model = chat_manager.current_model().await;
        let provider_name = format!("{:?}", provider_type);

        info!("✅ LLM: {} ({})", provider_name, default_model);

        // Create agent registry
        let agent_registry = Arc::new(RwLock::new(AgentRegistry::new()));

        // Create orchestrator with direct tool access
        let orchestrator = Arc::new(UnifiedOrchestrator::new(
            tool_registry.clone(),
            chat_manager.clone(),
        ));

        // Create broadcast channel for WebSocket
        let (broadcast_tx, _) = broadcast::channel(100);

        // Create SSE broadcaster
        let sse_broadcaster = Arc::new(SseEventBroadcaster::new());

        // Initialize privacy router components
        let user_store = Arc::new(
            UserStore::new("/var/lib/op-dbus/privacy-users.json")
                .await
                .unwrap_or_else(|e| {
                    warn!("Failed to load user store: {}, creating new", e);
                    // Create empty store
                    tokio::runtime::Handle::current()
                        .block_on(UserStore::new("/var/lib/op-dbus/privacy-users.json"))
                        .expect("Failed to create user store")
                }),
        );

        let email_config = EmailConfig::from_env().unwrap_or_else(|e| {
            warn!("Failed to load email config: {}", e);
            EmailConfig {
                smtp_host: "localhost".to_string(),
                smtp_port: 587,
                smtp_user: String::new(),
                smtp_pass: String::new(),
                from_email: "noreply@example.com".to_string(),
                from_name: "Privacy Router".to_string(),
                base_url: "http://localhost:8080".to_string(),
            }
        });
        let email_sender = Arc::new(EmailSender::new(email_config));

        // Load WireGuard server config (will need to be configured properly)
        let server_config = WgServerConfig::default();

        info!("✅ Application state initialized");

        Ok(Self {
            orchestrator,
            tool_registry,
            agent_registry,
            chat_manager,
            default_model,
            provider_name,
            broadcast_tx,
            sse_broadcaster,
            start_time: std::time::Instant::now(),
            conversations: Arc::new(RwLock::new(HashMap::new())),
            user_store,
            email_sender,
            server_config,
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
    tokio::fs::read_to_string(PERSISTED_MODEL_PATH)
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

async fn read_persisted_provider() -> Option<String> {
    tokio::fs::read_to_string(PERSISTED_PROVIDER_PATH)
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Register all tools from all sources
async fn register_all_tools(registry: &Arc<ToolRegistry>) -> anyhow::Result<()> {
    info!("Registering tools...");
    
    // Register builtin tools (OVS, D-Bus, file, shell, etc.)
    op_tools::register_builtin_tools(registry).await?;
    
    // Register agent tools
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

        // Skip if already registered
        if registry.get_definition(tool.name()).await.is_some() {
            continue;
        }

        registry.register_tool(tool).await?;
        count += 1;
    }

    info!("Registered {} agent tools", count);
    Ok(())
}

/// Log tool summary
fn log_tool_summary(tools: &[op_tools::registry::ToolDefinition]) {
    let ovs = tools.iter().filter(|t| t.name.starts_with("ovs_")).count();
    let dbus = tools.iter().filter(|t| t.name.starts_with("dbus_")).count();
    let file = tools.iter().filter(|t| t.name.starts_with("file_")).count();
    let shell = tools.iter().filter(|t| t.name.starts_with("shell_")).count();
    let agent = tools.iter().filter(|t| t.name.starts_with("agent_")).count();
    let other = tools.len() - ovs - dbus - file - shell - agent;

    debug!("  OVS: {}, D-Bus: {}, File: {}, Shell: {}, Agents: {}, Other: {}",
        ovs, dbus, file, shell, agent, other);
}
