//! Chat Actor - Central message processor
//!
//! The ChatActor is the "brain" of op-dbus-v2. It:
//! - Receives RPC requests from various frontends (web, MCP, CLI)
//! - Routes requests to appropriate handlers (tools, D-Bus, LLM)
//! - Manages sessions and conversation state
//! - **Executes tools with full tracking and accountability**
//! - Provides unified JSON responses

use anyhow::Result;
use op_execution_tracker::{ExecutionTracker, ExecutionMetrics, ExecutionTelemetry};
// use op_introspection::IntrospectionService;
use op_tools::ToolRegistry;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info};

use crate::session::SessionManager;
use crate::tool_executor::TrackedToolExecutor;

/// Configuration for ChatActor
#[derive(Debug, Clone)]
pub struct ChatActorConfig {
    /// Maximum concurrent requests
    pub max_concurrent: usize,
    /// Request timeout in seconds
    pub request_timeout_secs: u64,
    /// Enable execution tracking
    pub enable_tracking: bool,
    /// Maximum execution history to keep
    pub max_history: usize,
}

impl Default for ChatActorConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            request_timeout_secs: 300,
            enable_tracking: true,
            max_history: 1000,
        }
    }
}

/// RPC Request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RpcRequest {
    /// List available tools
    ListTools {
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Execute a tool
    ExecuteTool {
        name: String,
        arguments: Value,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Get tool definition
    GetTool { name: String },

    /// Chat with LLM
    Chat {
        message: String,
        session_id: String,
        #[serde(default)]
        model: Option<String>,
    },

    /// Get execution history
    GetHistory {
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Get execution statistics
    GetStats,

    /// Health check
    Health,

    /// Introspect D-Bus service
    Introspect {
        service: String,
        #[serde(default)]
        bus_type: Option<String>,
    },

    /// Call D-Bus method
    DbusCall {
        service: String,
        path: String,
        interface: String,
        method: String,
        #[serde(default)]
        args: Value,
        #[serde(default)]
        bus_type: Option<String>,
    },
}

/// RPC Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
}

impl RpcResponse {
    pub fn success(result: Value) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
            execution_id: None,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            result: None,
            error: Some(msg.into()),
            execution_id: None,
        }
    }

    pub fn with_execution_id(mut self, id: &str) -> Self {
        self.execution_id = Some(id.to_string());
        self
    }
}

/// Message sent to the actor
struct ActorMessage {
    request: RpcRequest,
    respond_to: oneshot::Sender<RpcResponse>,
}

/// Handle to interact with ChatActor
#[derive(Clone)]
pub struct ChatActorHandle {
    sender: mpsc::Sender<ActorMessage>,
}

impl ChatActorHandle {
    /// Send a request and wait for response
    pub async fn call(&self, request: RpcRequest) -> Result<RpcResponse> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ActorMessage {
                request,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Actor channel closed"))?;

        rx.await.map_err(|_| anyhow::anyhow!("Actor dropped"))
    }

