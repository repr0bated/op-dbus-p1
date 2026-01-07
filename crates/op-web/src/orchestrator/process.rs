use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use op_llm::{
    provider::{ChatMessage, ChatRequest, LlmProvider, ToolChoice, ModelInfo},
};
use op_core::types::ChatRole as CoreChatRole;

use super::{UnifiedOrchestrator, OrchestratorResponse, OrchestratorEvent, MAX_TURNS};

impl UnifiedOrchestrator {
    /// Process user input - main entry point
    pub async fn process(
        &self,
        _session_id: &str,
        input: &str,
        event_tx: Option<mpsc::Sender<OrchestratorEvent>>,
    ) -> Result<OrchestratorResponse> {
        let input_trimmed = input.trim();
        let input_preview = if input_trimmed.len() > 80 {
            format!("{}\
...", &input_trimmed[..80])
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
    pub(crate) async fn process_with_llm(
        &self,
        input: &str,
        event_tx: Option<mpsc::Sender<OrchestratorEvent>>,
    ) -> Result<OrchestratorResponse> {
        
        // Use compact mode - only expose 4 meta-tools
        let tool_defs = self.build_compact_mode_tools();
        
        info!("LLM using COMPACT mode with {} meta-tools", tool_defs.len());

        // Fetch all tools to populate the context
        let all_tools = self.tool_registry.list().await;
        let tool_list_context = all_tools.iter()
            .map(|t| format!("- {}: {}", t.name, t.description))
            .collect::<Vec<_>>()
            .join("\n");

        // Build system prompt: Capabilities + Compact Instructions + Tool Directory
        let system_msg_core = op_chat::system_prompt::generate_system_prompt().await;
        let compact_instructions = self.build_compact_mode_system_prompt();
        
        let combined_prompt = format!("{}

== INTERFACE MODE: COMPACT ==
{}

## GLOBAL TOOL DIRECTORY
The following tools are available via execute_tool():

{}", 
            system_msg_core.content,
            compact_instructions,
            tool_list_context
        );

        // Convert role (default to system)
        let role_str = match system_msg_core.role {
            CoreChatRole::User => "user",
            CoreChatRole::Assistant => "assistant",
            CoreChatRole::System => "system",
            CoreChatRole::Tool => "tool",
        }.to_string();

        let system_msg = ChatMessage {
            role: role_str,
            content: combined_prompt,
            tool_calls: None,
            tool_call_id: None,
        };

        // Build model info (simplified for context)
        let model_id = self.chat_manager.current_model().await;
        let model = ModelInfo {
            id: model_id.clone(),
            name: model_id.clone(),
            description: None,
            parameters: None,
            available: true,
            tags: vec![],
            downloads: None,
            updated_at: None,
        };

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

        // Orchestration loop
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
                tool_choice: if is_last_turn { ToolChoice::None } else { ToolChoice::Auto },
                max_tokens: Some(4096),
                temperature: Some(0.7),
                top_p: None,
            };

            // Call LLM with timeout (60 seconds per turn) and heartbeat
            let llm_future = self.chat_manager.chat_with_request(&model.id, request);

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
                    } else {
                        break; // No event channel
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
                warn!("Detected forbidden commands in response: {:?}", forbidden);
                all_forbidden.extend(forbidden);
            }

            // Parse tool calls from response
            let turn_tools = self.parse_tool_calls(&response.message.content, &response.message.tool_calls);

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
            messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: format!("Executing tools: {}", tool_call_summary.join(", ")),
                tool_calls: response.message.tool_calls.clone(),
                tool_call_id: None,
            });

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
                        args: args.clone(),
                    }).await;
                }

                // Execute the tool
                let tool_result = self.execute_tool(&name, args.clone()).await;

                // Emit ToolResult event
                if let Some(tx) = &event_tx {
                    let _ = tx.send(OrchestratorEvent::ToolResult {
                        name: name.clone(),
                        success: tool_result.success,
                        result: tool_result.result.clone(),
                        error: tool_result.error.clone(),
                    }).await;
                }

                // Add tool result to conversation
                let result_content = if tool_result.success {
                    serde_json::to_string(&tool_result.result).unwrap_or_default()
                } else {
                    format!("Error: {}", tool_result.error.clone().unwrap_or_default())
                };

                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: result_content,
                    tool_calls: None,
                    tool_call_id: Some(name.clone()),
                });

                all_results.push(tool_result.clone());

                // Check for response tool - if called, we're done
                if name == "respond" || name == "response" {
                    if let Some(ref res) = tool_result.result {
                        if let Some(msg) = res.get("message").and_then(|v| v.as_str()) {
                            response_message = Some(msg.to_string());
                        }
                    }
                }
            }

            // If respond tool was called, finish
            if let Some(msg) = response_message {
                final_response_text = msg;
                finished_with_response_tool = true;
                info!("💬 Chatbot finished with response tool");
                break;
            }
        }

        // Build final response
        let response = OrchestratorResponse {
            success: true,
            message: final_response_text,
            tools_executed: all_tools,
            tool_results: all_results,
            turns: 0, // TODO: track actual turns
        };

        Ok(response)
    }
}