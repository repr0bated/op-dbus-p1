//! D-Bus MCP Bridge Binary
//!
//! This binary delegates to the `op_dbus::mcp::chat::dbus_control` module.

use anyhow::Result;
use op_dbus::mcp::chat::dbus_control::{DbusMcpBridge, McpRequest};
use std::io::{self, BufRead, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Parse command line arguments
    let mut service_name = String::new();
    let mut use_system_bus = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--service" => {
                if i + 1 < args.len() {
                    service_name = args[i + 1].clone();
                    i += 1;
                }
            }
            "--system" => {
                use_system_bus = true;
            }
            _ => {}
        }
        i += 1;
    }

    // Fallback to env var if not provided via args
    if service_name.is_empty() {
        service_name = std::env::var("DBUS_SERVICE").unwrap_or_else(|_| {
            eprintln!("Usage: dbus-mcp-bridge --service <service-name> [--system]");
            eprintln!("   or: DBUS_SERVICE=<name> dbus-mcp-bridge");
            std::process::exit(1);
        });
    }

    let bridge = DbusMcpBridge::new(service_name, use_system_bus).await?;

    eprintln!("D-Bus MCP Bridge ready (stdio mode)");
    eprintln!("Waiting for MCP requests...\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;

        if line.trim().is_empty() {
            continue;
        }

        let request: McpRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to parse request: {}", e);
                continue;
            }
        };

        let response = bridge.handle_request(request).await;
        let response_json = serde_json::to_string(&response)?;

        writeln!(stdout, "{}", response_json)?;
        stdout.flush()?;
    }

    Ok(())
}
