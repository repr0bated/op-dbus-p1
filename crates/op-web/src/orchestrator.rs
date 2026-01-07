//! Simple Orchestrator - Direct Tool Access
//!
//! Clean, simple orchestration that gives the LLM direct access to ALL tools.
//! No MCP, no profiles, no aggregation - just direct tool execution.
//!
//! Includes anti-hallucination features:
//! - Detects forbidden CLI commands in LLM output
//! - Extracts tool calls from multiple formats (native, XML tags, function calls)
//! - Warns user when LLM suggests CLI instead of using tools

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use op_llm::chat::ChatManager;
use op_llm::provider::{ChatMessage, ChatRequest, LlmProvider, ToolChoice, ToolDefinition};
use op_tools::ToolRegistry;

/// Forbidden CLI commands that the LLM should NOT suggest
/// The chatbot runs as root and has direct tool access - no CLI needed
const FORBIDDEN_COMMANDS: &[&str] = &[
    // OVS CLI - use ovs_* tools instead
    "ovs-vsctl", "ovs-ofctl", "ovs-dpctl", "ovsdb-client",
    // Systemd CLI - use dbus_systemd_* tools instead
    "systemctl", "service ", "journalctl",
    // Network CLI - use rtnetlink_* tools instead
    "ip addr", "ip link", "ip route", "ifconfig", "nmcli",
    // Package managers - not supported yet
    "apt ", "apt-get", "yum ", "dnf ", "pacman",
    // Container CLI - use lxc_* tools instead
    "docker ", "kubectl", "lxc ",
];

/// Events emitted during orchestration for real-time streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum OrchestratorEvent {
    Thinking,
    ToolExecution { name: String, args: Value },
    ToolResult { name: String, success: bool, result: Option<Value>, error: Option<String> },
    Finished { success: bool, message: String, tools_executed: Vec<String> },
    Error { message: String },
}

/// Response from tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub name: String,
    pub success: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
}

/// Orchestrator response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorResponse {
    pub success: bool,
    pub message: String,
    pub tools_executed: Vec<String>,
    pub tool_results: Vec<ToolResult>,
}

impl OrchestratorResponse {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            tools_executed: vec![],
            tool_results: vec![],
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            tools_executed: vec![],
            tool_results: vec![],
        }
    }
}

/// Simple orchestrator with direct tool access
pub struct UnifiedOrchestrator {
    tool_registry: Arc<ToolRegistry>,
    chat_manager: Arc<ChatManager>,
}

impl UnifiedOrchestrator {
    pub fn new(
        tool_registry: Arc<ToolRegistry>,
        chat_manager: Arc<ChatManager>,
    ) -> Self {
        Self {
            tool_registry,
            chat_manager,
        }
    }

    /// Process user input - main entry point
    pub async fn process(
        &self,
        _session_id: &str,
        input: &str,
        event_tx: Option<mpsc::Sender<OrchestratorEvent>>,
    ) -> Result<OrchestratorResponse> {
        let input_trimmed = input.trim();
        let input_preview = if input_trimmed.len() > 80 {
            format!("{}...", &input_trimmed[..80])
        } else {
            input_trimmed.to_string()
        };
        info!("📩 User request: \"{}\"", input_preview);

        // Handle special commands
        match input_trimmed.to_lowercase().as_str() {
            "help" | "?" => return Ok(self.help_response()),
            "tools" | "list tools" => return Ok(self.list_tools_response().await),
            "status" => return Ok(self.status_response().await),
            _ => {}
        }

        // Direct tool execution: "run tool_name {args}"
        if input_trimmed.starts_with("run ") {
            return self.execute_direct_tool(&input_trimmed[4..]).await;
        }

        // Natural language → LLM with tools
        self.process_with_llm(input_trimmed, event_tx).await
    }

