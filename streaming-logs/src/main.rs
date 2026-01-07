use axum::{
    response::{Html, IntoResponse, sse::Event},
    routing::get,
    Router,
};
use axum::response::sse::Sse;
use futures::{Stream, StreamExt};
use std::convert::Infallible;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_stream::wrappers::ReceiverStream;
use tracing::info;

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
        .route("/api/info", get(server_info));

    let addr = "0.0.0.0:3000";
    info!("Server starting on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
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