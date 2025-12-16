//! op-mcp-server: Complete MCP Protocol Server
//!
//! Main entry point for the MCP server that exposes op-dbus-v2 functionality
//! via the Model Context Protocol. This implementation creates and initializes
//! the complete system including chat orchestration and tool management.

use anyhow::Result;
use op_mcp::{McpRequest, McpServer};
use op_chat::{ChatOrchestrator, ChatActor, ChatActorHandle, prelude::ChatActorHandle as _};
use op_tools::{ToolSystem, ToolSystemBuilder, prelude::ToolRegistry};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, Stdin, Stdout};
use tokio::sync::{mpsc, RwLock};
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
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    info!("Starting op-mcp-server (complete implementation)");

    // Create and initialize the complete system
    let (mcp_server, shutdown_signal) = initialize_complete_system().await?;
    
    // Set up shutdown handling
    let shutdown_future = async {
        tokio::signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
        info!("Received shutdown signal");
        shutdown_signal.send(()).ok();
    };

    // Run both the MCP server and shutdown handler
    tokio::select! {
        result = run_mcp_protocol(mcp_server) => {
            if let Err(e) = result {
                warn!("MCP protocol loop ended with error: {}", e);
            }
        }
        _ = shutdown_future => {
            info!("Shutting down gracefully");
        }
    }

    info!("op-mcp-server shutdown complete");
    Ok(())
}

/// Initialize the complete op-dbus-v2 system
async fn initialize_complete_system() -> Result<(Arc<McpServer>, tokio::sync::oneshot::Sender<()>)> {
    info!("Initializing complete op-dbus-v2 system");

    // 1. Create tool system with registry and executor
    info!("Creating tool system...");
    let tool_registry = Arc::new(RwLock::new(op_tools::ToolRegistryImpl::new()));
    
    let middleware = vec![
        Arc::new(op_tools::LoggingMiddleware::new(true, true)),
        Arc::new(op_tools::TimingMiddleware::new(true)),
        Arc::new(op_tools::ValidationMiddleware::new(true)),
        Arc::new(op_tools::RateLimitMiddleware::new(60)), // 60 requests per minute
    ];
    
    let tool_executor = Arc::new(op_tools::ToolExecutorImpl::new(middleware));
    let tool_discovery = op_tools::ToolDiscovery::new();
    
    let tool_system = Arc::new(ToolSystem::new(
        tool_registry.clone(),
        tool_executor,
        tool_discovery,
    ));

    // Initialize with built-in tools
    info!("Registering built-in tools...");
    tool_system.initialize_with_builtins().await?;

    // 2. Create chat orchestrator
    info!("Creating chat orchestrator...");
    let chat_orchestrator = Arc::new(ChatOrchestrator::new(tool_registry.clone()));

    // 3. Create chat actor for async message processing
    info!("Creating chat actor...");
    let (message_sender, message_receiver) = mpsc::unbounded_channel();
    let chat_actor_handle = ChatActorHandle::new(message_sender);
    
    let chat_actor = ChatActor::new(chat_orchestrator.clone(), message_receiver);
    
    // Start the chat actor in the background
    tokio::spawn(async move {
        if let Err(e) = chat_actor.run().await {
            warn!("Chat actor failed: {}", e);
        }
    });

    // 4. Create MCP server with full system integration
    info!("Creating MCP server...");
    let mcp_server = Arc::new(McpServer::with_full_system(
        chat_actor_handle,
        tool_system.clone(),
    ));

    // 5. Set up graceful shutdown
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Clone components for shutdown handling
    let tool_system_for_shutdown = tool_system.clone();
    let orchestrator_for_shutdown = chat_orchestrator.clone();
    
    tokio::spawn(async move {
        shutdown_rx.await.ok();
        info!("Performing graceful shutdown...");
        
        // Get final stats before shutdown
        if let Ok(registry) = tool_system_for_shutdown.registry().try_read() {
            let stats = registry.get_stats().await;
            info!("Final tool registry stats: {:?}", stats);
        }
        
        info!("Graceful shutdown complete");
    });

    info!("Complete system initialization finished");
    Ok((mcp_server, shutdown_tx))
}

/// Run the MCP protocol loop over stdio
async fn run_mcp_protocol(mcp_server: Arc<McpServer>) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (stdin_reader, mut stdout_writer) = (stdin, stdout);

    let mut lines = stdin_reader.lines();
    let mut request_count: u64 = 0;

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        request_count += 1;
        info!("Processing MCP request #{}: {}", request_count, line);

        // Parse MCP request
        let request: Result<McpRequest, _> = serde_json::from_str(line);
        match request {
            Ok(mcp_request) => {
                // Handle the request
                let response = mcp_server.handle_request(mcp_request).await;

                // Send response
                let response_json = serde_json::to_string(&response)?;
                stdout_writer.write_all(response_json.as_bytes()).await?;
                stdout_writer.write_all(b"\n").await?;
                stdout_writer.flush().await?;

                info!("Response sent for request #{}", request_count);
            }
            Err(e) => {
                warn!("Failed to parse MCP request #{}: {}", request_count, e);
                
                // Send error response
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": "Parse error",
                        "data": e.to_string()
                    }
                });

                let error_json = serde_json::to_string(&error_response)?;
                stdout_writer.write_all(error_json.as_bytes()).await?;
                stdout_writer.write_all(b"\n").await?;
                stdout_writer.flush().await?;
            }
        }
    }

    info!("MCP protocol loop ended after {} requests", request_count);
    Ok(())
}