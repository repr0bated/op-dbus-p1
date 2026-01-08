//! Agents MCP Server - Always-On Cognitive Agents
//!
//! **Run-On-Connection** (started when client connects):
//! - `rust_pro` - Rust development (cargo check/build/test/clippy/format)
//! - `backend_architect` - System design and architecture
//! - `sequential_thinking` - Step-by-step reasoning
//! - `memory` - Key-value session memory  
//! - `context_manager` - Persistent context across sessions
//!
//! **Available** (lazy-loaded on first call):
//! - `mem0` - Semantic memory with vector search
//! - `search_specialist` - Code/docs/web search
//! - `debugger` - Error analysis
//! - `python_pro` - Python analysis
//! - `deployment` - Service deployment
//! - `prompt_engineer` - Prompt optimization

use crate::protocol::{McpRequest, McpResponse, JsonRpcError};
use crate::{PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Agent startup mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentStartupMode {
    /// Agent process starts when client connects
    RunOnConnection,
    /// Agent is available but only started on first call
    #[default]
    Available,
}

/// Configuration for always-on agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlwaysOnAgent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub operations: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub startup_mode: AgentStartupMode,
    pub dbus_service: Option<String>,
}

fn default_true() -> bool { true }

impl AlwaysOnAgent {
    pub fn new(id: &str, name: &str, description: &str, operations: Vec<&str>) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            operations: operations.into_iter().map(String::from).collect(),
            enabled: true,
            priority: 0,
            startup_mode: AgentStartupMode::Available,
            dbus_service: None,
        }
    }
    
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
    
    pub fn run_on_connection(mut self) -> Self {
        self.startup_mode = AgentStartupMode::RunOnConnection;
        self
    }
    
    pub fn with_dbus_service(mut self, service: &str) -> Self {
        self.dbus_service = Some(service.to_string());
        self
    }
}

/// Server configuration
#[derive(Debug, Clone)]
pub struct AgentsServerConfig {
    pub name: Option<String>,
    pub agents: Vec<AlwaysOnAgent>,
    pub blocked_operations: Vec<String>,
    pub auto_start_agents: bool,
}

impl Default for AgentsServerConfig {
    fn default() -> Self {
        Self {
            name: Some("op-mcp-agents".to_string()),
            agents: default_always_on_agents(),
            blocked_operations: vec![],
            auto_start_agents: true,
        }
    }
}

