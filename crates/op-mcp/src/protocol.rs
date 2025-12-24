//! MCP Protocol Implementation
//!
//! Provides JSON-RPC 2.0 protocol handling for Model Context Protocol.
//! This is a thin adapter that translates MCP requests to op-chat RPC calls.

use op_chat::{ChatActorHandle, RpcRequest, RpcResponse};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::env;
use tracing::{debug, error};
use uuid::Uuid;

/// MCP JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

/// MCP JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<McpError>,
}

/// MCP Error type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

impl McpError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// MCP Server that delegates to op-chat
pub struct McpServer {
    chat_handle: ChatActorHandle,
    allowed_namespaces: AllowedNamespaces,
}

impl McpServer {
    /// Create new MCP server with ChatActor handle
    pub fn new(chat_handle: ChatActorHandle) -> Self {
        Self {
            chat_handle,
            allowed_namespaces: AllowedNamespaces::from_env(),
        }
    }

    /// Handle incoming MCP request and return response
    pub async fn handle_request(&self, request: McpRequest) -> McpResponse {
        debug!("Handling MCP request: {}", request.method);

        // Handle standard MCP methods
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request).await,
            "tools/list" => self.handle_tools_list(request).await,
            "tools/call" => self.handle_tools_call(request).await,
            "resources/list" => self.handle_resources_list(request).await,
            "resources/read" => self.handle_resources_read(request).await,
            _ => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError::new(-32601, "Method not found")),
            },
        }
    }

    /// Handle MCP initialize request
    async fn handle_initialize(&self, request: McpRequest) -> McpResponse {
        debug!("MCP initialize request");

        // Return server capabilities
        let capabilities = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {
                    "listChanged": false
                },
                "resources": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "op-mcp",
                "version": "0.2.0",
                "description": "MCP adapter for op-dbus-v2 system"
            }
        });

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(capabilities),
            error: None,
        }
    }

    /// Handle MCP tools/list request
    async fn handle_tools_list(&self, request: McpRequest) -> McpResponse {
        debug!("MCP tools/list request");

        let response = self.chat_handle.list_tools().await;
        if response.success {
            let tools_value = response.result.unwrap_or_else(|| json!({}));
            let tools_json = tools_value
                .get("tools")
                .and_then(|tools| tools.as_array())
                .map(|tools| {
                    tools
                        .iter()
                        .filter(|tool| self.allowed_namespaces.is_allowed(tool_namespace(tool)))
                        .map(|tool| {
                            json!({
                                "name": tool.get("name").cloned().unwrap_or(Value::Null),
                                "description": tool.get("description").cloned().unwrap_or(Value::Null),
                                "inputSchema": tool.get("input_schema").cloned().unwrap_or(Value::Null),
                                "annotations": {
                                    "category": tool.get("category").cloned().unwrap_or(Value::Null),
                                    "tags": tool.get("tags").cloned().unwrap_or(Value::Null),
                                    "namespace": tool.get("namespace").cloned().unwrap_or(Value::Null)
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let result = json!({
                "tools": tools_json
            });

            McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(result),
                error: None,
            }
        } else {
            let msg = response.error.unwrap_or_else(|| "Failed to list tools".to_string());
            error!("Failed to list tools: {}", msg);
            McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError::new(-32603, msg)),
            }
        }
    }

    /// Handle MCP tools/call request
    async fn handle_tools_call(&self, request: McpRequest) -> McpResponse {
        debug!("MCP tools/call request");

        let default_params = json!({});
        let params = request.params.as_ref().unwrap_or(&default_params);
        let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let default_args = json!({});
        let arguments = params.get("arguments").unwrap_or(&default_args);

        if tool_name.is_empty() {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError::new(-32602, "Missing tool name")),
            };
        }

        if !self.is_tool_name_allowed(tool_name).await {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError::new(-32001, "Tool not permitted by namespace policy")),
            };
        }

        // Create tool request for op-chat
        let tool_request = op_core::ToolRequest::new(tool_name, arguments.clone());

        let response = self.chat_handle.execute_tool(tool_request).await;
        if response.success {
            let content = response.result.unwrap_or(Value::Null);
            let mcp_result = json!({
                "content": [
                    {
                        "type": "text",
                        "text": serde_json::to_string_pretty(&content).unwrap_or_default()
                    }
                ],
                "isError": false,
                "metadata": {
                    "execution_id": response.execution_id
                }
            });

            McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(mcp_result),
                error: None,
            }
        } else {
            let msg = response.error.unwrap_or_else(|| "Tool execution failed".to_string());
            error!("Failed to execute tool '{}': {}", tool_name, msg);
            McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError::new(-32603, msg)),
            }
        }
    }

    /// Handle MCP resources/list request
    async fn handle_resources_list(&self, request: McpRequest) -> McpResponse {
        debug!("MCP resources/list request");

        // Return empty resources for now - can be extended with embedded docs
        let result = json!({
            "resources": []
        });

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(result),
            error: None,
        }
    }

    /// Handle MCP resources/read request
    async fn handle_resources_read(&self, request: McpRequest) -> McpResponse {
        debug!("MCP resources/read request");

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(McpError::new(-32601, "Resources not implemented")),
        }
    }

    async fn is_tool_name_allowed(&self, tool_name: &str) -> bool {
        if self.allowed_namespaces.allow_all {
            return true;
        }

        let response = self.chat_handle.list_tools().await;
        if !response.success {
            return false;
        }

        let tools_value = response.result.unwrap_or_else(|| json!({}));
        let tools = tools_value.get("tools").and_then(|t| t.as_array());
        let Some(tools) = tools else {
            return false;
        };

        for tool in tools {
            let name = tool.get("name").and_then(|v| v.as_str());
            if name == Some(tool_name) {
                let namespace = tool_namespace(tool);
                return self.allowed_namespaces.is_allowed(namespace);
            }
        }

        false
    }
}

