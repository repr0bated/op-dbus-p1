//! LLM provider abstraction for different AI backends

use async_trait::async_trait;
use op_core::{ChatMessage, Error, Result, ToolDefinition};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid;

/// Tool call request from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// LLM chat response - can be content or tool calls
#[derive(Debug, Clone)]
pub enum LlmResponse {
    /// Plain text response
    Content(ChatMessage),
    /// LLM wants to call tools
    ToolCalls(Vec<ToolCall>),
}

/// LLM provider trait
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Send a chat request - returns content or tool calls
    async fn chat(&self, messages: &[ChatMessage], tools: &[ToolDefinition])
        -> Result<LlmResponse>;

    /// Stream a chat response (optional)
    async fn chat_stream(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolDefinition],
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin>> {
        Err(Error::internal("Streaming not supported"))
    }

    /// List available models
    async fn list_models(&self) -> Result<Vec<String>>;

    /// Check if provider is available
    async fn is_available(&self) -> bool;
}

/// OpenAI-compatible provider
pub struct OpenAiProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
            model,
        }
    }

    pub fn with_base_url(api_key: String, base_url: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url,
            model,
        }
    }
}

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAiTool>,
    /// Force the model to call a tool - "required" means it MUST call a tool
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseMessage {
    role: String,
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCallFunction {
    name: String,
    arguments: String,
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse> {
        let openai_messages: Vec<OpenAiMessage> = messages
            .iter()
            .map(|m| OpenAiMessage {
                role: match m.role {
                    op_core::ChatRole::User => "user".to_string(),
                    op_core::ChatRole::Assistant => "assistant".to_string(),
                    op_core::ChatRole::System => "system".to_string(),
                    op_core::ChatRole::Tool => "tool".to_string(),
                },
                content: m.content.clone(),
            })
            .collect();

        let openai_tools: Vec<OpenAiTool> = tools
            .iter()
            .map(|t| OpenAiTool {
                tool_type: "function".to_string(),
                function: OpenAiFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect();

        // Force tool calling if tools are provided - this prevents hallucination
        // by requiring the LLM to always go through the tool execution path
        let tool_choice = if !openai_tools.is_empty() {
            Some("required".to_string())
        } else {
            None
        };

        let request = OpenAiRequest {
            model: self.model.clone(),
            messages: openai_messages,
            tools: openai_tools,
            tool_choice,
        };

        debug!(
            "Sending request to OpenAI: {} with {} tools, tool_choice={:?}",
            self.model,
            request.tools.len(),
            request.tool_choice
        );
        if !request.tools.is_empty() {
            debug!("First tool: {:?}", request.tools[0].function.name);
        }

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::internal(format!("HTTP error: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::internal(format!(
                "OpenAI API error {}: {}",
                status, body
            )));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| Error::internal(format!("Failed to read response: {}", e)))?;

        debug!(
            "LLM response: {}",
            &response_text[..std::cmp::min(500, response_text.len())]
        );

        let openai_response: OpenAiResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                Error::internal(format!(
                    "JSON parse error: {} - Response: {}",
                    e,
                    &response_text[..std::cmp::min(200, response_text.len())]
                ))
            })?;

        let choice = openai_response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| Error::internal("No response from OpenAI"))?;

        // Check if the LLM wants to call tools
        if let Some(tool_calls) = choice.message.tool_calls {
            if !tool_calls.is_empty() {
                let calls: Vec<ToolCall> = tool_calls
                    .into_iter()
                    .map(|tc| {
                        // Parse the arguments string as JSON
                        let args = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::json!({}));
                        ToolCall {
                            id: tc.id,
                            name: tc.function.name,
                            arguments: args,
                        }
                    })
                    .collect();

                debug!(
                    "LLM requested {} tool calls: {:?}",
                    calls.len(),
                    calls.iter().map(|t| &t.name).collect::<Vec<_>>()
                );
                return Ok(LlmResponse::ToolCalls(calls));
            }
        }

        let content = choice.message.content.unwrap_or_default();
        debug!("Parsed content: {}", content);

        // Check for tool calls in content (fallback for models that don't use tool_calls field)
        if let Some(tool_call_start) = content.find("<tool_call>") {
            if let Some(tool_call_end) = content[tool_call_start..].find("</tool_call>") {
                let tool_call_json =
                    &content[tool_call_start + 11..tool_call_start + tool_call_end];

                // Parse the JSON tool call
                if let Ok(tool_call_data) =
                    serde_json::from_str::<serde_json::Value>(tool_call_json)
                {
                    if let (Some(name), Some(args)) = (
                        tool_call_data.get("name").and_then(|n| n.as_str()),
                        tool_call_data.get("arguments"),
                    ) {
                        let tool_call = ToolCall {
                            id: format!("content-tool-{}", uuid::Uuid::new_v4()),
                            name: name.to_string(),
                            arguments: args.clone(),
                        };

                        debug!(
                            "Parsed tool call from content: {} with args {:?}",
                            name, args
                        );
                        return Ok(LlmResponse::ToolCalls(vec![tool_call]));
                    }
                }
            }
        }

        Ok(LlmResponse::Content(ChatMessage::assistant(content)))
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        Ok(vec![
            "gpt-4".to_string(),
            "gpt-4-turbo".to_string(),
            "gpt-3.5-turbo".to_string(),
        ])
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// Ollama provider for local models
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "http://localhost:11434".to_string(),
            model,
        }
    }

    pub fn with_base_url(base_url: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            model,
        }
    }
}

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
        _tools: &[ToolDefinition],
    ) -> Result<LlmResponse> {
        let ollama_messages: Vec<OllamaMessage> = messages
            .iter()
            .map(|m| OllamaMessage {
                role: match m.role {
                    op_core::ChatRole::User => "user".to_string(),
                    op_core::ChatRole::Assistant => "assistant".to_string(),
                    op_core::ChatRole::System => "system".to_string(),
                    op_core::ChatRole::Tool => "user".to_string(), // Ollama doesn't have tool role
                },
                content: m.content.clone(),
            })
            .collect();

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: ollama_messages,
            stream: false,
        };

        debug!("Sending request to Ollama: {}", self.model);

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::internal(format!("HTTP error: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::internal(format!(
                "Ollama API error {}: {}",
                status, body
            )));
        }

        let ollama_response: OllamaResponse = response
            .json()
            .await
            .map_err(|e| Error::internal(format!("JSON parse error: {}", e)))?;

        Ok(LlmResponse::Content(ChatMessage::assistant(
            ollama_response.message.content,
        )))
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        #[derive(Debug, Deserialize)]
        struct ModelsResponse {
            models: Vec<ModelInfo>,
        }

        #[derive(Debug, Deserialize)]
        struct ModelInfo {
            name: String,
        }

        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| Error::internal(format!("HTTP error: {}", e)))?;

        if response.status().is_success() {
            let models: ModelsResponse = response
                .json()
                .await
                .map_err(|e| Error::internal(format!("JSON parse error: {}", e)))?;
            Ok(models.models.into_iter().map(|m| m.name).collect())
        } else {
            Ok(vec![])
        }
    }

    async fn is_available(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

/// Create an LLM provider based on configuration
pub fn create_provider(
    provider_type: &str,
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
) -> Result<Arc<dyn LlmProvider>> {
    match provider_type.to_lowercase().as_str() {
        "openai" => {
            let key =
                api_key.ok_or_else(|| Error::InvalidArgument("OpenAI requires API key".into()))?;
            let model = model.unwrap_or_else(|| "gpt-4".to_string());

            let provider = if let Some(url) = base_url {
                OpenAiProvider::with_base_url(key, url, model)
            } else {
                OpenAiProvider::new(key, model)
            };

            Ok(Arc::new(provider))
        }
        "ollama" => {
            let model = model.unwrap_or_else(|| "llama2".to_string());

            let provider = if let Some(url) = base_url {
                OllamaProvider::with_base_url(url, model)
            } else {
                OllamaProvider::new(model)
            };

            Ok(Arc::new(provider))
        }
        _ => Err(Error::InvalidArgument(format!(
            "Unknown provider type: {}",
            provider_type
        ))),
    }
}
