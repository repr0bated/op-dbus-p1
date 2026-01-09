//! Chat Manager - Manages provider switching and chat sessions
//!
//! Streamlined for production use with essential providers only:
//! - Gemini 3 (Vertex AI) - Code Assist Enterprise
//! - Claude - Anthropic premium models
//! - OpenAI - GPT-4 and code models

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use async_trait::async_trait;
use crate::anthropic::AnthropicClient;
use crate::antigravity::AntigravityProvider;
use crate::gemini::GeminiClient;
use crate::provider::{
    BoxedProvider, ChatMessage, ChatRequest, ChatResponse, LlmProvider, ModelInfo, ProviderType,
};

/// Chat manager - handles multiple providers and model selection
///
/// Essential providers for production use:
/// 1. Gemini 3 (Vertex AI) - Default, included in Code Assist Enterprise
/// 2. Claude - Anthropic premium models
/// 3. OpenAI - GPT-4 and code models
pub struct ChatManager {
    providers: HashMap<ProviderType, BoxedProvider>,
    current_provider: Arc<RwLock<ProviderType>>,
    current_model: Arc<RwLock<String>>,
    model_cache: Arc<RwLock<HashMap<ProviderType, Vec<ModelInfo>>>>,
}

impl ChatManager {
    /// Create a new chat manager with essential providers only
    ///
    /// Initializes: Gemini (Vertex), Claude, OpenAI
    /// Respects LLM_PROVIDER environment variable for default provider selection
    pub fn new() -> Self {
        let mut providers: HashMap<ProviderType, BoxedProvider> = HashMap::new();
        let mut default_provider = None;
        let mut default_model = "gemini-2.5-flash".to_string(); // Auto-updates to latest 2.5 Flash

        // Check for LLM_PROVIDER and LLM_MODEL environment variables
        let env_provider = std::env::var("LLM_PROVIDER").ok();
        let env_model = std::env::var("LLM_MODEL").ok();

        let mut env_provider_type = None;
        if let Some(provider_name) = &env_provider {
            if let Ok(provider_type) = provider_name.parse::<ProviderType>() {
                env_provider_type = Some(provider_type);
                info!("📋 LLM_PROVIDER environment variable set: {}", provider_name);
            } else {
                warn!("⚠️  Invalid LLM_PROVIDER '{}', ignoring", provider_name);
            }
        }

        if let Some(model_name) = &env_model {
            info!("📋 LLM_MODEL environment variable set: {}", model_name);
            default_model = model_name.clone();
        }

        // =====================================================
        // Antigravity - IDE Bridge (PRIMARY - Enterprise Billing)
        // Code Assist Enterprise - ZERO API charges
        // =====================================================
        if let Ok(antigravity) = AntigravityProvider::from_env() {
            info!("✅ Antigravity provider initialized (IDE Bridge)");
            info!("   🏢 Code Assist Enterprise - ZERO charges");
            info!("   🔥 Models: claude-3-5-sonnet, gpt-4o");
            providers.insert(ProviderType::Antigravity, Box::new(antigravity));
            default_provider = Some(ProviderType::Antigravity);
            default_model = "claude-3-5-sonnet".to_string();
        } else {
            info!("⚠️  Antigravity provider failed to initialize - check GEMINI_API_KEY");
        }

        // =====================================================
        // Gemini 3 (Vertex AI) - FALLBACK
        // Code Assist Enterprise - included in subscription
        // =====================================================
        if default_provider.is_none() {
            if let Ok(gemini) = GeminiClient::from_env() {
                info!("✅ Gemini 3 provider initialized (Vertex AI)");
                info!("   🏢 Code Assist Enterprise");
                info!("   🔥 Model: gemini-2.5-flash (auto-updating)");
                providers.insert(ProviderType::Gemini, Box::new(gemini));
                default_provider = Some(ProviderType::Gemini);
                default_model = "gemini-2.5-flash".to_string();
            } else {
                debug!("Gemini provider not available (GEMINI_API_KEY not set)");
            }
        }

        // =====================================================
        // Claude (Anthropic) - Premium models
        // =====================================================
        if let Ok(anthropic) = AnthropicClient::from_env() {
            info!("✅ Claude provider initialized (Anthropic)");
            providers.insert(ProviderType::Anthropic, Box::new(anthropic));
        } else {
            debug!("Claude provider not available (ANTHROPIC_API_KEY not set)");
        }

        // TODO: Add OpenAI provider for GPT-4 models
        // Requires implementing openai.rs module in op-llm crate
        // Will provide access to GPT-4, GPT-4-turbo, etc.

        // Use environment provider if set and available, otherwise use auto-selected default
        let default_provider = if let Some(env_pt) = env_provider_type {
            if providers.contains_key(&env_pt) {
                info!("✅ Using LLM_PROVIDER environment variable: {:?}", env_pt);
                env_pt
            } else {
                warn!("⚠️  LLM_PROVIDER '{}' not available, falling back to auto-selected", env_provider.unwrap());
                default_provider.unwrap_or_else(|| {
                    providers
                        .keys()
                        .next()
                        .cloned()
                        .unwrap_or(ProviderType::Antigravity)
                })
            }
        } else {
            default_provider.unwrap_or_else(|| {
                providers
                    .keys()
                    .next()
                    .cloned()
                    .unwrap_or(ProviderType::Antigravity)
            })
        };

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

    /// Send chat message with custom credentials (for per-user authentication)
    pub async fn chat_with_credentials(
        &self,
        messages: Vec<ChatMessage>,
        gemini_key: Option<&str>,
        anthropic_key: Option<&str>,
        _openai_key: Option<&str>,
    ) -> Result<ChatResponse> {
        let provider_type = self.current_provider.read().await.clone();
        let model = self.current_model.read().await.clone();

        // Create a temporary provider with user credentials
        let provider = match provider_type {
            ProviderType::Gemini => {
                if let Some(key) = gemini_key {
                    // Temporarily set the API key for Gemini
                    let original_key = std::env::var("GEMINI_API_KEY").ok();
                    std::env::set_var("GEMINI_API_KEY", key);

                    let result = async {
                        match crate::gemini::GeminiClient::from_env() {
                            Ok(client) => {
                                let boxed: BoxedProvider = Box::new(client);
                                boxed.chat(&model, messages).await
                            }
                            Err(e) => Err(anyhow::anyhow!("Failed to create Gemini client: {}", e)),
                        }
                    }.await;

                    // Restore original key
                    if let Some(orig) = original_key {
                        std::env::set_var("GEMINI_API_KEY", orig);
                    } else {
                        std::env::remove_var("GEMINI_API_KEY");
                    }

                    return result;
                }
                // Fall back to regular provider
                self.providers
                    .get(&provider_type)
                    .ok_or_else(|| anyhow::anyhow!("Provider not available"))?
            }
            ProviderType::Anthropic => {
                if let Some(key) = anthropic_key {
                    let original_key = std::env::var("ANTHROPIC_API_KEY").ok();
                    std::env::set_var("ANTHROPIC_API_KEY", key);

                    let result = async {
                        match crate::anthropic::AnthropicClient::from_env() {
                            Ok(client) => {
                                let boxed: BoxedProvider = Box::new(client);
                                boxed.chat(&model, messages).await
                            }
                            Err(e) => Err(anyhow::anyhow!("Failed to create Anthropic client: {}", e)),
                        }
                    }.await;

                    if let Some(orig) = original_key {
                        std::env::set_var("ANTHROPIC_API_KEY", orig);
                    } else {
                        std::env::remove_var("ANTHROPIC_API_KEY");
                    }

                    return result;
                }
                self.providers
                    .get(&provider_type)
                    .ok_or_else(|| anyhow::anyhow!("Provider not available"))?
            }
            ProviderType::Antigravity => {
                if let Some(key) = gemini_key {
                    // Temporarily set the API key for Antigravity (uses Gemini API)
                    let original_key = std::env::var("GEMINI_API_KEY").ok();
                    std::env::set_var("GEMINI_API_KEY", key);

                    let result = async {
                        match AntigravityProvider::from_env() {
                            Ok(client) => {
                                let boxed: BoxedProvider = Box::new(client);
                                boxed.chat(&model, messages).await
                            }
                            Err(e) => Err(anyhow::anyhow!("Failed to create Antigravity client: {}", e)),
                        }
                    }.await;

                    // Restore original key
                    if let Some(orig) = original_key {
                        std::env::set_var("GEMINI_API_KEY", orig);
                    } else {
                        std::env::remove_var("GEMINI_API_KEY");
                    }

                    return result;
                }
                // Fall back to regular provider
                self.providers
                    .get(&provider_type)
                    .ok_or_else(|| anyhow::anyhow!("Provider not available"))?
            }
            _ => self.providers
                .get(&provider_type)
                .ok_or_else(|| anyhow::anyhow!("Provider not available"))?,
        };

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
                ProviderType::Antigravity => (
                    "Enterprise billing (covered by Code Assist)",
                    vec!["Claude 3.5 Sonnet", "GPT-4o", "Zero API charges"]
                ),
                ProviderType::Gemini => (
                    "Free tier available, then pay-per-use",
                    vec!["Gemini 2.5 Flash", "Multimodal", "Long context"]
                ),
                ProviderType::Perplexity => (
                    "~$5/1000 requests",
                    vec!["Online search", "Real-time data", "Citations"]
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
