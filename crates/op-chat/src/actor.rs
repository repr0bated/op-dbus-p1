//! Chat actor implementation for async message handling

use super::{ChatOrchestrator, ChatMessage, ChatResponse};
use op_core::{ToolDefinition, ToolRequest, ToolResult};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

/// Chat actor that processes messages asynchronously
pub struct ChatActor {
    orchestrator: Arc<ChatOrchestrator>,
    message_receiver: mpsc::UnboundedReceiver<ChatMessage>,
}

impl ChatActor {
    /// Create a new chat actor
    pub fn new(
        orchestrator: Arc<ChatOrchestrator>,
        message_receiver: mpsc::UnboundedReceiver<ChatMessage>,
    ) -> Self {
        Self {
            orchestrator,
            message_receiver,
        }
    }

    /// Run the actor message processing loop
    pub async fn run(mut self) -> anyhow::Result<()> {
        while let Some(message) = self.message_receiver.recv().await {
            match self.process_message(message).await {
                Ok(response) => {
                    if let Some(response_tx) = message.response_channel {
                        let _ = response_tx.send(response);
                    }
                }
                Err(e) => {
                    warn!("Failed to process message: {}", e);
                    if let Some(response_tx) = message.response_channel {
                        let error_response = ChatResponse::error(format!("Processing failed: {}", e));
                        let _ = response_tx.send(error_response);
                    }
                }
            }
        }
        Ok(())
    }

    /// Process a single chat message
    async fn process_message(&self, message: ChatMessage) -> anyhow::Result<ChatResponse> {
        match message.kind {
            ChatMessageKind::ListTools => {
                let tools = self.orchestrator.list_tools().await;
                Ok(ChatResponse::tools_list(tools))
            }
            ChatMessageKind::ExecuteTool { request } => {
                let result = self.orchestrator.execute_tool(request).await;
                Ok(ChatResponse::tool_result(result))
            }
            ChatMessageKind::GetToolsByCategory { category } => {
                let tools = self.orchestrator.get_tools_by_category(&category).await;
                Ok(ChatResponse::tools_list(tools))
            }
        }
    }
}

/// Handle for interacting with the chat actor
pub struct ChatActorHandle {
    message_sender: mpsc::UnboundedSender<ChatMessage>,
}

impl ChatActorHandle {
    /// Create a new actor handle
    pub fn new(message_sender: mpsc::UnboundedSender<ChatMessage>) -> Self {
        Self { message_sender }
    }

    /// Send a message and wait for response
    pub async fn send_message(&self, message: ChatMessage) -> anyhow::Result<ChatResponse> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        let message_with_channel = ChatMessage {
            response_channel: Some(response_tx),
            ..message
        };

        self.message_sender.send(message_with_channel)
            .map_err(|_| anyhow::anyhow!("Actor channel closed"))?;

        response_rx.await.map_err(|_| anyhow::anyhow!("Actor response dropped"))
    }

    /// List all available tools
    pub async fn list_tools(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let response = self.send_message(ChatMessage {
            kind: ChatMessageKind::ListTools,
            response_channel: None,
        }).await?;
        
        match response {
            ChatResponse::ToolsList { tools } => Ok(tools),
            _ => Err(anyhow::anyhow!("Unexpected response type")),
        }
    }

    /// Execute a tool
    pub async fn execute_tool(&self, request: ToolRequest) -> anyhow::Result<ToolResult> {
        let response = self.send_message(ChatMessage {
            kind: ChatMessageKind::ExecuteTool { request },
            response_channel: None,
        }).await?;
        
        match response {
            ChatResponse::ToolResult { result } => Ok(result),
            _ => Err(anyhow::anyhow!("Unexpected response type")),
        }
    }
}

/// Chat message types for actor communication
#[derive(Debug, Clone)]
pub enum ChatMessageKind {
    ListTools,
    ExecuteTool { request: ToolRequest },
    GetToolsByCategory { category: String },
}

/// Chat message with optional response channel
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub kind: ChatMessageKind,
    pub response_channel: Option<tokio::sync::oneshot::Sender<ChatResponse>>,
}

/// Chat response types
#[derive(Debug, Clone)]
pub enum ChatResponse {
    ToolsList { tools: Vec<ToolDefinition> },
    ToolResult { result: ToolResult },
    Error { message: String },
}

impl ChatResponse {
    /// Create a tools list response
    pub fn tools_list(tools: Vec<ToolDefinition>) -> Self {
        Self::ToolsList { tools }
    }

    /// Create a tool execution result response
    pub fn tool_result(result: ToolResult) -> Self {
        Self::ToolResult { result }
    }

    /// Create an error response
    pub fn error(message: String) -> Self {
        Self::Error { message }
    }
}