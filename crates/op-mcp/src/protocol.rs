//! MCP Protocol Implementation
//!
//! Provides JSON-RPC 2.0 protocol handling for Model Context Protocol.
//! This is a thin adapter that translates MCP requests to op-chat RPC calls.
//!
//! ## Compact Mode
//!
//! When a client like Gemini CLI connects, compact mode is auto-detected.
//! Instead of exposing all tools, only 4 meta-tools are exposed:
//! - list_tools, search_tools, get_tool_schema, execute_tool
//!
//! This saves ~95% of context tokens while keeping all tools accessible.

use op_chat::{ChatActorHandle, RpcRequest, RpcResponse};
use op_mcp_aggregator::{
    AggregatorConfig, Aggregator, ToolMode,
    config::ClientDetectionConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::resources::ResourceRegistry;

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
    resource_registry: ResourceRegistry,
    /// Client detection configuration
    client_detection: ClientDetectionConfig,
    /// Current client info (set during initialize)
    client_info: RwLock<Option<ClientInfo>>,
    /// Detected tool mode for current session
    tool_mode: RwLock<ToolMode>,
    /// Aggregator for compact mode (lazy initialized)
    aggregator: RwLock<Option<Arc<Aggregator>>>,
}

/// Client information from MCP initialize
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub name: String,
    pub version: Option<String>,
}

impl McpServer {
    /// Create new MCP server with ChatActor handle
    pub fn new(chat_handle: ChatActorHandle, resource_registry: ResourceRegistry) -> Self {
        let client_detection = ClientDetectionConfig::default();
        let default_mode = if client_detection.default_mode == "full" {
            ToolMode::Full
        } else {
            ToolMode::Compact
        };
        
        Self {
            chat_handle,
            allowed_namespaces: AllowedNamespaces::from_env(),
            resource_registry,
            client_detection,
            client_info: RwLock::new(None),
            tool_mode: RwLock::new(default_mode),
            aggregator: RwLock::new(None),
        }
    }
    
    /// Get the current tool mode
    pub async fn get_tool_mode(&self) -> ToolMode {
        *self.tool_mode.read().await
    }
    
    /// Check if running in compact mode
    pub async fn is_compact_mode(&self) -> bool {
        matches!(*self.tool_mode.read().await, ToolMode::Compact)
    }
    
