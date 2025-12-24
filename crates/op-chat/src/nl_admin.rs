//! Natural Language Server Administration
//!
//! This is the CORE module that enables natural language server administration.
//! It coordinates between:
//! - LLM (understands user intent)
//! - Tool Registry (executes operations)
//! - Response formatting (user-friendly output)
//!
//! The key insight: We instruct the LLM via system prompt to ALWAYS use tools,
//! then parse and execute those tool calls.

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use op_llm::provider::{ChatMessage, ChatRequest, ChatResponse, LlmProvider, ToolDefinition, ToolChoice};
use op_tools::ToolRegistry;

/// Extracted tool call from LLM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedToolCall {
    pub name: String,
    pub arguments: Value,
    pub source: ToolCallSource,
}

/// Where the tool call was extracted from
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolCallSource {
    /// Native tool_calls from LLM API
    Native,
    /// Parsed from <tool_call> tags in text
    XmlTags,
    /// Parsed from ```tool format in text
    CodeBlock,
    /// Parsed from JSON in text
    JsonInText,
}

/// Result of natural language admin operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NLAdminResult {
    /// User-friendly response message
    pub message: String,
    /// Whether operation was successful
    pub success: bool,
    /// Tools that were executed
    pub tools_executed: Vec<String>,
    /// Detailed results from each tool
    pub tool_results: Vec<ToolExecutionResult>,
    /// Raw LLM response (for debugging)
    pub llm_response: Option<String>,
}

/// Result of a single tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub tool_name: String,
    pub success: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
}

/// Tool call parser - extracts tool calls from LLM responses
pub struct ToolCallParser {
    /// Regex for <tool_call>name(args)</tool_call> format
    xml_tag_regex: Regex,
    /// Regex for ```tool\nname(args)\n``` format
    code_block_regex: Regex,
    /// Regex for function call format: tool_name({"arg": "value"})
    function_call_regex: Regex,
}

impl Default for ToolCallParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolCallParser {
    pub fn new() -> Self {
        Self {
            // Matches: <tool_call>tool_name({"arg": "value"})</tool_call>
            xml_tag_regex: Regex::new(
                r"<tool_call>\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\((.*)\)\s*</tool_call>"
            ).unwrap(),
            // Matches: ```tool\ntool_name({"arg": "value"})\n```
            code_block_regex: Regex::new(
                r"```tool\s*\n([a-zA-Z_][a-zA-Z0-9_]*)\s*\((.*)\)\s*\n```"
            ).unwrap(),
            // Matches: tool_name({"arg": "value"})
            function_call_regex: Regex::new(
                r"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\(\s*(\{[^}]*\})\s*\)"
            ).unwrap(),
        }
    }

    /// Extract tool calls from LLM response
    pub fn extract_tool_calls(
        &self,
        response: &ChatResponse,
        available_tools: &[String],
    ) -> Vec<ExtractedToolCall> {
        let mut calls = Vec::new();

        // 1. Check native tool_calls first (best case)
        if let Some(ref tool_calls) = response.tool_calls {
            for tc in tool_calls {
                calls.push(ExtractedToolCall {
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    source: ToolCallSource::Native,
                });
            }
        }

        // If we got native calls, use those
        if !calls.is_empty() {
            return calls;
        }

        // 2. Parse from text content
        let content = &response.message.content;

        // Try XML tags: <tool_call>name(args)</tool_call>
        for cap in self.xml_tag_regex.captures_iter(content) {
            if let (Some(name), Some(args)) = (cap.get(1), cap.get(2)) {
                let tool_name = name.as_str().to_string();
                if available_tools.contains(&tool_name) {
                    if let Ok(arguments) = serde_json::from_str(args.as_str()) {
                        calls.push(ExtractedToolCall {
                            name: tool_name,
                            arguments,
                            source: ToolCallSource::XmlTags,
                        });
                    }
                }
            }
        }

        if !calls.is_empty() {
            return calls;
        }

        // Try code blocks: ```tool\nname(args)\n```
        for cap in self.code_block_regex.captures_iter(content) {
            if let (Some(name), Some(args)) = (cap.get(1), cap.get(2)) {
                let tool_name = name.as_str().to_string();
                if available_tools.contains(&tool_name) {
                    if let Ok(arguments) = serde_json::from_str(args.as_str()) {
                        calls.push(ExtractedToolCall {
                            name: tool_name,
                            arguments,
                            source: ToolCallSource::CodeBlock,
                        });
                    }
                }
            }
        }

        if !calls.is_empty() {
            return calls;
        }

        // Try function call format: tool_name({"arg": "value"})
        for cap in self.function_call_regex.captures_iter(content) {
            if let (Some(name), Some(args)) = (cap.get(1), cap.get(2)) {
                let tool_name = name.as_str().to_string();
                if available_tools.contains(&tool_name) {
                    if let Ok(arguments) = serde_json::from_str(args.as_str()) {
                        calls.push(ExtractedToolCall {
                            name: tool_name,
                            arguments,
                            source: ToolCallSource::JsonInText,
                        });
                    }
                }
            }
        }

        calls
    }

