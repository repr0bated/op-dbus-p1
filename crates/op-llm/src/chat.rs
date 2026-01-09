//! Chat Manager - Manages provider switching and chat sessions
//!
//! ## Authentication Priority
//!
//! 1. **Antigravity** (OAuth from headless service) - Enterprise, no charges
//! 2. **Gemini** (API key fallback)
//! 3. **Anthropic** (API key)
//!
//! ## Environment Variables
//!
//! ```bash
//! # Primary: Headless OAuth (captured from Antigravity IDE)
//! GOOGLE_AUTH_TOKEN_FILE=~/.config/antigravity/token.json
//!
//! # Fallback: API keys
//! GEMINI_API_KEY=xxx
//! ANTHROPIC_API_KEY=xxx
//!
//! # Provider selection
//! LLM_PROVIDER=antigravity  # or gemini, anthropic
//! LLM_MODEL=gemini-2.5-flash
//! ```

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
pub struct ChatManager {
    providers: HashMap<ProviderType, BoxedProvider>,
    current_provider: Arc<RwLock<ProviderType>>,
    current_model: Arc<RwLock<String>>,
    model_cache: Arc<RwLock<HashMap<ProviderType, Vec<ModelInfo>>>>,
}

impl ChatManager {
    /// Create a new chat manager
    ///
    /// Initialization order:
    /// 1. Check LLM_PROVIDER environment variable
    /// 2. Try Antigravity (OAuth from headless service)
    /// 3. Try Gemini (API key)
    /// 4. Try Anthropic (API key)
    pub fn new() -> Self {
        let mut providers: HashMap<ProviderType, BoxedProvider> = HashMap::new();
        let mut default_provider = None;
        let mut default_model = "gemini-2.5-flash".to_string();

        // Check environment variables
        let env_provider = std::env::var("LLM_PROVIDER").ok();
        let env_model = std::env::var("LLM_MODEL").ok();

        if let Some(ref provider_name) = env_provider {
            info!("📋 LLM_PROVIDER={}", provider_name);
        }
        if let Some(ref model_name) = env_model {
            info!("📋 LLM_MODEL={}", model_name);
            default_model = model_name.clone();
        }

        // =====================================================
        // Antigravity - OAuth from headless service (PRIMARY)
        // Uses token captured when user logs into Antigravity via VNC
        // Enterprise Code Assist - ZERO API charges
        // =====================================================
        match AntigravityProvider::from_env() {
            Ok(antigravity) => {
                info!("✅ Antigravity provider initialized");
                info!("   🏢 Uses OAuth token from headless Antigravity service");
                providers.insert(ProviderType::Antigravity, Box::new(antigravity));
                if default_provider.is_none() {
                    default_provider = Some(ProviderType::Antigravity);
                }
            }
            Err(e) => {
                debug!("Antigravity provider not available: {}", e);
            }
        }

        // =====================================================
        // Gemini - API key fallback
        // =====================================================
        if std::env::var("GEMINI_API_KEY").is_ok() {
            match GeminiClient::from_env() {
                Ok(gemini) => {
                    info!("✅ Gemini provider initialized (API key)");
                    providers.insert(ProviderType::Gemini, Box::new(gemini));
                    if default_provider.is_none() {
                        default_provider = Some(ProviderType::Gemini);
                    }
                }
                Err(e) => {
                    debug!("Gemini provider failed: {}", e);
                }
            }
        }

        // =====================================================
        // Anthropic - API key
        // =====================================================
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            match AnthropicClient::from_env() {
                Ok(anthropic) => {
                    info!("✅ Anthropic provider initialized");
                    providers.insert(ProviderType::Anthropic, Box::new(anthropic));
                    if default_provider.is_none() {
                        default_provider = Some(ProviderType::Anthropic);
                    }
                }
                Err(e) => {
                    debug!("Anthropic provider failed: {}", e);
                }
            }
        }

        // Use environment provider if specified and available
        let final_provider = if let Some(ref provider_name) = env_provider {
            if let Ok(pt) = provider_name.parse::<ProviderType>() {
                if providers.contains_key(&pt) {
                    info!("✅ Using LLM_PROVIDER: {:?}", pt);
                    pt
                } else {
                    warn!("⚠️  LLM_PROVIDER '{}' not available", provider_name);
                    default_provider.unwrap_or(ProviderType::Antigravity)
                }
            } else {
                warn!("⚠️  Invalid LLM_PROVIDER '{}'", provider_name);
                default_provider.unwrap_or(ProviderType::Antigravity)
            }
        } else {
            default_provider.unwrap_or(ProviderType::Antigravity)
        };

        if providers.is_empty() {
            warn!("⚠️  No LLM providers available!");
            warn!("   Configure authentication:");
            warn!("   1. Antigravity headless: sudo systemctl start antigravity-display");
            warn!("   2. Or set GEMINI_API_KEY environment variable");
        } else {
            info!("\n📊 Default provider: {:?}", final_provider);
            info!("📊 Default model: {}", default_model);
            info!("📊 Available providers: {}\n", providers.len());
        }

