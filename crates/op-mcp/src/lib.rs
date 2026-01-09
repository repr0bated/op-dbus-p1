//! op-mcp: Unified MCP Protocol Server
//!
//! Supports three server modes:
//! - **Compact**: 4 meta-tools for discovering 148+ tools (recommended for LLMs)
//! - **Agents**: Always-on cognitive agents (memory, sequential_thinking, etc.)
//! - **Full**: All tools directly exposed (may hit client limits)
//!
//! Supports multiple transports:
//! - Stdio (standard MCP transport)
//! - HTTP (REST endpoints)
//! - SSE (Server-Sent Events)
//! - HTTP+SSE (bidirectional)
//! - WebSocket (full duplex)
//! - gRPC (high-performance RPC)

pub mod protocol;
pub mod server;
pub mod agents_server;
pub mod compact;
pub mod transport;
pub mod resources;

#[cfg(feature = "grpc")]
pub mod grpc;

// Re-exports
pub use protocol::{McpRequest, McpResponse, McpError, JsonRpcError};
pub use server::{McpServer, McpServerConfig, ToolInfo, ToolExecutor, DefaultToolExecutor};
pub use agents_server::AgentsServer;
pub use compact::{CompactServer, run_compact_stdio_server};
pub use resources::ResourceRegistry;
pub use transport::{
    Transport,
    StdioTransport,
    HttpTransport,
    SseTransport,
    HttpSseTransport,
    WebSocketTransport,
};

#[cfg(feature = "grpc")]
pub use grpc::GrpcTransport;

/// Protocol version
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// Server info
pub const SERVER_NAME: &str = "op-mcp";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Server mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerMode {
    /// 4 meta-tools for tool discovery
    Compact,
    /// Always-on cognitive agents
    Agents,
    /// All tools directly exposed
    Full,
}

impl std::fmt::Display for ServerMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerMode::Compact => write!(f, "compact"),
            ServerMode::Agents => write!(f, "agents"),
            ServerMode::Full => write!(f, "full"),
        }
    }
}

impl std::str::FromStr for ServerMode {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "compact" => Ok(ServerMode::Compact),
            "agents" => Ok(ServerMode::Agents),
            "full" | "standard" => Ok(ServerMode::Full),
            _ => Err(format!("Unknown server mode: {}", s)),
        }
    }
}
