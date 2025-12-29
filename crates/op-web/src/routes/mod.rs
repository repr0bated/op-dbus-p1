//! API routes and route handlers

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::mcp;
use crate::sse;
use crate::state::AppState;
use crate::websocket;

#[allow(dead_code)]
pub mod chat;
#[allow(dead_code)]
pub mod llm;


/// Create the complete router with all routes
pub fn create_router(state: Arc<AppState>) -> Router {
    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // API routes
    let api_routes = Router::new()
        // Health & Status
        .route("/health", get(handlers::health::health_handler))
        .route("/status", get(handlers::status::status_handler))
        // Chat endpoints
        .route("/chat", post(handlers::chat::chat_handler))
        .route("/chat/stream", post(handlers::chat::chat_stream_handler))
        .route("/chat/history/:session_id", get(handlers::chat::get_history_handler))
        // Tool endpoints
        .route("/tools", get(handlers::tools::list_tools_handler))
        .route("/tools/:name", get(handlers::tools::get_tool_handler))
        .route("/tool", post(handlers::tools::execute_tool_handler))
        .route("/tools/:name/execute", post(handlers::tools::execute_named_tool_handler))
        // Agent endpoints
        .route("/agents", get(handlers::agents::list_agents_handler))
        .route("/agents", post(handlers::agents::spawn_agent_handler))
        .route("/agents/types", get(handlers::agents::list_agent_types_handler))
        .route("/agents/:id", get(handlers::agents::get_agent_handler))
        .route(
            "/agents/:id",
            axum::routing::delete(handlers::agents::kill_agent_handler),
        )
        // LLM endpoints
        .route("/llm/status", get(handlers::llm::llm_status_handler))
        .route("/llm/providers", get(handlers::llm::list_providers_handler))
        .route("/llm/models", get(handlers::llm::list_models_handler))
        .route("/llm/models/:provider", get(handlers::llm::list_models_for_provider_handler))
        .route("/llm/provider", post(handlers::llm::switch_provider_handler))
        .route("/llm/model", post(handlers::llm::switch_model_handler))
        // MCP discovery endpoints
        .route("/mcp/_discover", get(mcp::discover_handler))
        .route("/mcp/_config", get(mcp::config_handler))
        .route("/mcp/_config/claude", get(mcp::claude_config_handler))
        // SSE events
        .route("/events", get(sse::sse_handler))
        .with_state(state.clone());

    // MCP JSON-RPC endpoint (at root level)
    let mcp_route = Router::new()
        .route("/mcp", post(mcp::mcp_handler))
        .with_state(state.clone());

    // WebSocket route
    let ws_route = Router::new()
        .route("/ws", get(websocket::websocket_handler))
        .with_state(state.clone());

    // Main router
    let mut router = Router::new()
        .nest("/api", api_routes)
        .merge(mcp_route)
        .merge(ws_route);

    // Serve static files (WASM frontend) from an explicit path if configured.
    if let Ok(dir) = std::env::var("OP_WEB_STATIC_DIR") {
        if std::path::Path::new(&dir).exists() {
            tracing::info!("Serving static files from OP_WEB_STATIC_DIR: {}", dir);
            router = router.fallback_service(ServeDir::new(dir).append_index_html_on_directories(true));
        } else {
            tracing::warn!("OP_WEB_STATIC_DIR does not exist: {}", dir);
        }
    } else {
        // Fallback to common local build directories.
        let static_dirs = vec!["static", "dist", "public", "chat-ui/build"];
        for dir in static_dirs {
            if std::path::Path::new(dir).exists() {
                tracing::info!("Serving static files from: {}", dir);
                router = router.fallback_service(ServeDir::new(dir).append_index_html_on_directories(true));
                break;
            }
        }
    }

    router
        .layer(cors)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}
