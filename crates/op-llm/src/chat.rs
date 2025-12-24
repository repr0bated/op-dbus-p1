//! Chat Manager - Manages provider switching and chat sessions
//!
//! Supports multiple LLM providers with HuggingFace Inference API as the
//! cost-effective default (~$10 for 20,000 messages).

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use async_trait::async_trait;
use crate::anthropic::AnthropicClient;
use crate::gemini::GeminiClient;
use crate::huggingface::HuggingFaceClient;
use crate::ollama::OllamaCloudClient;
use crate::perplexity::PerplexityClient;
use crate::provider::{
    BoxedProvider, ChatMessage, ChatRequest, ChatResponse, LlmProvider, ModelInfo, ProviderType,
};

/// Chat manager - handles multiple providers and model selection
/// 
/// Provider priority (by cost-effectiveness):
/// 1. HuggingFace Inference API - $10/20k messages, supports 236B models
/// 2. Ollama - Free (local) or cloud pricing
/// 3. Gemini - Free tier available
/// 4. Perplexity - ~$5/1000 requests (with search)
/// 5. Anthropic - Premium pricing but highest quality
pub struct ChatManager {
    providers: HashMap<ProviderType, BoxedProvider>,
    current_provider: Arc<RwLock<ProviderType>>,
    current_model: Arc<RwLock<String>>,
    model_cache: Arc<RwLock<HashMap<ProviderType, Vec<ModelInfo>>>>,
}

impl ChatManager {
    /// Create a new chat manager with available providers
    /// 
    /// Initializes providers in order of cost-effectiveness:
    /// - HuggingFace first (best value at $10/20k messages)
    /// - Then other providers based on API key availability
    pub fn new() -> Self {
        let mut providers: HashMap<ProviderType, BoxedProvider> = HashMap::new();
        let mut default_provider = None;
        let mut default_model = "llama3.2".to_string();

        // =====================================================
        // HuggingFace FIRST - Best value: $10 for 20,000 messages
        // Supports models up to 405B parameters!
        // =====================================================
        if let Ok(hf) = HuggingFaceClient::from_env() {
            info!("✅ HuggingFace provider initialized");
            info!("   💰 Cost: ~$10 for 20,000 messages (Inference API)");
            info!("   🚀 Supports: 1B to 405B parameter models");
            info!("   ⭐ Recommended: DeepSeek V2.5 (236B) for best quality");
            providers.insert(ProviderType::HuggingFace, Box::new(hf));
            if default_provider.is_none() {
                default_provider = Some(ProviderType::HuggingFace);
                // Default to DeepSeek V2.5 (236B) - the flagship model
                default_model = "deepseek-ai/DeepSeek-V2.5".to_string();
            }
        } else {
            debug!("HuggingFace provider not available (HF_TOKEN not set)");
        }

        // =====================================================
        // Ollama - Free (local) or cloud pricing
        // =====================================================
        let ollama = OllamaCloudClient::from_env();
        info!("✅ Ollama provider initialized (local or cloud)");
        providers.insert(ProviderType::Ollama, Box::new(ollama));
        if default_provider.is_none() {
            default_provider = Some(ProviderType::Ollama);
            default_model = "llama3.2".to_string();
        }

        // =====================================================
        // Gemini - Free tier available
        // =====================================================
        if let Ok(gemini) = GeminiClient::from_env() {
            info!("✅ Gemini provider initialized (free tier available)");
            providers.insert(ProviderType::Gemini, Box::new(gemini));
            if default_provider.is_none() {
                default_provider = Some(ProviderType::Gemini);
                default_model = "gemini-2.5-flash".to_string();
            }
        } else {
            debug!("Gemini provider not available (GEMINI_API_KEY not set)");
        }

        // =====================================================
        // Perplexity - ~$5/1000 requests (includes search)
        // =====================================================
        if let Ok(perplexity) = PerplexityClient::from_env() {
            info!("✅ Perplexity provider initialized (with search capability)");
            providers.insert(ProviderType::Perplexity, Box::new(perplexity));
            if default_provider.is_none() {
                default_provider = Some(ProviderType::Perplexity);
                default_model = "sonar".to_string();
            }
        } else {
            debug!("Perplexity provider not available (PERPLEXITY_API_KEY not set)");
        }

        // =====================================================
        // Anthropic - Premium pricing, highest quality
        // =====================================================
        if let Ok(anthropic) = AnthropicClient::from_env() {
            info!("✅ Anthropic provider initialized (premium quality)");
            providers.insert(ProviderType::Anthropic, Box::new(anthropic));
            // Don't set as default - it's the most expensive
        } else {
            debug!("Anthropic provider not available (ANTHROPIC_API_KEY not set)");
        }

        let default_provider = default_provider.unwrap_or(ProviderType::Ollama);
        info!("\n📊 Default provider: {:?}", default_provider);
        info!("📊 Default model: {}", default_model);
        info!("📊 Total providers available: {}\n", providers.len());

        Self {
            providers,
            current_provider: Arc::new(RwLock::new(default_provider)),
            current_model: Arc::new(RwLock::new(default_model)),
            model_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a provider
    pub fn add_provider(&mut self, provider: BoxedProvider) {
        let provider_type = provider.provider_type();
        self.providers.insert(provider_type, provider);
    }

    /// Get current provider type
    pub async fn current_provider(&self) -> ProviderType {
        self.current_provider.read().await.clone()
    }

    /// Get current model
    pub async fn current_model(&self) -> String {
        self.current_model.read().await.clone()
    }

    /// Switch provider
    pub async fn switch_provider(&self, provider_type: ProviderType) -> Result<()> {
        if !self.providers.contains_key(&provider_type) {
            return Err(anyhow::anyhow!(
                "Provider {:?} not available. Available: {:?}",
                provider_type,
                self.available_providers()
            ));
        }

        *self.current_provider.write().await = provider_type.clone();
        info!("Switched to provider: {:?}", provider_type);

        // Get first available model for this provider
        let models = self.list_models().await?;
        if let Some(first) = models.first() {
            *self.current_model.write().await = first.id.clone();
            info!("Default model set to: {}", first.id);
        }

        Ok(())
    }

    /// Switch model
    pub async fn switch_model(&self, model_id: impl Into<String>) -> Result<()> {
        let model_id = model_id.into();
        *self.current_model.write().await = model_id.clone();
        info!("Switched to model: {}", model_id);
        Ok(())
    }

    /// List available providers
    pub fn available_providers(&self) -> Vec<ProviderType> {
        self.providers.keys().cloned().collect()
    }

    /// Check if a provider is available
    pub fn has_provider(&self, provider_type: &ProviderType) -> bool {
        self.providers.contains_key(provider_type)
    }

    /// Dynamically fetch models from current provider
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let provider_type = self.current_provider.read().await.clone();

        // Check cache first
        {
            let cache = self.model_cache.read().await;
            if let Some(models) = cache.get(&provider_type) {
                debug!("Returning cached models for {:?}", provider_type);
                return Ok(models.clone());
            }
        }

        // Fetch from provider
        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| anyhow::anyhow!("Provider not available"))?;

        let models = provider.list_models().await?;

        // Cache the results
        {
            let mut cache = self.model_cache.write().await;
            cache.insert(provider_type, models.clone());
        }

        Ok(models)
    }