/// Default always-on agents
/// 
/// RUN-ON-CONNECTION (5 agents - started immediately when client connects):
/// 1. rust_pro (priority 100)
/// 2. backend_architect (priority 99)
/// 3. sequential_thinking (priority 98)
/// 4. memory (priority 97)
/// 5. context_manager (priority 96)
///
/// AVAILABLE ON-DEMAND (6 agents - started on first call):
/// - mem0, search_specialist, python_pro, debugger, deployment, prompt_engineer
pub fn default_always_on_agents() -> Vec<AlwaysOnAgent> {
    vec![
        // ============================================
        // RUN-ON-CONNECTION AGENTS (5 total)
        // These start immediately when client connects
        // ============================================
        
        // 1. Rust Pro - Primary development agent for this Rust project
        AlwaysOnAgent::new(
            "rust_pro",
            "Rust Pro",
            "Rust development: cargo check, build, test, clippy, format. Primary dev agent.",
            vec!["check", "build", "test", "clippy", "format", "run", "doc", "bench"],
        )
        .with_priority(100)
        .run_on_connection()
        .with_dbus_service("org.dbusmcp.Agent.RustPro"),
        
        // 2. Backend Architect - System design guidance
        AlwaysOnAgent::new(
            "backend_architect",
            "Backend Architect",
            "System design, architecture review, pattern suggestions, documentation.",
            vec!["analyze", "design", "review", "suggest", "document"],
        )
        .with_priority(99)
        .run_on_connection()
        .with_dbus_service("org.dbusmcp.Agent.BackendArchitect"),
        
        // 3. Sequential Thinking - Reasoning chains
        AlwaysOnAgent::new(
            "sequential_thinking",
            "Sequential Thinking",
            "Step-by-step reasoning, problem decomposition, planning, analysis.",
            vec!["think", "plan", "analyze", "conclude", "reflect"],
        )
        .with_priority(98)
        .run_on_connection()
        .with_dbus_service("org.dbusmcp.Agent.SequentialThinking"),
        
        // 4. Memory - Session state
        AlwaysOnAgent::new(
            "memory",
            "Memory",
            "Key-value session memory. Remember facts, recall context, manage state.",
            vec!["remember", "recall", "forget", "list", "search"],
        )
        .with_priority(97)
        .run_on_connection()
        .with_dbus_service("org.dbusmcp.Agent.Memory"),
        
        // 5. Context Manager - Persistent context
        AlwaysOnAgent::new(
            "context_manager",
            "Context Manager",
            "Persist context across sessions. Save, load, export, import context.",
            vec!["save", "load", "list", "delete", "export", "import", "clear"],
        )
        .with_priority(96)
        .run_on_connection()
        .with_dbus_service("org.dbusmcp.Agent.ContextManager"),
        
        // ============================================
        // AVAILABLE ON-DEMAND AGENTS (6 total)
        // These are lazy-loaded on first call
        // ============================================
        
        AlwaysOnAgent::new(
            "mem0",
            "Semantic Memory (Mem0)",
            "Vector-based semantic memory with similarity search.",
            vec!["add", "search", "get_all", "delete", "update"],
        )
        .with_priority(80)
        .with_dbus_service("org.dbusmcp.Agent.Mem0"),
        
        AlwaysOnAgent::new(
            "search_specialist",
            "Search Specialist",
            "Search code, documentation, and web resources.",
            vec!["search", "search_code", "search_docs", "search_web"],
        )
        .with_priority(75),
        
        AlwaysOnAgent::new(
            "python_pro",
            "Python Pro",
            "Python code analysis, execution, and formatting.",
            vec!["run", "test", "lint", "format", "typecheck"],
        )
        .with_priority(70)
        .with_dbus_service("org.dbusmcp.Agent.PythonPro"),
        
        AlwaysOnAgent::new(
            "debugger",
            "Debugger",
            "Error analysis and debugging assistance.",
            vec!["analyze", "trace", "explain", "fix"],
        )
        .with_priority(70),
        
        AlwaysOnAgent::new(
            "deployment",
            "Deployment",
            "Service deployment and management.",
            vec!["deploy", "rollback", "status", "logs"],
        )
        .with_priority(60),
        
        AlwaysOnAgent::new(
            "prompt_engineer",
            "Prompt Engineer",
            "Generate and optimize prompts.",
            vec!["generate", "optimize", "analyze", "template"],
        )
        .with_priority(50),
    ]
}

/// Agent executor trait
#[async_trait::async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn start_agent(&self, agent_id: &str, dbus_service: Option<&str>) -> Result<()>;
    async fn stop_agent(&self, agent_id: &str) -> Result<()>;
    async fn execute(&self, agent_id: &str, operation: &str, args: Value) -> Result<Value>;
    async fn is_running(&self, agent_id: &str) -> bool;
}

/// D-Bus agent executor
pub struct DbusAgentExecutor {
    bus_type: BusType,
    running_agents: RwLock<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy)]
pub enum BusType {
    System,
    Session,
}

impl DbusAgentExecutor {
    pub fn new() -> Self {
        Self {
            bus_type: BusType::System,
            running_agents: RwLock::new(HashMap::new()),
        }
    }
    
    pub fn with_session_bus() -> Self {
        Self {
            bus_type: BusType::Session,
            running_agents: RwLock::new(HashMap::new()),
        }
    }
    
    fn to_service_name(agent_id: &str) -> String {
        let pascal = agent_id
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect::<String>();
        format!("org.dbusmcp.Agent.{}", pascal)
    }
    
    fn to_object_path(agent_id: &str) -> String {
        let pascal = agent_id
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect::<String>();
        format!("/org/dbusmcp/Agent/{}", pascal)
    }
}

impl Default for DbusAgentExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentExecutor for DbusAgentExecutor {
    async fn start_agent(&self, agent_id: &str, dbus_service: Option<&str>) -> Result<()> {
        let service_name = dbus_service
            .map(String::from)
            .unwrap_or_else(|| Self::to_service_name(agent_id));
        
        info!(agent = %agent_id, service = %service_name, "Starting agent via D-Bus");
        self.running_agents.write().await.insert(agent_id.to_string(), true);
        Ok(())
    }
    
    async fn stop_agent(&self, agent_id: &str) -> Result<()> {
        info!(agent = %agent_id, "Stopping agent");
        self.running_agents.write().await.remove(agent_id);
        Ok(())
    }
    
