//! Chat API Handlers

use axum::{
    extract::{Path, State},
    response::{Json, sse::{Event, Sse}},
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, error};

use crate::state::AppState;
use crate::orchestrator::OrchestratorEvent;

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

/// POST /api/chat - Main chat endpoint (Blocking)
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

    // Pass None for event_tx to disable streaming
    // Wrap in timeout to ensure we return an error if it takes too long
    match tokio::time::timeout(
        Duration::from_secs(290), 
        state.orchestrator.process(&session_id, &request.message, None)
    ).await {
        Ok(Ok(result)) => {
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
        Ok(Err(e)) => {
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
        Err(_) => {
            error!("Chat processing timed out after 290s");
            let provider = state.chat_manager.current_provider().await;
            let model = state.chat_manager.current_model().await;
            Json(ChatResponse {
                success: false,
                message: None,
                error: Some("Request timed out internally after 290 seconds. The task may still be running in the background.".to_string()),
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

    // Create channel for streaming events
    let (tx, mut rx) = mpsc::channel(100);

    // Spawn orchestrator task
    tokio::spawn(async move {
        // Run process with the sender
        // We ignore the result here because it's streamed via events
        // The final event could be a "Finished" event if we wanted, 
        // but currently we stream intermediate steps.
        // We could emit a final event with the full response if needed.
        let result = orchestrator.process(&session_id, &message, Some(tx.clone())).await;
        
        // Send final result or error using dedicated event types
        match result {
            Ok(response) => {
                let _ = tx.send(OrchestratorEvent::Finished {
                    success: response.success,
                    message: response.message,
                    tools_executed: response.tools_executed,
                }).await;
            }
            Err(e) => {
                let _ = tx.send(OrchestratorEvent::Error {
                    message: e.to_string(),
                }).await;
            }
        }
    });

    // Create stream from receiver
    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            yield Ok(Event::default().data(serde_json::to_string(&event).unwrap_or_default()));
        }
    };

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

/// POST /api/chat/transcript - Save conversation transcript to file
/// Accepts either a session_id to save from memory, or direct messages array
pub async fn save_transcript_handler(
    State(state): State<Arc<AppState>>,
    Json(params): Json<Value>,
) -> Json<Value> {
    let filename = params
        .get("filename")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("chat-transcript-{}.txt", chrono::Utc::now().timestamp()));

    // Check if session_id is provided (for existing conversations)
    if let Some(session_id) = params.get("session_id").and_then(|v| v.as_str()) {
        let conversations = state.conversations.read().await;

        if let Some(history) = conversations.get(session_id) {
            if history.is_empty() {
                return Json(json!({
                    "success": false,
                    "error": "No messages in conversation"
                }));
            }
            return save_transcript_to_file(history, filename.as_str(), Some(session_id)).await;
        } else {
            return Json(json!({
                "success": false,
                "error": "Conversation not found"
            }));
        }
    }

    // Check if messages array is provided directly
    if let Some(messages) = params.get("messages").and_then(|v| v.as_array()) {
        if messages.is_empty() {
            return Json(json!({
                "success": false,
                "error": "No messages provided"
            }));
        }

        // Convert Value array to ChatMessage vector
        let mut history = Vec::new();
        for msg in messages {
            if let (Some(role), Some(content)) = (
                msg.get("role").and_then(|v| v.as_str()),
                msg.get("content").and_then(|v| v.as_str())
            ) {
                history.push(op_llm::ChatMessage {
                    role: role.to_string(),
                    content: content.to_string(),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
        }

        if history.is_empty() {
            return Json(json!({
                "success": false,
                "error": "Invalid message format"
            }));
        }

        return save_transcript_to_file(&history, filename.as_str(), None).await;
    }

    Json(json!({
        "success": false,
        "error": "Either session_id or messages array required",
        "usage": {
            "by_session_id": {"session_id": "session-123", "filename": "optional-filename.txt"},
            "by_messages": {"messages": [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi"}], "filename": "optional-filename.txt"}
        }
    }))
}

async fn save_transcript_to_file(history: &[op_llm::ChatMessage], filename: &str, session_id: Option<&str>) -> Json<Value> {
    // Format transcript
    let mut transcript = String::new();

    if let Some(session) = session_id {
        transcript.push_str(&format!("Chat Transcript - Session: {}
", session));
    } else {
        transcript.push_str("Chat Transcript\n");
    }

    transcript.push_str(&format!("Generated: {}
", chrono::Utc::now().to_rfc3339()));
    transcript.push_str(&"=".repeat(50));
    transcript.push_str("\n\n");

    for (i, message) in history.iter().enumerate() {
        let role = match message.role.as_str() {
            "user" => "👤 User",
            "assistant" => "🤖 Assistant",
            "system" => "⚙️ System",
            _ => "Unknown",
        };
        transcript.push_str(&format!("[{}] {}
\n", role, message.content));
        if i < history.len() - 1 {
            let separator = "─".repeat(30);
            transcript.push_str(&separator);
            transcript.push_str("\n\n");
        }
    }

    // Save to file
    let filepath = format!("/tmp/{}", filename);
    match tokio::fs::write(&filepath, &transcript).await {
        Ok(_) => Json(json!({
            "success": true,
            "message": "Transcript saved successfully",
            "filepath": filepath,
            "filename": filename,
            "message_count": history.len(),
            "transcript_preview": transcript.chars().take(200).collect::<String>() + "..."
        })),
        Err(e) => Json(json!({
            "success": false,
            "error": format!("Failed to save transcript: {}", e)
        }))
    }
}