    /// List models for a specific provider
    pub async fn list_models_for_provider(&self, provider_type: &ProviderType) -> Result<Vec<ModelInfo>> {
        // Check cache first
        {
            let cache = self.model_cache.read().await;
            if let Some(models) = cache.get(provider_type) {
                return Ok(models.clone());
            }
        }

        let provider = self
            .providers
            .get(provider_type)
            .ok_or_else(|| anyhow::anyhow!("Provider {:?} not available", provider_type))?;

        let models = provider.list_models().await?;

        // Cache the results
        {
            let mut cache = self.model_cache.write().await;
            cache.insert(provider_type.clone(), models.clone());
        }

        Ok(models)
    }

    /// Search models
    pub async fn search_models(&self, query: &str) -> Result<Vec<ModelInfo>> {
        let provider_type = self.current_provider.read().await.clone();
        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| anyhow::anyhow!("Provider not available"))?;

        provider.search_models(query, 20).await
    }

    /// Clear model cache (force refresh)
    pub async fn refresh_models(&self) -> Result<Vec<ModelInfo>> {
        let provider_type = self.current_provider.read().await.clone();

        {
            let mut cache = self.model_cache.write().await;
            cache.remove(&provider_type);
        }

        self.list_models().await
    }

    /// Send chat message
    pub async fn chat(&self, messages: Vec<ChatMessage>) -> Result<ChatResponse> {
        let provider_type = self.current_provider.read().await.clone();
        let model = self.current_model.read().await.clone();

        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| anyhow::anyhow!("Provider not available"))?;

        provider.chat(&model, messages).await
    }

    /// Send chat message with specific provider and model
    pub async fn chat_with(
        &self,
        provider_type: &ProviderType,
        model: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<ChatResponse> {
        let provider = self
            .providers
            .get(provider_type)
            .ok_or_else(|| anyhow::anyhow!("Provider {:?} not available", provider_type))?;

        provider.chat(model, messages).await
    }

    /// Get status info for API response
    pub async fn get_status(&self) -> serde_json::Value {
        let provider = self.current_provider.read().await.clone();
        let model = self.current_model.read().await.clone();
        let providers: Vec<String> = self
            .available_providers()
            .iter()
            .map(|p| p.to_string())
            .collect();

        serde_json::json!({
            "provider": provider.to_string(),
            "model": model,
            "available_providers": providers,
        })
    }

    /// Get detailed status with all provider info including pricing
    pub async fn get_detailed_status(&self) -> serde_json::Value {
        let current_provider = self.current_provider.read().await.clone();
        let current_model = self.current_model.read().await.clone();
        
        let mut provider_status = serde_json::Map::new();
        
        for ptype in self.providers.keys() {
            let models = self.list_models_for_provider(ptype).await.ok();
            let (cost_info, features) = match ptype {
                ProviderType::HuggingFace => (
                    "~$10 for 20,000 messages (Inference API)",
                    vec!["236B DeepSeek V2.5", "405B Llama 3.1", "Code models"]
                ),
                ProviderType::Anthropic => (
                    "~$3/1M input, $15/1M output tokens",
                    vec!["Claude Sonnet 4", "Best reasoning", "Tool use"]
                ),
                ProviderType::Gemini => (
                    "Free tier available, then pay-per-use",
                    vec!["Gemini 2.5 Flash", "Multimodal", "Long context"]
                ),
                ProviderType::Perplexity => (
                    "~$5/1000 requests",
                    vec!["Online search", "Real-time data", "Citations"]
                ),
                ProviderType::Ollama => (
                    "Free (local) or cloud pricing",
                    vec!["Local inference", "No API limits", "Privacy"]
                ),
                ProviderType::OpenAI => (
                    "Pay-per-token",
                    vec!["GPT-4o", "O1", "GPT-4 Turbo"]
                ),
            };
            
            provider_status.insert(
                ptype.to_string(),
                serde_json::json!({
                    "available": true,
                    "model_count": models.as_ref().map(|m| m.len()).unwrap_or(0),
                    "cost_info": cost_info,
                    "features": features,
                })
            );
        }

        serde_json::json!({
            "current_provider": current_provider.to_string(),
            "current_model": current_model,
            "providers": provider_status,
            "recommendation": "HuggingFace with DeepSeek V2.5 (236B) for best value"
        })
    }
}

