//! MCP Agents Server - Critical Agents for Chat UI
//!
//! Provides MCP-compatible access to core orchestration agents plus
//! additional requested agents.

use axum::{routing::{get, post},
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use op_agents::agents::base::{AgentTrait as Agent, AgentTask};
use op_agents::agents::orchestration::context_manager::ContextManagerAgent;
use op_agents::agents::orchestration::memory::MemoryAgent;
use op_agents::agents::orchestration::sequential_thinking::SequentialThinkingAgent;
use op_agents::agents::orchestration::mem0_wrapper::Mem0WrapperAgent;

// Additional agents
use op_agents::agents::seo::search_specialist::SearchSpecialistAgent;
use op_agents::agents::infrastructure::deployment::DeploymentAgent;
use op_agents::agents::language::python_pro::PythonProAgent;
use op_agents::agents::analysis::debugger::DebuggerAgent;
use op_agents::agents::aiml::prompt_engineer::PromptEngineerAgent;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

pub struct CriticalAgentsState {
    agents: Vec<Arc<dyn Agent + Send + Sync>>,
}

impl CriticalAgentsState {
    pub fn new() -> Self {
        let agents: Vec<Arc<dyn Agent + Send + Sync>> = vec![
            // Core orchestration agents
            Arc::new(MemoryAgent::new("memory".to_string())),
            Arc::new(ContextManagerAgent::new("context_manager".to_string())),
            Arc::new(SequentialThinkingAgent::new("sequential_thinking".to_string())),
            Arc::new(Mem0WrapperAgent::new("mem0".to_string())),
            // Additional requested agents
            Arc::new(SearchSpecialistAgent::new("search_specialist".to_string())),
            Arc::new(DeploymentAgent::new("deployment".to_string())),
            Arc::new(PythonProAgent::new("python_pro".to_string())),
            Arc::new(DebuggerAgent::new("debugger".to_string())),
            Arc::new(PromptEngineerAgent::new("prompt_engineer".to_string())),
        ];

        info!("Initialized Critical Agents MCP with {} agents", agents.len());
        Self { agents }
    }

    pub fn get_tools(&self) -> Vec<Value> {
        let mut tools = Vec::new();
        for agent in &self.agents {
            let agent_name = agent.name();
            let agent_description = agent.description();
            for op in agent.operations() {
                let tool_name = format!("{}_{}", agent_name, op);
                let description = format!("{} - {}", agent_description, op);
                tools.push(json!({
                    "name": tool_name,
                    "description": description,
                    "inputSchema": self.get_operation_schema(agent_name, &op)
                }));
            }
        }
        tools
    }

    fn get_operation_schema(&self, agent_name: &str, operation: &str) -> Value {
        match (agent_name, operation) {
            // Memory agent
            ("memory", "remember") => json!({
                "type": "object",
                "properties": {
                    "key": {"type": "string", "description": "Memory key"},
                    "value": {"type": "string", "description": "Value to remember"}
                },
                "required": ["key", "value"]
            }),
            ("memory", "recall") => json!({
                "type": "object",
                "properties": {
                    "key": {"type": "string", "description": "Memory key to recall"}
                },
                "required": ["key"]
            }),
            ("memory", "forget") => json!({
                "type": "object",
                "properties": {
                    "key": {"type": "string", "description": "Memory key to forget"}
                },
                "required": ["key"]
            }),
            // Context manager
            ("context_manager", "save") => json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Context name"},
                    "content": {"type": "string", "description": "Context content"}
                },
                "required": ["name", "content"]
            }),
            ("context_manager", "load") => json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Context name to load"}
                },
                "required": ["name"]
            }),
            ("context_manager", "list") => json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            ("context_manager", "delete") => json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Context name to delete"}
                },
                "required": ["name"]
            }),
            // Sequential thinking
            ("sequential_thinking", "think") => json!({
                "type": "object",
                "properties": {
                    "problem": {"type": "string", "description": "Problem to think through"},
                    "steps": {"type": "integer", "description": "Number of thinking steps", "default": 5}
                },
                "required": ["problem"]
            }),
            // Mem0 - semantic memory
            ("mem0", "add") => json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "Text to add to semantic memory"},
                    "user_id": {"type": "string", "description": "User ID for memory isolation", "default": "default"}
                },
                "required": ["text"]
            }),
            ("mem0", "search") => json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "user_id": {"type": "string", "description": "User ID", "default": "default"},
                    "limit": {"type": "integer", "description": "Max results", "default": 10}
                },
                "required": ["query"]
            }),
            ("mem0", "get_all") => json!({
                "type": "object",
                "properties": {
                    "user_id": {"type": "string", "description": "User ID", "default": "default"}
                },
                "required": []
            }),
            ("mem0", "delete") => json!({
                "type": "object",
                "properties": {
                    "memory_id": {"type": "string", "description": "Memory ID to delete"}
                },
                "required": ["memory_id"]
            }),
            ("mem0", "update") => json!({
                "type": "object",
                "properties": {
                    "memory_id": {"type": "string", "description": "Memory ID to update"},
                    "text": {"type": "string", "description": "New text content"}
                },
                "required": ["memory_id", "text"]
            }),
            // Search specialist
            ("search_specialist", "search") => json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "scope": {"type": "string", "description": "Search scope (code, docs, web)", "default": "code"}
                },
                "required": ["query"]
            }),
            ("search_specialist", "optimize") => json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Query to optimize"},
                    "target": {"type": "string", "description": "Optimization target"}
                },
                "required": ["query"]
            }),
            // Deployment
            ("deployment", "deploy") => json!({
                "type": "object",
                "properties": {
                    "service": {"type": "string", "description": "Service to deploy"},
                    "environment": {"type": "string", "description": "Target environment", "default": "staging"}
                },
                "required": ["service"]
            }),
            ("deployment", "rollback") => json!({
                "type": "object",
                "properties": {
                    "service": {"type": "string", "description": "Service to rollback"},
                    "version": {"type": "string", "description": "Version to rollback to"}
                },
                "required": ["service"]
            }),
            ("deployment", "status") => json!({
                "type": "object",
                "properties": {
                    "service": {"type": "string", "description": "Service to check"}
                },
                "required": []
            }),
            // Python Pro
            ("python_pro", "analyze") => json!({
                "type": "object",
                "properties": {
                    "code": {"type": "string", "description": "Python code to analyze"},
                    "path": {"type": "string", "description": "File path to analyze"}
                },
                "required": []
            }),
            ("python_pro", "refactor") => json!({
                "type": "object",
                "properties": {
                    "code": {"type": "string", "description": "Python code to refactor"},
                    "pattern": {"type": "string", "description": "Refactoring pattern"}
                },
                "required": ["code"]
            }),
            // Debugger
            ("debugger", "analyze") => json!({
                "type": "object",
                "properties": {
                    "error": {"type": "string", "description": "Error message or stack trace"},
                    "context": {"type": "string", "description": "Additional context"}
                },
                "required": ["error"]
            }),
            ("debugger", "trace") => json!({
                "type": "object",
                "properties": {
                    "function": {"type": "string", "description": "Function to trace"},
                    "depth": {"type": "integer", "description": "Trace depth", "default": 3}
                },
                "required": ["function"]
            }),
            // Prompt Engineer
            ("prompt_engineer", "generate") => json!({
                "type": "object",
                "properties": {
                    "task": {"type": "string", "description": "Task to generate prompt for"},
                    "style": {"type": "string", "description": "Prompt style", "default": "detailed"}
                },
                "required": ["task"]
            }),
            ("prompt_engineer", "optimize") => json!({
                "type": "object",
                "properties": {
                    "prompt": {"type": "string", "description": "Prompt to optimize"},
                    "goal": {"type": "string", "description": "Optimization goal"}
                },
                "required": ["prompt"]
            }),
            // Default
            _ => json!({
                "type": "object",
                "properties": {
                    "args": {"type": "object", "description": "Operation arguments"}
                },
                "required": []
            })
        }
    }

    pub fn find_agent(&self, tool_name: &str) -> Option<(&Arc<dyn Agent + Send + Sync>, String)> {
        for agent in &self.agents {
            let agent_name = agent.name();
            for op in agent.operations() {
                let expected_tool = format!("{}_{}", agent_name, op);
                if expected_tool == tool_name {
                    return Some((agent, op.clone()));
                }
            }
        }
        None
    }
}