        Self {
            providers,
            current_provider: Arc::new(RwLock::new(final_provider)),
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

    async fn resolve_provider(&self) -> Result<ProviderType> {
        let current = self.current_provider.read().await.clone();
        if self.providers.contains_key(&current) {
            return Ok(current);
        }

        if let Some(fallback) = self.providers.keys().next().cloned() {
            warn!(
                "Provider {:?} not available, falling back to {:?}",
                current, fallback
            );
            *self.current_provider.write().await = fallback.clone();
            return Ok(fallback);
        }

        Err(anyhow!(
            "No LLM providers configured.\n\n\
            To authenticate:\n\
            1. Start Antigravity headless: sudo systemctl start antigravity-display antigravity-vnc\n\
            2. Connect via VNC: vncviewer localhost:5900\n\
            3. Log in with Google account\n\
            4. Extract token: ./scripts/antigravity-extract-token.sh\n\n\
            Or set GEMINI_API_KEY environment variable."
        ))
    }

    /// List models from current provider
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let provider_type = self.current_provider.read().await.clone();

        // Check cache first
        {
            let cache = self.model_cache.read().await;
            if let Some(models) = cache.get(&provider_type) {
                return Ok(models.clone());
            }
        }

        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| anyhow!("Provider not available"))?;

        let models = provider.list_models().await?;

        // Cache
        {
            let mut cache = self.model_cache.write().await;
            cache.insert(provider_type, models.clone());
        }

        Ok(models)
    }

    /// List models for a specific provider
    pub async fn list_models_for_provider(&self, provider_type: &ProviderType) -> Result<Vec<ModelInfo>> {
        let provider = self
            .providers
            .get(provider_type)
            .ok_or_else(|| anyhow!("Provider {:?} not available", provider_type))?;

        provider.list_models().await
    }

    /// Search models
    pub async fn search_models(&self, query: &str) -> Result<Vec<ModelInfo>> {
        let provider_type = self.current_provider.read().await.clone();
        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| anyhow!("Provider not available"))?;

        provider.search_models(query, 20).await
    }

    /// Refresh models (clear cache)
    pub async fn refresh_models(&self) -> Result<Vec<ModelInfo>> {
        let provider_type = self.current_provider.read().await.clone();
        {
            let mut cache = self.model_cache.write().await;
            cache.remove(&provider_type);
        }
        self.list_models().await
    }

    /// Get model info
    pub async fn get_model(&self, model_id: &str) -> Result<Option<ModelInfo>> {
        let provider_type = self.current_provider.read().await.clone();
        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| anyhow!("Provider not available"))?;

        provider.get_model(model_id).await
    }

    /// Check if model is available
    pub async fn is_model_available(&self, model_id: &str) -> Result<bool> {
        let provider_type = self.current_provider.read().await.clone();
        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| anyhow!("Provider not available"))?;

        provider.is_model_available(model_id).await
    }

    /// Send chat message
    pub async fn chat(&self, messages: Vec<ChatMessage>) -> Result<ChatResponse> {
        let provider_type = self.resolve_provider().await?;
        let model = self.current_model.read().await.clone();

        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| anyhow!("Provider not available"))?;

        provider.chat(&model, messages).await
    }

    /// Send chat with specific provider and model
    pub async fn chat_with(
        &self,
        provider_type: &ProviderType,
        model: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<ChatResponse> {
        let provider = self
            .providers
            .get(provider_type)
            .ok_or_else(|| anyhow!("Provider {:?} not available", provider_type))?;

        provider.chat(model, messages).await
    }

    /// Get status
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

    /// Get detailed status
    pub async fn get_detailed_status(&self) -> serde_json::Value {
        let current_provider = self.current_provider.read().await.clone();
        let current_model = self.current_model.read().await.clone();
        
        let mut provider_status = serde_json::Map::new();
        
        for ptype in self.providers.keys() {
            let models = self.list_models_for_provider(ptype).await.ok();
            let (auth_type, features) = match ptype {
                ProviderType::Antigravity => (
                    "OAuth (headless Antigravity service)",
                    vec!["Enterprise billing", "Gemini models", "No API charges"]
                ),
                ProviderType::Gemini => (
                    "API key (GEMINI_API_KEY)",
                    vec!["Gemini models", "Multimodal", "Long context"]
                ),
                ProviderType::Anthropic => (
                    "API key (ANTHROPIC_API_KEY)",
                    vec!["Claude models", "Best reasoning", "Tool use"]
                ),
                _ => (
                    "API key",
                    vec![]
                ),
            };
            
            provider_status.insert(
                ptype.to_string(),
                serde_json::json!({
                    "available": true,
                    "model_count": models.as_ref().map(|m| m.len()).unwrap_or(0),
                    "auth_type": auth_type,
                    "features": features,
                })
            );
        }

        serde_json::json!({
            "current_provider": current_provider.to_string(),
            "current_model": current_model,
            "providers": provider_status,
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
        let provider_type = self.resolve_provider().await?;
        let provider = self.providers.get(&provider_type).ok_or_else(|| {
            anyhow!("Provider {:?} not available", provider_type)
        })?;

        provider.chat(model, messages).await
    }

    async fn chat_with_request(&self, model: &str, request: ChatRequest) -> Result<ChatResponse> {
        let provider_type = self.resolve_provider().await?;
        let provider = self.providers.get(&provider_type).ok_or_else(|| {
            anyhow!("Provider {:?} not available", provider_type)
        })?;

        provider.chat_with_request(model, request).await
    }

    async fn chat_stream(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<String>>> {
        let provider_type = self.resolve_provider().await?;
        let provider = self.providers.get(&provider_type).ok_or_else(|| {
            anyhow!("Provider {:?} not available", provider_type)
        })?;

        provider.chat_stream(model, messages).await
    }
}

impl Default for ChatManager {
    fn default() -> Self {
        Self::new()
    }
}