    /// Fire and forget (for notifications)
    pub async fn notify(&self, request: RpcRequest) -> Result<()> {
        let (tx, _rx) = oneshot::channel();
        self.sender
            .send(ActorMessage {
                request,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Actor channel closed"))
    }

    // === Convenience methods ===

    pub async fn health(&self) -> RpcResponse {
        self.call(RpcRequest::Health).await.unwrap_or_else(|e| RpcResponse::error(e.to_string()))
    }

    pub async fn list_tools(&self) -> RpcResponse {
        self.call(RpcRequest::ListTools { offset: None, limit: None }).await.unwrap_or_else(|e| RpcResponse::error(e.to_string()))
    }

    pub async fn execute_tool(&self, request: op_core::ToolRequest) -> RpcResponse {
        self.call(RpcRequest::ExecuteTool {
            name: request.tool_name.clone(),
            arguments: request.arguments,
            session_id: None,
        }).await.unwrap_or_else(|e| RpcResponse::error(e.to_string()))
    }

    pub async fn chat(&self, session_id: Option<String>, message: &str) -> RpcResponse {
        let session_id = session_id.unwrap_or_else(|| "default".to_string());
        self.call(RpcRequest::Chat {
            message: message.to_string(),
            session_id,
            model: None,
        }).await.unwrap_or_else(|e| RpcResponse::error(e.to_string()))
    }

    pub async fn list_services(&self, _bus_type: op_core::BusType) -> RpcResponse {
        // TODO: Add RpcRequest::ListServices
        RpcResponse::error("List services not supported via RPC yet")
    }

    pub async fn introspect(&self, bus_type: op_core::BusType, service: &str, _path: &str) -> RpcResponse {
        let bus_str = match bus_type {
            op_core::BusType::Session => "session",
            op_core::BusType::System => "system",
        };
        
        self.call(RpcRequest::Introspect {
            service: service.to_string(),
            bus_type: Some(bus_str.to_string()),
        }).await.unwrap_or_else(|e| RpcResponse::error(e.to_string()))
    }
}

/// The Chat Actor - central processing unit
pub struct ChatActor {
    // config: ChatActorConfig,
    tool_executor: Arc<TrackedToolExecutor>,
    tool_registry: Arc<ToolRegistry>,
    // introspection: Arc<IntrospectionService>,
    session_manager: Arc<SessionManager>,
    receiver: mpsc::Receiver<ActorMessage>,
}

impl ChatActor {
    /// Create a new ChatActor
    pub async fn new(config: ChatActorConfig) -> Result<(Self, ChatActorHandle)> {
        let (sender, receiver) = mpsc::channel(config.max_concurrent);

        // Initialize components
        let tool_registry = Arc::new(ToolRegistry::new());

        // Metrics and Telemetry
        let metrics = Arc::new(ExecutionMetrics::new().unwrap_or_default());
        let telemetry = Arc::new(ExecutionTelemetry::new("op-chat"));

        let tracker = Arc::new(ExecutionTracker::with_history_size(
            metrics, 
            telemetry, 
            config.max_history
        ));
        
        let tool_executor = Arc::new(TrackedToolExecutor::new(
            tool_registry.clone(),
            tracker,
        ));
        // let introspection = Arc::new(IntrospectionService::new());
        let session_manager = Arc::new(SessionManager::new());

        let actor = Self {
            // config,
            tool_executor,
            tool_registry,
            // introspection,
            session_manager,
            receiver,
        };

        let handle = ChatActorHandle { sender };

        Ok((actor, handle))
    }

    /// Create with existing tool registry
    pub async fn with_registry(
        config: ChatActorConfig,
        tool_registry: Arc<ToolRegistry>,
    ) -> Result<(Self, ChatActorHandle)> {
        let (sender, receiver) = mpsc::channel(config.max_concurrent);

        // Metrics and Telemetry
        let metrics = Arc::new(ExecutionMetrics::new().unwrap_or_default());
        let telemetry = Arc::new(ExecutionTelemetry::new("op-chat"));

        let tracker = Arc::new(ExecutionTracker::with_history_size(
            metrics, 
            telemetry, 
            config.max_history
        ));
        
        let tool_executor = Arc::new(TrackedToolExecutor::new(
            tool_registry.clone(),
            tracker,
        ));
        // let introspection = Arc::new(IntrospectionService::new());
        let session_manager = Arc::new(SessionManager::new());

        let actor = Self {
            // config,
            tool_executor,
            tool_registry,
            // introspection,
            session_manager,
            receiver,
        };

        let handle = ChatActorHandle { sender };

        Ok((actor, handle))
    }

    /// Get tool registry for external registration
    pub fn tool_registry(&self) -> &Arc<ToolRegistry> {
        &self.tool_registry
    }

    /// Get tool executor
    pub fn tool_executor(&self) -> &Arc<TrackedToolExecutor> {
        &self.tool_executor
    }

    // pub fn introspection(&self) -> &Arc<IntrospectionService> {
    //     &self.introspection
    // }

    /// Get session manager
    pub fn session_manager(&self) -> &Arc<SessionManager> {
        &self.session_manager
    }

    /// Run the actor event loop
    pub async fn run(mut self) {
        info!("ChatActor started");

        while let Some(msg) = self.receiver.recv().await {
            let response = self.handle_request(msg.request).await;
            let _ = msg.respond_to.send(response);
        }

        info!("ChatActor stopped");
    }

    /// Handle a single request
    async fn handle_request(&self, request: RpcRequest) -> RpcResponse {
        debug!(request = ?request, "Handling request");

        match request {
            RpcRequest::ListTools { offset, limit } => self.handle_list_tools(offset, limit).await,

            RpcRequest::ExecuteTool {
                name,
                arguments,
                session_id,
            } => self.handle_execute_tool(&name, arguments, session_id).await,

            RpcRequest::GetTool { name } => self.handle_get_tool(&name).await,

            RpcRequest::Chat {
                message,
                session_id,
                model,
            } => self.handle_chat(&message, &session_id, model).await,

            RpcRequest::GetHistory { limit } => {
                self.handle_get_history(limit.unwrap_or(50)).await
            }

            RpcRequest::GetStats => self.handle_get_stats().await,

            RpcRequest::Health => self.handle_health().await,

            RpcRequest::Introspect { service, bus_type } => {
                self.handle_introspect(&service, bus_type).await
            }

            RpcRequest::DbusCall {
                service,
                path,
                interface,
                method,
                args,
                bus_type,
            } => {
                self.handle_dbus_call(&service, &path, &interface, &method, args, bus_type)
                    .await
            }
        }
    }

    async fn handle_list_tools(
        &self,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> RpcResponse {
        let tools = self.tool_registry.list().await;

        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(tools.len());

        let paginated: Vec<_> = tools.into_iter().skip(offset).take(limit).collect();

        RpcResponse::success(json!({
            "tools": paginated,
            "total": paginated.len(),
            "offset": offset,
            "limit": limit
        }))
    }

    async fn handle_execute_tool(
        &self,
        name: &str,
        arguments: Value,
        session_id: Option<String>,
    ) -> RpcResponse {
        info!(tool = %name, "Executing tool");

        match self
            .tool_executor
            .execute(name, arguments, session_id)
            .await
        {
            Ok(tracked) => {
                if tracked.success() {
                    RpcResponse::success(tracked.result.result.clone().unwrap_or_default())
                        .with_execution_id(&tracked.execution_id)
                } else {
                    RpcResponse::error(
                        tracked
                            .error()
                            .cloned()
                            .unwrap_or_else(|| "Unknown error".to_string()),
                    )
                    .with_execution_id(&tracked.execution_id)
                }
            }
            Err(e) => RpcResponse::error(format!("Execution failed: {}", e)),
        }
    }

    async fn handle_get_tool(&self, name: &str) -> RpcResponse {
        match self.tool_registry.get(name).await {
            Some(tool) => RpcResponse::success(json!({
                "name": tool.name(),
                "description": tool.description(),
                "input_schema": tool.input_schema()
            })),
            None => RpcResponse::error(format!("Tool not found: {}", name)),
        }
    }

    async fn handle_chat(
        &self,
        _message: &str,
        _session_id: &str,
        _model: Option<String>,
    ) -> RpcResponse {
        // TODO: Integrate with LLM provider
        RpcResponse::error("Chat not yet implemented")
    }

    async fn handle_get_history(&self, limit: usize) -> RpcResponse {
        let history = self.tool_executor.get_history(limit).await;
        RpcResponse::success(json!({
            "executions": history,
            "count": history.len()
        }))
    }

    async fn handle_get_stats(&self) -> RpcResponse {
        let stats = self.tool_executor.get_stats().await;
        RpcResponse::success(stats)
    }

    async fn handle_health(&self) -> RpcResponse {
        let tool_count = self.tool_registry.list().await.len();
        // let stats = self.tool_executor.get_stats().await;

        RpcResponse::success(json!({
            "status": "healthy",
            "tools_registered": tool_count,
            // "total_executions": stats.total_executions,
            // "success_rate": stats.success_rate()
        }))
    }

    async fn handle_introspect(
        &self,
        _service: &str,
        _bus_type: Option<String>,
    ) -> RpcResponse {
        RpcResponse::error("Introspection service disabled")
    }

    async fn handle_dbus_call(
        &self,
        _service: &str,
        _path: &str,
        _interface: &str,
        _method: &str,
        _args: Value,
        _bus_type: Option<String>,
    ) -> RpcResponse {
        // Generic D-Bus calling is not yet implemented in ChatActor directly.
        // Users should use registered tools for specific D-Bus operations.
        RpcResponse::error("Generic D-Bus call not implemented")
    }
}