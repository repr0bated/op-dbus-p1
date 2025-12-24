//! Centralized HTTP Server for all MCP services
//!
//! This consolidates multiple scattered HTTP servers into a single,
//! unified Axum server with path-based routing.

use axum::{
    Router, routing::{get, post}
};
use tower_http::{cors::CorsLayer, trace::TraceLayer, services::ServeDir};
use std::sync::Arc;

// Import handlers
mod handlers;
use handlers::*;

// Import state types from existing modules
use crate::chat_main::ChatState;

/// Unified server state containing all service states
#[derive(Clone)]
pub struct HttpServerState {
    pub chat_state: ChatState,
    // TODO: Add other service states as we migrate them
}

/// Create the unified router with path-based routing
pub fn create_router(state: HttpServerState) -> Router {
    Router::new()
        // Chat service routes - /api/chat/*
        .nest("/api/chat", handlers::chat::create_chat_router(state.chat_state))
        // TODO: Add other service routes as they're migrated
        // .nest("/api/agents", create_agents_router(state.agent_state))
        // .nest("/api/dbus", create_dbus_router(state.dbus_state))
        // .nest("/api/manager", create_manager_router(state.manager_state))

        // Legacy compatibility routes (redirect to new paths)
        .route("/api/mcp", post(|state, headers, body| async move {
            // Redirect to new chat path
            handlers::chat::mcp_handler(state, headers, body).await
        }))
        .route("/mcp-chat", post(|state, headers, body| async move {
            // Redirect to new chat path
            handlers::chat::mcp_handler(state, headers, body).await
        }))
        .route("/mcp", post(|state, headers, body| async move {
            // Redirect to new chat path
            handlers::chat::mcp_handler(state, headers, body).await
        }))

        // WebSocket routes (will be organized by service)
        .route("/ws/chat", get(handlers::chat::websocket_handler))
        // TODO: .route("/ws/events", get(events_websocket_handler))

        // Static file serving for web UI
        .nest_service("/", ServeDir::new("src/mcp/web"))

        // Global middleware
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("🚀 Starting Centralized MCP HTTP Server...");

    // TODO: Initialize all service states
    // For now, just initialize chat state
    let chat_state = crate::chat_main::initialize_chat_state().await?;

    let server_state = HttpServerState {
        chat_state,
        // TODO: Initialize other states
    };

    let app = create_router(server_state);

    // TODO: HTTPS server setup with certificate detection
    // For now, use simple HTTP server
    let addr = "0.0.0.0:8443".parse()?;
    println!("🌐 HTTP server listening on http://{}", addr);
    println!("📡 MCP endpoints:");
    println!("   - http://{}:{}/api/chat/mcp", "localhost", 8443);
    println!("   - http://{}:{}/mcp-chat (legacy)", "localhost", 8443);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// TODO: This function needs to be extracted from chat_main.rs
async fn initialize_chat_state() -> Result<ChatState, Box<dyn std::error::Error>> {
    // Placeholder - will be implemented when we extract from chat_main.rs
    Err("Not implemented yet".into())
}


//!
//! This consolidates multiple scattered HTTP servers into a single,
//! unified Axum server with path-based routing.

use axum::{
    Router, routing::{get, post}
};
use tower_http::{cors::CorsLayer, trace::TraceLayer, services::ServeDir};
use std::sync::Arc;

// Import handlers
mod handlers;
use handlers::*;

// Import state types from existing modules
use crate::chat_main::ChatState;

/// Unified server state containing all service states
#[derive(Clone)]
pub struct HttpServerState {
    pub chat_state: ChatState,
    // TODO: Add other service states as we migrate them
}

/// Create the unified router with path-based routing
pub fn create_router(state: HttpServerState) -> Router {
    Router::new()
        // Chat service routes - /api/chat/*
        .nest("/api/chat", handlers::chat::create_chat_router(state.chat_state))
        // TODO: Add other service routes as they're migrated
        // .nest("/api/agents", create_agents_router(state.agent_state))
        // .nest("/api/dbus", create_dbus_router(state.dbus_state))
        // .nest("/api/manager", create_manager_router(state.manager_state))

        // Legacy compatibility routes (redirect to new paths)
        .route("/api/mcp", post(|state, headers, body| async move {
            // Redirect to new chat path
            handlers::chat::mcp_handler(state, headers, body).await
        }))
        .route("/mcp-chat", post(|state, headers, body| async move {
            // Redirect to new chat path
            handlers::chat::mcp_handler(state, headers, body).await
        }))
        .route("/mcp", post(|state, headers, body| async move {
            // Redirect to new chat path
            handlers::chat::mcp_handler(state, headers, body).await
        }))

        // WebSocket routes (will be organized by service)
        .route("/ws/chat", get(handlers::chat::websocket_handler))
        // TODO: .route("/ws/events", get(events_websocket_handler))

        // Static file serving for web UI
        .nest_service("/", ServeDir::new("src/mcp/web"))

        // Global middleware
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("🚀 Starting Centralized MCP HTTP Server...");

    // TODO: Initialize all service states
    // For now, just initialize chat state
    let chat_state = crate::chat_main::initialize_chat_state().await?;

    let server_state = HttpServerState {
        chat_state,
        // TODO: Initialize other states
    };

    let app = create_router(server_state);

    // TODO: HTTPS server setup with certificate detection
    // For now, use simple HTTP server
    let addr = "0.0.0.0:8443".parse()?;
    println!("🌐 HTTP server listening on http://{}", addr);
    println!("📡 MCP endpoints:");
    println!("   - http://{}:{}/api/chat/mcp", "localhost", 8443);
    println!("   - http://{}:{}/mcp-chat (legacy)", "localhost", 8443);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// TODO: This function needs to be extracted from chat_main.rs
async fn initialize_chat_state() -> Result<ChatState, Box<dyn std::error::Error>> {
    // Placeholder - will be implemented when we extract from chat_main.rs
    Err("Not implemented yet".into())
}


