//! Unified Orchestrator
//!
//! Central coordination layer that integrates all subsystems.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use op_chat::NLAdminOrchestrator;
use op_chat::SessionManager;
use op_llm::chat::ChatManager;
use op_llm::provider::ChatMessage;
use op_tools::registry::ToolRegistry;

/// Response from the orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<Value>,
    pub tools_executed: Vec<String>,
    pub intent: String,
}

impl OrchestratorResponse {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: None,
            tools_executed: vec![],
            intent: "success".to_string(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
            tools_executed: vec![],
            intent: "error".to_string(),
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools_executed = tools;
        self
    }

    pub fn with_intent(mut self, intent: impl Into<String>) -> Self {
        self.intent = intent.into();
        self
    }
}

/// Conversation context
#[derive(Debug, Clone)]
pub struct ConversationContext {
    #[allow(dead_code)]
    pub id: String,
    pub messages: Vec<ChatMessage>,
    #[allow(dead_code)]
    pub variables: HashMap<String, String>,
}

impl ConversationContext {
    pub fn new(id: String) -> Self {
        Self {
            id,
            messages: Vec::new(),
            variables: HashMap::new(),
        }
    }

    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
        // Keep only last 50 messages
        if self.messages.len() > 50 {
            self.messages.remove(0);
        }
    }
}

/// Unified orchestrator
pub struct UnifiedOrchestrator {
    tool_registry: Arc<ToolRegistry>,
    nl_admin: Arc<NLAdminOrchestrator>,
    #[allow(dead_code)]
    session_manager: Arc<SessionManager>,
    chat_manager: Arc<ChatManager>,
    conversations: RwLock<HashMap<String, ConversationContext>>,
    default_model: String,
}

impl UnifiedOrchestrator {
    pub fn new(
        tool_registry: Arc<ToolRegistry>,
        nl_admin: Arc<NLAdminOrchestrator>,
        session_manager: Arc<SessionManager>,
        chat_manager: Arc<ChatManager>,
        default_model: String,
    ) -> Self {
        Self {
            tool_registry,
            nl_admin,
            session_manager,
            chat_manager,
            conversations: RwLock::new(HashMap::new()),
            default_model,
        }
    }

    #[allow(dead_code)]
    pub fn session_manager(&self) -> Arc<SessionManager> {
        Arc::clone(&self.session_manager)
    }

    /// Process an incoming request
    pub async fn process(
        &self,
        conversation_id: &str,
        input: &str,
    ) -> Result<OrchestratorResponse> {
        info!("Processing: {} (session: {})", input, &conversation_id[..8]);

        // Ensure conversation exists
        self.ensure_conversation(conversation_id).await;

        // Check for direct commands
        let lower = input.to_lowercase().trim().to_string();

        // Help command
        if lower == "help" || lower == "?" {
            return Ok(self.handle_help().await);
        }

        // Status command
        if lower == "status" {
            return Ok(self.handle_status().await);
        }

        // List tools command
        if lower == "tools" || lower == "list tools" {
            return Ok(self.handle_list_tools().await);
        }

        // Direct tool execution: "run <tool> [args]"
        if lower.starts_with("run ") || lower.starts_with("execute ") {
            return self.handle_direct_tool(input).await;
        }

        // Default: Natural language processing via NL Admin
        self.handle_natural_language(conversation_id, input).await
    }

    async fn handle_help(&self) -> OrchestratorResponse {
        OrchestratorResponse::success(HELP_TEXT).with_intent("help")
    }

    async fn handle_status(&self) -> OrchestratorResponse {
        let tools = self.tool_registry.list().await;
        let tool_count = tools.len();

        let ovs_count = tools.iter().filter(|t| t.name.starts_with("ovs_")).count();
        let systemd_count = tools.iter().filter(|t| t.name.starts_with("systemd_")).count();
        let plugin_count = tools.iter().filter(|t| t.name.starts_with("plugin_")).count();

        OrchestratorResponse::success(format!(
            "📊 System Status\n\
            ═══════════════\n\
            🔧 Tools: {} total\n\
            • OVS: {}\n\
            • Systemd: {}\n\
            • Plugins: {}\n\
            🤖 Model: {}\n\
            💬 Sessions: {}",
            tool_count, ovs_count, systemd_count, plugin_count,
            self.default_model,
            self.conversations.read().await.len()
        ))
        .with_data(json!({
            "tools": tool_count,
            "model": self.default_model
        }))
        .with_intent("status")
    }

