//! API client for op-web backend

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

/// Chat request
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub message: String,
    pub session_id: Option<String>,
}

/// Chat response
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub session_id: String,
    pub message: String,
}

/// Tool definition
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub category: Option<String>,
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
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Health check response
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: Option<String>,
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
    pub async fn chat(&self, message: &str, session_id: Option<&str>) -> Result<ChatResponse, String> {
        let request = ChatRequest {
            message: message.to_string(),
            session_id: session_id.map(String::from),
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
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>, String> {
        let response = Request::get(&format!("{}/api/tools", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .json::<Vec<ToolDefinition>>()
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

        let response = Request::post(&format!("{}/api/tools/execute", self.base_url))
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
}