impl Default for CriticalAgentsState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AgentsMcpState {
    pub agents: RwLock<CriticalAgentsState>,
}

impl AgentsMcpState {
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(CriticalAgentsState::new()),
        }
    }
}

impl Default for AgentsMcpState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn create_router() -> axum::Router {
    let state = Arc::new(AgentsMcpState::new());
    axum::Router::new()
        .route("/mcp/agents", get(mcp_agents_sse_handler))
        .route("/mcp/agents/message", post(mcp_agents_message_handler))
        .with_state(state)
}

pub async fn mcp_agents_sse_handler(
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("MCP Agents SSE client connected");

    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");

    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("https");

    let post_url = format!("{}://{}/mcp/agents/message", scheme, host);
    info!("MCP Agents POST endpoint: {}", post_url);

    let endpoint_event = Event::default()
        .event("endpoint")
        .data(&post_url);

    let stream = stream::once(async move { Ok(endpoint_event) });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

pub async fn mcp_agents_message_handler(
    State(state): State<Arc<AgentsMcpState>>,
    Json(request): Json<JsonRpcRequest>,
) -> Response {
    debug!("MCP Agents request: method={} id={}", request.method, request.id);

    let response = match request.method.as_str() {
        "initialize" => handle_initialize(&request),
        "initialized" => JsonRpcResponse::success(request.id.clone(), json!({})),
        "tools/list" => handle_tools_list(&state, &request).await,
        "tools/call" => handle_tools_call(&state, &request).await,
        "ping" => JsonRpcResponse::success(request.id.clone(), json!({})),
        "notifications/initialized" => JsonRpcResponse::success(request.id.clone(), json!({})),
        _ => {
            warn!("Unknown MCP method: {}", request.method);
            JsonRpcResponse::error(
                request.id.clone(),
                -32601,
                format!("Method not found: {}", request.method),
            )
        }
    };

    let json_body = serde_json::to_string(&response).unwrap_or_else(|e| {
        error!("Failed to serialize response: {}", e);
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal error"}}"#.to_string()
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(json_body.into())
        .unwrap_or_else(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response()
        })
}

fn handle_initialize(request: &JsonRpcRequest) -> JsonRpcResponse {
    info!("MCP Agents initialize request");
    JsonRpcResponse::success(
        request.id.clone(),
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "op-dbus-agents",
                "version": "1.0.0"
            },
            "instructions": "Critical agents MCP: memory, context_manager, sequential_thinking, mem0 (semantic), search_specialist, deployment, python_pro, debugger, prompt_engineer."
        }),
    )
}

