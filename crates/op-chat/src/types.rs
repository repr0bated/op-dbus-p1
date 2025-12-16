//! Types and data structures for op-chat

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Chat session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub session_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub context: HashMap<String, serde_json::Value>,
}

/// Chat message with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageWithMetadata {
    pub message: super::ChatMessage,
    pub session_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub correlation_id: Option<String>,
}

/// Chat execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub session_id: String,
    pub user_id: Option<String>,
    pub permissions: Vec<String>,
    pub environment: HashMap<String, serde_json::Value>,
}

/// Chat configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    pub max_concurrent_executions: usize,
    pub default_timeout: std::time::Duration,
    pub enable_logging: bool,
    pub tool_cache_size: usize,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            max_concurrent_executions: 10,
            default_timeout: std::time::Duration::from_secs(30),
            enable_logging: true,
            tool_cache_size: 100,
        }
    }
}

/// Chat statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatStats {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub average_execution_time: f64,
    pub active_sessions: usize,
}

/// Chat event types for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatEvent {
    SessionStarted { session_id: String },
    SessionEnded { session_id: String },
    ToolExecuted { tool_name: String, execution_id: String, success: bool },
    ToolExecutionError { tool_name: String, error: String },
    RateLimitExceeded { session_id: String },
}

/// Chat event handler trait
#[async_trait::async_trait]
pub trait ChatEventHandler: Send + Sync {
    async fn handle_event(&self, event: ChatEvent);
}

/// Simple console event handler for debugging
pub struct ConsoleEventHandler;

#[async_trait::async_trait]
impl ChatEventHandler for ConsoleEventHandler {
    async fn handle_event(&self, event: ChatEvent) {
        match event {
            ChatEvent::SessionStarted { session_id } => {
                tracing::info!("Chat session started: {}", session_id);
            }
            ChatEvent::SessionEnded { session_id } => {
                tracing::info!("Chat session ended: {}", session_id);
            }
            ChatEvent::ToolExecuted { tool_name, execution_id, success } => {
                tracing::info!("Tool executed: {} ({}), success: {}", tool_name, execution_id, success);
            }
            ChatEvent::ToolExecutionError { tool_name, error } => {
                tracing::warn!("Tool execution error: {} - {}", tool_name, error);
            }
            ChatEvent::RateLimitExceeded { session_id } => {
                tracing::warn!("Rate limit exceeded for session: {}", session_id);
            }
        }
    }
}