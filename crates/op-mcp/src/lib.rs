//! op-mcp: MCP Protocol Adapter
//!
//! This crate provides a thin adapter that exposes op-chat functionality via the
//! Model Context Protocol (MCP). It delegates all intelligence to:
//! - op-chat (orchestration)
//! - op-tools (tool system)
//! - op-introspection (D-Bus discovery)
//!
//! Architecture:
//! - stdio: stdin → MCP JSON-RPC → ChatActorHandle → stdout
//! - SSE:   HTTP POST /message → MCP JSON-RPC → GET /sse (streaming)
//! 
//! Methods:
//! - initialize → handshake
//! - tools/list → chat.list_tools()
//! - tools/call → chat.execute_tool()
//! - resources/list → serve embedded docs
//! - resources/read → serve embedded docs

pub mod protocol;
pub mod resources;
pub mod sse;

// Re-export main types
pub use protocol::{McpError, McpRequest, McpResponse, McpServer};
pub use resources::ResourceRegistry;
pub use sse::run_sse_server;

/// Prelude for convenient imports
pub mod prelude {
    pub use super::{McpError, McpRequest, McpResponse, McpServer, ResourceRegistry, run_sse_server};
}