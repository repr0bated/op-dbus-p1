//! MCP Service — handles MCP tool calls via gRPC.

use std::sync::Arc;

use op_cache::proto::{
    mcp_service_server::McpService, ListToolsRequest, ListToolsResponse, McpError, McpRequest,
    McpResponse, McpTool,
};
use op_tools::ToolRegistry;
use serde_json::Value;
use tonic::{Request, Response, Status};
use tracing::debug;

pub struct McpServiceImpl {
    registry: Arc<ToolRegistry>,
}

impl McpServiceImpl {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    async fn list_tools_internal(&self) -> Vec<McpTool> {
        let definitions = self.registry.list().await;
        definitions
            .into_iter()
            .map(|definition| McpTool {
                name: definition.name,
                description: definition.description,
                input_schema: serde_json::to_vec(&definition.input_schema).unwrap_or_default(),
            })
            .collect()
    }

    async fn handle_tool_call(&self, params: &Value) -> Result<Vec<u8>, McpError> {
        let tool_name = params["name"].as_str().unwrap_or("");
        if tool_name.is_empty() {
            return Err(McpError {
                code: -32602,
                message: "Invalid params: tool name is required".to_string(),
                data: Vec::new(),
            });
        }
        
        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

        let tool = self.registry.get(tool_name).await.ok_or_else(|| McpError {
            code: -32601,
            message: format!("Unknown tool: {}", tool_name),
            data: Vec::new(),
        })?;

        let result = tool.execute(arguments).await.map_err(|err| McpError {
            code: -32603,
            message: format!("Tool execution failed: {}", err),
            data: Vec::new(),
        })?;

        let result = serde_json::json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&result).unwrap_or_default()
            }]
        });

        Ok(serde_json::to_vec(&result).unwrap_or_default())
    }
}

#[tonic::async_trait]
impl McpService for McpServiceImpl {
    async fn handle_request(
        &self,
        request: Request<McpRequest>,
    ) -> Result<Response<McpResponse>, Status> {
        let req = request.into_inner();
        debug!("MCP request: method={}", req.method);

        let result = match req.method.as_str() {
            "tools/list" => {
                let definitions = self.registry.list().await;
                let tools: Vec<Value> = definitions
                    .into_iter()
                    .map(|definition| {
                        serde_json::json!({
                            "name": definition.name,
                            "description": definition.description,
                            "inputSchema": definition.input_schema,
                        })
                    })
                    .collect();
                let result = serde_json::json!({ "tools": tools });
                Ok(serde_json::to_vec(&result).unwrap_or_default())
            }
            "tools/call" => {
                let params: Value = serde_json::from_slice(&req.params)
                    .unwrap_or(Value::Null);
                self.handle_tool_call(&params).await
            }
            _ => Err(McpError {
                code: -32601,
                message: format!("Method not found: {}", req.method),
                data: Vec::new(),
            }),
        };

        let response = match result {
            Ok(result) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result,
                error: None,
            },
            Err(error) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: Vec::new(),
                error: Some(error),
            },
        };

        Ok(Response::new(response))
    }

    async fn list_tools(
        &self,
        _request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        let tools = self.list_tools_internal().await;
        Ok(Response::new(ListToolsResponse { tools }))
    }
}
