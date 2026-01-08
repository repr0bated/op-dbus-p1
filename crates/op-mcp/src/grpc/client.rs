//! gRPC Client for MCP

#[cfg(feature = "grpc")]
use crate::grpc::proto::*;
#[cfg(feature = "grpc")]
use crate::grpc::proto::mcp_service_client::McpServiceClient;
use anyhow::Result;
use serde_json::Value;
use std::time::Duration;
#[cfg(feature = "grpc")]
use tonic::transport::{Channel, Endpoint};
use tracing::info;

/// gRPC client configuration
#[derive(Debug, Clone)]
pub struct GrpcClientConfig {
    pub endpoint: String,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub tls_enabled: bool,
    pub tls_domain: Option<String>,
}

impl Default for GrpcClientConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://[::1]:50051".to_string(),
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            tls_enabled: false,
            tls_domain: None,
        }
    }
}

impl GrpcClientConfig {
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }
    
    pub fn with_tls(mut self, domain: Option<String>) -> Self {
        self.tls_enabled = true;
        self.tls_domain = domain;
        self
    }
}

/// gRPC client for MCP server
#[cfg(feature = "grpc")]
pub struct GrpcClient {
    client: McpServiceClient<Channel>,
    session_id: Option<String>,
}

#[cfg(feature = "grpc")]
impl GrpcClient {
    pub async fn connect(config: GrpcClientConfig) -> Result<Self> {
        info!(endpoint = %config.endpoint, "Connecting to gRPC MCP server");
        
        let endpoint = Endpoint::from_shared(config.endpoint.clone())?
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout);
        
        let channel = endpoint.connect().await?;
        let client = McpServiceClient::new(channel);
        
        Ok(Self {
            client,
            session_id: None,
        })
    }
    
    pub async fn connect_default() -> Result<Self> {
        Self::connect(GrpcClientConfig::default()).await
    }
    
    pub async fn initialize(&mut self, client_name: &str) -> Result<InitializeResponse> {
        let request = InitializeRequest {
            client_name: client_name.to_string(),
            client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            session_id: None,
            capabilities: vec!["tools".to_string()],
        };
        
        let response = self.client.initialize(request).await?.into_inner();
        self.session_id = Some(response.session_id.clone());
        
        info!(
            session = %response.session_id,
            agents = ?response.started_agents,
            "Session initialized"
        );
        
        Ok(response)
    }
    
    pub async fn health(&mut self) -> Result<HealthResponse> {
        let response = self.client.health(HealthRequest {}).await?.into_inner();
        Ok(response)
    }
    
    pub async fn list_tools(
        &mut self,
        category: Option<&str>,
        query: Option<&str>,
        limit: u32,
    ) -> Result<ListToolsResponse> {
        let request = ListToolsRequest {
            category: category.map(String::from),
            query: query.map(String::from),
            limit,
            offset: 0,
        };
        
        let response = self.client.list_tools(request).await?.into_inner();
        Ok(response)
    }
    
    pub async fn call_tool(&mut self, tool_name: &str, arguments: Value) -> Result<CallToolResponse> {
        let request = CallToolRequest {
            tool_name: tool_name.to_string(),
            arguments_json: arguments.to_string(),
            session_id: self.session_id.clone(),
            timeout_ms: None,
        };
        
        let response = self.client.call_tool(request).await?.into_inner();
        Ok(response)
    }
    
    pub async fn call_tool_streaming(
        &mut self,
        tool_name: &str,
        arguments: Value,
    ) -> Result<impl futures::Stream<Item = Result<ToolOutput, tonic::Status>>> {
        let request = CallToolRequest {
            tool_name: tool_name.to_string(),
            arguments_json: arguments.to_string(),
            session_id: self.session_id.clone(),
            timeout_ms: None,
        };
        
        let response = self.client.call_tool_streaming(request).await?;
        Ok(response.into_inner())
    }
    
    pub async fn subscribe(
        &mut self,
        event_types: Vec<String>,
    ) -> Result<impl futures::Stream<Item = Result<McpEvent, tonic::Status>>> {
        let request = SubscribeRequest {
            event_types,
            session_id: self.session_id.clone(),
        };
        
        let response = self.client.subscribe(request).await?;
        Ok(response.into_inner())
    }
    
    pub async fn call_raw(&mut self, method: &str, params: Option<Value>) -> Result<McpResponse> {
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(uuid::Uuid::new_v4().to_string()),
            method: method.to_string(),
            params_json: params.map(|p| p.to_string()),
        };
        
        let response = self.client.call(request).await?.into_inner();
        Ok(response)
    }
    
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
}
