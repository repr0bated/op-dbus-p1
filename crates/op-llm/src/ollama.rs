//! Ollama API Client (Cloud and Local)
//!
//! ## API Endpoints
//!
//! | Endpoint | URL | Purpose |
//! |----------|-----|--------|
//! | Cloud API | `https://api.ollama.com` | Ollama cloud service |
//! | Local API | `http://localhost:11434` | Local Ollama instance |
//! | Chat | `/api/chat` | Chat completions |
//! | Generate | `/api/generate` | Text generation |
//! | Models | `/api/tags` | List local models |
//! | Library | `https://ollama.com/library` | Browse available models |
//!
//! ## Authentication
//! - Header: `Authorization: Bearer {OLLAMA_API_KEY}` (cloud only)
//! - Environment: `OLLAMA_API_KEY` (optional)
//!
//! ## Pricing
//! - Local: Free
//! - Cloud: Pay-per-use

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info};

use crate::provider::{
    ChatMessage, ChatResponse, LlmProvider, ModelInfo, ProviderType, TokenUsage,
};

// =============================================================================
// API ENDPOINT CONFIGURATION
// =============================================================================

/// Ollama API endpoints
pub mod endpoints {
    /// Cloud API URL
    pub const CLOUD_API: &str = "https://api.ollama.com";
    
    /// Local API URL (default)
    pub const LOCAL_API: &str = "http://localhost:11434";
    
    /// Library URL for model discovery
    pub const LIBRARY_URL: &str = "https://ollama.com";
    
    /// Chat endpoint
    /// Full URL: {API}/api/chat
    pub const CHAT: &str = "/api/chat";
    
    /// Generate endpoint
    /// Full URL: {API}/api/generate
    pub const GENERATE: &str = "/api/generate";
    
    /// Tags/models endpoint
    /// Full URL: {API}/api/tags
    pub const TAGS: &str = "/api/tags";
}

// =============================================================================
// DATA STRUCTURES
// =============================================================================

