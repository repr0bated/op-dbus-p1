//! Chat handler for processing different types of chat messages

use super::{ChatActorHandle, ChatMessage, ChatMessageKind, ChatResponse};
use op_core::{ToolDefinition, ToolRequest, ToolResult};
use std::sync::Arc;
use tracing::{info, warn};

/// Handler trait for processing chat messages
#[async_trait::async_trait]
pub trait ChatHandler: Send + Sync {
    /// Handle a list tools request
    async fn handle_list_tools(&self) -> Vec<ToolDefinition>;
    
    /// Handle a tool execution request
    async fn handle_execute_tool(&self, request: ToolRequest) -> ToolResult;
    
    /// Handle a get tools by category request
    async fn handle_get_tools_by_category(&self, category: &str) -> Vec<ToolDefinition>;
}

/// Basic chat handler implementation
pub struct BasicChatHandler {
    actor_handle: ChatActorHandle,
}

impl BasicChatHandler {
    /// Create a new basic chat handler
    pub fn new(actor_handle: ChatActorHandle) -> Self {
        Self { actor_handle }
    }
}

#[async_trait::async_trait]
impl ChatHandler for BasicChatHandler {
    async fn handle_list_tools(&self) -> Vec<ToolDefinition> {
        match self.actor_handle.list_tools().await {
            Ok(tools) => {
                info!("Successfully listed {} tools", tools.len());
                tools
            }
            Err(e) => {
                warn!("Failed to list tools: {}", e);
                vec![]
            }
        }
    }

    async fn handle_execute_tool(&self, request: ToolRequest) -> ToolResult {
        match self.actor_handle.execute_tool(request).await {
            Ok(result) => {
                info!("Tool execution completed with success: {}", result.success);
                result
            }
            Err(e) => {
                warn!("Tool execution failed: {}", e);
                ToolResult {
                    success: false,
                    content: serde_json::json!({
                        "error": e.to_string()
                    }),
                    duration_ms: 0,
                    execution_id: uuid::Uuid::new_v4(),
                }
            }
        }
    }

    async fn handle_get_tools_by_category(&self, category: &str) -> Vec<ToolDefinition> {
        // For now, we'll get all tools and filter by category
        // In a full implementation, this would be more efficient
        let all_tools = self.handle_list_tools().await;
        all_tools
            .into_iter()
            .filter(|tool| tool.category == category)
            .collect()
    }
}

/// Chat message processor that routes to appropriate handlers
pub struct ChatMessageProcessor {
    handler: Arc<dyn ChatHandler>,
}

impl ChatMessageProcessor {
    /// Create a new message processor
    pub fn new(handler: Arc<dyn ChatHandler>) -> Self {
        Self { handler }
    }

    /// Process a chat message and return response
    pub async fn process(&self, message: ChatMessage) -> ChatResponse {
        match message.kind {
            ChatMessageKind::ListTools => {
                let tools = self.handler.handle_list_tools().await;
                ChatResponse::tools_list(tools)
            }
            ChatMessageKind::ExecuteTool { request } => {
                let result = self.handler.handle_execute_tool(request).await;
                ChatResponse::tool_result(result)
            }
            ChatMessageKind::GetToolsByCategory { category } => {
                let tools = self.handler.handle_get_tools_by_category(&category).await;
                ChatResponse::tools_list(tools)
            }
        }
    }
}