    /// Get client info if set
    pub async fn get_client_info(&self) -> Option<ClientInfo> {
        self.client_info.read().await.clone()
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
        
        // Extract client info from params
        let client_name = request.params
            .as_ref()
            .and_then(|p| p.get("clientInfo"))
            .and_then(|ci| ci.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");
        
        let client_version = request.params
            .as_ref()
            .and_then(|p| p.get("clientInfo"))
            .and_then(|ci| ci.get("version"))
            .and_then(|v| v.as_str());
        
        // Store client info
        *self.client_info.write().await = Some(ClientInfo {
            name: client_name.to_string(),
            version: client_version.map(String::from),
        });
        
        // Auto-detect tool mode based on client
        let detected_mode = self.client_detection.detect_mode(client_name);
        *self.tool_mode.write().await = detected_mode;
        
        // Log detection result
        let mode_str = match detected_mode {
            ToolMode::Compact => "COMPACT (4 meta-tools)",
            ToolMode::Full => "FULL (all tools)",
            ToolMode::Hybrid => "HYBRID (essential + meta)",
        };
        
        info!(
            "🔌 Client connected: {} v{} -> {} mode",
            client_name,
            client_version.unwrap_or("?"),
            mode_str
        );
        
        // Check if Gemini CLI specifically
        if ClientDetectionConfig::is_gemini(client_name) {
            info!("🔷 Gemini CLI detected! Using optimized compact mode.");
        }

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
                "description": "MCP adapter for op-dbus-v2 system",
                "mode": format!("{:?}", detected_mode).to_lowercase(),
                "compact_mode": matches!(detected_mode, ToolMode::Compact)
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
        
        let tool_mode = *self.tool_mode.read().await;
        
        // In compact mode, return only the 4 meta-tools
        if matches!(tool_mode, ToolMode::Compact) {
            return self.handle_tools_list_compact(request).await;
        }

        // Full mode: return all tools from chat handle
        let response = self.chat_handle.list_tools().await;
        if response.success {
            let tools_value = response.result.unwrap_or_else(|| json!({}));
            let mut category_counts: HashMap<String, usize> = HashMap::new();
            let tools_json = tools_value
                .get("tools")
                .and_then(|tools| tools.as_array())
                .map(|tools| {
                    tools
                        .iter()
                        .filter(|tool| self.allowed_namespaces.is_allowed(tool_namespace(tool)))
                        .map(|tool| {
                            let base_category = tool
                                .get("category")
                                .and_then(|value| value.as_str())
                                .unwrap_or("general");
                            let bucketed_category =
                                bucket_category(base_category, &mut category_counts);
                            json!({
                                "name": tool.get("name").cloned().unwrap_or(Value::Null),
                                "description": tool.get("description").cloned().unwrap_or(Value::Null),
                                "inputSchema": tool.get("input_schema").cloned().unwrap_or(Value::Null),
                                "annotations": {
                                    "category": bucketed_category,
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
    
    /// Return compact mode meta-tools (4 tools instead of 750+)
    async fn handle_tools_list_compact(&self, request: McpRequest) -> McpResponse {
        debug!("Returning COMPACT mode tools (4 meta-tools)");
        
        let compact_tools = vec![
            json!({
                "name": "list_tools",
                "description": "List available tools. Filter by 'category' or 'namespace'. Returns names and descriptions. Use 'get_tool_schema' before executing.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "category": {
                            "type": "string",
                            "description": "Filter by category (e.g., 'systemd', 'network', 'filesystem', 'dbus')"
                        },
                        "namespace": {
                            "type": "string",
                            "description": "Filter by namespace (e.g., 'system', 'external')"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Max tools to return (default: 20)"
                        }
                    }
                },
                "annotations": {
                    "category": "meta",
                    "namespace": "compact"
                }
            }),
            json!({
                "name": "search_tools",
                "description": "Search for tools by keyword in names and descriptions. Use this to find relevant tools.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query (searches tool names and descriptions)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Max results (default: 10)"
                        }
                    },
                    "required": ["query"]
                },
                "annotations": {
                    "category": "meta",
                    "namespace": "compact"
                }
            }),
            json!({
                "name": "get_tool_schema",
                "description": "Get the full input schema for a specific tool. ALWAYS call this before execute_tool to see required arguments.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tool_name": {
                            "type": "string",
                            "description": "Name of the tool to get schema for"
                        }
                    },
                    "required": ["tool_name"]
                },
                "annotations": {
                    "category": "meta",
                    "namespace": "compact"
                }
            }),
            json!({
                "name": "execute_tool",
                "description": "Execute any tool by name. First use list_tools/search_tools to find tools, then get_tool_schema to see required arguments.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tool_name": {
                            "type": "string",
                            "description": "Name of the tool to execute"
                        },
                        "arguments": {
                            "type": "object",
                            "description": "Arguments to pass to the tool (see get_tool_schema for required args)"
                        }
                    },
                    "required": ["tool_name"]
                },
                "annotations": {
                    "category": "meta",
                    "namespace": "compact"
                }
            }),
        ];
        
        info!("📦 Compact mode: returning {} meta-tools (saves ~95% context tokens)", compact_tools.len());
        
        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({
                "tools": compact_tools,
                "_compact_mode": true,
                "_hint": "Use list_tools to browse, search_tools to find, get_tool_schema for args, execute_tool to run"
            })),
            error: None,
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

        let resources = self
            .resource_registry
            .list_resources()
            .iter()
            .map(|resource| {
                json!({
                    "uri": resource.uri,
                    "name": resource.name,
                    "description": resource.description,
                    "mimeType": resource.mime_type
                })
            })
            .collect::<Vec<_>>();

        let result = json!({
            "resources": resources
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

        let params = request.params.unwrap_or_else(|| json!({}));
        let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
        if uri.is_empty() {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError::new(-32602, "Missing resource uri")),
            };
        }

        match self.resource_registry.read_resource(uri).await {
            Some(content) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(json!({
                    "contents": [
                        {
                            "uri": uri,
                            "mimeType": "text/plain",
                            "text": content
                        }
                    ]
                })),
                error: None,
            },
            None => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError::new(-32602, "Unknown resource uri")),
            },
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

fn bucket_category(base: &str, counts: &mut HashMap<String, usize>) -> String {
    let count = counts.entry(base.to_string()).or_insert(0);
    let bucket = *count / 25;
    *count += 1;
    if bucket == 0 {
        base.to_string()
    } else {
        format!("{}-{}", base, bucket + 1)
    }
}
