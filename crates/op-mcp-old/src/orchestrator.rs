//! Orchestrator Binary
//!
//! This binary delegates to the `op_dbus::mcp::chat::orchestrator` module.

use anyhow::Result;
use op_dbus::mcp::chat::orchestrator::{Orchestrator, LoggingEventListener};
use zbus::connection::Builder;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    log::info!("Starting orchestrator (delegating to chat module)");

    // Create orchestrator
    let orchestrator = Orchestrator::new().await?;

    // Add logging listener
    orchestrator
        .add_listener(Box::new(LoggingEventListener))
        .await;

    // Set up D-Bus connection
    let _connection = Builder::system()?
        .name("org.dbusmcp.Orchestrator")?
        .serve_at("/org/dbusmcp/Orchestrator", orchestrator)?
        .build()
        .await?;

    log::info!("Orchestrator ready on D-Bus");
    log::info!("Service: org.dbusmcp.Orchestrator");
    log::info!("Path: /org/dbusmcp/Orchestrator");

    // Keep running
    std::future::pending::<()>().await;

    Ok(())
}
