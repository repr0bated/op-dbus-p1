//! HTTP MCP Server - Exposes MCP functionality via HTTP endpoints
//!
//! This server acts as an HTTP proxy for MCP, allowing remote clients
//! like Antigravity IDE to connect via HTTPS.

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use op_agents::list_agent_types;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command as TokioCommand;
use tracing::{error, info, warn};

#[derive(Clone)]
pub struct HttpMcpServer {
    mcp_command: Vec<String>,
    chat_control: Option<ChatControlConfig>,
}

impl HttpMcpServer {
    pub fn new(mcp_command: Vec<String>) -> Self {
        Self {
            mcp_command,
            chat_control: ChatControlConfig::from_env(),
        }
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/", get(handle_sse).post(handle_mcp_request)) // Root: GET for SSE, POST for MCP
            .route("/health", get(health_check))
            .route("/mcp", post(handle_mcp_request))
            .route("/initialize", post(handle_initialize))
            .route("/tools/list", post(handle_tools_list))
            .route("/tools/call", post(handle_tools_call))
            .route("/sse", get(handle_sse))
            .with_state(Arc::new(self))
    }
}

#[derive(Deserialize, Serialize)]
struct McpRequest {
    jsonrpc: String,
    id: serde_json::Value,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct McpResponse {
    jsonrpc: String,
    id: serde_json::Value,
    result: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "mcp-http-proxy",
        "version": "1.0.0"
    }))
}

