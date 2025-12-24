//! op-mcp: MCP Protocol Adapter
//!
//! This crate provides:
//! - MCP JSON-RPC 2.0 protocol implementation
//! - Lazy tool loading with LRU caching
//! - HTTP router for op-http integration
//! - External MCP server aggregation

pub mod lazy_tools;
pub mod router;
pub mod server;
pub mod http_server;
pub mod config;

// Re-export main types
pub use lazy_tools::{LazyToolConfig, LazyToolManager, McpToolInfo, ToolListResponse};
pub use router::{create_router, McpServiceRouter, McpState};
pub use server::{McpError, McpRequest, McpResponse, McpServer, McpServerConfig};
pub use http_server::HttpMcpServer;

// External MCP client support (if enabled)
#[cfg(feature = "external-mcp")]
pub mod external_client;

#[cfg(feature = "external-mcp")]
pub use external_client::ExternalMcpClient;
