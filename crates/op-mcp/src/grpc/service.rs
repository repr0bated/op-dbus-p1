//! MCP gRPC Service Implementation

#[cfg(feature = "grpc")]
use crate::grpc::proto::*;
#[cfg(feature = "grpc")]
use crate::grpc::proto::mcp_service_server::McpService;
use crate::grpc::server::ServerMode;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::pin::Pin;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
#[cfg(feature = "grpc")]
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
#[cfg(feature = "grpc")]
use tonic::{Request, Response, Status};
use tracing::{debug, info, warn};
use uuid::Uuid;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "op-mcp-grpc";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(feature = "grpc")]
type ResponseStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

struct Session {
    id: String,
    client_name: String,
    started_agents: Vec<String>,
    created_at: Instant,
}

/// Infrastructure integrations
pub struct GrpcInfrastructure {
    pub cache_path: Option<PathBuf>,
    pub state_db_path: Option<PathBuf>,
    pub blockchain_path: Option<PathBuf>,
}

impl Default for GrpcInfrastructure {
    fn default() -> Self {
        Self {
            cache_path: None,
            state_db_path: None,
            blockchain_path: None,
        }
    }
}

impl GrpcInfrastructure {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn from_paths(
        cache_path: Option<PathBuf>,
        state_db_path: Option<PathBuf>,
        blockchain_path: Option<PathBuf>,
    ) -> Result<Self> {
        // Create directories if they don't exist
        if let Some(ref path) = cache_path {
            tokio::fs::create_dir_all(path).await.ok();
        }
        if let Some(ref path) = state_db_path {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await.ok();
            }
        }
        if let Some(ref path) = blockchain_path {
            tokio::fs::create_dir_all(path).await.ok();
        }

        Ok(Self {
            cache_path,
            state_db_path,
            blockchain_path,
        })
    }
}

/// MCP gRPC service implementation
pub struct McpGrpcService {
    mode: ServerMode,
    sessions: RwLock<HashMap<String, Session>>,
    start_time: Instant,
    request_counter: AtomicU64,
    error_counter: AtomicU64,
    infrastructure: GrpcInfrastructure,
}

impl McpGrpcService {
    pub fn new(mode: ServerMode) -> Self {
        Self {
            mode,
            sessions: RwLock::new(HashMap::new()),
            start_time: Instant::now(),
            request_counter: AtomicU64::new(0),
            error_counter: AtomicU64::new(0),
            infrastructure: GrpcInfrastructure::default(),
        }
    }

    pub fn with_infrastructure(
        mode: ServerMode,
        infrastructure: GrpcInfrastructure,
    ) -> Self {
        info!(
            "gRPC service initialized with: cache={:?}, state_store={:?}, blockchain={:?}",
            infrastructure.cache_path,
            infrastructure.state_db_path,
            infrastructure.blockchain_path
        );
        Self {
            mode,
            sessions: RwLock::new(HashMap::new()),
            start_time: Instant::now(),
            request_counter: AtomicU64::new(0),
            error_counter: AtomicU64::new(0),
            infrastructure,
        }
    }

    async fn start_session_agents(&self, session_id: &str, client_name: &str) -> Vec<String> {
        let mut started = Vec::new();
        
        let agents_to_start: Vec<&str> = match self.mode {
            ServerMode::Agents => vec![
                "rust_pro",
                "backend_architect",
                "sequential_thinking",
                "memory",
                "context_manager",
            ],
            _ => vec![],
        };
        
        for agent_id in agents_to_start {
            info!(session = %session_id, agent = %agent_id, "Starting run-on-connection agent");
            started.push(agent_id.to_string());
        }
        
        let session = Session {
            id: session_id.to_string(),
            client_name: client_name.to_string(),
            started_agents: started.clone(),
            created_at: Instant::now(),
        };
        
        self.sessions.write().await.insert(session_id.to_string(), session);
        
        started
    }

    fn mode_to_proto(&self) -> i32 {
        match self.mode {
            ServerMode::Compact => 1,
            ServerMode::Agents => 2,
            ServerMode::Full => 3,
        }
    }
}

#[cfg(feature = "grpc")]
#[tonic::async_trait]
impl McpService for McpGrpcService {
    async fn call(
        &self,
        request: Request<McpRequest>,
    ) -> Result<Response<McpResponse>, Status> {
        self.request_counter.fetch_add(1, Ordering::Relaxed);
        let proto_req = request.into_inner();
        
        debug!(method = %proto_req.method, "gRPC MCP call");
        
        // Simulated response - integrate with actual MCP server
        let proto_resp = McpResponse {
            jsonrpc: "2.0".to_string(),
            id: proto_req.id,
            result_json: Some(json!({"status": "ok"}).to_string()),
            error: None,
        };
        
        Ok(Response::new(proto_resp))
    }
    
    type SubscribeStream = ResponseStream<McpEvent>;
    
