//! LLM API Handlers

use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::str::FromStr;

use crate::state::AppState;
use op_llm::provider::ProviderType;

#[derive(Serialize)]
pub struct LlmStatusResponse {
    pub provider: String,
    pub model: String,
    pub available: bool,
}

#[derive(Serialize)]
pub struct LlmProvidersResponse {
    pub providers: Vec<String>,
    pub current: String,
}

/// GET /api/llm/status - Get LLM status
/// TEMPORARILY DISABLED: Providers are disabled
pub async fn llm_status_handler(
    State(state): State<Arc<AppState>>,
) -> Json<LlmStatusResponse> {
    // TEMPORARILY DISABLED: Return disabled status
    Json(LlmStatusResponse {
        provider: "disabled".to_string(),
        model: "disabled".to_string(),
        available: false,
    })
}

/// GET /api/llm/providers - List available providers
/// TEMPORARILY DISABLED: No providers available
pub async fn list_providers_handler(
    State(state): State<Arc<AppState>>,
) -> Json<LlmProvidersResponse> {
    // TEMPORARILY DISABLED: Return empty provider list
    Json(LlmProvidersResponse {
        providers: vec![],
        current: "disabled".to_string(),
    })
}

/// GET /api/llm/models - List available models
/// TEMPORARILY DISABLED: No models available
pub async fn list_models_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    // TEMPORARILY DISABLED: Return empty models list
    Json(json!({
        "models": [],
        "current": "disabled",
        "status": "LLM providers are temporarily disabled"
    }))
}

/// GET /api/llm/models/:provider - List models for a provider
/// TEMPORARILY DISABLED: No models available for any provider
pub async fn list_models_for_provider_handler(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Json<Value> {
    // TEMPORARILY DISABLED: Return empty models list for all providers
    Json(json!({
        "provider": provider,
        "models": [],
        "current": null,
        "status": "LLM providers are temporarily disabled"
    }))
}

#[derive(Debug, Deserialize)]
pub struct SwitchModelRequest {
    pub model: String,
}

/// POST /api/llm/model - Switch model
/// TEMPORARILY DISABLED: Model switching not allowed
pub async fn switch_model_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SwitchModelRequest>,
) -> Json<Value> {
    // TEMPORARILY DISABLED: Reject all model switches
    Json(json!({
        "success": false,
        "model": request.model,
        "note": "LLM providers are temporarily disabled"
    }))
}

#[derive(Debug, Deserialize)]
pub struct SwitchProviderRequest {
    pub provider: String,
}

/// POST /api/llm/provider - Switch provider
/// TEMPORARILY DISABLED: Provider switching not allowed
pub async fn switch_provider_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SwitchProviderRequest>,
) -> Json<Value> {
    // TEMPORARILY DISABLED: Reject all provider switches
    Json(json!({
        "success": false,
        "provider": request.provider,
        "note": "LLM providers are temporarily disabled"
    }))
}

const PERSISTED_MODEL_PATH: &str = "/etc/op-dbus/llm-model";
const PERSISTED_PROVIDER_PATH: &str = "/etc/op-dbus/llm-provider";

async fn persist_model(model: &str) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(PERSISTED_MODEL_PATH).parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("create dir: {}", e))?;
    }
    tokio::fs::write(PERSISTED_MODEL_PATH, format!("{model}\n"))
        .await
        .map_err(|e| format!("write model: {}", e))?;
    Ok(())
}

async fn persist_provider(provider: &str) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(PERSISTED_PROVIDER_PATH).parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("create dir: {}", e))?;
    }
    tokio::fs::write(PERSISTED_PROVIDER_PATH, format!("{provider}\n"))
        .await
        .map_err(|e| format!("write provider: {}", e))?;
    Ok(())
}