    /// Process through LLM with tool calling (multi-turn)
    async fn process_with_llm(
        &self, 
        input: &str, 
        event_tx: Option<mpsc::Sender<OrchestratorEvent>>
    ) -> Result<OrchestratorResponse> {
        const MAX_TURNS: usize = 50; // Allow complex multi-step tasks to complete
        
        // Use compact mode - only expose 4 meta-tools
        let tool_defs = self.build_compact_mode_tools();

        // Fetch all tools to populate the context
        let available_tools = self.tool_registry.list().await;

        info!("🤖 Chatbot starting conversation ({} tools available)", available_tools.len());
        let tool_list_context = available_tools.iter()
            .map(|t| format!("- {}: {}", t.name, t.description))
            .collect::<Vec<_>>()
            .join("\n");

        // Build system prompt: Capabilities + Compact Instructions + Tool Directory
        let system_msg_core = op_chat::system_prompt::generate_system_prompt().await;
        let compact_instructions = self.build_compact_mode_system_prompt();
        
        let combined_prompt = format!("{}\n\n== INTERFACE MODE: COMPACT ==\n{}\n\n## GLOBAL TOOL DIRECTORY\nThe following tools are available via execute_tool():\n\n{}", 
            system_msg_core.content, 
            compact_instructions,
            tool_list_context
        );

        // Convert role (default to system)
        let role_str = match system_msg_core.role {
            op_core::types::ChatRole::User => "user",
            op_core::types::ChatRole::Assistant => "assistant",
            op_core::types::ChatRole::System => "system",
            op_core::types::ChatRole::Tool => "tool",
        }.to_string();

        let system_msg = ChatMessage {
            role: role_str,
            content: combined_prompt,
            tool_calls: None,
            tool_call_id: None,
        };

        // Get model
        let model = self.chat_manager.current_model().await;

        // Initialize conversation
        let mut messages = vec![
            system_msg,
            ChatMessage::user(input),
        ];

        // Collect all results across turns
        let mut all_results = Vec::new();
        let mut all_tools = Vec::new();
        let mut all_forbidden = Vec::new();
        let mut final_response_text = String::new();
        let mut finished_with_response_tool = false;

        // Multi-turn loop
        for turn in 0..MAX_TURNS {
            // Check if we're on the last turn - force completion
            let is_last_turn = turn == MAX_TURNS - 1;
            if is_last_turn {
                info!("⚠️  Step {}: Final step - chatbot will respond after this", turn + 1);
            }

            info!("🧠 Step {}: Chatbot is thinking...", turn + 1);
            
            // Emit Thinking event
            if let Some(tx) = &event_tx {
                let _ = tx.send(OrchestratorEvent::Thinking).await;
            }

            // Build request
            let request = ChatRequest {
                messages: messages.clone(),
                tools: tool_defs.clone(),
                tool_choice: ToolChoice::Auto,
                max_tokens: Some(4096),
                temperature: Some(0.3),
                top_p: None,
            };

            // Call LLM with timeout (60 seconds per turn) and heartbeat
            let llm_future = self.chat_manager.chat_with_request(&model, request);

            // Spawn heartbeat task to send Thinking events every 10s during LLM call
            let heartbeat_tx = event_tx.clone();
            let heartbeat_handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
                interval.tick().await; // Skip immediate first tick
                loop {
                    interval.tick().await;
                    if let Some(ref tx) = heartbeat_tx {
                        if tx.send(OrchestratorEvent::Thinking).await.is_err() {
                            break; // Channel closed
                        }
                    }
                }
            });

            let response = match tokio::time::timeout(
                std::time::Duration::from_secs(60),
                llm_future
            ).await {
                Ok(Ok(resp)) => {
                    heartbeat_handle.abort();
                    resp
                }
                Ok(Err(e)) => {
                    heartbeat_handle.abort();
                    error!("❌ Step {}: Chatbot encountered an error: {}", turn + 1, e);
                    return Err(anyhow::anyhow!("Chatbot error at step {}: {}", turn + 1, e));
                }
                Err(_) => {
                    heartbeat_handle.abort();
                    error!("⏱️  Step {}: Chatbot timed out after 60 seconds", turn + 1);
                    return Err(anyhow::anyhow!("Chatbot timed out at step {} (60s limit)", turn + 1));
                }
            };

            debug!("Step {} raw response: {:?}", turn + 1, response.message.content);

            // Check for forbidden CLI commands
            let forbidden = self.detect_forbidden_commands(&response.message.content);
            if !forbidden.is_empty() {
                warn!("LLM suggested forbidden CLI commands: {:?}", forbidden);
                all_forbidden.extend(forbidden);
            }

            // Collect tool calls (native + text extraction)
            let mut turn_tools: Vec<(String, Value)> = Vec::new();

            // Native tool calls
            if let Some(ref tool_calls) = response.tool_calls {
                for tc in tool_calls {
                    turn_tools.push((tc.name.clone(), tc.arguments.clone()));
                }
            }

            // Text-based tool calls (fallback) - in compact mode, only check for the 4 meta-tools
            let compact_tool_names = vec![
                "list_tools".to_string(),
                "search_tools".to_string(),
                "get_tool_schema".to_string(),
                "execute_tool".to_string(),
            ];
            let text_tools = self.extract_tool_calls_from_text(&response.message.content, &compact_tool_names);
            for (name, args) in text_tools {
                if !turn_tools.iter().any(|(n, _)| n == &name) {
                    turn_tools.push((name, args));
                }
            }

            // If no tool calls, we're done - this is the final response
            if turn_tools.is_empty() {
                final_response_text = response.message.content.clone();
                info!("💬 Step {}: Chatbot is ready to respond", turn + 1);
                break;
            }

            // Execute all tool calls for this turn
            let tool_names: Vec<&str> = turn_tools.iter().map(|(n, _)| n.as_str()).collect();
            info!("🔧 Step {}: Chatbot is calling {} tool(s): {}", turn + 1, turn_tools.len(), tool_names.join(", "));
            
