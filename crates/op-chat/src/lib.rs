//! op-chat: Orchestration layer for op-dbus-v2
//!
//! This crate provides the central orchestration layer that coordinates
//! between the MCP protocol and the various tool systems.

pub mod actor;
pub mod handler;
pub mod types;

// Re-export main types
pub use actor::ChatActor;
pub use actor::ChatActorHandle;
pub use handler::ChatHandler;
pub use types::*;

use op_core::{ToolDefinition, ToolRequest, ToolResult};
use op_tools::ToolRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Main chat orchestrator that manages tool execution and message routing
pub struct ChatOrchestrator {
    tool_registry: Arc<RwLock<dyn ToolRegistry>>,
}

impl ChatOrchestrator {
    /// Create a new chat orchestrator
    pub fn new(tool_registry: Arc<RwLock<dyn ToolRegistry>>) -> Self {
        Self { tool_registry }
    }

    /// List all available tools
    pub async fn list_tools(&self) -> Vec<ToolDefinition> {
        let registry = self.tool_registry.read().await;
        registry.list_tools().await
    }

    /// Execute a tool with the given request
    pub async fn execute_tool(&self, request: ToolRequest) -> ToolResult {
        info!("Executing tool: {}", request.name);
        
        let registry = self.tool_registry.read().await;
        if let Some(tool) = registry.get_tool(&request.name).await {
            let result = tool.execute(request).await;
            info!("Tool execution completed: {}", result.success);
            result
        } else {
            warn!("Tool not found: {}", request.name);
            ToolResult {
                success: false,
                content: serde_json::json!({
                    "error": "Tool not found",
                    "tool_name": request.name
                }),
                duration_ms: 0,
                execution_id: uuid::Uuid::new_v4(),
            }
        }
    }

    /// Get tools by category
    pub async fn get_tools_by_category(&self, category: &str) -> Vec<ToolDefinition> {
        let registry = self.tool_registry.read().await;
        registry.get_tools_by_category(category).await
    }
}

/// Prelude for convenient imports
pub mod prelude {
    pub use super::{ChatOrchestrator, ChatActor, ChatActorHandle};
}