    async fn execute(&self, agent_id: &str, operation: &str, args: Value) -> Result<Value> {
        use zbus::Connection;
        
        let service_name = Self::to_service_name(agent_id);
        let object_path = Self::to_object_path(agent_id);
        
        let task = json!({
            "type": agent_id.replace('_', "-"),
            "operation": operation,
            "args": serde_json::to_string(&args).unwrap_or_default()
        });
        
        let task_json = serde_json::to_string(&task)?;
        
        debug!(agent = %agent_id, operation = %operation, "Executing via D-Bus");
        
        let connection = match self.bus_type {
            BusType::System => Connection::system().await?,
            BusType::Session => Connection::session().await?,
        };
        
        let proxy: zbus::Proxy = zbus::proxy::Builder::new(&connection)
            .destination(service_name.as_str())?
            .path(object_path.as_str())?
            .interface("org.dbusmcp.Agent")?
            .build()
            .await?;
        
        let result: String = proxy.call("Execute", &(task_json,)).await?;
        let parsed: Value = serde_json::from_str(&result)?;
        
        Ok(parsed)
    }
    
    async fn is_running(&self, agent_id: &str) -> bool {
        self.running_agents.read().await.get(agent_id).copied().unwrap_or(false)
    }
}

/// In-memory agent executor (for testing)
pub struct InMemoryAgentExecutor {
    handlers: HashMap<String, Box<dyn Fn(&str, Value) -> Result<Value> + Send + Sync>>,
    running: RwLock<HashMap<String, bool>>,
}

impl InMemoryAgentExecutor {
    pub fn new() -> Self {
        let mut executor = Self {
            handlers: HashMap::new(),
            running: RwLock::new(HashMap::new()),
        };
        executor.register_defaults();
        executor
    }
    
    fn register_defaults(&mut self) {
        // Memory agent
        self.handlers.insert("memory".to_string(), Box::new(|op, args| {
            match op {
                "remember" => {
                    let key = args.get("key").and_then(|k| k.as_str()).unwrap_or("");
                    Ok(json!({ "stored": key, "success": true }))
                }
                "recall" => {
                    let key = args.get("key").and_then(|k| k.as_str()).unwrap_or("");
                    Ok(json!({ "key": key, "value": null, "found": false }))
                }
                "list" => Ok(json!({ "keys": [] })),
                "forget" => Ok(json!({ "success": true })),
                "search" => Ok(json!({ "results": [] })),
                _ => Err(anyhow::anyhow!("Unknown operation: {}", op)),
            }
        }));
        
        // Sequential thinking agent
        self.handlers.insert("sequential_thinking".to_string(), Box::new(|op, args| {
            let thought = args.get("thought").and_then(|t| t.as_str()).unwrap_or("");
            let step = args.get("step").and_then(|s| s.as_u64()).unwrap_or(1);
            let total = args.get("total_steps").and_then(|t| t.as_u64()).unwrap_or(5);
            
            Ok(json!({
                "operation": op,
                "thought": thought,
                "step": step,
                "total_steps": total,
                "status": if step >= total { "complete" } else { "continue" },
                "success": true
            }))
        }));
        
        // Context manager agent
        self.handlers.insert("context_manager".to_string(), Box::new(|op, args| {
            let name = args.get("name").and_then(|n| n.as_str()).unwrap_or("");
            match op {
                "save" => {
                    let content = args.get("content").and_then(|c| c.as_str()).unwrap_or("");
                    Ok(json!({ "saved": name, "size": content.len(), "success": true }))
                }
                "load" => Ok(json!({ "name": name, "content": null, "found": false })),
                "list" => Ok(json!({ "contexts": [] })),
                "delete" => Ok(json!({ "deleted": name, "success": true })),
                "clear" => Ok(json!({ "cleared": true })),
                "export" | "import" => Ok(json!({ "success": true })),
                _ => Err(anyhow::anyhow!("Unknown operation: {}", op)),
            }
        }));
        
        // Rust pro agent
        self.handlers.insert("rust_pro".to_string(), Box::new(|op, args| {
            let path = args.get("path").and_then(|p| p.as_str()).unwrap_or(".");
            Ok(json!({
                "operation": op,
                "path": path,
                "status": "ready",
                "success": true,
                "output": format!("cargo {} ready at {}", op, path)
            }))
        }));
        
        // Backend architect agent
        self.handlers.insert("backend_architect".to_string(), Box::new(|op, args| {
            let context = args.get("context").and_then(|c| c.as_str()).unwrap_or("");
            Ok(json!({
                "operation": op,
                "context": context,
                "status": "ready",
                "success": true
            }))
        }));
    }
}

