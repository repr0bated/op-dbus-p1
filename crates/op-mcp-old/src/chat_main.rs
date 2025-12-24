//! Standalone MCP Chat Server Binary
//!
//! This binary delegates to the `op_dbus::mcp::chat` module, which serves as the
//! central "brain" of the project, integrating orchestration, D-Bus control, and introspection.

use anyhow::Result;
use op_dbus::mcp::chat::server;

#[tokio::main]
async fn main() -> Result<()> {
    server::run().await
}
