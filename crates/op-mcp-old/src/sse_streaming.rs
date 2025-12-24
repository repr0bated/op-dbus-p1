//! Server-Sent Events (SSE) support for MCP streaming
//!
//! Provides SSE endpoints for long-running MCP operations

use axum::{
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
    extract::State,
};
use futures::stream::{self, Stream};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// SSE event types for MCP operations
#[derive(Debug, Clone)]
pub enum McpEvent {
    /// Tool execution started
    ToolStart {
        tool_name: String,
        server_name: String,
    },
    /// Tool execution progress update
    ToolProgress {
        tool_name: String,
        progress: f64,
        message: String,
    },
    /// Tool execution completed
    ToolComplete {
        tool_name: String,
        result: Value,
    },
    /// Tool execution error
    ToolError {
        tool_name: String,
        error: String,
    },
    /// Agent status update
    AgentStatus {
        agent_id: String,
        status: String,
    },
    /// Generic message
    Message(String),
}

impl McpEvent {
    /// Convert to SSE event
    pub fn to_sse_event(&self) -> Result<Event, Infallible> {
        let (event_type, data) = match self {
            McpEvent::ToolStart { tool_name, server_name } => (
                "tool_start",
                json!({
                    "tool": tool_name,
                    "server": server_name,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            ),
            McpEvent::ToolProgress { tool_name, progress, message } => (
                "tool_progress",
                json!({
                    "tool": tool_name,
                    "progress": progress,
                    "message": message,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            ),
            McpEvent::ToolComplete { tool_name, result } => (
                "tool_complete",
                json!({
                    "tool": tool_name,
                    "result": result,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            ),
            McpEvent::ToolError { tool_name, error } => (
                "tool_error",
                json!({
                    "tool": tool_name,
                    "error": error,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            ),
            McpEvent::AgentStatus { agent_id, status } => (
                "agent_status",
                json!({
                    "agent_id": agent_id,
                    "status": status,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            ),
            McpEvent::Message(msg) => (
                "message",
                json!({
                    "message": msg,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            ),
        };

        Ok(Event::default()
            .event(event_type)
            .json_data(data)
            .unwrap())
    }
}

/// SSE event broadcaster
pub struct SseEventBroadcaster {
    tx: mpsc::UnboundedSender<McpEvent>,
}

impl SseEventBroadcaster {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<McpEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    /// Send an event to all connected SSE clients
    pub fn send_event(&self, event: McpEvent) {
        let _ = self.tx.send(event);
    }

    /// Send a tool start event
    pub fn tool_started(&self, tool_name: String, server_name: String) {
        self.send_event(McpEvent::ToolStart { tool_name, server_name });
    }

    /// Send a tool progress event
    pub fn tool_progress(&self, tool_name: String, progress: f64, message: String) {
        self.send_event(McpEvent::ToolProgress { tool_name, progress, message });
    }

    /// Send a tool complete event
    pub fn tool_completed(&self, tool_name: String, result: Value) {
        self.send_event(McpEvent::ToolComplete { tool_name, result });
    }

    /// Send a tool error event
    pub fn tool_error(&self, tool_name: String, error: String) {
        self.send_event(McpEvent::ToolError { tool_name, error });
    }

    /// Send an agent status event
    pub fn agent_status(&self, agent_id: String, status: String) {
        self.send_event(McpEvent::AgentStatus { agent_id, status });
    }
}

impl Default for SseEventBroadcaster {
    fn default() -> Self {
        Self::new().0
    }
}

/// Create SSE stream from event receiver
pub async fn create_sse_stream(
    mut rx: mpsc::UnboundedReceiver<McpEvent>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Some(event) => {
                debug!("Sending SSE event: {:?}", event);
                Some((event.to_sse_event(), rx))
            }
            None => None,
        }
    })
}

/// SSE handler for a specific MCP server
pub async fn sse_handler(
    server_name: String,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("SSE connection established for MCP server: {}", server_name);

    // Create event broadcaster
    let (_broadcaster, rx) = SseEventBroadcaster::new();

    // Create SSE stream
    let stream = create_sse_stream(rx).await;

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive")
    )
}

/// Global SSE event broadcaster (for sharing across requests)
pub type SharedSseBroadcaster = std::sync::Arc<tokio::sync::RwLock<SseEventBroadcaster>>;