    async fn subscribe(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        
        info!(session = %session_id, event_types = ?req.event_types, "New subscription");
        
        let (tx, rx) = mpsc::channel(32);
        
        tokio::spawn(async move {
            let mut sequence = 0u32;
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            // Send initial event
            let _ = tx.send(Ok(McpEvent {
                event_type: "connected".to_string(),
                data_json: json!({"session_id": session_id}).to_string(),
                timestamp: chrono::Utc::now().timestamp(),
                sequence,
            })).await;
            sequence += 1;
            
            loop {
                interval.tick().await;
                
                let event = McpEvent {
                    event_type: "ping".to_string(),
                    data_json: json!({"sequence": sequence}).to_string(),
                    timestamp: chrono::Utc::now().timestamp(),
                    sequence,
                };
                sequence += 1;
                
                if tx.send(Ok(event)).await.is_err() {
                    break;
                }
            }
        });
        
        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::SubscribeStream))
    }
    
    type StreamStream = ResponseStream<McpResponse>;
    
    async fn stream(
        &self,
        request: Request<tonic::Streaming<McpRequest>>,
    ) -> Result<Response<Self::StreamStream>, Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(32);
        
        tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(proto_req) => {
                        let proto_resp = McpResponse {
                            jsonrpc: "2.0".to_string(),
                            id: proto_req.id,
                            result_json: Some(json!({"status": "ok"}).to_string()),
                            error: None,
                        };
                        
                        if tx.send(Ok(proto_resp)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Stream error");
                        break;
                    }
                }
            }
        });
        
        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::StreamStream))
    }
    
    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        let sessions = self.sessions.read().await;
        let connected_agents: Vec<String> = sessions
            .values()
            .flat_map(|s| s.started_agents.clone())
            .collect();
        
        let response = HealthResponse {
            healthy: true,
            version: SERVER_VERSION.to_string(),
            server_name: SERVER_NAME.to_string(),
            mode: self.mode_to_proto(),
            connected_agents,
            uptime_secs: self.start_time.elapsed().as_secs(),
        };
        
        Ok(Response::new(response))
    }
    
    async fn initialize(
        &self,
        request: Request<InitializeRequest>,
    ) -> Result<Response<InitializeResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        
        info!(
            client = %req.client_name,
            session = %session_id,
            "Initializing gRPC session"
        );
        
        let started_agents = self.start_session_agents(&session_id, &req.client_name).await;
        
        let response = InitializeResponse {
            protocol_version: PROTOCOL_VERSION.to_string(),
            server_name: SERVER_NAME.to_string(),
            server_version: SERVER_VERSION.to_string(),
            capabilities: vec!["tools".to_string(), "resources".to_string()],
            started_agents,
            session_id,
        };
        
        Ok(Response::new(response))
    }
    
    async fn list_tools(
        &self,
        request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        let _req = request.into_inner();
        
        // Return placeholder - integrate with actual tool registry
        let tools = vec![
            ToolInfo {
                name: "dbus_list_services".to_string(),
                description: "List all D-Bus services".to_string(),
                input_schema_json: json!({"type": "object", "properties": {}}).to_string(),
                category: Some("dbus".to_string()),
                tags: vec!["dbus".to_string(), "system".to_string()],
            },
        ];
        
        Ok(Response::new(ListToolsResponse {
            tools,
            total: 1,
            has_more: false,
        }))
    }
    
    async fn call_tool(
        &self,
        request: Request<CallToolRequest>,
    ) -> Result<Response<CallToolResponse>, Status> {
        let req = request.into_inner();
        let start = Instant::now();
        
        debug!(tool = %req.tool_name, "Executing tool via gRPC");
        
        // Simulated execution - integrate with actual tool executor
        let result = json!({"success": true, "tool": req.tool_name});
        
        let response = CallToolResponse {
            success: true,
            result_json: result.to_string(),
            error: None,
            duration_ms: start.elapsed().as_millis() as u64,
        };
        
        Ok(Response::new(response))
    }
    
    type CallToolStreamingStream = ResponseStream<ToolOutput>;
    
    async fn call_tool_streaming(
        &self,
        request: Request<CallToolRequest>,
    ) -> Result<Response<Self::CallToolStreamingStream>, Status> {
        let req = request.into_inner();
        let (tx, rx) = mpsc::channel(32);
        let tool_name = req.tool_name.clone();
        
        tokio::spawn(async move {
            let mut sequence = 0u32;
            
            // Send progress
            let _ = tx.send(Ok(ToolOutput {
                output_type: 3, // Progress
                content: format!("Starting {}...", tool_name),
                sequence,
                is_final: false,
                exit_code: None,
            })).await;
            sequence += 1;
            
            // Simulate execution
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Send result
            let _ = tx.send(Ok(ToolOutput {
                output_type: 4, // Result
                content: json!({"success": true, "tool": tool_name}).to_string(),
                sequence,
                is_final: true,
                exit_code: Some(0),
            })).await;
        });
        
        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::CallToolStreamingStream))
    }
}