impl Default for InMemoryAgentExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentExecutor for InMemoryAgentExecutor {
    async fn start_agent(&self, agent_id: &str, _dbus_service: Option<&str>) -> Result<()> {
        info!(agent = %agent_id, "Starting in-memory agent");
        self.running.write().await.insert(agent_id.to_string(), true);
        Ok(())
    }
    
    async fn stop_agent(&self, agent_id: &str) -> Result<()> {
        self.running.write().await.remove(agent_id);
        Ok(())
    }
    
    async fn execute(&self, agent_id: &str, operation: &str, args: Value) -> Result<Value> {
        if let Some(handler) = self.handlers.get(agent_id) {
            handler(operation, args)
        } else {
            Err(anyhow::anyhow!("Agent not found: {}", agent_id))
        }
    }
    
    async fn is_running(&self, agent_id: &str) -> bool {
        self.running.read().await.get(agent_id).copied().unwrap_or(false)
    }
}

/// Agents MCP Server
pub struct AgentsServer {
    config: AgentsServerConfig,
    executor: Arc<dyn AgentExecutor>,
    client_info: RwLock<Option<ClientInfo>>,
    running_agents: RwLock<HashMap<String, RunningAgent>>,
}

#[derive(Debug, Clone)]
struct ClientInfo {
    name: String,
    version: Option<String>,
}

#[derive(Debug)]
struct RunningAgent {
    agent_id: String,
    started_at: chrono::DateTime<chrono::Utc>,
    #[allow(dead_code)]
    dbus_service: Option<String>,
}

impl AgentsServer {
    pub fn new(config: AgentsServerConfig) -> Self {
        Self {
            config,
            executor: Arc::new(DbusAgentExecutor::new()),
            client_info: RwLock::new(None),
            running_agents: RwLock::new(HashMap::new()),
        }
    }
    
    pub fn with_executor(config: AgentsServerConfig, executor: Arc<dyn AgentExecutor>) -> Self {
        Self {
            config,
            executor,
            client_info: RwLock::new(None),
            running_agents: RwLock::new(HashMap::new()),
        }
    }
    
    pub fn in_memory(config: AgentsServerConfig) -> Self {
        Self::with_executor(config, Arc::new(InMemoryAgentExecutor::new()))
    }
    
    /// Start run-on-connection agents
    async fn start_run_on_connection_agents(&self) -> Result<()> {
        if !self.config.auto_start_agents {
            return Ok(());
        }
        
        let agents_to_start: Vec<_> = self.config.agents.iter()
            .filter(|a| a.enabled && a.startup_mode == AgentStartupMode::RunOnConnection)
            .collect();
        
        info!(
            count = agents_to_start.len(),
            agents = ?agents_to_start.iter().map(|a| &a.id).collect::<Vec<_>>(),
            "Starting run-on-connection agents"
        );
        
        for agent in agents_to_start {
            match self.executor.start_agent(&agent.id, agent.dbus_service.as_deref()).await {
                Ok(()) => {
                    let running = RunningAgent {
                        agent_id: agent.id.clone(),
                        started_at: chrono::Utc::now(),
                        dbus_service: agent.dbus_service.clone(),
                    };
                    self.running_agents.write().await.insert(agent.id.clone(), running);
                    info!(agent = %agent.id, "✓ Run-on-connection agent started");
                }
                Err(e) => {
                    warn!(agent = %agent.id, error = %e, "✗ Failed to start agent");
                }
            }
        }
        
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down agents server");
        let running = self.running_agents.read().await;
        for agent_id in running.keys() {
            if let Err(e) = self.executor.stop_agent(agent_id).await {
                warn!(agent = %agent_id, error = %e, "Failed to stop agent");
            }
        }
        Ok(())
    }
    