async fn handle_tools_list(state: &Arc<AgentsMcpState>, request: &JsonRpcRequest) -> JsonRpcResponse {
    info!("MCP Agents tools/list request");
    let agents = state.agents.read().await;
    let tools = agents.get_tools();
    
    JsonRpcResponse::success(
        request.id.clone(),
        json!({
            "tools": tools
        }),
    )
}

async fn handle_tools_call(
    state: &Arc<AgentsMcpState>,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let params = &request.params;
    
    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(name) => name,
        None => {
            error!("Missing tool name in params: {:?}", params);
            return JsonRpcResponse::error(
                request.id.clone(),
                -32602,
                "Missing required parameter: name".to_string(),
            );
        }
    };
    
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(json!({}));

    info!("MCP Agents tool call: {} with args: {}", tool_name, arguments);

    let agents = state.agents.read().await;
    
    let (agent, operation) = match agents.find_agent(tool_name) {
        Some((a, op)) => (a.clone(), op),
        None => {
            return JsonRpcResponse::success(
                request.id.clone(),
                json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Unknown tool: {}. Use tools/list to see available tools.", tool_name)
                    }],
                    "isError": true
                }),
            );
        }
    };
    
    drop(agents);
    
    let agent_type = tool_name.split('_').next().unwrap_or(tool_name);
    
    let task = AgentTask {
        task_type: agent_type.to_string(),
        operation: operation.clone(),
        path: arguments.get("path").and_then(|p| p.as_str()).map(String::from),
        args: Some(serde_json::to_string(&arguments).unwrap_or_else(|_| "{}".to_string())),
        config: arguments.as_object()
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
    };
    
    match agent.execute(task).await {
        Ok(result) => {
            let text = serde_json::to_string_pretty(&result)
                .unwrap_or_else(|_| format!("{:?}", result));
            JsonRpcResponse::success(
                request.id.clone(),
                json!({
                    "content": [{
                        "type": "text",
                        "text": text
                    }]
                }),
            )
        }
        Err(e) => {
            error!("Agent execution error: {}", e);
            JsonRpcResponse::success(
                request.id.clone(),
                json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Error: {}", e)
                    }],
                    "isError": true
                }),
            )
        }
    }
}
