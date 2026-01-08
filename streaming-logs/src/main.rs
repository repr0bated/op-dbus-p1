use axum::{
    extract::{ConnectInfo, Request},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response, sse::Event},
    routing::get,
    Router,
};
use axum::response::sse::Sse;
use futures::{Stream, StreamExt};
use op_core::security::{AccessZone, SecurityLevel};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, warn};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    // Build our application with routes
    let app = Router::new()
        .route("/", get(admin_page))
        .route("/logs/stream", get(log_stream_handler))
        .route("/api/info", get(server_info))
        .layer(middleware::from_fn(ip_security_middleware));

    let addr = "0.0.0.0:3000";
    info!("Server starting on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    
    // Use into_make_service_with_connect_info to enable IP extraction
    axum::serve(
        listener, 
        app.into_make_service_with_connect_info::<SocketAddr>()
    ).await.unwrap();
}

async fn admin_page() -> Html<&'static str> {
    Html(include_str!("../static/admin.html"))
}

async fn server_info() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "status": "running",
        "version": "1.0.0"
    }))
}

/// Extract IP from headers or connection info
fn extract_ip(headers: &HeaderMap, addr: Option<&SocketAddr>) -> String {
    // 1. Check X-Forwarded-For (standard proxy header)
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(s) = forwarded.to_str() {
            if let Some(client_ip) = s.split(',').next() {
                return client_ip.trim().to_string();
            }
        }
    }

    // 2. Check X-Real-IP (nginx convention)
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(s) = real_ip.to_str() {
            return s.trim().to_string();
        }
    }

    // 3. Fallback to socket address
    if let Some(addr) = addr {
        return addr.ip().to_string();
    }

    "0.0.0.0".to_string()
}

/// Check if IP is allowed (Localhost, TrustedMesh, or PrivateNetwork)
fn is_ip_allowed(ip: &str) -> bool {
    let zone = AccessZone::from_ip(ip);
    
    // We allow everything except Public
    match zone {
        AccessZone::Localhost => true,
        AccessZone::TrustedMesh => true,
        AccessZone::PrivateNetwork => true,
        AccessZone::Public => false,
    }
}

/// Middleware to restrict access to trusted IPs
async fn ip_security_middleware(
    // We try to extract ConnectInfo. It might be None if behind some proxies or misconfigured,
    // but in axum::serve with connect_info it should be present for direct connections.
    connect_info: Option<ConnectInfo<SocketAddr>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = request.headers();
    let addr = connect_info.map(|ci| ci.0);
    
    let ip = extract_ip(headers, addr.as_ref());
    
    if !is_ip_allowed(&ip) {
        warn!("⛔ Blocked access from unauthorized IP: {}", ip);
        return Err(StatusCode::FORBIDDEN);
    }
    
    // info!("✅ Access allowed from: {}", ip); // Optional: verbose logging
    Ok(next.run(request).await)
}

async fn log_stream_handler() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("New client connected to log stream");
    let (tx, rx) = tokio::sync::mpsc::channel(100);

    tokio::spawn(async move {
        // Run journalctl to tail logs for both services
        let mut child = Command::new("journalctl")
            .arg("-f") // Follow
            .arg("-u") // Unit
            .arg("streaming-logs.service")
            .arg("-u") // Unit
            .arg("op-web.service")
            .arg("--output=short-iso") // Format with timestamps
            .arg("--no-pager")
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to spawn journalctl");

        let stdout = child.stdout.take().expect("failed to open stdout");
        let mut reader = BufReader::new(stdout).lines();

        // Read lines from journalctl and send them to the SSE client
        while let Ok(Some(line)) = reader.next_line().await {
            if tx.send(Ok(Event::default().data(line))).await.is_err() {
                break; // Client disconnected
            }
        }
        
        // If the loop ends, kill the child process
        let _ = child.kill().await;
    });

    let stream = ReceiverStream::new(rx);

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}