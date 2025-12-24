//! WebSocket Handler for Real-time Chat

use axum::{
    extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::Response,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, error, debug};

use op_llm::provider::ChatMessage;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    Chat { message: String, session_id: Option<String> },
    Response { success: bool, message: String, tools_executed: Vec<String> },
    System { message: String },
    Error { message: String },
    Ping,
    Pong,
}

/// WebSocket upgrade handler
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    let session_id = uuid::Uuid::new_v4().to_string();
    info!("WebSocket connected: {}", &session_id[..8]);

    // Subscribe to broadcast channel
    let mut broadcast_rx = state.broadcast_tx.subscribe();

    // Send welcome message
    let welcome = WsMessage::System {
        message: format!(
            "Connected to op-dbus server. Session: {}\nType 'help' for commands.",
            &session_id[..8]
        ),
    };
    if let Err(e) = sender.send(Message::Text(serde_json::to_string(&welcome).unwrap())).await {
        error!("Failed to send welcome: {}", e);
        return;
    }

    // Clone for tasks
    let state_clone = state.clone();
    let session_clone = session_id.clone();

    // Handle broadcast messages
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = broadcast_rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    debug!("WS received: {}", text);
                    
                    // Try to parse as WsMessage
                    let ws_msg: Result<WsMessage, _> = serde_json::from_str(&text);
                    
                    let message_text = match ws_msg {
                        Ok(WsMessage::Chat { message, .. }) => message,
                        Ok(WsMessage::Ping) => {
                            let pong = WsMessage::Pong;
                            let _ = state_clone.broadcast_tx.send(
                                serde_json::to_string(&pong).unwrap()
                            );
                            continue;
                        }
                        _ => text.clone(), // Treat as plain text
                    };

                    if message_text.trim().is_empty() {
                        continue;
                    }

                    // Process through orchestrator
                    match state_clone.orchestrator.process(&session_clone, &message_text).await {
                        Ok(result) => {
                            // Store in conversation history
                            {
                                let mut conversations = state_clone.conversations.write().await;
                                let history = conversations
                                    .entry(session_clone.clone())
                                    .or_insert_with(Vec::new);
                                history.push(ChatMessage::user(&message_text));
                                history.push(ChatMessage::assistant(&result.message));
                            }

                            let response = WsMessage::Response {
                                success: result.success,
                                message: result.message,
                                tools_executed: result.tools_executed,
                            };
                            let _ = state_clone.broadcast_tx.send(
                                serde_json::to_string(&response).unwrap()
                            );
                        }
                        Err(e) => {
                            let error = WsMessage::Error {
                                message: e.to_string(),
                            };
                            let _ = state_clone.broadcast_tx.send(
                                serde_json::to_string(&error).unwrap()
                            );
                        }
                    }
                }
                Message::Close(_) => {
                    info!("WebSocket closed: {}", &session_clone[..8]);
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }

    info!("WebSocket disconnected: {}", &session_id[..8]);
}
