//! MCP Server Discovery & Client Configuration Generator
//!
//! Auto-generates MCP configuration for clients (Claude, etc.)

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use anyhow::Result;

/// MCP server configuration for clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    /// Server name
    pub name: String,
    
    /// Connection method
    #[serde(flatten)]
    pub connection: ConnectionMethod,
    
    /// Optional environment variables
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

/// How clients connect to this MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "transport")]
pub enum ConnectionMethod {
    /// HTTP/HTTPS endpoint
    #[serde(rename = "http")]
    Http {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
    
    /// Stdio command
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
    },
    
    /// SSE endpoint
    #[serde(rename = "sse")]
    Sse {
        url: String,
    },
}

/// Generate MCP configuration for popular clients
pub fn generate_client_configs(
    base_url: &str,
    expose_native: bool,
    external_servers: Vec<String>,
) -> Result<Value> {
    let mut servers = HashMap::new();

    // op-dbus native server (plugins, agents, D-Bus)
    if expose_native {
        servers.insert(
            "op-dbus-native".to_string(),
            json!({
                "transport": "http",
                "url": format!("{}/api/mcp/native", base_url),
                "description": "op-dbus native system tools (plugins, agents, D-Bus)"
            }),
        );
    }

    // Each external MCP server gets its own entry
    for server_name in external_servers {
        servers.insert(
            format!("op-dbus-{}", server_name),
            json!({
                "transport": "http",
                "url": format!("{}/api/mcp/{}", base_url, server_name),
                "description": format!("{} operations via op-dbus", server_name)
            }),
        );
    }

    Ok(json!({
        "mcpServers": servers,
        "version": "1.0",
        "generated_by": "op-dbus",
        "generated_at": chrono::Utc::now().to_rfc3339()
    }))
}

/// Generate Claude Desktop config
pub fn generate_claude_config(base_url: &str) -> Result<String> {
    let config = json!({
        "mcpServers": {
            "op-dbus": {
                "command": "http",
                "url": format!("{}/api/mcp/native", base_url),
                "description": "Linux system management via D-Bus"
            },
            "op-dbus-npm": {
                "command": "http",
                "url": format!("{}/api/mcp/npm", base_url)
            },
            "op-dbus-github": {
                "command": "http",
                "url": format!("{}/api/mcp/github", base_url)
            }
        }
    });

    serde_json::to_string_pretty(&config)
        .map_err(|e| anyhow::anyhow!("Failed to generate config: {}", e))
}

/// Generate MCP service advertisement (for auto-discovery)
pub fn generate_service_advertisement(
    hostname: &str,
    http_port: u16,
    https_port: Option<u16>,
) -> Value {
    let base_url = if let Some(https) = https_port {
        format!("https://{}:{}", hostname, https)
    } else {
        format!("http://{}:{}", hostname, http_port)
    };

    json!({
        "service": "op-dbus-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "MCP JSON-RPC 2.0",
        "endpoints": {
            "native": format!("{}/api/mcp/native", base_url),
            "discovery": format!("{}/api/mcp/_discover", base_url),
            "config": format!("{}/api/mcp/_config", base_url),
        },
        "capabilities": {
            "tools": true,
            "resources": true,
            "prompts": false,
            "sampling": false
        },
        "metadata": {
            "name": "op-dbus",
            "description": "Linux system management via D-Bus and MCP",
            "author": "op-dbus contributors"
        }
    })
}

/// Generate stdio wrapper script for CLI usage
pub fn generate_stdio_wrapper() -> String {
    r#"#!/bin/bash
# op-dbus MCP stdio wrapper
# This allows MCP clients to use op-dbus via stdio

# Check if op-dbus server is running
if ! curl -s http://localhost:8080/api/chat/health > /dev/null 2>&1; then
    echo "Error: op-dbus server not running. Start with: cargo run --bin chat-server" >&2
    exit 1
fi

# Proxy stdio to HTTP
while IFS= read -r line; do
    response=$(curl -s -X POST http://localhost:8080/api/mcp/native \
        -H "Content-Type: application/json" \
        -d "$line")
    echo "$response"
done
"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_claude_config() {
        let config = generate_claude_config("http://localhost:8080").unwrap();
        assert!(config.contains("op-dbus"));
        assert!(config.contains("/api/mcp/native"));
    }

    #[test]
    fn test_service_advertisement() {
        let ad = generate_service_advertisement("localhost", 8080, None);
        assert_eq!(ad["service"], "op-dbus-mcp");
        assert_eq!(ad["protocol"], "MCP JSON-RPC 2.0");
    }
}