    async fn handle_list_tools(&self) -> OrchestratorResponse {
        let tools = self.tool_registry.list().await;
        let mut content = String::from("🔧 Available Tools\n═════════════════\n");

        // Group by category
        let categories = ["ovs_", "systemd_", "nm_", "file_", "system_", "plugin_"];
        let category_names = ["OVS", "Systemd", "NetworkManager", "File", "System", "Plugin"];

        for (prefix, name) in categories.iter().zip(category_names.iter()) {
            let cat_tools: Vec<_> = tools.iter().filter(|t| t.name.starts_with(prefix)).collect();
            if !cat_tools.is_empty() {
                content.push_str(&format!("\n### {} ({})\n", name, cat_tools.len()));
                for tool in cat_tools.iter().take(5) {
                    content.push_str(&format!("• {}\n", tool.name));
                }
                if cat_tools.len() > 5 {
                    content.push_str(&format!("  ... and {} more\n", cat_tools.len() - 5));
                }
            }
        }

        OrchestratorResponse::success(content)
            .with_data(json!({"tool_count": tools.len()}))
            .with_intent("list_tools")
    }

    async fn handle_direct_tool(&self, input: &str) -> Result<OrchestratorResponse> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() < 2 {
            return Ok(OrchestratorResponse::error("Usage: run <tool_name> [args]"));
        }

        let tool_name = parts[1];
        let args = if parts.len() > 2 {
            let args_str = parts[2..].join(" ");
            serde_json::from_str(&args_str).unwrap_or(json!({}))
        } else {
            json!({})
        };

        match self.tool_registry.get(tool_name).await {
            Some(tool) => {
                match tool.execute(args).await {
                    Ok(result) => Ok(OrchestratorResponse::success(format!(
                        "✅ Tool '{}' executed successfully:\n{}",
                        tool_name,
                        serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
                    ))
                    .with_tools(vec![tool_name.to_string()])
                    .with_data(result)
                    .with_intent("tool_execution")),
                    Err(e) => Ok(OrchestratorResponse::error(format!(
                        "❌ Tool '{}' failed: {}",
                        tool_name, e
                    ))
                    .with_tools(vec![tool_name.to_string()])
                    .with_intent("tool_execution")),
                }
            }
            None => Ok(OrchestratorResponse::error(format!(
                "Tool '{}' not found. Use 'tools' to list available tools.",
                tool_name
            ))),
        }
    }

    async fn handle_natural_language(
        &self,
        conversation_id: &str,
        message: &str,
    ) -> Result<OrchestratorResponse> {
        // Get conversation history
        let history = {
            let conversations = self.conversations.read().await;
            conversations
                .get(conversation_id)
                .map(|c| c.messages.clone())
                .unwrap_or_default()
        };

        // Process through NL Admin
        match self
            .nl_admin
            .process(
                self.chat_manager.as_ref(),
                &self.default_model,
                message,
                history,
            )
            .await
        {
            Ok(result) => {
                // Update conversation
                {
                    let mut conversations = self.conversations.write().await;
                    if let Some(ctx) = conversations.get_mut(conversation_id) {
                        ctx.add_message(ChatMessage::user(message));
                        ctx.add_message(ChatMessage::assistant(&result.message));
                    }
                }

                Ok(OrchestratorResponse::success(result.message)
                    .with_tools(result.tools_executed)
                    .with_data(json!({ "tool_results": result.tool_results }))
                    .with_intent("natural_language"))
            }
            Err(e) => Ok(OrchestratorResponse::error(format!("Processing failed: {}", e))
                .with_intent("natural_language")),
        }
    }

    async fn ensure_conversation(&self, id: &str) {
        let mut conversations = self.conversations.write().await;
        if !conversations.contains_key(id) {
            conversations.insert(id.to_string(), ConversationContext::new(id.to_string()));
        }
    }
}

const HELP_TEXT: &str = r#"📚 op-dbus Help
══════════════

Commands:
• help          - Show this help
• status        - System status
• tools         - List available tools
• run <tool>    - Execute a tool directly

Natural Language:
Just type what you want to do:
• "Create an OVS bridge called ovsbr0"
• "Restart nginx"
• "List network interfaces"
• "What services are running?"

The AI uses native protocols:
• D-Bus for systemd, NetworkManager
• OVSDB JSON-RPC for Open vSwitch
• Netlink for kernel networking

It will NEVER suggest CLI commands."#;
