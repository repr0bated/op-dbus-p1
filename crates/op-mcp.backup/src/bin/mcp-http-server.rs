//! MCP HTTP Server Binary
//!
//! Runs an HTTP server that proxies MCP requests to the local MCP server.

use anyhow::Result;
use axum::serve;
use clap::Parser;
use op_mcp::http_server::HttpMcpServer;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing_subscriber;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "8081")]
    port: u16,

    /// MCP server command to proxy to
    #[arg(
        long,
        default_value = "/home/jeremy/op-dbus-v2/target/release/op-mcp-server"
    )]
    mcp_command: String,

    /// Additional arguments for MCP command
    #[arg(long)]
    mcp_args: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();

    // Build MCP command
    let mut mcp_command = vec![args.mcp_command];
    mcp_command.extend(args.mcp_args);

    tracing::info!("Starting MCP HTTP proxy server on port {}", args.port);
    tracing::info!("Proxying to MCP command: {:?}", mcp_command);

    // Create HTTP MCP server
    let server = HttpMcpServer::new(mcp_command);
    let app = server.router();

    // Bind to address
    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Listening on {}", addr);

    // Serve
    serve(listener, app).await?;

    Ok(())
}
