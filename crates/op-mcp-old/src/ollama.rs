// Ollama HTTP client for AI chat integration
//
// This module provides a model-agnostic interface for communicating with
// Ollama (or compatible) API servers, supporting both local and cloud deployments.

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Ollama chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user", "assistant", "system"
    pub content: String,
}

/// Ollama chat request
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    options: Option<ChatOptions>,
}

/// Chat options for Ollama
#[derive(Debug, Serialize)]
struct ChatOptions {
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<i32>,
}

/// Ollama chat response
#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: ChatMessage,
    done: bool,
    model: Option<String>,
    #[serde(rename = "created_at")]
    created_at: Option<String>,
    #[serde(rename = "done_reason")]
    done_reason: Option<String>,
    #[serde(rename = "total_duration")]
    total_duration: Option<u64>,
    #[serde(rename = "load_duration")]
    load_duration: Option<u64>,
    #[serde(rename = "prompt_eval_count")]
    prompt_eval_count: Option<u32>,
    #[serde(rename = "eval_count")]
    eval_count: Option<u32>,
}

/// Ollama model info
#[derive(Debug, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub size: u64,
    pub modified_at: String,
}

/// Ollama client for interacting with local Ollama server or Ollama Cloud API
pub struct OllamaClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
    default_model: Option<String>,
}

impl Default for OllamaClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OllamaClient {
    /// Get the default model name from environment variable or configured value
    pub fn default_model(&self) -> String {
        self.default_model
            .clone()
            .or_else(|| std::env::var("OLLAMA_DEFAULT_MODEL").ok())
            .unwrap_or_else(|| "llama2".to_string())
    }

    /// Set the default model for this client
    pub fn with_default_model(mut self, model: String) -> Self {
        self.default_model = Some(model);
        self
    }

    /// Create a new Ollama client for local server
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(300)) // 5 minutes timeout for model responses
                .build()
                .unwrap_or_default(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            default_model: None,
        }
    }

    /// Create client with custom base URL
    pub fn with_url(base_url: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
            base_url,
            api_key: None,
            default_model: None,
        }
    }

    /// Create client for Ollama Cloud API
    pub fn cloud(api_key: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
            base_url: "https://ollama.com".to_string(),
            api_key: Some(api_key),
            default_model: None,
        }
    }

    /// Create client for Ollama Cloud API with custom API endpoint
    pub fn cloud_with_endpoint(api_key: String, endpoint: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
            base_url: endpoint,
            api_key: Some(api_key),
            default_model: None,
        }
    }

    /// Create client with API key for cloud access
    pub fn with_api_key(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
            base_url,
            api_key: Some(api_key),
            default_model: None,
        }
    }

    /// Check if Ollama server is running
    pub async fn health_check(&self) -> Result<bool> {
        let response = self
            .client
            .get(format!("{}/api/version", self.base_url))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => Ok(true),
            _ => Ok(false),
        }
    }

    /// Check if the AI service is available (alias for health_check)
    pub async fn is_available(&self) -> bool {
        self.health_check().await.unwrap_or(false)
    }

    /// List available models (local or cloud)
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        #[derive(Deserialize)]
        struct ModelsResponse {
            models: Vec<ModelInfo>,
        }

        let mut request_builder = self.client.get(format!("{}/api/tags", self.base_url));

        // Add authentication header for cloud API
        if let Some(ref api_key) = self.api_key {
            request_builder =
                request_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request_builder
            .send()
            .await?
            .json::<ModelsResponse>()
            .await?;

        Ok(response.models)
    }

    /// Check if a specific model is available
    pub async fn has_model(&self, model_name: &str) -> Result<bool> {
        let models = self.list_models().await?;
        Ok(models.iter().any(|m| m.name == model_name))
    }

    /// Pull a model (if not available)
    pub async fn pull_model(&self, model_name: &str) -> Result<()> {
        #[derive(Serialize)]
        struct PullRequest {
            name: String,
        }

        let request = PullRequest {
            name: model_name.to_string(),
        };

        self.client
            .post(format!("{}/api/pull", self.base_url))
            .json(&request)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    /// Generate a chat response using the specified model
    pub async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        temperature: Option<f32>,
        max_tokens: Option<i32>,
    ) -> Result<String> {
        let options = ChatOptions {
            temperature,
            top_p: Some(0.9), // Default top_p
            max_tokens,
        };

        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: false, // For simplicity, we'll do non-streaming requests
            options: Some(options),
        };

        let mut request_builder = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request);

        // Add authentication header for cloud API
        if let Some(ref api_key) = self.api_key {
            request_builder =
                request_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request_builder.send().await?.error_for_status()?;

        let chat_response: ChatResponse = response.json().await?;
        Ok(chat_response.message.content)
    }

    /// Simple chat method for single user message
    pub async fn simple_chat(&self, model: &str, user_message: &str) -> Result<String> {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        }];

        self.chat(model, &messages, Some(0.7), None).await
    }

    /// Advanced chat with system context and conversation history
    pub async fn chat_with_context(
        &self,
        model: &str,
        system_context: &str,
        conversation_history: &[ChatMessage],
        user_message: &str,
        temperature: Option<f32>,
    ) -> Result<String> {
        let mut messages = Vec::new();

        // Add system context if provided
        if !system_context.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: system_context.to_string(),
            });
        }

        // Add conversation history
        messages.extend_from_slice(conversation_history);

        // Add current user message
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        });

        self.chat(model, &messages, temperature, None).await
    }

    /// Chat with full context and tool awareness
    /// This method enhances the system context with available tools information
    pub async fn chat_with_tools(
        &self,
        model: &str,
        user_message: &str,
        system_context: &str,
        conversation_history: &[ChatMessage],
        available_tools: &str,
    ) -> Result<String> {
        let enhanced_context = format!(
            "{}\n\nAVAILABLE TOOLS AND CAPABILITIES:\n{}\n\nYou can reference these tools in your responses and suggest using them when appropriate.",
            system_context, available_tools
        );

        self.chat_with_context(
            model,
            &enhanced_context,
            conversation_history,
            user_message,
            Some(0.7),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ollama_health_check() {
        let client = OllamaClient::new();
        let is_healthy = client.health_check().await.unwrap_or(false);
        // This test will pass if Ollama is running, fail if not
        // We don't assert since Ollama might not be running in CI
        println!("Ollama health check: {}", is_healthy);
    }
}
