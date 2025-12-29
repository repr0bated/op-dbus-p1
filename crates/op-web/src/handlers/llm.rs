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
pub async fn llm_status_handler(
    State(state): State<Arc<AppState>>,
) -> Json<LlmStatusResponse> {
    let provider = state.chat_manager.current_provider().await;
    let model = state.chat_manager.current_model().await;
    Json(LlmStatusResponse {
        provider: provider.to_string(),
        model,
        available: true,
    })
}

/// GET /api/llm/providers - List available providers
pub async fn list_providers_handler(
    State(state): State<Arc<AppState>>,
) -> Json<LlmProvidersResponse> {
    let current = state.chat_manager.current_provider().await;
    let providers = state
        .chat_manager
        .available_providers()
        .iter()
        .map(|p| p.to_string())
        .collect();

    Json(LlmProvidersResponse {
        providers,
        current: current.to_string(),
    })
}

/// GET /api/llm/models - List available models
pub async fn list_models_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    match state.chat_manager.list_models().await {
        Ok(models) => Json(json!({
            "models": models,
            "current": state.chat_manager.current_model().await
        })),
        Err(e) => Json(json!({
            "error": e.to_string()
        })),
    }
}

/// GET /api/llm/models/:provider - List models for a provider
pub async fn list_models_for_provider_handler(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Json<Value> {
    let provider_type = match ProviderType::from_str(&provider) {
        Ok(provider) => provider,
        Err(e) => {
            return Json(json!({
                "error": e
            }));
        }
    };

    match state.chat_manager.list_models_for_provider(&provider_type).await {
        Ok(models) => {
            let current_provider = state.chat_manager.current_provider().await;
            let current_model = if current_provider == provider_type {
                Some(state.chat_manager.current_model().await)
            } else {
                None
            };

            Json(json!({
                "provider": provider,
                "models": models,
                "current": current_model
            }))
        }
        Err(e) => Json(json!({
            "error": e.to_string()
        })),
    }
}

#[derive(Debug, Deserialize)]
pub struct SwitchModelRequest {
    pub model: String,
}

/// POST /api/llm/model - Switch model
pub async fn switch_model_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SwitchModelRequest>,
) -> Json<Value> {
    match state.chat_manager.switch_model(request.model.clone()).await {
        Ok(()) => {
            let mut note = None;
            if let Err(e) = persist_model(&request.model).await {
                note = Some(format!("Model switched but persistence failed: {}", e));
            }
            Json(json!({
                "success": true,
                "model": request.model,
                "note": note
            }))
        }
        Err(e) => Json(json!({
            "success": false,
            "model": request.model,
            "note": e.to_string()
        })),
    }
}

#[derive(Debug, Deserialize)]
pub struct SwitchProviderRequest {
    pub provider: String,
}

/// POST /api/llm/provider - Switch provider
pub async fn switch_provider_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SwitchProviderRequest>,
) -> Json<Value> {
    let provider = match ProviderType::from_str(&request.provider) {
        Ok(provider) => provider,
        Err(e) => {
            return Json(json!({
                "success": false,
                "provider": request.provider,
                "note": e
            }));
        }
    };

    match state.chat_manager.switch_provider(provider).await {
        Ok(()) => {
            let mut note = None;
            if let Err(e) = persist_provider(&request.provider).await {
                note = Some(format!("Provider switched but persistence failed: {}", e));
            }
            Json(json!({
                "success": true,
                "provider": request.provider,
                "note": note
            }))
        }
        Err(e) => Json(json!({
            "success": false,
            "provider": request.provider,
            "note": e.to_string()
        })),
    }
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
