//! op-mcp Server Binary
//!
//! MCP server with lazy tool loading support.

use anyhow::Result;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use op_mcp::{config::Settings, McpServer, McpServerConfig, LazyToolConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to stderr (stdout is reserved for MCP JSON-RPC)
    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false).with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env().add_directive("op_mcp=info".parse()?))
        .init();

    // Load configuration
    let settings = Settings::new()?;

    let tool_config = LazyToolConfig {
        max_loaded_tools: settings.tool_config.max_loaded_tools,
        min_idle_secs: settings.tool_config.min_idle_secs,
        enable_dbus_discovery: settings.tool_config.enable_dbus_discovery,
        enable_plugin_discovery: settings.tool_config.enable_plugin_discovery,
        enable_agent_discovery: settings.tool_config.enable_agent_discovery,
        preload_essential: settings.tool_config.preload_essential,
    };

    let config = McpServerConfig {
        name: settings.name,
        version: settings.version,
        tool_config,
    };

    // Create and run server
    let server = McpServer::new(config).await?;
    server.run_stdio().await?;

    Ok(())
}
