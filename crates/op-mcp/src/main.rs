//! op-mcp-server: MCP Protocol Server
//!
//! Main entry point for the MCP server that exposes op-dbus-v2 functionality
//! via the Model Context Protocol. This is a thin adapter that delegates
//! all functionality to op-chat, op-tools, and op-introspection.

use anyhow::Result;
use op_chat::{ChatActor, ChatActorConfig};
use op_mcp::{McpRequest, McpServer, ResourceRegistry};
use tokio::io::BufReader;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Main entry point
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "op_mcp=debug,tokio=warn,warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting op-mcp-server");

    // Create ChatActor with default configuration
    let config = ChatActorConfig::default();
    let (chat_actor, chat_handle) = ChatActor::new(config).await?;
    tokio::spawn(chat_actor.run());

    info!("ChatActor started, ready to handle MCP requests");

    // Create MCP server
    let mcp_server = Arc::new(McpServer::new(chat_handle));

    // Create resource registry (can be extended with actual docs)
    let resource_registry = ResourceRegistry::new();

    // Run the MCP protocol loop
    run_mcp_protocol(mcp_server, resource_registry).await?;

    info!("op-mcp-server shutting down");
    Ok(())
}

/// Run the MCP protocol loop over stdio
async fn run_mcp_protocol(
    mcp_server: Arc<McpServer>,
    _resource_registry: ResourceRegistry,
) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let mut stdout_writer = stdout;
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse MCP request
        let request: Result<McpRequest, _> = serde_json::from_str(line);
        match request {
            Ok(mcp_request) => {
                info!("Received MCP request: {}", mcp_request.method);

                // Handle the request
                let response = mcp_server.handle_request(mcp_request).await;

                // Send response
                let response_json = serde_json::to_string(&response)?;
                stdout_writer.write_all(response_json.as_bytes()).await?;
                stdout_writer.write_all(b"\n").await?;
                stdout_writer.flush().await?;

                info!("Response sent");
            }
            Err(e) => {
                warn!("Failed to parse MCP request: {}", e);
                
                // Send error response
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": "Parse error"
                    }
                });

                let error_json = serde_json::to_string(&error_response)?;
                stdout_writer.write_all(error_json.as_bytes()).await?;
                stdout_writer.write_all(b"\n").await?;
                stdout_writer.flush().await?;
            }
        }
    }

    Ok(())
}
