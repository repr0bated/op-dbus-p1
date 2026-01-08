//! op-mcp-server: Unified MCP Protocol Server
//!
//! Supports three modes:
//!   - compact: 4 meta-tools for discovering 148+ tools (default for LLMs)
//!   - agents:  Always-on cognitive agents (memory, sequential_thinking, rust_pro, etc.)
//!   - full:    All tools directly exposed
//!
//! Supports multiple transports:
//!   op-mcp-server                           # stdio, compact mode
//!   op-mcp-server --mode agents             # stdio, agents mode (starts rust_pro, memory, etc.)
//!   op-mcp-server --http 0.0.0.0:3001       # HTTP+SSE
//!   op-mcp-server --ws 0.0.0.0:3002         # WebSocket
//!   op-mcp-server --all                     # All transports
//!
//! Run-On-Connection Agents (started automatically when client connects):
//!   - rust_pro         : Rust development (cargo check/build/test/clippy)
//!   - backend_architect: System design and architecture
//!   - sequential_thinking: Step-by-step reasoning
//!   - memory           : Key-value session memory
//!   - context_manager  : Persistent context across sessions
//!
//! Example configurations:
//!   # Agents MCP (recommended for development)
//!   op-mcp-server --mode agents --http 0.0.0.0:3002
//!
//!   # Compact MCP (for Cursor, Gemini CLI, etc.)
//!   op-mcp-server --mode compact --http 0.0.0.0:3001
//!
//!   # Both servers on different ports
//!   op-mcp-server --mode compact --http 0.0.0.0:3001 &
//!   op-mcp-server --mode agents --http 0.0.0.0:3002 &

use anyhow::Result;
use clap::Parser;
use op_mcp::{
    McpServer, McpServerConfig,
    AgentsServer, AgentsServerConfig,
    ServerMode,
    transport::{Transport, StdioTransport, HttpSseTransport, WebSocketTransport},
};
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "op-mcp-server")]
#[command(about = "Unified MCP Protocol Server")]
struct Cli {
    /// Server mode: compact (4 meta-tools), agents (always-on), full (all tools)
    #[arg(long, short, default_value = "compact")]
    mode: String,
    
    /// Run stdio transport (default if no network transport specified)
    #[arg(long)]
    stdio: bool,
    
    /// Run HTTP+SSE transport on specified address
    #[arg(long, value_name = "ADDR")]
    http: Option<String>,
    
    /// Run SSE-only transport on specified address
    #[arg(long, value_name = "ADDR")]
    sse: Option<String>,
    
    /// Run WebSocket transport on specified address
    #[arg(long, value_name = "ADDR")]
    ws: Option<String>,
    
    /// Run gRPC transport on specified address
    #[cfg(feature = "grpc")]
    #[arg(long, value_name = "ADDR")]
    grpc: Option<String>,
    
    /// Run all transports with default addresses
    #[arg(long)]
    all: bool,
    
    /// Disable auto-start of run-on-connection agents (agents mode only)
    #[arg(long)]
    no_auto_start: bool,
    
    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,
    
    /// Server name override
    #[arg(long)]
    name: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging (stderr to not interfere with stdio transport)
    let level = match cli.log_level.as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };
    
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    
    // Parse server mode
    let mode: ServerMode = cli.mode.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    
    info!(mode = %mode, "Starting op-mcp-server");
    
    // Determine transports
    let run_stdio = cli.stdio || cli.all || 
        (cli.http.is_none() && cli.sse.is_none() && cli.ws.is_none());
    let http_addr = cli.http.or(cli.sse).or(if cli.all { Some("0.0.0.0:3001".into()) } else { None });
    let ws_addr = cli.ws.or(if cli.all { Some("0.0.0.0:3002".into()) } else { None });
    
    // Create and run server based on mode
    match mode {
        ServerMode::Compact | ServerMode::Full => {
            let config = McpServerConfig {
                name: cli.name,
                compact_mode: mode == ServerMode::Compact,
                ..Default::default()
            };
            
            let server = McpServer::new(config).await?;
            info!(mode = %mode, "MCP server initialized");
            
            run_transports(server, run_stdio, http_addr, ws_addr).await
        }
        
        ServerMode::Agents => {
            let config = AgentsServerConfig {
                name: cli.name.or(Some("op-mcp-agents".to_string())),
                auto_start_agents: !cli.no_auto_start,
                ..Default::default()
            };
            
            let server = Arc::new(AgentsServer::new(config));
            
            // Log run-on-connection agents
            let roc_agents: Vec<_> = server.run_on_connection_agents()
                .iter()
                .map(|a| a.id.as_str())
                .collect();
            info!(
                run_on_connection = ?roc_agents,
                total = server.enabled_agents().len(),
                "Agents MCP server initialized"
            );
            
            run_transports(server, run_stdio, http_addr, ws_addr).await
        }
    }
}

async fn run_transports<H>(
    server: Arc<H>,
    run_stdio: bool,
    http_addr: Option<String>,
    ws_addr: Option<String>,
) -> Result<()>
where
    H: op_mcp::transport::McpHandler + 'static,
{
    let mut handles = Vec::new();
    
    // Spawn HTTP+SSE transport
    if let Some(addr) = http_addr {
        let server = server.clone();
        handles.push(tokio::spawn(async move {
            info!(addr = %addr, "Starting HTTP+SSE transport");
            HttpSseTransport::new(addr).serve(server).await
        }));
    }
    
    // Spawn WebSocket transport
    if let Some(addr) = ws_addr {
        let server = server.clone();
        handles.push(tokio::spawn(async move {
            info!(addr = %addr, "Starting WebSocket transport");
            WebSocketTransport::new(addr).serve(server).await
        }));
    }
    
    // Run stdio in main thread if enabled (blocks)
    if run_stdio {
        info!("Starting stdio transport");
        StdioTransport::new().serve(server).await?;
    } else {
        // Wait for spawned transports
        for handle in handles {
            handle.await??;
        }
    }
    
    Ok(())
}
