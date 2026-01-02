//! API client for op-web backend

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

/// Chat request
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub message: String,
    pub session_id: Option<String>,
    pub model: Option<String>,
}

/// Chat response
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub success: bool,
    pub message: Option<String>,
    pub error: Option<String>,
    pub tools_executed: Vec<String>,
    pub session_id: String,
    pub model: String,
    pub provider: String,
}

/// Tool definition
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

/// Tool execution request
#[derive(Debug, Clone, Serialize)]
pub struct ToolExecutionRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// Tool execution response - must be Clone for Leptos signals
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolExecutionResponse {
    pub success: bool,
    pub tool_name: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

/// Tool list response
#[derive(Debug, Clone, Deserialize)]
pub struct ToolListResponse {
    pub tools: Vec<ToolDefinition>,
    pub count: usize,
}

/// Health check response
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: Option<String>,
}

/// LLM status response
#[derive(Debug, Clone, Deserialize)]
pub struct LlmStatusResponse {
    pub provider: String,
    pub model: String,
    pub available: bool,
}

/// LLM providers response
#[derive(Debug, Clone, Deserialize)]
pub struct LlmProvidersResponse {
    pub providers: Vec<String>,
    pub current: String,
}

/// LLM model info
#[derive(Debug, Clone, Deserialize)]
pub struct LlmModelInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// LLM models response
#[derive(Debug, Clone, Deserialize)]
pub struct LlmModelsResponse {
    #[serde(default)]
    pub models: Option<Vec<LlmModelInfo>>,
    #[serde(default)]
    pub current: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

/// LLM switch response
#[derive(Debug, Clone, Deserialize)]
pub struct LlmSwitchResponse {
    pub success: bool,
    pub model: String,
    pub note: Option<String>,
}

/// Privacy signup request
#[derive(Debug, Clone, Serialize)]
pub struct PrivacySignupRequest {
    pub email: String,
}

/// Privacy signup response
#[derive(Debug, Clone, Deserialize)]
pub struct PrivacySignupResponse {
    pub success: bool,
    pub message: String,
}

/// Privacy verify response
#[derive(Debug, Clone, Deserialize)]
pub struct PrivacyVerifyResponse {
    pub success: bool,
    pub user_id: Option<String>,
    pub config: Option<String>,
    pub qr_code: Option<String>,
    pub message: String,
}

/// Privacy status response
#[derive(Debug, Clone, Deserialize)]
pub struct PrivacyStatusResponse {
    pub available: bool,
    pub server_public_key: Option<String>,
    pub endpoint: Option<String>,
    pub registered_users: usize,
}

/// API client
pub struct ApiClient {
    base_url: String,
}

impl ApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
        }
    }

    pub fn default() -> Self {
        // Use relative URL for same-origin requests
        Self::new("")
    }

    /// Send a chat message
    pub async fn chat(
        &self,
        message: &str,
        session_id: Option<&str>,
        model: Option<&str>,
    ) -> Result<ChatResponse, String> {
        let request = ChatRequest {
            message: message.to_string(),
            session_id: session_id.map(String::from),
            model: model.map(String::from),
        };

        let response = Request::post(&format!("{}/api/chat", self.base_url))
            .header("Content-Type", "application/json")
            .json(&request)
            .map_err(|e| format!("Failed to serialize request: {}", e))?
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<ChatResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// List available tools
    pub async fn list_tools(&self) -> Result<ToolListResponse, String> {
        let response = Request::get(&format!("{}/api/tools", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<ToolListResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Execute a tool
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolExecutionResponse, String> {
        let request = ToolExecutionRequest {
            tool_name: tool_name.to_string(),
            arguments,
        };

        let response = Request::post(&format!("{}/api/tool", self.base_url))
            .header("Content-Type", "application/json")
            .json(&request)
            .map_err(|e| format!("Failed to serialize request: {}", e))?
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<ToolExecutionResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Health check
    pub async fn health(&self) -> Result<HealthResponse, String> {
        let response = Request::get(&format!("{}/api/health", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<HealthResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// LLM status
    pub async fn llm_status(&self) -> Result<LlmStatusResponse, String> {
        let response = Request::get(&format!("{}/api/llm/status", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<LlmStatusResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// List LLM models
    pub async fn llm_models(&self) -> Result<LlmModelsResponse, String> {
        let response = Request::get(&format!("{}/api/llm/models", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        let parsed = response
            .json::<LlmModelsResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(error) = parsed.error.clone() {
            return Err(error);
        }

        Ok(parsed)
    }

    /// List LLM models for a provider
    pub async fn llm_models_for_provider(&self, provider: &str) -> Result<LlmModelsResponse, String> {
        let response = Request::get(&format!("{}/api/llm/models/{}", self.base_url, provider))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        let parsed = response
            .json::<LlmModelsResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(error) = parsed.error.clone() {
            return Err(error);
        }

        Ok(parsed)
    }

    /// Switch LLM model
    pub async fn switch_model(&self, model: &str) -> Result<LlmSwitchResponse, String> {
        let response = Request::post(&format!("{}/api/llm/model", self.base_url))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "model": model }))
            .map_err(|e| format!("Failed to serialize request: {}", e))?
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<LlmSwitchResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// List LLM providers
    pub async fn llm_providers(&self) -> Result<LlmProvidersResponse, String> {
        let response = Request::get(&format!("{}/api/llm/providers", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<LlmProvidersResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Switch LLM provider
    pub async fn switch_provider(&self, provider: &str) -> Result<LlmSwitchResponse, String> {
        let response = Request::post(&format!("{}/api/llm/provider", self.base_url))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "provider": provider }))
            .map_err(|e| format!("Failed to serialize request: {}", e))?
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<LlmSwitchResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Privacy router signup
    pub async fn privacy_signup(&self, email: &str) -> Result<PrivacySignupResponse, String> {
        let request = PrivacySignupRequest {
            email: email.to_string(),
        };

        let response = Request::post(&format!("{}/api/privacy/signup", self.base_url))
            .header("Content-Type", "application/json")
            .json(&request)
            .map_err(|e| format!("Failed to serialize request: {}", e))?
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<PrivacySignupResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Privacy router verify magic link
    pub async fn privacy_verify(&self, token: &str) -> Result<PrivacyVerifyResponse, String> {
        let response = Request::get(&format!("{}/api/privacy/verify?token={}", self.base_url, token))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<PrivacyVerifyResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Privacy router status
    pub async fn privacy_status(&self) -> Result<PrivacyStatusResponse, String> {
        let response = Request::get(&format!("{}/api/privacy/status", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<PrivacyStatusResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }
}
