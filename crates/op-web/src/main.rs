//! op-web: Main Entry Point
//!
//! Unified web server for op-dbus-v2 that integrates:
//! - HTTP REST API
//! - WebSocket for real-time chat
//! - MCP protocol for Claude Desktop
//! - SSE for streaming events
//! - Static file serving (WASM frontend)
//! - All op-* crate functionality

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod handlers;
mod mcp;
mod mcp_picker;
mod groups_admin;
mod orchestrator;
mod routes;
mod sse;
mod state;
mod websocket;

use routes::create_router;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment from /etc/op-dbus/environment (if exists)
    op_core::config::load_environment();

    // Initialize logging with environment filter
    tracing_subscriber::registry()
        .with(fmt::layer().compact())
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,op_web=debug")),
        )
        .init();

    println!(r#"
╔═══════════════════════════════════════════════════════════════════╗
║                                                                   ║
║   ██████╗ ██████╗       ██╗    ██╗███████╗██████╗                ║
║  ██╔═══██╗██╔══██╗      ██║    ██║██╔════╝██╔══██╗               ║
║  ██║   ██║██████╔╝█████╗██║ █╗ ██║█████╗  ██████╔╝               ║
║  ██║   ██║██╔═══╝ ╚════╝██║███╗██║██╔══╝  ██╔══██╗               ║
║  ╚██████╔╝██║           ╚███╔███╔╝███████╗██████╔╝               ║
║   ╚═════╝ ╚═╝            ╚══╝╚══╝ ╚══════╝╚═════╝                ║
║                                                                   ║
║   Unified Server for op-dbus-v2                                   ║
║   Version: {}                                            ║
╚═══════════════════════════════════════════════════════════════════╝
"#, env!("CARGO_PKG_VERSION"));

    info!("Initializing application state...");

    // Initialize application state (loads all tools, agents, plugins)
    let state = Arc::new(AppState::new().await?);

    // Log what was loaded
    let tool_count = state.tool_registry.list().await.len();
    let agent_types = op_agents::list_agent_types().len();
    
    info!("✅ Loaded {} tools", tool_count);
    info!("✅ {} agent types available", agent_types);
    info!("✅ LLM Provider: {} ({})", state.provider_name, state.default_model);

    // Create router with all routes
    let app = create_router(state.clone());

    // Bind to address
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    println!(r#"
┌─────────────────────────────────────────────────────────────────┐
│                         Server Ready                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  🌐 Web UI:        http://localhost:{:<5}                      │
│  📡 REST API:      http://localhost:{:<5}/api/                 │
│  💬 WebSocket:     ws://localhost:{:<5}/ws                     │
│  📊 Health:        http://localhost:{:<5}/api/health           │
│                                                                 │
│  🔧 MCP Tool Picker:                                            │
│     http://localhost:{:<5}/mcp-picker                          │
│                                                                 │
│  📋 MCP Endpoints:                                              │
│     Profiles:  /mcp/profiles                                    │
│     Custom:    /mcp/custom/{{name}}                               │
│     Discover:  /mcp/_discover                                   │
│                                                                 │
│  Press Ctrl+C to stop                                           │
└─────────────────────────────────────────────────────────────────┘
"#, port, port, port, port, port);

    // Start server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down...");
        },
        _ = terminate => {
            info!("Received terminate signal, shutting down...");
        },
    }
}
