//! Google Gemini API Client
//!
//! ## API Endpoints
//!
//! | Endpoint | URL | Purpose |
//! |----------|-----|--------|
//! | Base URL | `https://generativelanguage.googleapis.com/v1beta` | All Gemini APIs |
//! | Chat | `/models/{model}:generateContent?key={API_KEY}` | Generate content |
//! | Stream | `/models/{model}:streamGenerateContent?key={API_KEY}` | Streaming |
//!
//! ## Authentication
//! - Query parameter: `?key={GEMINI_API_KEY}`
//! - Environment: `GEMINI_API_KEY` or `GOOGLE_API_KEY`
//!
//! ## Models
//! Models are statically defined based on API key quota.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};
use std::time::Duration;
use tracing::{debug, info};

use crate::provider::{LlmProvider, ProviderType, ModelInfo, ChatMessage, ChatResponse, TokenUsage};

// =============================================================================
// API ENDPOINT CONFIGURATION
// =============================================================================

/// Gemini API endpoints
pub mod endpoints {
    /// Base API URL
    pub const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
    
    /// Generate content (chat)
    /// Full URL: {BASE_URL}/models/{model}:generateContent?key={API_KEY}
    pub const GENERATE_CONTENT: &str = "/models/{model}:generateContent";
    
    /// Stream generate content
    /// Full URL: {BASE_URL}/models/{model}:streamGenerateContent?key={API_KEY}
    pub const STREAM_GENERATE: &str = "/models/{model}:streamGenerateContent";
}

// =============================================================================
// DATA STRUCTURES
// =============================================================================

/// Gemini model category
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeminiCategory {
    TextOut,
    MultiModalGenerative,
    LiveApi,
    Other,
}

impl std::fmt::Display for GeminiCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeminiCategory::TextOut => write!(f, "Text-out models"),
            GeminiCategory::MultiModalGenerative => write!(f, "Multi-modal generative"),
            GeminiCategory::LiveApi => write!(f, "Live API"),
            GeminiCategory::Other => write!(f, "Other models"),
        }
    }
}

/// Gemini model with rate limits
#[derive(Debug, Clone)]
pub struct GeminiModel {
    pub id: String,
    pub category: GeminiCategory,
    pub rpm: u32,
    pub tpm: u64,
    pub rpd: u32,
}

impl GeminiModel {
    fn new(id: &str, category: GeminiCategory, rpm: u32, tpm: u64, rpd: u32) -> Self {
        Self { id: id.to_string(), category, rpm, tpm, rpd }
    }
}

/// Static list of Gemini models
fn get_gemini_models() -> Vec<GeminiModel> {
    use GeminiCategory::*;
    
    vec![
        // Text-out models (main chat)
        GeminiModel::new("gemini-2.5-pro", TextOut, 150, 2_000_000, 10_000),
        GeminiModel::new("gemini-2.5-flash", TextOut, 1_000, 1_000_000, 10_000),
        GeminiModel::new("gemini-2.5-flash-lite", TextOut, 4_000, 4_000_000, 0),
        GeminiModel::new("gemini-2.0-flash", TextOut, 2_000, 4_000_000, 0),
        GeminiModel::new("gemini-2.0-flash-lite", TextOut, 4_000, 4_000_000, 0),
        
        // Multi-modal
        GeminiModel::new("gemini-2.5-flash-preview-image", MultiModalGenerative, 500, 500_000, 2_000),
        GeminiModel::new("imagen-4.0-generate", MultiModalGenerative, 10, 0, 70),
        
        // Live API
        GeminiModel::new("gemini-2.0-flash-live", LiveApi, 0, 4_000_000, 0),
        GeminiModel::new("gemini-2.5-flash-live", LiveApi, 0, 1_000_000, 0),
        
        // Other
        GeminiModel::new("gemma-3-27b", Other, 30, 15_000, 14_400),
    ]
}