    /// Check if response contains forbidden CLI commands
    pub fn contains_forbidden_commands(&self, content: &str) -> Vec<String> {
        let forbidden = [
            "ovs-vsctl", "ovs-ofctl", "ovs-dpctl", "ovsdb-client",
            "systemctl", "service ", "journalctl",
            "ip addr", "ip link", "ip route", "ifconfig", "nmcli",
            "apt ", "apt-get", "yum ", "dnf ", "pacman",
            "sudo ", "su -",
        ];

        let lower = content.to_lowercase();
        forbidden
            .iter()
            .filter(|cmd| lower.contains(*cmd))
            .map(|s| s.to_string())
            .collect()
    }
}

/// Natural Language Admin Orchestrator
///
/// This is the main entry point for natural language server administration.
pub struct NLAdminOrchestrator {
    tool_registry: Arc<ToolRegistry>,
    tool_parser: ToolCallParser,
}

impl NLAdminOrchestrator {
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            tool_registry,
            tool_parser: ToolCallParser::new(),
        }
    }

    /// Generate the system prompt that instructs the LLM to use tools
    pub async fn generate_system_prompt(&self) -> String {
        let tools = self.tool_registry.list().await;

        let mut prompt = String::from(r#"You are an AI server administrator assistant. You help users manage their Linux server using ONLY the provided tools.

## CRITICAL RULES

1. **ALWAYS USE TOOLS** - For ANY system operation, you MUST call the appropriate tool.
2. **NEVER SUGGEST CLI COMMANDS** - Do NOT mention or suggest commands like:
   - ovs-vsctl, ovs-ofctl (use ovs_* tools instead)
   - systemctl, service (use dbus_systemd_* tools instead)
   - ip, ifconfig, nmcli (use network_* or dbus_networkmanager_* tools instead)
   - apt, yum, dnf (use dbus_packagekit_* tools instead)

3. **TOOL CALL FORMAT** - When you need to perform an action, use this format:
   <tool_call>tool_name({"arg1": "value1", "arg2": "value2"})</tool_call>

4. **ONE TOOL AT A TIME** - Call one tool, wait for result, then proceed.

5. **EXPLAIN WHAT YOU'RE DOING** - Tell the user what operation you're performing.

## AVAILABLE TOOLS

"#);

        // Group tools by category
        let mut ovs_tools = Vec::new();
        let mut systemd_tools = Vec::new();
        let mut network_tools = Vec::new();
        let mut other_tools = Vec::new();

        for tool in &tools {
            let entry = format!("- **{}**: {}\n", tool.name, tool.description);
            if tool.name.starts_with("ovs_") {
                ovs_tools.push(entry);
            } else if tool.name.contains("systemd") {
                systemd_tools.push(entry);
            } else if tool.name.contains("network") || tool.name.contains("dbus_networkmanager") {
                network_tools.push(entry);
            } else {
                other_tools.push(entry);
            }
        }

        if !ovs_tools.is_empty() {
            prompt.push_str("### Open vSwitch (OVS) Tools\n");
            for t in ovs_tools {
                prompt.push_str(&t);
            }
            prompt.push('\n');
        }

        if !systemd_tools.is_empty() {
            prompt.push_str("### Systemd/Service Tools\n");
            for t in systemd_tools {
                prompt.push_str(&t);
            }
            prompt.push('\n');
        }

        if !network_tools.is_empty() {
            prompt.push_str("### Network Tools\n");
            for t in network_tools {
                prompt.push_str(&t);
            }
            prompt.push('\n');
        }

        if !other_tools.is_empty() {
            prompt.push_str("### Other Tools\n");
            for t in other_tools {
                prompt.push_str(&t);
            }
            prompt.push('\n');
        }

        prompt.push_str(r#"
## EXAMPLES

**User:** "Create an OVS bridge called ovsbr0"
**You:** I'll create the OVS bridge for you.
<tool_call>ovs_create_bridge({"name": "ovsbr0"})</tool_call>

**User:** "Restart nginx"
**You:** I'll restart the nginx service.
<tool_call>dbus_systemd_restart_unit({"unit": "nginx.service"})</tool_call>

**User:** "List all OVS bridges"
**You:** Let me list the OVS bridges.
<tool_call>ovs_list_bridges({})</tool_call>

**User:** "Add port eth1 to bridge ovsbr0"
**You:** I'll add port eth1 to the bridge.
<tool_call>ovs_add_port({"bridge": "ovsbr0", "port": "eth1"})</tool_call>

## REMEMBER
- ALWAYS use tools, NEVER suggest CLI commands
- Use the exact tool names listed above
- Format tool calls with <tool_call> tags
- Explain what you're doing before calling tools
"#);

        prompt
    }

    /// Get tool definitions for LLM API
    pub async fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        let tools = self.tool_registry.list().await;

        tools
            .into_iter()
            .map(|tool| ToolDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.input_schema.clone(),
            })
            .collect()
    }

    /// Get list of available tool names
    pub async fn get_tool_names(&self) -> Vec<String> {
        self.tool_registry
            .list()
            .await
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    /// Process a natural language admin request
    pub async fn process<P: LlmProvider>(
        &self,
        provider: &P,
        model: &str,
        user_message: &str,
        conversation_history: Vec<ChatMessage>,
    ) -> Result<NLAdminResult> {
        info!("Processing NL admin request: {}", user_message);

        // Build messages with system prompt
        let system_prompt = self.generate_system_prompt().await;
        let mut messages = vec![ChatMessage::system(&system_prompt)];
        messages.extend(conversation_history);
        messages.push(ChatMessage::user(user_message));

        // Get tool definitions
        let tools = self.get_tool_definitions().await;
        let tool_names = self.get_tool_names().await;

        // Build request
        let request = ChatRequest {
            messages,
            tools,
            tool_choice: ToolChoice::Auto, // Let LLM decide, but system prompt guides it
            max_tokens: Some(2048),
            temperature: Some(0.3), // Lower temperature for more deterministic tool selection
            top_p: None,
        };

        // Send to LLM
        let response = provider
            .chat_with_request(model, request)
            .await
            .context("LLM request failed")?;

        debug!("LLM response: {:?}", response.message.content);

        // Extract tool calls
        let tool_calls = self.tool_parser.extract_tool_calls(&response, &tool_names);

        // Check for forbidden commands in response
        let forbidden = self.tool_parser.contains_forbidden_commands(&response.message.content);
        if !forbidden.is_empty() {
            warn!("LLM suggested forbidden commands: {:?}", forbidden);
            // We'll still try to execute any tool calls found
        }

        // Execute tool calls
        let mut tool_results = Vec::new();
        let mut tools_executed = Vec::new();

        for call in &tool_calls {
            info!("Executing tool: {} with args: {:?}", call.name, call.arguments);
            tools_executed.push(call.name.clone());

            match self.execute_tool(&call.name, call.arguments.clone()).await {
                Ok(result) => {
                    tool_results.push(ToolExecutionResult {
                        tool_name: call.name.clone(),
                        success: true,
                        result: Some(result),
                        error: None,
                    });
                }
                Err(e) => {
                    error!("Tool execution failed: {}", e);
                    tool_results.push(ToolExecutionResult {
                        tool_name: call.name.clone(),
                        success: false,
                        result: None,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        // Generate user-friendly response
        let message = self.format_response(&response.message.content, &tool_results, &forbidden);
        let success = tool_results.iter().all(|r| r.success) && forbidden.is_empty();

        Ok(NLAdminResult {
            message,
            success,
            tools_executed,
            tool_results,
            llm_response: Some(response.message.content),
        })
    }

    /// Execute a single tool
    async fn execute_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let tool = self
            .tool_registry
            .get(name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", name))?;

        let result = tool.execute(arguments).await;

        match result {
            Ok(content) => Ok(content),
            Err(e) => Err(anyhow::anyhow!("Tool execution failed: {}", e)),
        }
    }

    /// Format the final response for the user
    fn format_response(
        &self,
        llm_response: &str,
        tool_results: &[ToolExecutionResult],
        forbidden_commands: &[String],
    ) -> String {
        let mut response = String::new();

        // Add warning if forbidden commands were detected
        if !forbidden_commands.is_empty() {
            response.push_str("⚠️ Note: The AI attempted to suggest CLI commands, but I executed the proper tools instead.\n\n");
        }

        // If no tools were executed, just return the LLM response (cleaned)
        if tool_results.is_empty() {
            // Remove tool_call tags from response
            let cleaned = self.clean_llm_response(llm_response);
            return cleaned;
        }

        // Format tool results
        for result in tool_results {
            if result.success {
                response.push_str(&format!("✅ **{}** executed successfully\n", result.tool_name));
                if let Some(ref data) = result.result {
                    // Format the result nicely
                    if let Some(obj) = data.as_object() {
                        for (key, value) in obj {
                            if key != "_internal" {
                                response.push_str(&format!("   - {}: {}\n", key, format_value(value)));
                            }
                        }
                    } else {
                        response.push_str(&format!("   Result: {}\n", format_value(data)));
                    }
                }
            } else {
                response.push_str(&format!("❌ **{}** failed\n", result.tool_name));
                if let Some(ref err) = result.error {
                    response.push_str(&format!("   Error: {}\n", err));
                }
            }
            response.push('\n');
        }

        response
    }

    /// Clean tool_call tags from LLM response
    fn clean_llm_response(&self, response: &str) -> String {
        // Remove <tool_call>...</tool_call> tags
        let re = Regex::new(r"<tool_call>.*?</tool_call>").unwrap();
        let cleaned = re.replace_all(response, "");

        // Remove ```tool...``` blocks
        let re2 = Regex::new(r"```tool\s*\n.*?\n```").unwrap();
        let cleaned = re2.replace_all(&cleaned, "");

        cleaned.trim().to_string()
    }
}

/// Format a JSON value for display
fn format_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(arr) => {
            if arr.len() <= 5 {
                format!("[{}]", arr.iter().map(format_value).collect::<Vec<_>>().join(", "))
            } else {
                format!("[{} items]", arr.len())
            }
        }
        Value::Object(_) => serde_json::to_string_pretty(value).unwrap_or_else(|_| "[object]".to_string()),
        Value::Null => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call_parser_xml_tags() {
        let parser = ToolCallParser::new();
        let content = r#"I'll create the bridge for you.
<tool_call>ovs_create_bridge({"name": "ovsbr0"})</tool_call>"#;

        // Create a mock response
        let response = ChatResponse {
            message: ChatMessage::assistant(content),
            model: "test".to_string(),
            provider: "test".to_string(),
            finish_reason: None,
            usage: None,
            tool_calls: None,
        };

        let available = vec!["ovs_create_bridge".to_string()];
        let calls = parser.extract_tool_calls(&response, &available);

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "ovs_create_bridge");
        assert_eq!(calls[0].arguments["name"], "ovsbr0");
    }

    #[test]
    fn test_forbidden_command_detection() {
        let parser = ToolCallParser::new();

        let content = "You can use ovs-vsctl add-br ovsbr0 to create the bridge";
        let forbidden = parser.contains_forbidden_commands(content);
        assert!(forbidden.contains(&"ovs-vsctl".to_string()));

        let clean = "I'll create the bridge using the ovs_create_bridge tool";
        let forbidden = parser.contains_forbidden_commands(clean);
        assert!(forbidden.is_empty());
    }
}
