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

// Server modules from v2
// Note: Some modules commented out due to missing dependencies
// TODO: Integrate these modules after resolving dependencies
// pub mod config; // Requires 'config' crate
// pub mod lazy_tools; // Requires op_tools::builtin and op_tools::discovery APIs that don't exist
// pub mod server; // Requires lazy_tools
// pub mod router; // Requires 'op_http' crate
// pub mod tool_adapter; // File has corrupted format (line numbers embedded) and missing dependencies
// pub mod tool_adapter_orchestrated; // Requires op_chat types (ExecutionMode, OrchestratedExecutor, etc.)
pub mod external_client;
pub mod http_server;

// Re-export main types
pub use protocol::{McpError, McpRequest, McpResponse, McpServer};
pub use resources::ResourceRegistry;
pub use sse::run_sse_server;

/// Prelude for convenient imports
pub mod prelude {
    pub use super::{McpError, McpRequest, McpResponse, McpServer, ResourceRegistry, run_sse_server};
}