    pub async fn handle_request(&self, request: McpRequest) -> McpResponse {
        debug!(method = %request.method, "Handling agents MCP request");
        
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request).await,
            "initialized" => McpResponse::success(request.id, json!({})),
            "ping" => McpResponse::success(request.id, json!({})),
            "tools/list" => self.handle_tools_list(request).await,
            "tools/call" => self.handle_tools_call(request).await,
            "notifications/initialized" => McpResponse::success(request.id, json!({})),
            "shutdown" => {
                let _ = self.shutdown().await;
                McpResponse::success(request.id, json!({ "shutdown": true }))
            }
            _ => McpResponse::error(
                request.id,
                JsonRpcError::method_not_found(&request.method),
            ),
        }
    }
    
    async fn handle_initialize(&self, request: McpRequest) -> McpResponse {
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
        
        *self.client_info.write().await = Some(ClientInfo {
            name: client_name.to_string(),
            version: client_version.map(String::from),
        });
        
        // Start run-on-connection agents
        if let Err(e) = self.start_run_on_connection_agents().await {
            error!(error = %e, "Failed to start run-on-connection agents");
        }
        
        // Build list of started agents
        let started_agents: Vec<_> = self.config.agents.iter()
            .filter(|a| a.enabled && a.startup_mode == AgentStartupMode::RunOnConnection)
            .map(|a| a.id.as_str())
            .collect();
        
        let enabled_count = self.config.agents.iter().filter(|a| a.enabled).count();
        
        info!(
            client = %client_name,
            started = ?started_agents,
            total = enabled_count,
            "Agents MCP initialized - run-on-connection agents started"
        );
        
        let server_name = self.config.name.as_deref().unwrap_or("op-mcp-agents");
        
        McpResponse::success(request.id, json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": { "listChanged": false }
            },
            "serverInfo": {
                "name": server_name,
                "version": SERVER_VERSION
            },
            "instructions": format!(
                "Run-on-connection agents STARTED: {}. Use <agent>_<operation> to call.",
                started_agents.join(", ")
            ),
            "_meta": {
                "startedAgents": started_agents,
                "totalAgents": enabled_count,
                "mode": "agents"
            }
        }))
    }
    
    async fn handle_tools_list(&self, request: McpRequest) -> McpResponse {
        let mut tools: Vec<Value> = Vec::new();
        
        let mut agents: Vec<_> = self.config.agents.iter()
            .filter(|a| a.enabled)
            .collect();
        agents.sort_by(|a, b| b.priority.cmp(&a.priority));
        
        for agent in agents {
            let prefix = if agent.startup_mode == AgentStartupMode::RunOnConnection {
                "[RUNNING] "
            } else {
                ""
            };
            
            for op in &agent.operations {
                let tool_name = format!("{}_{}", agent.id, op);
                let description = format!("{}{} - {}", prefix, agent.description, op);
                
                tools.push(json!({
                    "name": tool_name,
                    "description": description,
                    "inputSchema": self.get_operation_schema(&agent.id, op),
                    "annotations": {
                        "runOnConnection": agent.startup_mode == AgentStartupMode::RunOnConnection,
                        "priority": agent.priority
                    }
                }));
            }
        }
        
        info!(tool_count = %tools.len(), "Listed agent tools");
        McpResponse::success(request.id, json!({ "tools": tools }))
    }
    
    async fn handle_tools_call(&self, request: McpRequest) -> McpResponse {
        let params = match &request.params {
            Some(p) => p,
            None => return McpResponse::error(
                request.id,
                JsonRpcError::invalid_params("Missing params"),
            ),
        };
        
        let tool_name = match params.get("name").and_then(|n| n.as_str()) {
            Some(n) => n,
            None => return McpResponse::error(
                request.id,
                JsonRpcError::invalid_params("Missing tool name"),
            ),
        };
        
        let (agent_id, operation) = match self.parse_tool_name(tool_name) {
            Some(parsed) => parsed,
            None => return McpResponse::error(
                request.id,
                JsonRpcError::new(-32001, format!("Invalid tool name: {}", tool_name)),
            ),
        };
        
        let agent = self.config.agents.iter().find(|a| a.id == agent_id);
        if agent.is_none() || !agent.unwrap().enabled {
            return McpResponse::error(
                request.id,
                JsonRpcError::new(-32001, format!("Agent not available: {}", agent_id)),
            );
        }
        
        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
        let agent = agent.unwrap();
        
        // For non-run-on-connection agents, ensure started
        if agent.startup_mode != AgentStartupMode::RunOnConnection {
            if !self.executor.is_running(&agent_id).await {
                info!(agent = %agent_id, "Starting on-demand agent");
                if let Err(e) = self.executor.start_agent(&agent_id, agent.dbus_service.as_deref()).await {
                    return McpResponse::success(request.id, json!({
                        "content": [{ "type": "text", "text": format!("Failed to start agent: {}", e) }],
                        "isError": true
                    }));
                }
            }
        }
        
        info!(agent = %agent_id, operation = %operation, "Executing agent");
        
        match self.executor.execute(&agent_id, &operation, arguments).await {
            Ok(result) => {
                let text = serde_json::to_string_pretty(&result).unwrap_or_default();
                McpResponse::success(request.id, json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": false
                }))
            }
            Err(e) => {
                error!(agent = %agent_id, error = %e, "Agent execution failed");
                McpResponse::success(request.id, json!({
                    "content": [{ "type": "text", "text": format!("Error: {}", e) }],
                    "isError": true
                }))
            }
        }
    }
    
    fn parse_tool_name(&self, tool_name: &str) -> Option<(String, String)> {
        for agent in &self.config.agents {
            let prefix = format!("{}_", agent.id);
            if tool_name.starts_with(&prefix) {
                let operation = tool_name.strip_prefix(&prefix)?;
                if agent.operations.contains(&operation.to_string()) {
                    return Some((agent.id.clone(), operation.to_string()));
                }
            }
        }
        None
    }
    
    fn get_operation_schema(&self, agent_id: &str, operation: &str) -> Value {
        match (agent_id, operation) {
            // Rust Pro
            ("rust_pro", "check") | ("rust_pro", "build") | ("rust_pro", "run") |
            ("rust_pro", "doc") | ("rust_pro", "bench") => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Project path", "default": "." },
                    "release": { "type": "boolean", "description": "Release build", "default": false },
                    "features": { "type": "string", "description": "Comma-separated features" }
                }
            }),
            ("rust_pro", "test") => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "default": "." },
                    "filter": { "type": "string", "description": "Test filter" },
                    "features": { "type": "string" }
                }
            }),
            ("rust_pro", "clippy") | ("rust_pro", "format") => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "default": "." },
                    "fix": { "type": "boolean", "default": false }
                }
            }),
            
            // Backend Architect
            ("backend_architect", "analyze") => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Code path" },
                    "scope": { "type": "string", "enum": ["file", "module", "crate", "workspace"] }
                },
                "required": ["path"]
            }),
            ("backend_architect", "design") | ("backend_architect", "suggest") => json!({
                "type": "object",
                "properties": {
                    "context": { "type": "string", "description": "Context or requirements" },
                    "constraints": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["context"]
            }),
            ("backend_architect", "review") => json!({
                "type": "object",
                "properties": {
                    "design": { "type": "string", "description": "Design to review" },
                    "criteria": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["design"]
            }),
            ("backend_architect", "document") => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "format": { "type": "string", "default": "markdown" }
                },
                "required": ["path"]
            }),
            
            // Sequential Thinking
            ("sequential_thinking", _) => json!({
                "type": "object",
                "properties": {
                    "thought": { "type": "string", "description": "Current thought" },
                    "step": { "type": "integer", "description": "Step number" },
                    "total_steps": { "type": "integer", "description": "Total steps" },
                    "context": { "type": "string", "description": "Additional context" }
                },
                "required": ["thought", "step", "total_steps"]
            }),
            
            // Memory
            ("memory", "remember") => json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" },
                    "value": { "type": "string" }
                },
                "required": ["key", "value"]
            }),
            ("memory", "recall") | ("memory", "forget") => json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" }
                },
                "required": ["key"]
            }),
            ("memory", "list") => json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" }
                }
            }),
            ("memory", "search") => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
            
            // Context Manager
            ("context_manager", "save") => json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "content": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["name", "content"]
            }),
            ("context_manager", "load") | ("context_manager", "delete") => json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "required": ["name"]
            }),
            ("context_manager", "list") => json!({
                "type": "object",
                "properties": {
                    "tag": { "type": "string" }
                }
            }),
            ("context_manager", "clear") => json!({
                "type": "object",
                "properties": {}
            }),
            ("context_manager", "export") | ("context_manager", "import") => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "format": { "type": "string", "default": "json" }
                },
                "required": ["path"]
            }),
            
            // Default
            _ => json!({
                "type": "object",
                "properties": {
                    "args": { "type": "object" }
                }
            }),
        }
    }
    
    pub fn enabled_agents(&self) -> Vec<&AlwaysOnAgent> {
        self.config.agents.iter().filter(|a| a.enabled).collect()
    }
    
    pub fn run_on_connection_agents(&self) -> Vec<&AlwaysOnAgent> {
        self.config.agents.iter()
            .filter(|a| a.enabled && a.startup_mode == AgentStartupMode::RunOnConnection)
            .collect()
    }
    
    pub async fn running_agent_ids(&self) -> Vec<String> {
        self.running_agents.read().await.keys().cloned().collect()
    }
}
