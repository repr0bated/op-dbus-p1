//! LLM API Handlers

use axum::{
    extract::State,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::state::AppState;

#[derive(Serialize)]
pub struct LlmStatusResponse {
    pub provider: String,
    pub model: String,
    pub available: bool,
}

/// GET /api/llm/status - Get LLM status
pub async fn llm_status_handler(
    State(state): State<Arc<AppState>>,
) -> Json<LlmStatusResponse> {
    Json(LlmStatusResponse {
        provider: state.provider_name.clone(),
        model: state.default_model.clone(),
        available: true,
    })
}

/// GET /api/llm/models - List available models
pub async fn list_models_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    match state.chat_manager.list_models().await {
        Ok(models) => Json(json!({
            "models": models,
            "current": state.default_model
        })),
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
    State(_state): State<Arc<AppState>>,
    Json(request): Json<SwitchModelRequest>,
) -> Json<Value> {
    // Note: This would need to update the state, which requires interior mutability
    // For now, just acknowledge the request
    Json(json!({
        "success": true,
        "model": request.model,
        "note": "Model switch requested (may require restart)"
    }))
}