/// Gemini API request
#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Debug, Serialize)]
struct GenerationConfig {
    temperature: Option<f32>,
    #[serde(rename = "topP")]
    top_p: Option<f32>,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<UsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContentResponse,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiContentResponse {
    parts: Vec<GeminiPartResponse>,
}

#[derive(Debug, Deserialize)]
struct GeminiPartResponse {
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u32>,
}

// =============================================================================
// CLIENT IMPLEMENTATION
// =============================================================================

/// Google Gemini Client
pub struct GeminiClient {
    client: Client,
    api_key: String,
    /// Base API URL
    api_url: String,
    models: Vec<GeminiModel>,
}

impl GeminiClient {
    /// Create a new Gemini client
    ///
    /// Uses default endpoint: https://generativelanguage.googleapis.com/v1beta
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
            api_key: api_key.into(),
            api_url: endpoints::BASE_URL.to_string(),
            models: get_gemini_models(),
        }
    }
    
    /// Create from environment variable
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("GEMINI_API_KEY")
            .or_else(|_| std::env::var("GOOGLE_API_KEY"))
            .context("GEMINI_API_KEY or GOOGLE_API_KEY not set")?;
        Ok(Self::new(api_key))
    }

    /// Create with custom endpoint
    pub fn with_endpoint(api_key: impl Into<String>, endpoint: impl Into<String>) -> Self {
        let mut client = Self::new(api_key);
        client.api_url = endpoint.into();
        client
    }

    /// Get the current API URL
    pub fn api_url(&self) -> &str {
        &self.api_url
    }
    
    fn to_model_info(&self, model: &GeminiModel) -> ModelInfo {
        let description = format!(
            "{} - RPM: {}, TPM: {}{}",
            model.category,
            if model.rpm == 0 { "Unlimited".to_string() } else { model.rpm.to_string() },
            if model.tpm >= 1_000_000 { format!("{}M", model.tpm / 1_000_000) } 
            else if model.tpm >= 1_000 { format!("{}K", model.tpm / 1_000) }
            else if model.tpm == 0 { "N/A".to_string() }
            else { model.tpm.to_string() },
            if model.rpd == 0 { ", RPD: Unlimited".to_string() } 
            else { format!(", RPD: {}", model.rpd) }
        );
        
        ModelInfo {
            id: model.id.clone(),
            name: model.id.clone(),
            description: Some(description),
            parameters: None,
            available: true,
            tags: vec![model.category.to_string()],
            downloads: None,
            updated_at: None,
        }
    }
}

#[async_trait]
impl LlmProvider for GeminiClient {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Gemini
    }
    
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        info!("Gemini models (static list)");
        info!("  Endpoint: {}", self.api_url);
        Ok(self.models.iter().map(|m| self.to_model_info(m)).collect())
    }
    
    async fn search_models(&self, query: &str, limit: usize) -> Result<Vec<ModelInfo>> {
        let query_lower = query.to_lowercase();
        Ok(self.models.iter()
            .filter(|m| m.id.to_lowercase().contains(&query_lower))
            .take(limit)
            .map(|m| self.to_model_info(m))
            .collect())
    }
    
    async fn get_model(&self, model_id: &str) -> Result<Option<ModelInfo>> {
        Ok(self.models.iter()
            .find(|m| m.id == model_id)
            .map(|m| self.to_model_info(m)))
    }
    
    async fn is_model_available(&self, model_id: &str) -> Result<bool> {
        Ok(self.models.iter().any(|m| m.id == model_id))
    }
    
    async fn chat(&self, model: &str, messages: Vec<ChatMessage>) -> Result<ChatResponse> {
        // Build URL: {api_url}/models/{model}:generateContent?key={api_key}
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.api_url, model, self.api_key
        );
        
        info!("Gemini chat: model={}, endpoint={}", model, self.api_url);
        
        let contents: Vec<GeminiContent> = messages.iter()
            .map(|m| GeminiContent {
                role: if m.role == "assistant" { "model".to_string() } else { m.role.clone() },
                parts: vec![GeminiPart { text: m.content.clone() }],
            })
            .collect();
        
        let request = GeminiRequest {
            contents,
            generation_config: Some(GenerationConfig {
                temperature: Some(0.7),
                top_p: Some(0.95),
                max_output_tokens: Some(2048),
            }),
        };
        
        debug!("Gemini request to: {}", url.split('?').next().unwrap_or(&url));
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send Gemini request")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Gemini API error {}: {}", status, body));
        }
        
        let result: GeminiResponse = response.json().await
            .context("Failed to parse Gemini response")?;
        
        let text = result.candidates.first()
            .and_then(|c| c.content.parts.first())
            .and_then(|p| p.text.clone())
            .unwrap_or_default();
        
        let finish_reason = result.candidates.first()
            .and_then(|c| c.finish_reason.clone());
        
        let usage = result.usage_metadata.map(|u| TokenUsage {
            prompt_tokens: u.prompt_token_count.unwrap_or(0),
            completion_tokens: u.candidates_token_count.unwrap_or(0),
            total_tokens: u.total_token_count.unwrap_or(0),
        });
        
        Ok(ChatResponse {
            message: ChatMessage {
                role: "assistant".to_string(),
                content: text,
                tool_calls: None,
                tool_call_id: None,
            },
            model: "gemini-pro".to_string(),
            provider: "gemini".to_string(),
            finish_reason,
            usage,
            tool_calls: None,
        })
    }
    
    async fn chat_stream(&self, model: &str, messages: Vec<ChatMessage>) -> Result<tokio::sync::mpsc::Receiver<Result<String>>> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let response = self.chat(model, messages).await?;
        tx.send(Ok(response.message.content)).await.ok();
        Ok(rx)
    }
}