#[async_trait]
impl LlmProvider for ChatManager {
    fn provider_type(&self) -> ProviderType {
        self.current_provider.blocking_read().clone()
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        ChatManager::list_models(self).await
    }

    async fn search_models(&self, query: &str, limit: usize) -> Result<Vec<ModelInfo>> {
        let _limit = limit;
        ChatManager::search_models(self, query).await
    }

    async fn get_model(&self, model_id: &str) -> Result<Option<ModelInfo>> {
        ChatManager::get_model(self, model_id).await
    }

    async fn is_model_available(&self, model_id: &str) -> Result<bool> {
        ChatManager::is_model_available(self, model_id).await
    }

    async fn chat(&self, model: &str, messages: Vec<ChatMessage>) -> Result<ChatResponse> {
        let provider_type = self.current_provider.read().await.clone();
        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| anyhow!("Provider {:?} not available", provider_type))?;

        provider.chat(model, messages).await
    }

    async fn chat_with_request(&self, model: &str, request: ChatRequest) -> Result<ChatResponse> {
        let provider_type = self.current_provider.read().await.clone();
        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| anyhow!("Provider {:?} not available", provider_type))?;

        provider.chat_with_request(model, request).await
    }

    async fn chat_stream(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<String>>> {
        let provider_type = self.current_provider.read().await.clone();
        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| anyhow!("Provider {:?} not available", provider_type))?;

        provider.chat_stream(model, messages).await
    }
}

impl Default for ChatManager {
    fn default() -> Self {
        Self::new()
    }
}
