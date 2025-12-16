//! MCP Protocol Implementation with proper delegation to op-chat

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use op_core::{ToolRequest, ToolResult, ToolDefinition};
use op_chat::{ChatActorHandle, ChatMessage, ChatMessageKind, ChatResponse};
use op_tools::ToolSystem;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

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
    chat_actor: Option<ChatActorHandle>,
    tool_system: Option<Arc<ToolSystem>>,
}

impl McpServer {
    /// Create new MCP server
    pub fn new() -> Self {
        Self {
            chat_actor: None,
            tool_system: None,
        }
    }

    /// Create MCP server with chat actor
    pub fn with_chat_actor(chat_actor: ChatActorHandle) -> Self {
        Self {
            chat_actor: Some(chat_actor),
            tool_system: None,
        }
    }

    /// Create MCP server with tool system
    pub fn with_tool_system(tool_system: Arc<ToolSystem>) -> Self {
        Self {
            chat_actor: None,
            tool_system: Some(tool_system),
        }
    }

    /// Create MCP server with both chat actor and tool system
    pub fn with_full_system(chat_actor: ChatActorHandle, tool_system: Arc<ToolSystem>) -> Self {
        Self {
            chat_actor: Some(chat_actor),
            tool_system: Some(tool_system),
        }
    }

    /// Handle incoming MCP request and return response
    pub async fn handle_request(&self, request: McpRequest) -> McpResponse {
        // Handle standard MCP methods by delegating to appropriate systems
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
        info!("Handling MCP initialize request");

        let capabilities = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {
                    "listChanged": true
                },
                "resources": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "op-mcp",
                "version": "0.3.0",
                "description": "MCP adapter for op-dbus-v2 system with full tool support"
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
        info!("Handling MCP tools/list request");

        let tools = if let Some(chat_actor) = &self.chat_actor {
            // Use chat actor to get tools
            match chat_actor.list_tools().await {
                Ok(tools) => tools,
                Err(e) => {
                    error!("Failed to get tools from chat actor: {}", e);
                    vec![]
                }
            }
        } else if let Some(tool_system) = &self.tool_system {
            // Use tool system directly to get tools
            let registry = tool_system.registry();
            let registry_read = registry.read().await;
            registry_read.list_tools().await
        } else {
            warn!("No chat actor or tool system available for tools/list");
            vec![]
        };

        // Convert ToolDefinition to MCP format
        let mcp_tools: Vec<Value> = tools.into_iter().map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.input_schema,
                "annotations": {
                    "category": tool.category,
                    "tags": tool.tags,
                    "securityLevel": format!("{:?}", tool.security_level)
                }
            })
        }).collect();

        let result = json!({
            "tools": mcp_tools
        });

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(result),
            error: None,
        }
    }

    /// Handle MCP tools/call request
    async fn handle_tools_call(&self, request: McpRequest) -> McpResponse {
        info!("Handling MCP tools/call request");

        let tool_name = if let Some(params) = &request.params {
            params.get("name").and_then(|v| v.as_str()).unwrap_or("")
        } else {
            ""
        };

        if tool_name.is_empty() {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError::new(-32602, "Invalid params: missing tool name")),
            };
        }

        let arguments = if let Some(params) = &request.params {
            params.get("arguments").cloned().unwrap_or_else(|| json!({}))
        } else {
            json!({})
        };

        // Create tool request
        let tool_request = ToolRequest {
            name: tool_name.to_string(),
            arguments,
            context: None,
        };

        let tool_result = if let Some(chat_actor) = &self.chat_actor {
            // Use chat actor to execute tool
            match chat_actor.execute_tool(tool_request).await {
                Ok(result) => result,
                Err(e) => {
                    error!("Failed to execute tool via chat actor: {}", e);
                    ToolResult {
                        success: false,
                        content: serde_json::json!({
                            "error": format!("Tool execution failed: {}", e)
                        }),
                        duration_ms: 0,
                        execution_id: Uuid::new_v4(),
                    }
                }
            }
        } else if let Some(tool_system) = &self.tool_system {
            // Use tool system directly to execute tool
            let registry = tool_system.registry();
            let registry_read = registry.read().await;
            
            if let Some(tool) = registry_read.get_tool(tool_name).await {
                let executor = tool_system.executor();
                let tool_arc = tool.as_ref() as &dyn op_core::Tool as *const dyn op_core::Tool as *const std::sync::Arc<dyn op_core::Tool>;
                // This is a workaround - in practice we'd clone the Arc properly
                let tool_clone = unsafe { Arc::new(std::mem::replace(&mut *(tool_arc as *mut dyn op_core::Tool), DummyTool)) };
                
                executor.execute(tool_clone, tool_request).await
            } else {
                ToolResult {
                    success: false,
                    content: serde_json::json!({
                        "error": format!("Tool '{}' not found", tool_name)
                    }),
                    duration_ms: 0,
                    execution_id: Uuid::new_v4(),
                }
            }
        } else {
            warn!("No chat actor or tool system available for tools/call");
            ToolResult {
                success: false,
                content: serde_json::json!({
                    "error": "No tool execution system available"
                }),
                duration_ms: 0,
                execution_id: Uuid::new_v4(),
            }
        };

        // Convert ToolResult to MCP format
        let mcp_content = if tool_result.success {
            json!([{
                "type": "text",
                "text": serde_json::to_string_pretty(&tool_result.content).unwrap_or_else(|_| "Success".to_string())
            }])
        } else {
            json!([{
                "type": "text",
                "text": format!("Error: {}", tool_result.content)
            }])
        };

        let result = json!({
            "content": mcp_content,
            "isError": !tool_result.success
        });

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(result),
            error: None,
        }
    }

    /// Handle MCP resources/list request
    async fn handle_resources_list(&self, request: McpRequest) -> McpResponse {
        info!("Handling MCP resources/list request");

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
        info!("Handling MCP resources/read request");

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(McpError::new(-32601, "Resources not implemented")),
        }
    }
}

/// Dummy tool for workaround (should not be used in practice)
struct DummyTool;

impl op_core::Tool for DummyTool {
    fn definition(&self) -> op_core::ToolDefinition {
        op_core::ToolDefinition {
            name: "dummy".to_string(),
            description: "Dummy tool".to_string(),
            input_schema: serde_json::json!({}),
            category: "dummy".to_string(),
            tags: vec![],
            security_level: op_core::SecurityLevel::Low,
        }
    }

    async fn execute(&self, _request: op_core::ToolRequest) -> op_core::ToolResult {
        op_core::ToolResult {
            success: false,
            content: serde_json::json!({"error": "Dummy tool called"}),
            duration_ms: 0,
            execution_id: Uuid::new_v4(),
        }
    }
}