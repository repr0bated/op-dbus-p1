//! MCP Server with Lazy Tool Loading
//!
//! This module implements the MCP JSON-RPC server with integrated
//! lazy tool loading support.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, info};

use crate::lazy_tools::{get_mcp_tool_list, LazyToolConfig, LazyToolManager};
use op_tools::tool::Tool;

/// MCP Server configuration
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Server name
    pub name: String,
    /// Server version
    pub version: String,
    /// Lazy tool loading config
    pub tool_config: LazyToolConfig,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            name: "op-mcp".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            tool_config: LazyToolConfig::default(),
        }
    }
}

/// MCP JSON-RPC Request
#[derive(Debug, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// MCP JSON-RPC Response
#[derive(Debug, Serialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

/// MCP Error
#[derive(Debug, Serialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl McpResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(McpError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// MCP Server with lazy tool loading
pub struct McpServer {
    config: McpServerConfig,
    tool_manager: Arc<LazyToolManager>,
}

impl McpServer {
    /// Create a new MCP server
    pub async fn new(config: McpServerConfig) -> Result<Self> {
        let tool_manager = Arc::new(LazyToolManager::with_config(config.tool_config.clone()).await?);

        Ok(Self {
            config,
            tool_manager,
        })
    }

    /// Run the server on stdio
    pub async fn run_stdio(&self) -> Result<()> {
        info!("Starting MCP server: {} v{}", self.config.name, self.config.version);

        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }

            debug!("Received: {}", line);

            let response = match serde_json::from_str::<McpRequest>(&line) {
                Ok(request) => self.handle_request(request).await,
                Err(e) => McpResponse::error(None, -32700, format!("Parse error: {}", e)),
            };

            let response_json = serde_json::to_string(&response)?;
            debug!("Sending: {}", response_json);

            stdout.write_all(response_json.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }

        info!("MCP server shutting down");
        Ok(())
    }

    /// Handle a single request
    async fn handle_request(&self, request: McpRequest) -> McpResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id, request.params).await,
            "initialized" => McpResponse::success(request.id, json!({})),
            "tools/list" => self.handle_tools_list(request.id, request.params).await,
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            "resources/list" => self.handle_resources_list(request.id).await,
            "resources/read" => self.handle_resources_read(request.id, request.params).await,
            "ping" => McpResponse::success(request.id, json!({})),
            _ => McpResponse::error(request.id, -32601, format!("Method not found: {}", request.method)),
        }
    }

    /// Handle initialize request
    async fn handle_initialize(&self, id: Option<Value>, _params: Option<Value>) -> McpResponse {
        let (registry_stats, discovery_stats) = self.tool_manager.stats().await;

        McpResponse::success(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {
                        "listChanged": true
                    },
                    "resources": {
                        "subscribe": false,
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": self.config.name,
                    "version": self.config.version
                },
                "_meta": {
                    "lazyLoading": true,
                    "totalToolsAvailable": discovery_stats.total_tools,
                    "toolsCurrentlyLoaded": registry_stats.currently_loaded
                }
            }),
        )
    }

    /// Handle tools/list request
    async fn handle_tools_list(&self, id: Option<Value>, params: Option<Value>) -> McpResponse {
        // Extract pagination parameters
        let (offset, limit, context) = if let Some(p) = params {
            let offset = p.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
            let context = p.get("context").and_then(|v| v.as_str()).map(String::from);
            (offset, limit, context)
        } else {
            (0, 100, None)
        };

        let tool_list = get_mcp_tool_list(
            &self.tool_manager,
            offset,
            limit,
            context.as_deref(),
        )
        .await;

        McpResponse::success(
            id,
            json!({
                "tools": tool_list.tools,
                "_meta": {
                    "total": tool_list.total,
                    "offset": tool_list.offset,
                    "limit": tool_list.limit,
                    "hasMore": tool_list.has_more
                }
            }),
        )
    }

    /// Handle tools/call request
    async fn handle_tools_call(&self, id: Option<Value>, params: Option<Value>) -> McpResponse {
        let params = match params {
            Some(p) => p,
            None => return McpResponse::error(id, -32602, "Missing params"),
        };

        let tool_name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return McpResponse::error(id, -32602, "Missing tool name"),
        };

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        // Get tool (lazy loading if needed)
        let tool = match self.tool_manager.get_tool(tool_name).await {
            Some(t) => t,
            None => return McpResponse::error(id, -32602, format!("Tool not found: {}", tool_name)),
        };

        // Execute tool
        match tool.execute(arguments).await {
            Ok(result) => McpResponse::success(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
                    }],
                    "isError": false
                }),
            ),
            Err(e) => McpResponse::success(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Error: {}", e)
                    }],
                    "isError": true
                }),
            ),
        }
    }

    /// Handle resources/list request
    async fn handle_resources_list(&self, id: Option<Value>) -> McpResponse {
        // Return embedded documentation resources
        McpResponse::success(
            id,
            json!({
                "resources": [
                    {
                        "uri": "docs://architecture",
                        "name": "Architecture Documentation",
                        "description": "System architecture overview",
                        "mimeType": "text/markdown"
                    },
                    {
                        "uri": "docs://tools",
                        "name": "Tool Documentation",
                        "description": "Available tools and usage",
                        "mimeType": "text/markdown"
                    }
                ]
            }),
        )
    }

    /// Handle resources/read request
    async fn handle_resources_read(
        &self,
        id: Option<Value>,
        params: Option<Value>,
    ) -> McpResponse {
        let uri = params
            .as_ref()
            .and_then(|p| p.get("uri"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let content = match uri {
            "docs://architecture" => ARCHITECTURE_DOC,
            "docs://tools" => "# Tools\n\nUse tools/list to see available tools.",
            _ => return McpResponse::error(id, -32602, format!("Resource not found: {}", uri)),
        };

        McpResponse::success(
            id,
            json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "text/markdown",
                    "text": content
                }]
            }),
        )
    }

    /// Get tool manager reference
    pub fn tool_manager(&self) -> Arc<LazyToolManager> {
        Arc::clone(&self.tool_manager)
    }
}

