//! op-mcp: Model Context Protocol Integration
//!
//! This crate provides MCP server functionality with:
//! - Agent registry with specialized agents
//! - Tool registry for D-Bus and system tools
//! - Chat server with LLM integration
//! - Introspection tools
//! - Workflow orchestration
//! - External MCP server connections

#![allow(dead_code)]
#![allow(unused_imports)]

// Agent modules
pub mod agents;

// Core MCP modules
pub mod discovery;
pub mod hybrid_dbus_bridge;
pub mod hybrid_scanner;
pub mod introspection_parser;
pub mod json_introspection;
pub mod system_introspection;

// Registry modules
pub mod agent_registry;
pub mod tool_registry;
pub mod external_mcp_client;
pub mod sse_streaming;
pub mod client_config_generator;

// Tool modules
pub mod tools;

// Chat interface
pub mod ai_context_provider;
pub mod chat_server;
pub mod ollama;
// TODO: chat module has incomplete dependencies
// pub mod chat;

// Flow-based workflows (requires pocketflow_rs)
// TODO: workflows requires optional pocketflow_rs dependency
// pub mod workflows;

// MCP client and discovery
pub mod mcp_client;
pub mod mcp_discovery;

// Bridge modules
pub mod plugin_tool_bridge;
pub mod dbus_indexer;

// Caching
pub mod introspection_cache;
pub mod introspection_tools;

// Workflow introspection
pub mod workflow_plugin_introspection;

// Resources
pub mod resources;

// Comprehensive introspection
pub mod comprehensive_introspection;
pub mod native_introspection;

// Inspector Gadget
pub mod introspective_gadget;

// Embedded agents (requires rust_embed)
// TODO: embedded_agents requires optional rust_embed dependency
// pub mod embedded_agents;

// Web bridges (optional)
#[cfg(feature = "web")]
pub mod web_bridge;
#[cfg(feature = "web")]
pub mod web_bridge_improved;

/// Prelude for convenient imports
pub mod prelude {
    pub use super::agent_registry::AgentRegistry;
    pub use super::tool_registry::ToolRegistry;
    pub use super::introspection_parser::IntrospectionParser;
}