/// Convert op-chat RpcRequest to MCP format (for internal use if needed)
impl From<RpcRequest> for McpRequest {
    fn from(rpc: RpcRequest) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(Uuid::new_v4())),
            method: "rpc_call".to_string(),
            params: Some(match rpc {
                RpcRequest::ListTools { .. } => json!({"type": "list_tools"}),
                RpcRequest::ExecuteTool { name, arguments, .. } => json!({
                    "type": "execute_tool",
                    "tool_name": name,
                    "arguments": arguments
                }),
                _ => json!({"type": "unknown"}),
            }),
        }
    }
}

/// Convert op-chat RpcResponse to MCP format (for internal use if needed)
impl From<RpcResponse> for McpResponse {
    fn from(rpc: RpcResponse) -> Self {
        let error = rpc.error.map(|msg| McpError::new(-32000, msg));
        let result = if error.is_none() { rpc.result } else { None };

        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(Uuid::new_v4())),
            result,
            error,
        }
    }
}

struct AllowedNamespaces {
    allow_all: bool,
    allowed: HashSet<String>,
}

impl AllowedNamespaces {
    fn from_env() -> Self {
        let raw = env::var("OP_MCP_ALLOWED_NAMESPACES").unwrap_or_default();
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "*" {
            return Self {
                allow_all: true,
                allowed: HashSet::new(),
            };
        }

        let allowed = trimmed
            .split(',')
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<HashSet<_>>();

        if allowed.is_empty() {
            return Self {
                allow_all: true,
                allowed,
            };
        }

        Self {
            allow_all: false,
            allowed,
        }
    }

    fn is_allowed(&self, namespace: &str) -> bool {
        self.allow_all || self.allowed.contains(namespace)
    }
}

fn tool_namespace(tool: &Value) -> &str {
    tool.get("namespace")
        .and_then(|value| value.as_str())
        .unwrap_or("system")
}
