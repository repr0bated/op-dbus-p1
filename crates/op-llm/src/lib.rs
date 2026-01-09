//! op-llm: Multi-Provider LLM Integration
//!
//! ## Supported Providers & Endpoints
//!
//! | Provider | Base URL | Auth Method |
//! |----------|----------|-------------|
//! | HuggingFace | `https://api-inference.huggingface.co` | `Bearer {HF_TOKEN}` |
//! | Gemini | `https://generativelanguage.googleapis.com/v1beta` | `?key={API_KEY}` |
//! | Anthropic | `https://api.anthropic.com/v1` | `x-api-key: {KEY}` |
//! | Perplexity | `https://api.perplexity.ai` | `Bearer {KEY}` |
//!
//! ## Environment Variables
//!
//! ```bash
//! HF_TOKEN=hf_xxx              # HuggingFace
//! GEMINI_API_KEY=xxx           # Google Gemini  
//! ANTHROPIC_API_KEY=sk-xxx     # Anthropic Claude
//! PERPLEXITY_API_KEY=pplx-xxx  # Perplexity
//! ```

pub mod anthropic;
pub mod antigravity;
pub mod chat;
pub mod gemini;
pub mod huggingface;
pub mod perplexity;
pub mod provider;

pub use anthropic::AnthropicClient;
pub use antigravity::AntigravityProvider;
pub use gemini::GeminiClient;
pub use huggingface::HuggingFaceClient;
pub use perplexity::PerplexityClient;
pub use provider::{ChatMessage, ChatResponse, LlmProvider, ModelInfo, ProviderConfig, ProviderType};

/// Prelude for convenient imports
pub mod prelude {
    pub use super::anthropic::AnthropicClient;
    pub use super::gemini::GeminiClient;
    pub use super::huggingface::HuggingFaceClient;
    pub use super::perplexity::PerplexityClient;
    pub use super::provider::{ChatMessage, ChatResponse, LlmProvider, ModelInfo, ProviderConfig, ProviderType};
}