/// Embedded architecture documentation
const ARCHITECTURE_DOC: &str = r#"# op-mcp Architecture

## Overview

op-mcp is a clean MCP (Model Context Protocol) server that provides:

1. **MCP JSON-RPC 2.0 Protocol** - Standard MCP protocol over stdio
2. **Lazy Tool Loading** - Tools loaded on-demand with LRU caching
3. **Discovery System** - Multiple sources for tool discovery
4. **External MCP Aggregation** - Connect to other MCP servers

## Key Components

### McpServer
The main server component that handles MCP JSON-RPC 2.0 protocol.

### LazyToolManager  
Manages tool loading with on-demand loading and LRU caching.

### ToolRegistry (from op-tools)
Provides tool storage with usage tracking and LRU eviction.

### ToolDiscoverySystem (from op-tools)
Manages tool discovery from multiple sources.

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MCP_MAX_TOOLS` | 50 | Max tools to keep loaded |
| `MCP_IDLE_SECS` | 300 | Idle time before eviction |
| `MCP_DBUS_DISCOVERY` | true | Enable D-Bus discovery |
| `MCP_PLUGIN_DISCOVERY` | true | Enable plugin discovery |
| `MCP_AGENT_DISCOVERY` | true | Enable agent discovery |
| `MCP_PRELOAD` | true | Preload essential tools |
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_creation() {
        let config = McpServerConfig {
            tool_config: LazyToolConfig {
                enable_dbus_discovery: false,
                enable_plugin_discovery: false,
                enable_agent_discovery: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let server = McpServer::new(config).await.unwrap();
        assert_eq!(server.config.name, "op-mcp");
    }

    #[tokio::test]
    async fn test_initialize_handler() {
        let config = McpServerConfig {
            tool_config: LazyToolConfig {
                enable_dbus_discovery: false,
                enable_plugin_discovery: false,
                enable_agent_discovery: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let server = McpServer::new(config).await.unwrap();

        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "initialize".to_string(),
            params: None,
        };

        let response = server.handle_request(request).await;
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_tools_list_handler() {
        let config = McpServerConfig {
            tool_config: LazyToolConfig {
                enable_dbus_discovery: false,
                enable_plugin_discovery: false,
                enable_agent_discovery: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let server = McpServer::new(config).await.unwrap();

        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(2)),
            method: "tools/list".to_string(),
            params: None,
        };

        let response = server.handle_request(request).await;
        assert!(response.result.is_some());

        let result = response.result.unwrap();
        assert!(result.get("tools").is_some());
    }
}
