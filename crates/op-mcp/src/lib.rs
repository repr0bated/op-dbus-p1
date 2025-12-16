//! op-mcp: Minimal MCP Protocol Adapter
//!
//! This crate provides a thin adapter that exposes op-dbus-v2 functionality via the
//! Model Context Protocol (MCP). It delegates all intelligence to op-chat.
//!
//! Architecture:
//! stdin → MCP JSON-RPC → ChatActorHandle → stdout
//!
//! Methods:
//! - initialize → handshake
//! - tools/list → chat.list_tools()
//! - tools/call → chat.execute_tool()
//! - resources/list → serve embedded docs
//! - resources/read → serve embedded docs

pub mod protocol;
pub mod resources;

// Re-export main types
pub use protocol::{McpError, McpRequest, McpResponse, McpServer};
pub use resources::ResourceRegistry;

/// Prelude for convenient imports
pub mod prelude {
    pub use super::{McpError, McpRequest, McpResponse, McpServer, ResourceRegistry};
}