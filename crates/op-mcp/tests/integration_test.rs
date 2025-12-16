//! Integration tests for the complete op-dbus-v2 MCP system

#[cfg(test)]
mod tests {
    use op_mcp::{McpRequest, McpServer};
    use op_chat::{ChatOrchestrator, ChatActorHandle};
    use op_tools::{ToolSystem, ToolSystemBuilder, prelude::ToolRegistry};
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_mcp_initialize() {
        let mcp_server = McpServer::new();
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "initialize".to_string(),
            params: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "test-client",
                    "version": "1.0.0"
                }
            })),
        };

        let response = mcp_server.handle_request(request).await;
        
        assert!(response.error.is_none());
        assert!(response.result.is_some());
        
        let result = response.result.unwrap();
        assert_eq!(result.get("protocolVersion").unwrap(), "2024-11-05");
    }

    #[tokio::test]
    async fn test_mcp_tools_list_with_system() {
        // Create a minimal tool system
        let tool_registry = Arc::new(RwLock::new(op_tools::ToolRegistryImpl::new()));
        let tool_executor = Arc::new(op_tools::ToolExecutorImpl::new(vec![]));
        let tool_discovery = op_tools::ToolDiscovery::disabled();
        
        let tool_system = Arc::new(ToolSystem::new(
            tool_registry.clone(),
            tool_executor,
            tool_discovery,
        ));

        // Register a built-in tool
        tool_system.initialize_with_builtins().await.unwrap();

        let mcp_server = McpServer::with_tool_system(tool_system);
        
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "tools/list".to_string(),
            params: None,
        };

        let response = mcp_server.handle_request(request).await;
        
        assert!(response.error.is_none());
        assert!(response.result.is_some());
        
        let result = response.result.unwrap();
        let tools = result.get("tools").unwrap().as_array().unwrap();
        
        // Should have at least the built-in tools
        assert!(!tools.is_empty());
    }

    #[tokio::test]
    async fn test_mcp_tools_call_echo() {
        // Create a minimal tool system
        let tool_registry = Arc::new(RwLock::new(op_tools::ToolRegistryImpl::new()));
        let tool_executor = Arc::new(op_tools::ToolExecutorImpl::new(vec![]));
        let tool_discovery = op_tools::ToolDiscovery::disabled();
        
        let tool_system = Arc::new(ToolSystem::new(
            tool_registry.clone(),
            tool_executor,
            tool_discovery,
        ));

        // Register built-in tools
        tool_system.initialize_with_builtins().await.unwrap();

        let mcp_server = McpServer::with_tool_system(tool_system);
        
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": "echo",
                "arguments": {
                    "text": "Hello, World!"
                }
            })),
        };

        let response = mcp_server.handle_request(request).await;
        
        assert!(response.error.is_none());
        assert!(response.result.is_some());
        
        let result = response.result.unwrap();
        let content = result.get("content").unwrap().as_array().unwrap();
        let text_content = content[0].get("text").unwrap().as_str().unwrap();
        
        assert!(text_content.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_mcp_unknown_method() {
        let mcp_server = McpServer::new();
        
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "unknown_method".to_string(),
            params: None,
        };

        let response = mcp_server.handle_request(request).await;
        
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, -32601); // Method not found
        assert!(error.message.contains("Method not found"));
    }

    #[tokio::test]
    async fn test_tool_system_builder() {
        let tool_system = ToolSystemBuilder::new()
            .builtin_tools(true)
            .tool_discovery(false)
            .build()
            .await
            .unwrap();

        // Verify that built-in tools were registered
        let registry = tool_system.registry();
        let registry_read = registry.read().await;
        let tools = registry_read.list_tools().await;
        
        assert!(!tools.is_empty());
        
        // Check for expected built-in tools
        let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        assert!(tool_names.contains(&"echo".to_string()));
        assert!(tool_names.contains(&"calculate".to_string()));
    }
}