            // Add assistant message with tool calls
            let tool_call_summary: Vec<String> = turn_tools.iter()
                .map(|(name, args)| format!("{}({})", name, args))
                .collect();
            messages.push(ChatMessage::assistant(&format!(
                "Executing tools: {}", tool_call_summary.join(", ")
            )));

            // Execute tools and collect results
            let mut tool_results_text = String::new();
            let mut should_finish = false;
            let mut response_message: Option<String> = None;

            for (name, args) in turn_tools {
                // Format a human-readable description of what the tool does
                let tool_desc = self.describe_tool_call(&name, &args);
                info!("   → {}", tool_desc);
                all_tools.push(name.clone());
                
                // Emit ToolExecution event
                if let Some(tx) = &event_tx {
                    let _ = tx.send(OrchestratorEvent::ToolExecution { 
                        name: name.clone(), 
                        args: args.clone() 
                    }).await;
                }

                // Check if this is a response tool - these signal completion
                if name == "respond_to_user" || name == "cannot_perform" || name == "request_clarification" {
                    should_finish = true;
                    // Extract the message from args
                    if let Some(msg) = args.get("message").and_then(|v| v.as_str()) {
                        response_message = Some(msg.to_string());
                    }
                }

                let result = self.execute_tool(&name, args).await;
                
                // Emit ToolResult event
                if let Some(tx) = &event_tx {
                    let _ = tx.send(OrchestratorEvent::ToolResult { 
                        name: name.clone(),
                        success: result.success,
                        result: result.result.clone(),
                        error: result.error.clone(),
                    }).await;
                }
                
                // Build result message for LLM
                if result.success {
                    let result_preview = result.result.as_ref()
                        .map(|v| {
                            let s = v.to_string();
                            if s.len() > 500 { format!("{}...", &s[..500]) } else { s }
                        })
                        .unwrap_or_default();
                    tool_results_text.push_str(&format!(
                        "✅ {} succeeded: {}\n", name, result_preview
                    ));
                } else {
                    tool_results_text.push_str(&format!(
                        "❌ {} failed: {}\n", name, 
                        result.error.as_ref().unwrap_or(&"Unknown error".to_string())
                    ));
                }

                all_results.push(result);
            }

            // If a response tool was called, we're done
            if should_finish {
                if let Some(msg) = response_message {
                    final_response_text = msg;
                }
                finished_with_response_tool = true;
                info!("💬 Chatbot finished with response tool");
                break;
            }

            // Add tool results as user message (simulating tool response)
            messages.push(ChatMessage::user(&format!(
                "Tool execution results:\n{}\n\nContinue with the task or provide final response.",
                tool_results_text
            )));

