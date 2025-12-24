//! op-tools: Tool Registry and Execution
//!
//! Provides the tool registry, built-in tools, and HTTP router.

pub mod builtin;
pub mod registry;
pub mod router;
pub mod tool;
// pub mod lazy_factory;
// pub mod discovery;

// Re-export main types
pub use registry::ToolRegistry;
pub use tool::{BoxedTool, Tool};
pub use router::{create_router, ToolsServiceRouter, ToolsState};

/// Register all built-in tools
pub async fn register_builtin_tools(registry: &ToolRegistry) -> anyhow::Result<()> {
    builtin::register_response_tools(registry).await?;
    Ok(())
}