#[derive(Debug, Deserialize)]
struct OllamaModelsResponse {
    models: Option<Vec<OllamaModel>>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    modified_at: Option<String>,
    // size: Option<u64>,
    // digest: Option<String>,
    details: Option<OllamaModelDetails>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelDetails {
    family: Option<String>,
    parameter_size: Option<String>,
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatMessage>,
    stream: bool,
    options: Option<OllamaChatOptions>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaChatOptions {
    temperature: Option<f32>,
    top_p: Option<f32>,
    num_predict: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaChatMessage,
    model: Option<String>,
    #[serde(rename = "done_reason")]
    done_reason: Option<String>,
    #[serde(rename = "prompt_eval_count")]
    prompt_eval_count: Option<u32>,
    #[serde(rename = "eval_count")]
    eval_count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct OllamaLibraryModel {
    name: String,
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    pulls: Option<u64>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
}

// =============================================================================
// CLIENT IMPLEMENTATION
// =============================================================================

/// Ollama Client (Cloud or Local)
pub struct OllamaCloudClient {
    client: Client,
    api_key: Option<String>,
    /// API URL for chat/generate
    api_url: String,
    /// Library URL for model discovery
    library_url: String,
}

impl OllamaCloudClient {
    /// Create a new Ollama client for cloud
    ///
    /// Uses cloud endpoint: https://api.ollama.com
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            api_key,
            api_url: endpoints::CLOUD_API.to_string(),
            library_url: endpoints::LIBRARY_URL.to_string(),
        }
    }

    /// Create from environment
    pub fn from_env() -> Self {
        let api_key = std::env::var("OLLAMA_API_KEY").ok();
        let api_url = std::env::var("OLLAMA_API_URL").ok();
        let library_url = std::env::var("OLLAMA_LIBRARY_URL").unwrap_or_else(|_| {
            endpoints::LIBRARY_URL.to_string()
        });

        if let Some(api_url) = api_url {
            let mut client = Self::new(api_key);
            client.api_url = api_url;
            client.library_url = library_url;
            return client;
        }

        if api_key.is_some() {
            let mut client = Self::new(api_key);
            client.library_url = library_url;
            return client;
        }

        let mut client = Self::local();
        client.library_url = library_url;
        client
    }

    /// Create for local Ollama instance
    pub fn local() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            api_key: None,
            api_url: endpoints::LOCAL_API.to_string(),
            library_url: endpoints::LIBRARY_URL.to_string(),
        }
    }

    /// Create with custom endpoint
    pub fn with_endpoint(api_key: Option<String>, endpoint: impl Into<String>) -> Self {
        let mut client = Self::new(api_key);
        client.api_url = endpoint.into();
        client
    }

    /// Get the current API URL
    pub fn api_url(&self) -> &str {
        &self.api_url
    }

    /// Fetch models dynamically
    async fn fetch_models_dynamic(&self) -> Result<Vec<ModelInfo>> {
        let endpoints = [
            format!("{}/api/tags", self.api_url),
            format!("{}/api/tags", self.library_url),
            format!("{}/api/models", self.library_url),
        ];

        for endpoint in &endpoints {
            debug!("Trying Ollama endpoint: {}", endpoint);

            let mut request = self.client.get(endpoint);
            if let Some(ref key) = self.api_key {
                request = request.header("Authorization", format!("Bearer {}", key));
            }

            if let Ok(response) = request.send().await {
                if response.status().is_success() {
                    if let Ok(data) = response.json::<OllamaModelsResponse>().await {
                        if let Some(models) = data.models {
                            let infos: Vec<ModelInfo> = models
                                .into_iter()
                                .map(|m| self.convert_model(m))
                                .collect();
                            if !infos.is_empty() {
                                info!("Fetched {} models from Ollama", infos.len());
                                return Ok(infos);
                            }
                        }
                    }
                }
            }
        }

        self.fetch_from_library().await
    }

    async fn fetch_from_library(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/library", self.library_url);
        debug!("Fetching Ollama library: {}", url);

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to fetch Ollama library")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Ollama library request failed: {}", response.status()));
        }

        let text = response.text().await?;

        if let Ok(models) = serde_json::from_str::<Vec<OllamaLibraryModel>>(&text) {
            let infos: Vec<ModelInfo> = models
                .into_iter()
                .map(|m| ModelInfo {
                    id: m.name.clone(),
                    name: m.name,
                    description: m.description,
                    parameters: None,
                    available: true,
                    tags: m.tags,
                    downloads: m.pulls,
                    updated_at: m.updated_at,
                })
                .collect();

            if !infos.is_empty() {
                return Ok(infos);
            }
        }

        if text.contains("<!DOCTYPE html>") {
            return Ok(self.parse_html_models(&text));
        }

        Err(anyhow::anyhow!("Could not fetch models from Ollama"))
    }

    fn parse_html_models(&self, html: &str) -> Vec<ModelInfo> {
        let mut models = Vec::new();

        for line in html.lines() {
            if line.contains("/library/") && line.contains("href=") {
                if let Some(start) = line.find("/library/") {
                    let rest = &line[start + 9..];
                    if let Some(end) = rest.find('"').or_else(|| rest.find('\'')) {
                        let name = &rest[..end];
                        if !name.is_empty() && !name.contains('/') {
                            models.push(ModelInfo {
                                id: name.to_string(),
                                name: name.to_string(),
                                description: None,
                                parameters: None,
                                available: true,
                                tags: vec![],
                                downloads: None,
                                updated_at: None,
                            });
                        }
                    }
                }
            }
        }

        models.sort_by(|a, b| a.id.cmp(&b.id));
        models.dedup_by(|a, b| a.id == b.id);
        models
    }

    fn convert_model(&self, model: OllamaModel) -> ModelInfo {
        let parameters = model.details.as_ref().and_then(|d| d.parameter_size.clone());

        ModelInfo {
            id: model.name.clone(),
            name: model.name,
            description: model.details.as_ref().and_then(|d| d.family.clone()),
            parameters,
            available: true,
            tags: vec![],
            downloads: None,
            updated_at: model.modified_at,
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaCloudClient {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Ollama
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        info!("Fetching Ollama models");
        info!("  API: {}", self.api_url);
        info!("  Library: {}", self.library_url);
        self.fetch_models_dynamic().await
    }

    async fn search_models(&self, query: &str, limit: usize) -> Result<Vec<ModelInfo>> {
        let all_models = self.list_models().await?;
        let query_lower = query.to_lowercase();

        Ok(all_models
            .into_iter()
            .filter(|m| m.name.to_lowercase().contains(&query_lower))
            .take(limit)
            .collect())
    }

    async fn get_model(&self, model_id: &str) -> Result<Option<ModelInfo>> {
        let models = self.list_models().await?;
        Ok(models.into_iter().find(|m| m.id == model_id || m.name == model_id))
    }

    async fn is_model_available(&self, model_id: &str) -> Result<bool> {
        let model = self.get_model(model_id).await?;
        Ok(model.is_some())
    }

    async fn chat(&self, model: &str, messages: Vec<ChatMessage>) -> Result<ChatResponse> {
        // Build URL: {api_url}/api/chat
        let url = format!("{}/api/chat", self.api_url);

        info!("Ollama chat: model={}, endpoint={}", model, self.api_url);

        let ollama_messages: Vec<OllamaChatMessage> = messages
            .iter()
            .map(|m| OllamaChatMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();

        let request = OllamaChatRequest {
            model: model.to_string(),
            messages: ollama_messages,
            stream: false,
            options: Some(OllamaChatOptions {
                temperature: Some(0.7),
                top_p: Some(0.95),
                num_predict: Some(1024),
            }),
        };

        debug!("Ollama request to: {}", url);

        let mut req = self.client.post(&url);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req
            .json(&request)
            .send()
            .await
            .context("Failed to send Ollama request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Ollama API error {}: {}", status, body));
        }

        let result: OllamaChatResponse = response
            .json()
            .await
            .context("Failed to parse Ollama response")?;

        let usage = if result.prompt_eval_count.is_some() || result.eval_count.is_some() {
            Some(TokenUsage {
                prompt_tokens: result.prompt_eval_count.unwrap_or(0),
                completion_tokens: result.eval_count.unwrap_or(0),
                total_tokens: result.prompt_eval_count.unwrap_or(0) + result.eval_count.unwrap_or(0),
            })
        } else {
            None
        };

        Ok(ChatResponse {
            message: ChatMessage {
                role: result.message.role,
                content: result.message.content,
                tool_calls: None,
                tool_call_id: None,
            },
            model: result.model.unwrap_or_else(|| model.to_string()),
            provider: "ollama".to_string(),
            finish_reason: result.done_reason,
            usage,
            tool_calls: None,
        })
    }

    async fn chat_stream(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<String>>> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let response = self.chat(model, messages).await?;
        tx.send(Ok(response.message.content)).await.ok();
        Ok(rx)
    }
}