            // Save last response text
            final_response_text = response.message.content.clone();
        }

        // If we exhausted all turns, add a note
        if all_tools.len() > 0 && final_response_text.is_empty() {
            info!("Max turns reached with {} tools executed", all_tools.len());
            final_response_text = format!(
                "Task processing completed after {} tool executions.",
                all_tools.len()
            );
        }

        // Build final message
        let final_message = if finished_with_response_tool {
            // Response tool provides the final message directly
            final_response_text
        } else if all_results.is_empty() {
            if !all_forbidden.is_empty() {
                format!(
                    "⚠️ **Warning:** The AI suggested CLI commands instead of using tools.\n\
                    Detected commands: {}\n\n\
                    Please rephrase your request or use a specific tool.\n\n---\n\n{}",
                    all_forbidden.join(", "),
                    self.clean_llm_text(&final_response_text)
                )
            } else {
                final_response_text
            }
        } else {
            self.format_results(&final_response_text, &all_results, &all_forbidden)
        };

        // Success if: response tool was called, OR all tools succeeded, OR no tools were called
        let success = finished_with_response_tool
            || all_results.iter().all(|r| r.success)
            || all_results.is_empty();

        Ok(OrchestratorResponse {
            success,
            message: final_message,
            tools_executed: all_tools,
            tool_results: all_results,
        })
    }

    /// Detect forbidden CLI commands in LLM output
    fn detect_forbidden_commands(&self, content: &str) -> Vec<String> {
        let lower = content.to_lowercase();
        FORBIDDEN_COMMANDS
            .iter()
            .filter(|cmd| lower.contains(*cmd))
            .map(|s| s.to_string())
            .collect()
    }

    /// Execute a single tool
    async fn execute_tool(&self, name: &str, args: Value) -> ToolResult {
        // Handle compact mode meta-tools
        match name {
            "list_tools" => return self.handle_list_tools(args).await,
            "search_tools" => return self.handle_search_tools(args).await,
            "get_tool_schema" => return self.handle_get_tool_schema(args).await,
            "execute_tool" => {
                // Extract the actual tool name and arguments
                let tool_name = args.get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let tool_args = args.get("arguments")
                    .cloned()
                    .unwrap_or(json!({}));
                // Recursively execute the actual tool (boxed to avoid infinite future)
                return Box::pin(self.execute_tool(tool_name, tool_args)).await;
            }
            _ => {}
        }

        // Execute actual tool from registry
        match self.tool_registry.get(name).await {
            Some(tool) => {
                match tool.execute(args).await {
                    Ok(result) => ToolResult {
                        name: name.to_string(),
                        success: true,
                        result: Some(result),
                        error: None,
                    },
                    Err(e) => {
                        error!("Tool {} failed: {}", name, e);
                        ToolResult {
                            name: name.to_string(),
                            success: false,
                            result: None,
                            error: Some(e.to_string()),
                        }
                    }
                }
            }
            None => {
                error!("Tool not found: {}", name);
                ToolResult {
                    name: name.to_string(),
                    success: false,
                    result: None,
                    error: Some(format!("Tool not found: {}. Use list_tools or search_tools to find available tools.", name)),
                }
            }
        }
    }

    /// Handle list_tools meta-tool
    async fn handle_list_tools(&self, args: Value) -> ToolResult {
        let category = args.get("category").and_then(|v| v.as_str()).unwrap_or("all");
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

        let all_tools = self.tool_registry.list().await;
        
        let filtered: Vec<_> = if category == "all" {
            all_tools
        } else {
            all_tools.into_iter()
                .filter(|t| {
                    match category {
                        "ovs" => t.name.starts_with("ovs_"),
                        "systemd" => t.name.starts_with("dbus_systemd_"),
                        "dbus" => t.name.starts_with("dbus_"),
                        "file" => t.name.starts_with("file_"),
                        "shell" => t.name.starts_with("shell_"),
                        "network" => t.name.starts_with("rtnetlink_"),
                        "openflow" => t.name.starts_with("openflow_"),
                        "agent" => t.name.starts_with("agent_"),
                        _ => false,
                    }
                })
                .collect()
        };

        let tools_json: Vec<Value> = filtered.iter()
            .take(limit)
            .map(|t| json!({
                "name": t.name,
                "description": t.description,
            }))
            .collect();

        ToolResult {
            name: "list_tools".to_string(),
            success: true,
            result: Some(json!({
                "tools": tools_json,
                "total": filtered.len(),
                "showing": tools_json.len(),
                "category": category,
            })),
            error: None,
        }
    }

    /// Handle search_tools meta-tool
    async fn handle_search_tools(&self, args: Value) -> ToolResult {
        let query = args.get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();

        if query.is_empty() {
            return ToolResult {
                name: "search_tools".to_string(),
                success: false,
                result: None,
                error: Some("Query parameter is required".to_string()),
            };
        }

        let all_tools = self.tool_registry.list().await;
        let matches: Vec<Value> = all_tools.iter()
            .filter(|t| {
                t.name.to_lowercase().contains(&query) ||
                t.description.to_lowercase().contains(&query)
            })
            .map(|t| json!({
                "name": t.name,
                "description": t.description,
            }))
            .collect();

        ToolResult {
            name: "search_tools".to_string(),
            success: true,
            result: Some(json!({
                "query": query,
                "matches": matches,
                "count": matches.len(),
            })),
            error: None,
        }
    }

    /// Handle get_tool_schema meta-tool
    async fn handle_get_tool_schema(&self, args: Value) -> ToolResult {
        let tool_name = args.get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if tool_name.is_empty() {
            return ToolResult {
                name: "get_tool_schema".to_string(),
                success: false,
                result: None,
                error: Some("tool_name parameter is required".to_string()),
            };
        }

        match self.tool_registry.get(tool_name).await {
            Some(tool_def) => {
                ToolResult {
                    name: "get_tool_schema".to_string(),
                    success: true,
                    result: Some(json!({
                        "tool_name": tool_name,
                        "description": tool_def.description(),
                        "input_schema": tool_def.input_schema(),
                    })),
                    error: None,
                }
            }
            None => {
                ToolResult {
                    name: "get_tool_schema".to_string(),
                    success: false,
                    result: None,
                    error: Some(format!("Tool not found: {}. Use list_tools or search_tools to find available tools.", tool_name)),
                }
            }
        }
    }


    /// Execute direct tool command: "tool_name {json_args}"
    async fn execute_direct_tool(&self, input: &str) -> Result<OrchestratorResponse> {
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let tool_name = parts[0].trim();
        let args: Value = if parts.len() > 1 {
            serde_json::from_str(parts[1].trim()).unwrap_or(json!({}))
        } else {
            json!({})
        };

        let result = self.execute_tool(tool_name, args).await;
        
        let message = if result.success {
            format!("✅ **{}**\n```json\n{}\n```",
                tool_name,
                serde_json::to_string_pretty(&result.result).unwrap_or_default())
        } else {
            format!("❌ **{}** failed: {}", 
                tool_name, 
                result.error.as_ref().unwrap_or(&"Unknown".to_string()))
        };

        Ok(OrchestratorResponse {
            success: result.success,
            message,
            tools_executed: vec![tool_name.to_string()],
            tool_results: vec![result],
        })
    }

    /// Extract tool calls from text (for models without native tool calling)
    fn extract_tool_calls_from_text(&self, text: &str, available: &[String]) -> Vec<(String, Value)> {
        let mut calls = Vec::new();

        // Pattern 1: <tool_call>name({"arg": "val"})</tool_call> (with multiline support)
        if let Ok(re) = Regex::new(r"(?s)<tool_call>\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\((.*?)\)\s*</tool_call>") {
            for cap in re.captures_iter(text) {
                if let (Some(name), Some(args)) = (cap.get(1), cap.get(2)) {
                    let tool_name = name.as_str().to_string();
                    if available.contains(&tool_name) {
                        if let Ok(parsed) = serde_json::from_str(args.as_str().trim()) {
                            info!("Extracted tool call from XML tags: {}", tool_name);
                            calls.push((tool_name, parsed));
                        }
                    }
                }
            }
        }

        // If we found XML tag calls, use those (preferred format)
        if !calls.is_empty() {
            return calls;
        }

        // Pattern 2: ```tool or ```tool_code blocks
        if let Ok(re) = Regex::new(r"(?s)```(?:tool|tool_code)\s*\n(.+?)\n```") {
            for cap in re.captures_iter(text) {
                if let Some(block) = cap.get(1) {
                    // Parse tool calls from inside the block
                    let inner_calls = self.parse_function_calls(block.as_str(), available);
                    for call in inner_calls {
                        if !calls.iter().any(|(n, _)| n == &call.0) {
                            calls.push(call);
                        }
                    }
                }
            }
        }

        if !calls.is_empty() {
            return calls;
        }

        // Pattern 3: tool_name({"arg": "val"}) - direct function call syntax
        calls.extend(self.parse_function_calls(text, available));

        calls
    }

    /// Parse function call patterns from text
    fn parse_function_calls(&self, text: &str, available: &[String]) -> Vec<(String, Value)> {
        let mut calls = Vec::new();
        
        // Match: tool_name({...}) with multiline JSON support
        if let Ok(re) = Regex::new(r"(?s)\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\(\s*(\{.*?\})\s*\)") {
            for cap in re.captures_iter(text) {
                if let (Some(name), Some(args)) = (cap.get(1), cap.get(2)) {
                    let tool_name = name.as_str().to_string();
                    if available.contains(&tool_name) && !calls.iter().any(|(n, _)| n == &tool_name) {
                        if let Ok(parsed) = serde_json::from_str(args.as_str().trim()) {
                            info!("Extracted tool call from function syntax: {}", tool_name);
                            calls.push((tool_name, parsed));
                        }
                    }
                }
            }
        }

        calls
    }

    /// Build compact mode tool definitions (4 meta-tools)
    fn build_compact_mode_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "list_tools".to_string(),
                description: "List available tools by category. Use this to discover what tools are available.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "category": {
                            "type": "string",
                            "description": "Optional category filter (ovs, systemd, dbus, file, shell, network, openflow, agent)",
                            "enum": ["ovs", "systemd", "dbus", "file", "shell", "network", "openflow", "agent", "all"]
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of tools to return (default: 50)",
                            "default": 50
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "search_tools".to_string(),
                description: "Search for tools by keyword in name or description.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query (e.g., 'bridge', 'restart', 'network')"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "get_tool_schema".to_string(),
                description: "Get the input schema for a specific tool before executing it.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "tool_name": {
                            "type": "string",
                            "description": "Name of the tool to get schema for"
                        }
                    },
                    "required": ["tool_name"]
                }),
            },
            ToolDefinition {
                name: "execute_tool".to_string(),
                description: "Execute any tool by name with the provided arguments.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "tool_name": {
                            "type": "string",
                            "description": "Name of the tool to execute"
                        },
                        "arguments": {
                            "type": "object",
                            "description": "Arguments to pass to the tool"
                        }
                    },
                    "required": ["tool_name", "arguments"]
                }),
            },
        ]
    }

    /// Build system prompt for compact mode
    fn build_compact_mode_system_prompt(&self) -> String {
        r#"You are an AI system administrator with access to 138+ system management tools via a compact interface.

CRITICAL RULES:
1. ALWAYS use tools for system operations - NEVER suggest CLI commands
2. Use the 4 meta-tools to discover and execute the actual tools:
   - list_tools() - Browse available tools by category
   - search_tools(query) - Find tools by keyword
   - get_tool_schema(tool_name) - Get input schema before executing
   - execute_tool(tool_name, arguments) - Execute any tool

WORKFLOW:
1. If you don't know which tool to use, call list_tools() or search_tools()
2. Once you find the right tool, call get_tool_schema() to see what arguments it needs
3. Then call execute_tool() with the tool name and arguments

AVAILABLE TOOL CATEGORIES:
- **OVS**: Open vSwitch management (ovs_list_bridges, ovs_add_port, etc.)
- **Systemd**: Service management via D-Bus (dbus_systemd_restart_unit, etc.)
- **D-Bus**: Direct D-Bus calls (dbus_call, dbus_introspect, etc.)
- **File**: File operations (file_read, file_write, file_list, etc.)
- **Shell**: Command execution (shell_exec, shell_which, etc.)
- **Network**: Kernel networking via rtnetlink (rtnetlink_list_links, etc.)
- **OpenFlow**: OpenFlow rule management (openflow_add_flow, etc.)
- **Agent**: AI agent operations (agent_spawn, agent_list, etc.)

SPECIAL AGENTS (ALWAYS AVAILABLE):
The following specialized agents are pre-loaded. Use them for complex tasks in their domain. NO need to check availability:
- agent_rust_pro: Rust development (build, check, test, fix)
- agent_backend_architect: System architecture design
- agent_network_engineer: Complex network diagnostics and routing
- agent_context_manager: Session context and memory management

IMPORTANT: Only call these agents if the user request matches their expertise. If the request is unrelated (e.g., "list files" does not require backend-architect), simply use the standard tools or ignore the agents.

EXAMPLES:
User: "List all OVS bridges"
1. search_tools({"query": "bridge"})  → Find ovs_list_bridges
2. execute_tool({"tool_name": "ovs_list_bridges", "arguments": {}})

User: "Restart nginx"
1. search_tools({"query": "restart"})  → Find dbus_systemd_restart_unit
2. get_tool_schema({"tool_name": "dbus_systemd_restart_unit"})  → See it needs "unit" param
3. execute_tool({"tool_name": "dbus_systemd_restart_unit", "arguments": {"unit": "nginx.service"}})

User: "What tools are available for networking?"
1. list_tools({"category": "network"})  → Browse network tools

REMEMBER: You have access to D-Bus (systemd, NetworkManager), OVSDB (OVS), and Netlink (kernel) - all via native protocols, not CLI.

HINT - OVS NETWORKING:
Creating an OVS bridge (`ovs_create_bridge`) does NOT create a Linux network interface automatically.
To assign an IP address to a bridge, you MUST add an internal port with the same name (or different name) to the bridge first.
Example:
1. `execute_tool("ovs_create_bridge", {"name": "br0"})`
2. `execute_tool("ovs_add_port", {"bridge": "br0", "port": "br0", "type": "internal"})`
3. `execute_tool("rtnetlink_add_address", {"interface": "br0", ...})`
"#.to_string()
    }



    /// Format results for display
    fn format_results(&self, llm_text: &str, results: &[ToolResult], forbidden: &[String]) -> String {
        let mut output = String::new();

        // Add warning if LLM suggested forbidden commands
        if !forbidden.is_empty() {
            output.push_str("⚠️ Note: The AI attempted to suggest CLI commands, but I executed the proper tools instead.\n\n");
        }

        // Summary for multiple tools
        let success_count = results.iter().filter(|r| r.success).count();
        let failed_count = results.iter().filter(|r| !r.success).count();
        
        if results.len() > 1 {
            output.push_str(&format!("**Executed {} tools** ({} success, {} failed)\n\n", 
                results.len(), success_count, failed_count));
        }

        // Tool results with actual data
        for r in results {
            if r.success {
                output.push_str(&format!("✅ **{}**\n", r.name));
                if let Some(ref data) = r.result {
                    // Format the result data nicely
                    output.push_str(&self.format_tool_result(data));
                }
            } else {
                output.push_str(&format!("❌ **{}** failed: {}\n", 
                    r.name, 
                    r.error.as_ref().unwrap_or(&"Unknown".to_string())));
            }
            output.push('\n');
        }

        // Add LLM commentary (cleaned) only if it adds value
        let cleaned = self.clean_llm_text(llm_text);
        if !cleaned.is_empty() && cleaned.len() > 20 {
            output.push_str("---\n\n");
            output.push_str(&cleaned);
        }

        output
    }

    /// Format a tool result for display
    fn format_tool_result(&self, data: &Value) -> String {
        match data {
            Value::Object(obj) => {
                let mut result = String::new();
                for (key, value) in obj {
                    // Skip internal fields
                    if key.starts_with('_') {
                        continue;
                    }
                    // Special handling for arrays - show them expanded
                    if let Value::Array(arr) = value {
                        result.push_str(&format!("  • **{}**:\n", key));
                        result.push_str(&self.format_array(arr, 20)); // Show up to 20 items
                    } else {
                        let formatted_value = self.format_value(value);
                        result.push_str(&format!("  • **{}**: {}\n", key, formatted_value));
                    }
                }
                result
            }
            Value::Array(arr) => self.format_array(arr, 20),
            Value::String(s) => format!("  {}\n", s),
            Value::Number(n) => format!("  {}\n", n),
            Value::Bool(b) => format!("  {}\n", b),
            Value::Null => "  *(null)*\n".to_string(),
        }
    }

    /// Format an array for display
    fn format_array(&self, arr: &[Value], max_items: usize) -> String {
        if arr.is_empty() {
            return "    *(empty list)*\n".to_string();
        }

        let mut result = String::new();
        let show_count = arr.len().min(max_items);
        
        for item in arr.iter().take(show_count) {
            match item {
                Value::Object(obj) => {
                    // For objects, show key fields inline
                    let summary = self.summarize_object(obj);
                    result.push_str(&format!("    - {}\n", summary));
                }
                Value::String(s) => {
                    result.push_str(&format!("    - {}\n", s));
                }
                _ => {
                    result.push_str(&format!("    - {}\n", self.format_value(item)));
                }
            }
        }

        if arr.len() > max_items {
            result.push_str(&format!("    ... and {} more\n", arr.len() - max_items));
        }

        result
    }

    /// Summarize an object into a single line
    fn summarize_object(&self, obj: &serde_json::Map<String, Value>) -> String {
        // Look for common identifying fields
        let name_fields = ["name", "unit", "id", "path", "service", "interface", "bridge"];
        let status_fields = ["state", "status", "active_state", "sub_state", "load_state"];
        
        let mut parts = Vec::new();
        
        // Get the name/id
        for field in name_fields {
            if let Some(Value::String(v)) = obj.get(field) {
                parts.push(v.clone());
                break;
            }
        }
        
        // Get status if available
        for field in status_fields {
            if let Some(Value::String(v)) = obj.get(field) {
                parts.push(format!("({})", v));
                break;
            }
        }

        if parts.is_empty() {
            // Fallback: show first few fields
            let keys: Vec<String> = obj.keys().take(3).cloned().collect();
            format!("{{{}...}}", keys.join(", "))
        } else {
            parts.join(" ")
        }
    }

    /// Format a single value for display
    fn format_value(&self, value: &Value) -> String {
        match value {
            Value::String(s) => {
                if s.len() > 100 {
                    format!("{}...", &s[..100])
                } else {
                    s.clone()
                }
            }
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Array(arr) => {
                if arr.is_empty() {
                    "[]".to_string()
                } else if arr.len() <= 5 {
                    let items: Vec<String> = arr.iter().map(|v| self.format_value(v)).collect();
                    format!("[{}]", items.join(", "))
                } else {
                    format!("[{} items]", arr.len())
                }
            }
            Value::Object(obj) => {
                if obj.is_empty() {
                    "{}".to_string()
                } else {
                    self.summarize_object(obj)
                }
            }
            Value::Null => "null".to_string(),
        }
    }

    /// Clean tool call syntax from LLM text
    fn clean_llm_text(&self, text: &str) -> String {
        let mut cleaned = text.to_string();
        
        // Remove <tool_call>...</tool_call>
        if let Ok(re) = regex::Regex::new(r"<tool_call>.*?</tool_call>") {
            cleaned = re.replace_all(&cleaned, "").to_string();
        }
        
        // Remove tool_name({...})
        if let Ok(re) = regex::Regex::new(r"\w+\(\s*\{[^}]*\}\s*\)") {
            cleaned = re.replace_all(&cleaned, "").to_string();
        }

        // Clean multiple newlines
        if let Ok(re) = regex::Regex::new(r"\n{3,}") {
            cleaned = re.replace_all(&cleaned, "\n\n").to_string();
        }

        cleaned.trim().to_string()
    }

    // === Command handlers ===

    fn help_response(&self) -> OrchestratorResponse {
        OrchestratorResponse::success(r#"📚 **op-dbus Help**

**Commands:**
- `help` - Show this help
- `tools` - List all available tools
- `status` - System status
- `run <tool> {args}` - Execute tool directly

**Natural Language:**
Just describe what you want:
- "Create an OVS bridge called ovsbr0"
- "Restart nginx"
- "List all network interfaces"
- "Show systemd unit status for sshd"

The AI uses native protocols (D-Bus, OVSDB, Netlink) - never CLI commands."#)
    }

    async fn list_tools_response(&self) -> OrchestratorResponse {
        let tools = self.tool_registry.list().await;
        let mut output = format!("🔧 **{} Tools Available**\n\n", tools.len());

        // Group by prefix
        let prefixes = ["ovs_", "dbus_systemd_", "dbus_", "file_", "shell_", "rtnetlink_", "openflow_", "agent_"];
        let names = ["OVS", "Systemd", "D-Bus", "File", "Shell", "Network", "OpenFlow", "Agents"];

        for (prefix, name) in prefixes.iter().zip(names.iter()) {
            let group: Vec<_> = tools.iter().filter(|t| t.name.starts_with(prefix)).collect();
            if !group.is_empty() {
                output.push_str(&format!("**{}** ({})\n", name, group.len()));
                for t in group.iter().take(5) {
                    output.push_str(&format!("  • `{}`\n", t.name));
                }
                if group.len() > 5 {
                    output.push_str(&format!("  ... +{} more\n", group.len() - 5));
                }
                output.push('\n');
            }
        }

        // Other
        let other: Vec<_> = tools.iter()
            .filter(|t| !prefixes.iter().any(|p| t.name.starts_with(p)))
            .collect();
        if !other.is_empty() {
            output.push_str(&format!("**Other** ({})\n", other.len()));
            for t in other.iter().take(5) {
                output.push_str(&format!("  • `{}`\n", t.name));
            }
            if other.len() > 5 {
                output.push_str(&format!("  ... +{} more\n", other.len() - 5));
            }
        }

        OrchestratorResponse::success(output)
    }

    async fn status_response(&self) -> OrchestratorResponse {
        let tools = self.tool_registry.list().await;
        let model = self.chat_manager.current_model().await;
        let provider = format!("{:?}", self.chat_manager.current_provider().await);

        OrchestratorResponse::success(format!(
            r#"📊 **System Status**

🔧 Tools: {} registered
🤖 LLM: {} ({})
✅ Ready for commands"#,
            tools.len(), model, provider
        ))
    }

    /// Generate a human-readable description of a tool call
    fn describe_tool_call(&self, name: &str, args: &Value) -> String {
        match name {
            "execute_tool" => {
                let tool_name = args.get("tool_name").and_then(|v| v.as_str()).unwrap_or("unknown");
                let inner_args = args.get("arguments").cloned().unwrap_or(json!({}));
                self.describe_actual_tool(tool_name, &inner_args)
            }
            "list_tools" => {
                let category = args.get("category").and_then(|v| v.as_str()).unwrap_or("all");
                format!("Listing available tools (category: {})", category)
            }
            "search_tools" => {
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                format!("Searching for tools matching \"{}\"", query)
            }
            "get_tool_schema" => {
                let tool = args.get("tool_name").and_then(|v| v.as_str()).unwrap_or("unknown");
                format!("Getting schema for tool: {}", tool)
            }
            _ => format!("Calling {}", name)
        }
    }

    /// Generate human-readable description for actual tool execution
    fn describe_actual_tool(&self, name: &str, args: &Value) -> String {
        match name {
            // OVS tools
            "ovs_list_bridges" => "Listing OVS bridges".to_string(),
            "ovs_create_bridge" => {
                let bridge = args.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Creating OVS bridge '{}'", bridge)
            }
            "ovs_delete_bridge" => {
                let bridge = args.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Deleting OVS bridge '{}'", bridge)
            }
            "ovs_add_port" => {
                let bridge = args.get("bridge").and_then(|v| v.as_str()).unwrap_or("?");
                let port = args.get("port").and_then(|v| v.as_str()).unwrap_or("?");
                let port_type = args.get("type").and_then(|v| v.as_str()).unwrap_or("normal");
                format!("Adding {} port '{}' to bridge '{}'", port_type, port, bridge)
            }
            "ovs_list_ports" => {
                let bridge = args.get("bridge").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Listing ports on bridge '{}'", bridge)
            }
            // Systemd tools
            "dbus_systemd_restart_unit" => {
                let unit = args.get("unit").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Restarting service '{}'", unit)
            }
            "dbus_systemd_start_unit" => {
                let unit = args.get("unit").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Starting service '{}'", unit)
            }
            "dbus_systemd_stop_unit" => {
                let unit = args.get("unit").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Stopping service '{}'", unit)
            }
            "dbus_systemd_get_unit_status" => {
                let unit = args.get("unit").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Checking status of '{}'", unit)
            }
            "dbus_systemd_list_units" => "Listing systemd units".to_string(),
            // Network tools
            "rtnetlink_list_links" | "list_network_interfaces" => "Listing network interfaces".to_string(),
            "rtnetlink_add_address" => {
                let iface = args.get("interface").and_then(|v| v.as_str()).unwrap_or("?");
                let addr = args.get("address").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Adding IP address {} to interface '{}'", addr, iface)
            }
            "rtnetlink_link_up" => {
                let iface = args.get("interface").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Bringing interface '{}' up", iface)
            }
            "rtnetlink_link_down" => {
                let iface = args.get("interface").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Bringing interface '{}' down", iface)
            }
            // File tools
            "file_read" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Reading file '{}'", path)
            }
            "file_write" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Writing to file '{}'", path)
            }
            "file_list" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                format!("Listing files in '{}'", path)
            }
            // Shell tools
            "shell_exec" => {
                let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
                let cmd_preview = if cmd.len() > 50 { format!("{}...", &cmd[..50]) } else { cmd.to_string() };
                format!("Running command: {}", cmd_preview)
            }
            // Default
            _ => format!("Executing {}", name)
        }
    }
}
