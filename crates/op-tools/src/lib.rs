//! op-tools: Tool Registry and Execution
//!
//! Provides the tool registry, built-in tools, and HTTP router.
//!
//! ## Security
//!
//! Security is enforced at the ACCESS level, not command level:
//! - **Unrestricted (Admin)**: Full access - can run any command
//! - **Restricted**: Limited read-only access for untrusted users
//!
//! The chatbot is designed to be a full system administrator.
//! Rate limiting prevents runaway loops.
//!
//! ## Orchestration Plugin
//!
//! The `orchestration_plugin` module provides hooks for tracking all activity:
//! - Tool executions (commands, file ops, etc.)
//! - LLM decisions and tool calls
//! - Session lifecycle events
//!
//! This integrates with blockchain for immutable audit logging.

pub mod builtin;
mod mcptools;
pub mod orchestration_plugin;
pub mod registry;
pub mod router;
pub mod security;
pub mod tool;

use tracing::warn;

// Re-export main types
pub use registry::ToolRegistry;
pub use security::{AccessLevel, SecurityError, SecurityValidator, ToolSecurityProfile, get_security_validator};
pub use tool::{BoxedTool, Tool};
pub use router::{create_router, ToolsServiceRouter, ToolsState};
pub use orchestration_plugin::{
    OrchestrationActivityPlugin, OrchestrationPluginRegistry,
    ToolExecutedEvent, LlmDecisionEvent, SessionEvent,
    get_orchestration_registry, create_tool_event,
};

/// Register all built-in tools
pub async fn register_builtin_tools(registry: &ToolRegistry) -> anyhow::Result<()> {
    builtin::register_all_builtin_tools(registry).await?;
    builtin::register_response_tools(registry).await?;
    if let Err(err) = mcptools::register_mcp_tools(registry).await {
        warn!("Failed to register MCP tools: {}", err);
    }
    Ok(())
}
