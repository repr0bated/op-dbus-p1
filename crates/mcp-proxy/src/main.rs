//! MCP Proxy — spawned by MCP clients, connects to op-dbus via gRPC.
//!
//! This is a thin shim:
//! - Reads JSON-RPC from stdin
//! - Forwards to op-dbus daemon via gRPC
//! - Writes responses to stdout
//!
//! All state lives in the daemon.

use std::io::{BufRead, Write};

use op_cache::proto::mcp_service_client::McpServiceClient;
use op_cache::proto::McpRequest;
use tonic::transport::Channel;
use tracing::{error, info};

const DEFAULT_DAEMON_ADDR: &str = "http://[::1]:50051";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let daemon_addr = std::env::var("OP_DBUS_ADDR")
        .unwrap_or_else(|_| DEFAULT_DAEMON_ADDR.to_string());

    info!("Connecting to op-dbus at {}", daemon_addr);

    let channel = Channel::from_shared(daemon_addr.clone())
        .map_err(|e| anyhow::anyhow!("Invalid daemon address {}: {}", daemon_addr, e))?
        .connect()
        .await
        .map_err(|e| {
            error!("Failed to connect to op-dbus daemon at {}: {}", daemon_addr, e);
            error!("Make sure op-dbus daemon is running");
            e
        })?;

    let mut client = McpServiceClient::new(channel);

    info!("Connected to op-dbus daemon");

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to read from stdin: {}", e);
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let json_request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                });
                writeln!(stdout, "{}", error_response)?;
                stdout.flush()?;
                continue;
            }
        };

        let grpc_request = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: json_request["method"].as_str().unwrap_or("").to_string(),
            id: json_request["id"].as_str().unwrap_or("null").to_string(),
            params: serde_json::to_vec(&json_request["params"]).unwrap_or_default(),
        };

        let response = match client.handle_request(grpc_request).await {
            Ok(resp) => resp.into_inner(),
            Err(e) => {
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": json_request["id"],
                    "error": {
                        "code": -32603,
                        "message": format!("Internal error: {}", e)
                    }
                });
                writeln!(stdout, "{}", error_response)?;
                stdout.flush()?;
                continue;
            }
        };

        let json_response = if let Some(error) = response.error {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::from_str::<serde_json::Value>(&response.id)
                    .unwrap_or(serde_json::Value::Null),
                "error": {
                    "code": error.code,
                    "message": error.message
                }
            })
        } else {
            let result: serde_json::Value = serde_json::from_slice(&response.result)
                .unwrap_or(serde_json::Value::Null);
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::from_str::<serde_json::Value>(&response.id)
                    .unwrap_or(serde_json::Value::Null),
                "result": result
            })
        };

        writeln!(stdout, "{}", json_response)?;
        stdout.flush()?;
    }

    Ok(())
}
