//! Health Check Handler

use axum::{extract::State, response::Json};
use serde::Serialize;
use std::sync::Arc;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
}

/// GET /api/health - Health check endpoint
pub async fn health_handler(
    State(state): State<Arc<AppState>>,
) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: state.uptime_secs(),
    })
}
