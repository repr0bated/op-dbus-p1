//! Chat API Handlers

use axum::{
    extract::{Path, State},
    response::{Json, sse::{Event, Sse}},
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::convert::Infallible;
use std::time::Duration;
use tracing::{info, error};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub success: bool,
    pub message: Option<String>,
    pub error: Option<String>,
    pub tools_executed: Vec<String>,
    pub session_id: String,
    pub model: String,
    pub provider: String,
}

/// POST /api/chat - Main chat endpoint
pub async fn chat_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatRequest>,
) -> Json<ChatResponse> {
    info!("Chat request: {} chars", request.message.len());
    if let Some(model) = request.model.as_ref() {
        if let Err(e) = state.chat_manager.switch_model(model.clone()).await {
            error!("Model switch failed: {}", e);
        }
    }

    let session_id = request
        .session_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    match state
        .orchestrator
        .process(&session_id, &request.message)
        .await
    {
        Ok(result) => {
            let provider = state.chat_manager.current_provider().await;
            let model = state.chat_manager.current_model().await;
            Json(ChatResponse {
                success: result.success,
                message: Some(result.message),
                error: None,
                tools_executed: result.tools_executed,
                session_id,
            model,
            provider: provider.to_string(),
        })
        }
        Err(e) => {
            error!("Chat processing failed: {}", e);
            let provider = state.chat_manager.current_provider().await;
            let model = state.chat_manager.current_model().await;
            Json(ChatResponse {
                success: false,
                message: None,
                error: Some(e.to_string()),
                tools_executed: vec![],
                session_id,
                model,
                provider: provider.to_string(),
            })
        }
    }
}

/// POST /api/chat/stream - Streaming chat endpoint (SSE)
pub async fn chat_stream_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let _requested_model = request.model.clone();
    let session_id = request
        .session_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let orchestrator = state.orchestrator.clone();
    let message = request.message.clone();

    let stream = stream::unfold(
        (orchestrator, session_id, message, false),
        |(orch, sid, msg, done)| async move {
            if done {
                return None;
            }

            match orch.process(&sid, &msg).await {
                Ok(result) => {
                    let event = Event::default()
                        .data(serde_json::to_string(&result).unwrap_or_default());
                    Some((Ok(event), (orch, sid, msg, true)))
                }
                Err(e) => {
                    let event = Event::default()
                        .data(json!({"error": e.to_string()}).to_string());
                    Some((Ok(event), (orch, sid, msg, true)))
                }
            }
        },
    );

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// GET /api/chat/history/:session_id - Get conversation history
pub async fn get_history_handler(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Json<Value> {
    let conversations = state.conversations.read().await;
    
    if let Some(history) = conversations.get(&session_id) {
        Json(json!({
            "session_id": session_id,
            "messages": history.iter().map(|m| json!({
                "role": m.role,
                "content": m.content
            })).collect::<Vec<_>>()
        }))
    } else {
        Json(json!({
            "session_id": session_id,
            "messages": []
        }))
    }
}
