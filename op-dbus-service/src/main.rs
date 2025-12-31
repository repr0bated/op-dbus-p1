//! OP D-Bus Service (v2)
//!
//! Consolidated service providing:
//! - HTTP/WebSocket API
//! - D-Bus Interfaces (Chat, ExecutionTracker)
//! - Internal Actor System (ChatActor)

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, error};

use op_http::prelude::*;
use op_chat::{ChatActor, ChatActorConfig};
use op_tools::ToolRegistry;
use op_state::StateManager;
use zbus::connection;
use op_http::axum;

mod chat;
// mod tracker;
mod state;

use chat::ChatInterface;
use state::StateInterface;

#[derive(Parser, Debug)]
#[command(name = "op-dbus-service")]
#[command(about = "Unified D-Bus and HTTP server for op-dbus")]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Bind address (host:port)
    #[arg(short, long, default_value = "0.0.0.0:8080")]
    bind: String,

    /// HTTPS port (if TLS enabled)
    #[arg(long, default_value = "8443")]
    https_port: u16,

    /// Enable auto TLS detection
    #[arg(long)]
    tls_auto: bool,

    /// TLS certificate path
    #[arg(long)]
    tls_cert: Option<String>,

    /// TLS key path
    #[arg(long)]
    tls_key: Option<String>,

    /// Static files directory
    #[arg(long)]
    static_dir: Option<PathBuf>,

    /// Disable CORS
    #[arg(long)]
    no_cors: bool,

    /// Disable compression
    #[arg(long)]
    no_compression: bool,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Set desired state from file
    SetDesiredState {
        /// Path to state file
        #[arg(default_value = "/etc/state/state.json")]
        path: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment from /etc/op-dbus/environment (if exists)
    op_core::config::load_environment();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("op_dbus_service=info".parse()?)
                .add_directive("op_http=info".parse()?)
                // .add_directive("op_mcp=info".parse()?)
                // .add_directive("op_web=info".parse()?)
                .add_directive("op_chat=info".parse()?)
                .add_directive("tower_http=debug".parse()?),
        )
        .init();

    let args = Args::parse();

    // --- 1. Initialize Core State ---

    // Tool Registry
    let registry = Arc::new(ToolRegistry::new());
    op_tools::register_builtin_tools(&registry).await?;
    info!("Initialized Tool Registry");

    // --- 2. Initialize Streaming Blockchain ---
    let blockchain_path = std::env::var("OP_BLOCKCHAIN_PATH")
        .unwrap_or_else(|_| "/var/lib/op-dbus/blockchain".to_string());
    
    let blockchain = match op_blockchain::StreamingBlockchain::new(&blockchain_path).await {
        Ok(bc) => {
            info!("Initialized StreamingBlockchain at {}", blockchain_path);
            Some(Arc::new(bc))
        }
        Err(e) => {
            error!("Failed to initialize blockchain at {}: {} - continuing without blockchain", blockchain_path, e);
            None
        }
    };

    // Create footprint channel
    let (footprint_tx, footprint_rx) = tokio::sync::mpsc::unbounded_channel();

    // State Manager with blockchain integration
    let mut state_manager = StateManager::new();
    state_manager.set_blockchain_sender(footprint_tx.clone());
    
    // Register the Full System Plugin for complete state capture
    let full_system_plugin = Arc::new(op_plugins::state_plugins::FullSystemPlugin::new());
    state_manager.register_plugin(full_system_plugin).await;
    info!("Registered FullSystemPlugin for disaster recovery");
    
    let state_manager = Arc::new(state_manager);

    // Start blockchain footprint receiver if blockchain is available
    let blockchain_handle = if let Some(bc) = blockchain.clone() {
        let bc_clone = bc.clone();
        Some(tokio::spawn(async move {
            if let Err(e) = bc_clone.start_footprint_receiver(footprint_rx).await {
                error!("Blockchain footprint receiver error: {}", e);
            }
        }))
    } else {
        // Drain the channel if no blockchain
        tokio::spawn(async move {
            let mut rx = footprint_rx;
            while rx.recv().await.is_some() {
                // Discard footprints when blockchain is not available
            }
        });
        None
    };
    
    if blockchain_handle.is_some() {
        info!("Blockchain footprint recording enabled - changes will be tracked for DR");
    }

    // Handle CLI commands if present
    if let Some(Commands::SetDesiredState { path }) = args.command {
        info!("Setting desired state from {:?}", path);
        let desired_state = state_manager.load_desired_state(&path).await?;
        let report = state_manager.apply_state(desired_state).await?;
        
        // Update blockchain state after apply
        if let Some(bc) = &blockchain {
            if let Ok(current_state) = state_manager.query_current_state().await {
                let state_value = serde_json::to_value(&current_state)?;
                if let Err(e) = bc.write_current_state(&state_value).await {
                    error!("Failed to update blockchain state: {}", e);
                }
            }
        }
        
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    info!("Starting OP D-Bus Service...");

    // Chat Actor
    let chat_config = ChatActorConfig::default();
    let (chat_actor, chat_handle) = ChatActor::with_registry(chat_config, registry.clone()).await?;
    
    // Get tracker from actor to subscribe to events
    // let tracker = chat_actor.tool_executor().tracker().clone();
    // let tracker_interface = TrackerInterface::new(tracker.as_ref().clone());

    // Spawn ChatActor in background
    let actor_handle_future = tokio::spawn(chat_actor.run());
    info!("Started Chat Actor");

    // --- 2. Setup D-Bus ---

    let dbus_future = async {
        let conn_builder = connection::Builder::system()?;
        let conn_builder = conn_builder.name("org.op_dbus.Service")?;
        
        let conn_builder = conn_builder.serve_at("/org/op_dbus/Chat", ChatInterface::new(chat_handle.clone()))?;
        // let conn_builder = conn_builder.serve_at("/org/op_dbus/ExecutionTracker", tracker_interface)?;
        let conn_builder = conn_builder.serve_at("/org/op_dbus/State", StateInterface::new(state_manager.clone()))?;
        
        let _conn = conn_builder.internal_executor(false).build().await?;
        
        info!("D-Bus interfaces exported at org.op_dbus.Service");

        // Signal Forwarding Loop
        /*
        let object_server = conn.object_server();
        let tracker_iface_ref: zbus::object_server::InterfaceRef<TrackerInterface> = object_server.interface("/org/op_dbus/ExecutionTracker").await?;
        let mut rx = tracker.subscribe();

        info!("Starting Execution Tracker signal forwarder");
        loop {
            match rx.recv().await {
                Ok(event) => {
                    use op_execution_tracker::ExecutionEvent::*;
                    let res = match event {
                        Started(ctx) => {
                            let input = ctx.metadata.get("arguments").map(|v| v.to_string()).unwrap_or_default();
                            tracker_iface_ref.get().await.execution_started(&ctx.execution_id, &ctx.tool_name, &input).await
                        }
                        Completed(id, res) => {
                            let summary = res.result.map(|v| v.to_string()).unwrap_or_default();
                            tracker_iface_ref.get().await.execution_completed(&id, res.success, &summary).await
                        }
                        StatusUpdated(id, status) => {
                            tracker_iface_ref.get().await.status_updated(&id, &status.to_string()).await
                        }
                    };
                    
                    if let Err(e) = res {
                        error!("Failed to emit D-Bus signal: {}", e);
                    }
                }
                Err(e) => {
                    warn!("Tracker subscription error: {}", e);
                    // If lagged, we might miss messages, but we should continue. 
                    // If closed, we exit loop.
                    if matches!(e, tokio::sync::broadcast::error::RecvError::Closed) {
                        break;
                    }
                }
            }
        }
        */
        std::future::pending::<()>().await;
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    // --- 3. Setup HTTP Server ---

    let http_future = async {
        let mut router_builder = RouterBuilder::new();

        // Mount MCP
        /*
        {
            let mcp_state = op_mcp::McpState::new().await?;
            router_builder = router_builder.nest("/api/mcp", "mcp", op_mcp::create_router(mcp_state));
        }
        */

        // Mount Chat (using our SHARED handle!)
        /*
        {
            let chat_state = op_chat::router::ChatState::new(chat_handle.clone());
            router_builder = router_builder.nest("/api/chat", "chat", op_chat::router::create_router(chat_state));
        }
        */

        // Mount Web UI
        /*
        {
            let web_state = op_web::AppState::new();
            router_builder = router_builder
                .nest("/api/web", "web", op_web::create_router(web_state.clone()))
                .nest("/ws", "websocket", op_web::create_websocket_router(web_state));
        }
        */

        // Mount Tools (using SHARED registry!)
        {
            let tools_state = op_tools::router::ToolsState::new(registry.clone());
            router_builder = router_builder.nest("/api/tools", "tools", op_tools::router::create_router(tools_state));
        }

        // Mount Agents
        {
            let agents_state = op_agents::router::AgentsState::new();
            router_builder = router_builder.nest("/api/agents", "agents", op_agents::router::create_router(agents_state));
        }

        // Health & Static
        router_builder = router_builder.route("/health", get(health_check));
        if let Some(static_dir) = &args.static_dir {
            router_builder = router_builder.static_dir(static_dir.clone());
        }

        let router = router_builder.build();

        let mut server_builder = HttpServer::builder()
            .bind(&args.bind)
            .https_port(args.https_port)
            .router(router)
            .cors(!args.no_cors)
            .compression(!args.no_compression);

        if args.tls_auto {
            server_builder = server_builder.https_auto();
        } else if let (Some(cert), Some(key)) = (&args.tls_cert, &args.tls_key) {
            server_builder = server_builder.https(cert.clone(), key.clone());
        }

        let server = server_builder.build()?;
        
        info!("HTTP Server listening on {}", args.bind);
        server.serve().await?;
        
        Ok::<(), anyhow::Error>(())
    };

    // --- 4. Run All ---

    tokio::select! {
        res = dbus_future => {
            error!("D-Bus loop exited: {:?}", res);
        }
        res = http_future => {
            error!("HTTP server exited: {:?}", res);
        }
        res = actor_handle_future => {
            error!("ChatActor exited: {:?}", res);
        }
    }

    Ok(())
}

async fn health_check() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "healthy",
        "service": "op-dbus-service",
        "version": env!("CARGO_PKG_VERSION")
    }))
}
