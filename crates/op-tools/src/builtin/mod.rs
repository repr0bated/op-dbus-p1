//! Built-in tools for op-dbus
//!
//! All tools registered eagerly at startup.
//! Agents started as D-Bus services when registered.

pub mod agent_tool;
pub mod response_tools;

// Include other modules if they exist in your codebase
// pub mod dbus;
// pub mod dbus_introspection;
// pub mod files;
// pub mod network;
// pub mod ovs;
// pub mod self_tools;
// pub mod shell;

use anyhow::Result;
use crate::ToolRegistry;
use crate::registry::ToolDefinition;

/// Register all built-in tools
pub async fn register_all_builtin_tools(registry: &ToolRegistry) -> Result<()> {
    tracing::info!("Registering built-in tools...");

    // Register agent tools (starts D-Bus services)
    tracing::info!("Starting agent D-Bus services...");
    agent_tool::register_all_agents(registry).await?;

    let count = registry.list().await.len();
    tracing::info!("Registered {} tools", count);

    Ok(())
}

/// Register response tools (respond_to_user, cannot_perform, request_clarification)
pub async fn register_response_tools(registry: &ToolRegistry) -> Result<()> {
    tracing::info!("Registering response tools...");
    
    // Initialize response accumulator
    response_tools::init_response_accumulator();
    
    // Create and register response tools
    let tools = response_tools::create_response_tools();
    let tool_count = tools.len();
    for tool in tools {
        let name = tool.name().to_string();
        let definition = ToolDefinition {
            name: name.clone(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
            category: tool.category().to_string(),
            namespace: tool.namespace().to_string(),
            tags: tool.tags(),
        };
        registry.register(name.into(), tool, definition).await?;
    }
    
    tracing::info!("Registered {} response tools", tool_count);
    Ok(())
}

// Re-exports
pub use agent_tool::{
    AgentConnectionRegistry, AgentDef, AgentExecutor, AgentTool, 
    BusType, DbusAgentExecutor, AGENT_DEFINITIONS,
    create_agent_tool, create_agent_tool_with_executor,
};
