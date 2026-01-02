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
    ) -> Result<OrchestratorResponse> {
        let input_trimmed = input.trim();
        info!("Processing: {}", input_trimmed);

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
        self.process_with_llm(input_trimmed).await
    }

    /// Process through LLM with tool calling (multi-turn)
    async fn process_with_llm(&self, input: &str) -> Result<OrchestratorResponse> {
        const MAX_TURNS: usize = 10; // Prevent infinite loops
        
        // Build tool definitions from registry
        let tools = self.tool_registry.list().await;
        let tool_defs: Vec<ToolDefinition> = tools
            .iter()
            .map(|t| ToolDefinition {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.input_schema.clone(),
            })
            .collect();

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        
        info!("LLM has access to {} tools", tool_defs.len());

        // Build system prompt
        let system_prompt = self.build_system_prompt(&tool_names);

        // Get model
        let model = self.chat_manager.current_model().await;

        // Initialize conversation
        let mut messages = vec![
            ChatMessage::system(&system_prompt),
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
                info!("Turn {}: FINAL TURN - will return results after this", turn + 1);
            }
            
            info!("Turn {}: calling LLM with {} messages", turn + 1, messages.len());

            // Build request
            let request = ChatRequest {
                messages: messages.clone(),
                tools: tool_defs.clone(),
                tool_choice: ToolChoice::Auto,
                max_tokens: Some(4096),
                temperature: Some(0.3),
                top_p: None,
            };

            // Call LLM
            let response = self.chat_manager
                .chat_with_request(&model, request)
                .await
                .context("LLM request failed")?;

            debug!("Turn {} response: {:?}", turn + 1, response.message.content);

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

            // Text-based tool calls (fallback)
            let text_tools = self.extract_tool_calls_from_text(&response.message.content, &tool_names);
            for (name, args) in text_tools {
                if !turn_tools.iter().any(|(n, _)| n == &name) {
                    turn_tools.push((name, args));
                }
            }

            // If no tool calls, we're done - this is the final response
            if turn_tools.is_empty() {
                final_response_text = response.message.content.clone();
                info!("Turn {}: no tool calls, finishing", turn + 1);
                break;
            }

            // Execute all tool calls for this turn
            info!("Turn {}: executing {} tools", turn + 1, turn_tools.len());
            
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
                info!("Executing tool: {} with args: {}", name, args);
                all_tools.push(name.clone());

                // Check if this is a response tool - these signal completion
                if name == "respond_to_user" || name == "cannot_perform" || name == "request_clarification" {
                    should_finish = true;
                    // Extract the message from args
                    if let Some(msg) = args.get("message").and_then(|v| v.as_str()) {
                        response_message = Some(msg.to_string());
                    }
                }

                let result = self.execute_tool(&name, args).await;
                
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
                info!("Response tool called, finishing orchestration");
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
                    error: Some(format!("Tool not found: {}", name)),
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

    /// Build system prompt with tool list
    fn build_system_prompt(&self, tool_names: &[String]) -> String {
        let mut prompt = String::from(r#"You are an AI system administrator with DIRECT access to system tools.

CRITICAL RULES:
1. ALWAYS use tools for system operations - NEVER suggest CLI commands
2. Call tools using the native tool_call mechanism
3. You have access to D-Bus (systemd, NetworkManager), OVSDB (OVS), and Netlink (kernel)

"#);

        // Group tools by prefix
        let mut groups: std::collections::HashMap<&str, Vec<&String>> = std::collections::HashMap::new();
        for name in tool_names {
            let prefix = if name.starts_with("ovs_") { "OVS" }
                else if name.starts_with("dbus_systemd") { "Systemd" }
                else if name.starts_with("dbus_") { "D-Bus" }
                else if name.starts_with("file_") { "File" }
                else if name.starts_with("shell_") { "Shell" }
                else if name.starts_with("rtnetlink_") { "Network" }
                else if name.starts_with("openflow_") { "OpenFlow" }
                else if name.starts_with("agent_") { "Agents" }
                else { "Other" };
            groups.entry(prefix).or_default().push(name);
        }

        prompt.push_str("AVAILABLE TOOLS:\n");
        for (group, tools) in groups.iter() {
            prompt.push_str(&format!("\n## {} ({} tools)\n", group, tools.len()));
            for tool in tools.iter().take(10) {
                prompt.push_str(&format!("- {}\n", tool));
            }
            if tools.len() > 10 {
                prompt.push_str(&format!("  ... and {} more\n", tools.len() - 10));
            }
        }

        prompt.push_str(r#"
EXAMPLES:
- "List OVS bridges" → call ovs_list_bridges({})
- "Restart nginx" → call dbus_systemd_restart_unit({"unit": "nginx.service"})
- "Read /etc/hosts" → call file_read({"path": "/etc/hosts"})

When asked to do something, USE THE TOOLS. Explain what you're doing briefly.
"#);

        prompt
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
}