async fn handle_mcp_request(
    State(server): State<Arc<HttpMcpServer>>,
    Json(request): Json<McpRequest>,
) -> Result<Json<McpResponse>, StatusCode> {
    match server.call_mcp(&request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("MCP call failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_initialize(
    State(server): State<Arc<HttpMcpServer>>,
) -> Result<Json<McpResponse>, StatusCode> {
    let request = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: serde_json::json!(1),
        method: "initialize".to_string(),
        params: Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "mcp-http-proxy",
                "version": "1.0.0"
            }
        })),
    };

    match server.call_mcp(&request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Initialize failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_tools_list(
    State(server): State<Arc<HttpMcpServer>>,
) -> Result<Json<McpResponse>, StatusCode> {
    let request = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: serde_json::json!(2),
        method: "tools/list".to_string(),
        params: None,
    };

    match server.call_mcp(&request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Tools list failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_tools_call(
    State(server): State<Arc<HttpMcpServer>>,
    Json(params): Json<serde_json::Value>,
) -> Result<Json<McpResponse>, StatusCode> {
    let request = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: serde_json::json!(3),
        method: "tools/call".to_string(),
        params: Some(params),
    };

    match server.call_mcp(&request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Tools call failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

use axum::response::sse::{Event, Sse};
use futures::stream::{self, Stream};
use futures::StreamExt;
use std::convert::Infallible;
use std::time::Duration;

async fn handle_sse(
    State(server): State<Arc<HttpMcpServer>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut events = Vec::new();
    events.push(server.endpoint_event());

    if let Some(control_event) = server.chat_control_event() {
        events.push(control_event);
    }

    if let Some(tool_event) = server.snapshot_tools_event().await {
        events.push(tool_event);
    }

    if let Some(agent_event) = server.agents_event() {
        events.push(agent_event);
    }

    // Send collected events, then keep connection alive with periodic pings
    let initial_stream = stream::iter(events.into_iter().map(Ok::<_, Infallible>));

    let keep_alive_stream = stream::unfold(0u64, move |counter| async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let event = Event::default()
            .event("ping")
            .data(json!({ "counter": counter }).to_string());
        Some((Ok::<_, Infallible>(event), counter + 1))
    });

    let stream = initial_stream.chain(keep_alive_stream);

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keepalive"),
    )
}

impl HttpMcpServer {
    async fn call_mcp(
        &self,
        request: &McpRequest,
    ) -> Result<McpResponse, Box<dyn std::error::Error + Send + Sync>> {
        // Serialize request to JSON
        let request_json = serde_json::to_string(request)?;
        info!("MCP Request: {}", request_json);

        // Spawn MCP process with environment variables inherited
        let mut cmd = TokioCommand::new(&self.mcp_command[0]);
        cmd.args(&self.mcp_command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Inherit environment variables (including MCP_TOOL_OFFSET, MCP_TOOL_LIMIT)
        // This allows chunking to work across instances
        for (key, value) in std::env::vars() {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn()?;

        // Send request to MCP server
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(request_json.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
            drop(stdin); // Close stdin to signal end of input
        }

        // Read response from MCP server
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let mut reader = BufReader::new(stdout).lines();
        let mut error_reader = BufReader::new(stderr).lines();

        // Read stderr for errors
        let error_handle = tokio::spawn(async move {
            let mut errors = Vec::new();
            while let Some(line) = error_reader.next_line().await.unwrap_or(None) {
                warn!("MCP stderr: {}", line);
                errors.push(line);
            }
            errors
        });

        // Read stdout for response
        let mut response_line = None;
        while let Some(line) = reader.next_line().await? {
            if !line.trim().is_empty() {
                response_line = Some(line);
                break;
            }
        }

        // Wait for process to complete
        let status = child.wait().await?;
        let errors = error_handle.await.unwrap_or_default();

        if !status.success() {
            let error_msg = if !errors.is_empty() {
                format!("MCP process failed with status: {}. Errors: {}", status, errors.join(" | "))
            } else {
                format!("MCP process failed with status: {}", status)
            };
            tracing::error!("{}", error_msg);
            return Err(error_msg.into());
        }

        if !errors.is_empty() {
            tracing::warn!("MCP process completed successfully but had stderr output: {}", errors.join(" | "));
        }

        if let Some(response_str) = response_line {
            info!("MCP Response: {}", response_str);

            // Parse and return response
            let parsed: serde_json::Value = serde_json::from_str(&response_str)?;
            Ok(McpResponse {
                jsonrpc: parsed
                    .get("jsonrpc")
                    .unwrap_or(&serde_json::json!("2.0"))
                    .as_str()
                    .unwrap_or("2.0")
                    .to_string(),
                id: parsed.get("id").unwrap_or(&serde_json::json!(null)).clone(),
                result: parsed.get("result").cloned(),
                error: parsed.get("error").cloned(),
            })
        } else {
            Err("No response from MCP server".into())
        }
    }

    fn endpoint_event(&self) -> Event {
        Event::default().event("endpoint").data("/mcp")
    }

    fn chat_control_event(&self) -> Option<Event> {
        self.chat_control.as_ref().map(|control| control.as_event())
    }

    fn agents_event(&self) -> Option<Event> {
        let agents = list_agent_types();
        if agents.is_empty() {
            return None;
        }

        let payload = json!({
            "name": "op-agents",
            "description": "Agent registry exposed alongside op-mcp",
            "count": agents.len(),
            "agents": agents,
        });

        Some(Event::default().event("agents").data(payload.to_string()))
    }

    async fn snapshot_tools_event(&self) -> Option<Event> {
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!("sse-tools"),
            method: "tools/list".to_string(),
            params: None,
        };

        match self.call_mcp(&request).await {
            Ok(response) => {
                if let Some(result) = response.result {
                    let tools = result.get("tools").cloned().unwrap_or_else(|| json!([]));
                    let count = tools.as_array().map(|arr| arr.len()).unwrap_or(0);
                    let payload = json!({
                        "name": "op-mcp",
                        "description": "Aggregated tool snapshot",
                        "count": count,
                        "tools": tools,
                    });
                    Some(Event::default().event("tools").data(payload.to_string()))
                } else {
                    warn!("Snapshot tools response missing result field");
                    None
                }
            }
            Err(e) => {
                warn!("Failed to snapshot tools for SSE: {}", e);
                None
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ChatControlConfig {
    name: String,
    description: String,
    sse_url: String,
    post_url: String,
}

impl ChatControlConfig {
    fn from_env() -> Option<Self> {
        let base = std::env::var("CHAT_CONTROL_MCP_BASE_URL").ok();

        let sse_url = std::env::var("CHAT_CONTROL_MCP_SSE_URL").ok().or_else(|| {
            base.as_ref()
                .map(|b| format!("{}/sse", b.trim_end_matches('/')))
        });

        let post_url = std::env::var("CHAT_CONTROL_MCP_POST_URL").ok().or_else(|| {
            base.as_ref()
                .map(|b| format!("{}/mcp", b.trim_end_matches('/')))
        });

        let sse_url = sse_url?;
        let post_url = post_url.unwrap_or_else(|| "/api/chat/mcp".to_string());
        let name =
            std::env::var("CHAT_CONTROL_MCP_NAME").unwrap_or_else(|_| "chat-control".to_string());
        let description = std::env::var("CHAT_CONTROL_MCP_DESCRIPTION")
            .unwrap_or_else(|_| "Chat Control MCP (op-web) coordinator".to_string());

        Some(Self {
            name,
            description,
            sse_url,
            post_url,
        })
    }

    fn as_event(&self) -> Event {
        let payload = json!({
            "name": &self.name,
            "description": &self.description,
            "sseUrl": &self.sse_url,
            "postUrl": &self.post_url,
        });

        Event::default()
            .event("chat_control")
            .data(payload.to_string())
    }
}
