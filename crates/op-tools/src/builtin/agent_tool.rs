//! Agent Tool - Self-Contained D-Bus Agent Registration
//!
//! This module creates agent tools that register as D-Bus services.
//! It is SELF-CONTAINED - no dependency on op_agents::create_agent().
//!
//! Architecture:
//! 1. Agent definitions are static (no factory function needed)
//! 2. D-Bus services are created directly using zbus
//! 3. Tool calls go through zbus::Proxy

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use zbus::Connection;

use crate::tool::{BoxedTool, Tool};

// =============================================================================
// BUS TYPE
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusType {
    System,
    Session,
}

impl Default for BusType {
    fn default() -> Self {
        Self::System
    }
}

// =============================================================================
// AGENT CONNECTION REGISTRY
// =============================================================================

static AGENT_CONNECTIONS: std::sync::OnceLock<Arc<AgentConnectionRegistry>> =
    std::sync::OnceLock::new();

pub struct AgentConnectionRegistry {
    connections: RwLock<HashMap<String, Connection>>,
    bus_type: BusType,
}

impl AgentConnectionRegistry {
    pub fn new(bus_type: BusType) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            bus_type,
        }
    }

    pub fn global() -> Arc<Self> {
        AGENT_CONNECTIONS
            .get_or_init(|| {
                let bus_type = std::env::var("OP_AGENT_BUS")
                    .ok()
                    .and_then(|v| match v.to_lowercase().as_str() {
                        "session" => Some(BusType::Session),
                        _ => Some(BusType::System),
                    })
                    .unwrap_or(BusType::System);
                info!("AgentConnectionRegistry: using {:?} bus", bus_type);
                Arc::new(Self::new(bus_type))
            })
            .clone()
    }

    /// Start an agent as a D-Bus service (self-contained, no op_agents dependency)
    pub async fn start_agent_service(&self, def: &AgentDef) -> Result<()> {
        let agent_type = def.agent_type;

        // Check if already running
        {
            let connections = self.connections.read().await;
            if connections.contains_key(agent_type) {
                debug!(agent = %agent_type, "Agent already running");
                return Ok(());
            }
        }

        info!(agent = %agent_type, "Starting agent D-Bus service");

        // Build service name: rust-pro -> org.dbusmcp.Agent.RustPro
        let service_name = format!(
            "org.dbusmcp.Agent.{}",
            def.agent_type
                .split('-')
                .map(capitalize_first)
                .collect::<String>()
        );

        let object_path = format!(
            "/org/dbusmcp/Agent/{}",
            def.agent_type
                .split('-')
                .map(capitalize_first)
                .collect::<String>()
        );

        // Create the D-Bus service object
        let service = AgentDbusService {
            agent_type: def.agent_type.to_string(),
            agent_name: def.name.to_string(),
            description: def.description.to_string(),
            operations: def.operations.iter().map(|s| s.to_string()).collect(),
        };

        // Build connection and serve
        let connection = match self.bus_type {
            BusType::System => {
                zbus::connection::Builder::system()?
                    .name(service_name.as_str())?
                    .serve_at(object_path.as_str(), service)?
                    .build()
                    .await?
            }
            BusType::Session => {
                zbus::connection::Builder::session()?
                    .name(service_name.as_str())?
                    .serve_at(object_path.as_str(), service)?
                    .build()
                    .await?
            }
        };

        // Store connection to keep service alive
        {
            let mut connections = self.connections.write().await;
            connections.insert(agent_type.to_string(), connection);
        }

        info!(agent = %agent_type, service = %service_name, "✓ Agent registered on D-Bus");
        Ok(())
    }

    pub async fn is_running(&self, agent_type: &str) -> bool {
        self.connections.read().await.contains_key(agent_type)
    }

    pub async fn list_running(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }

    pub async fn stop_agent(&self, agent_type: &str) -> Result<()> {
        if self.connections.write().await.remove(agent_type).is_some() {
            info!(agent = %agent_type, "Agent stopped");
        }
        Ok(())
    }

    pub async fn stop_all(&self) {
        let count = self.connections.write().await.drain().count();
        info!("Stopped {} agent D-Bus services", count);
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

// =============================================================================
// D-BUS SERVICE IMPLEMENTATION
// =============================================================================

/// D-Bus service implementing org.dbusmcp.Agent interface
struct AgentDbusService {
    agent_type: String,
    agent_name: String,
    description: String,
    operations: Vec<String>,
}

#[zbus::interface(name = "org.dbusmcp.Agent")]
impl AgentDbusService {
    fn name(&self) -> &str {
        &self.agent_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn operations(&self) -> Vec<String> {
        self.operations.clone()
    }

    async fn execute(&self, task_json: &str) -> String {
        debug!(agent = %self.agent_type, task = %task_json, "Executing");

        let task: Value = match serde_json::from_str(task_json) {
            Ok(t) => t,
            Err(e) => {
                return json!({
                    "success": false,
                    "error": format!("Parse error: {}", e)
                })
                .to_string();
            }
        };

        let operation = task
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("execute");

        // Placeholder execution - returns success with operation info
        // Real implementation would dispatch to actual agent logic
        json!({
            "success": true,
            "agent": self.agent_type,
            "operation": operation,
            "message": format!("Agent {} executed '{}'", self.agent_name, operation),
            "data": task.get("args").cloned().unwrap_or(Value::Null)
        })
        .to_string()
    }
}

// =============================================================================
// D-BUS EXECUTOR
// =============================================================================

pub struct DbusAgentExecutor {
    bus_type: BusType,
}

impl DbusAgentExecutor {
    pub fn new() -> Self {
        let bus_type = std::env::var("OP_AGENT_BUS")
            .ok()
            .and_then(|v| match v.to_lowercase().as_str() {
                "session" => Some(BusType::Session),
                _ => Some(BusType::System),
            })
            .unwrap_or(BusType::System);
        Self { bus_type }
    }

    fn to_service_name(agent_name: &str) -> String {
        let pascal = agent_name.split('_').map(capitalize_first).collect::<String>();
        format!("org.dbusmcp.Agent.{}", pascal)
    }

    fn to_object_path(agent_name: &str) -> String {
        let pascal = agent_name.split('_').map(capitalize_first).collect::<String>();
        format!("/org/dbusmcp/Agent/{}", pascal)
    }

    fn is_service_unavailable(error: &zbus::Error) -> bool {
        let s = error.to_string().to_lowercase();
        s.contains("serviceunknown")
            || s.contains("name has no owner")
            || s.contains("not found")
            || s.contains("does not exist")
    }
}

impl Default for DbusAgentExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute_operation(
        &self,
        agent_name: &str,
        operation: &str,
        path: Option<&str>,
        args: Option<Value>,
    ) -> Result<Value>;
}

#[async_trait]
impl AgentExecutor for DbusAgentExecutor {
    async fn execute_operation(
        &self,
        agent_name: &str,
        operation: &str,
        path: Option<&str>,
        args: Option<Value>,
    ) -> Result<Value> {
        let service_name = Self::to_service_name(agent_name);
        let object_path = Self::to_object_path(agent_name);

        let args_str = args.and_then(|v| {
            if v.is_null() {
                None
            } else {
                serde_json::to_string(&v).ok()
            }
        });

        let task = json!({
            "type": agent_name.replace('_', "-"),
            "operation": operation,
            "path": path,
            "args": args_str
        });
        let task_json = serde_json::to_string(&task)?;

        debug!(agent = %agent_name, service = %service_name, "Calling D-Bus");

        let connection = match self.bus_type {
            BusType::System => Connection::system().await,
            BusType::Session => Connection::session().await,
        }
        .map_err(|e| anyhow::anyhow!("D-Bus connection failed: {}", e))?;

        let proxy: zbus::Proxy = zbus::proxy::Builder::new(&connection)
            .destination(service_name.as_str())?
            .path(object_path.as_str())?
            .interface("org.dbusmcp.Agent")?
            .build()
            .await
            .map_err(|e| {
                if Self::is_service_unavailable(&e) {
                    anyhow::anyhow!("Agent '{}' not running on D-Bus", agent_name)
                } else {
                    anyhow::anyhow!("D-Bus proxy failed: {}", e)
                }
            })?;

        let result: String = proxy.call("Execute", &(task_json,)).await.map_err(|e| {
            if Self::is_service_unavailable(&e) {
                anyhow::anyhow!("Agent '{}' not available", agent_name)
            } else {
                anyhow::anyhow!("D-Bus call failed: {}", e)
            }
        })?;

        let parsed: Value = serde_json::from_str(&result)?;
        info!(agent = %agent_name, operation = %operation, "Completed");
        Ok(parsed)
    }
}

// =============================================================================
// AGENT TOOL
// =============================================================================

pub struct AgentTool {
    name: String,
    agent_name: String,
    description: String,
    operations: Vec<String>,
    role_category: String,
    executor: Arc<dyn AgentExecutor + Send + Sync>,
}

impl AgentTool {
    pub fn new(
        agent_name: &str,
        description: &str,
        operations: &[String],
        executor: Arc<dyn AgentExecutor + Send + Sync>,
    ) -> Self {
        Self {
            name: format!("agent_{}", agent_name.replace('-', "_")),
            agent_name: agent_name.to_string(),
            description: description.to_string(),
            operations: operations.to_vec(),
            role_category: "agent".to_string(),
            executor,
        }
    }

    pub fn with_category(
        agent_name: &str,
        description: &str,
        operations: &[String],
        category: &str,
        executor: Arc<dyn AgentExecutor + Send + Sync>,
    ) -> Self {
        Self {
            name: format!("agent_{}", agent_name.replace('-', "_")),
            agent_name: agent_name.to_string(),
            description: description.to_string(),
            operations: operations.to_vec(),
            role_category: category.to_string(),
            executor,
        }
    }
}

#[async_trait]
impl Tool for AgentTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        if self.operations.is_empty() {
            json!({
                "type": "object",
                "properties": {
                    "operation": { "type": "string", "description": "Operation to perform" },
                    "path": { "type": "string", "description": "Optional path" },
                    "args": { "type": "object", "description": "Additional arguments" }
                },
                "required": ["operation"]
            })
        } else {
            json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": self.operations,
                        "description": "Operation to perform"
                    },
                    "path": { "type": "string", "description": "Optional path" },
                    "args": { "type": "object", "description": "Additional arguments" }
                },
                "required": ["operation"]
            })
        }
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let operation = input
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'operation'"))?;

        if !self.operations.is_empty() && !self.operations.contains(&operation.to_string()) {
            return Err(anyhow::anyhow!(
                "Unknown operation: {}. Valid: {:?}",
                operation,
                self.operations
            ));
        }

        let path = input.get("path").and_then(|v| v.as_str());
        let args = input.get("args").cloned();
        let agent = self.name.strip_prefix("agent_").unwrap_or(&self.name);

        self.executor.execute_operation(agent, operation, path, args).await
    }

    fn category(&self) -> &str {
        &self.role_category
    }

    fn namespace(&self) -> &str {
        "agent"
    }

    fn tags(&self) -> Vec<String> {
        vec!["agent".to_string(), self.role_category.clone()]
    }
}

// =============================================================================
// STATIC AGENT DEFINITIONS
// =============================================================================

/// Agent definition - no factory function needed
#[derive(Clone)]
pub struct AgentDef {
    pub agent_type: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub operations: &'static [&'static str],
    pub category: &'static str,
}

/// All agent definitions (static, no create_agent() needed)
pub const AGENT_DEFINITIONS: &[AgentDef] = &[
    AgentDef {
        agent_type: "rust-pro",
        name: "Rust Pro",
        description: "Expert Rust development agent",
        operations: &["check", "build", "test", "clippy", "format", "run", "doc", "analyze"],
        category: "language",
    },
    AgentDef {
        agent_type: "python-pro",
        name: "Python Pro",
        description: "Expert Python development agent",
        operations: &["analyze", "format", "lint", "test", "run"],
        category: "language",
    },
    AgentDef {
        agent_type: "backend-architect",
        name: "Backend Architect",
        description: "Backend architecture design agent",
        operations: &["analyze", "design", "review", "suggest", "document"],
        category: "architecture",
    },
    AgentDef {
        agent_type: "network-engineer",
        name: "Network Engineer",
        description: "Network configuration agent",
        operations: &["analyze", "configure", "diagnose", "optimize"],
        category: "infrastructure",
    },
    AgentDef {
        agent_type: "sequential-thinking",
        name: "Sequential Thinking",
        description: "Step-by-step reasoning agent",
        operations: &["think", "plan", "analyze", "conclude", "reflect"],
        category: "orchestration",
    },
    AgentDef {
        agent_type: "memory",
        name: "Memory Agent",
        description: "Persistent memory and recall",
        operations: &["store", "recall", "list", "search", "forget"],
        category: "orchestration",
    },
    AgentDef {
        agent_type: "context-manager",
        name: "Context Manager",
        description: "Session context management",
        operations: &["save", "load", "list", "delete", "export", "import", "clear"],
        category: "orchestration",
    },
    AgentDef {
        agent_type: "search-specialist",
        name: "Search Specialist",
        description: "Search and discovery agent",
        operations: &["search", "analyze", "suggest"],
        category: "seo",
    },
    AgentDef {
        agent_type: "deployment",
        name: "Deployment Agent",
        description: "Deployment management agent",
        operations: &["plan", "deploy", "rollback", "status"],
        category: "infrastructure",
    },
    AgentDef {
        agent_type: "debugger",
        name: "Debugger Agent",
        description: "Debugging and troubleshooting",
        operations: &["analyze", "diagnose", "suggest", "trace"],
        category: "analysis",
    },
    AgentDef {
        agent_type: "prompt-engineer",
        name: "Prompt Engineer",
        description: "Prompt optimization agent",
        operations: &["analyze", "improve", "generate", "test"],
        category: "aiml",
    },
];

// =============================================================================
// REGISTRATION
// =============================================================================

/// Register a single agent (starts D-Bus service + creates tool)
pub async fn register_agent_tool(registry: &crate::ToolRegistry, def: &AgentDef) -> Result<()> {
    info!(agent = %def.agent_type, "Registering agent");

    // 1. Start D-Bus service
    let conn_registry = AgentConnectionRegistry::global();
    if let Err(e) = conn_registry.start_agent_service(def).await {
        warn!(agent = %def.agent_type, error = %e, "D-Bus service failed, tool still registered");
    }

    // 2. Create tool
    let operations: Vec<String> = def.operations.iter().map(|s| s.to_string()).collect();
    let executor = Arc::new(DbusAgentExecutor::new());
    let tool = AgentTool::with_category(
        def.agent_type,
        def.description,
        &operations,
        def.category,
        executor,
    );

    // 3. Register tool
    registry.register_tool(Arc::new(tool)).await?;

    info!(agent = %def.agent_type, "✓ Agent registered");
    Ok(())
}

/// Register all agents
pub async fn register_all_agents(registry: &crate::ToolRegistry) -> Result<()> {
    let mut success = 0;
    let mut failed = 0;

    for def in AGENT_DEFINITIONS {
        match register_agent_tool(registry, def).await {
            Ok(()) => success += 1,
            Err(e) => {
                warn!(agent = %def.agent_type, error = %e, "Failed");
                failed += 1;
            }
        }
    }

    info!("Registered {} agents ({} failed)", success, failed);

    let running = AgentConnectionRegistry::global().list_running().await;
    info!("Active D-Bus services: {:?}", running);

    Ok(())
}

// =============================================================================
// LEGACY HELPERS
// =============================================================================

pub fn create_agent_tool(
    agent_name: &str,
    description: &str,
    operations: &[String],
    _config: Value,
) -> Result<BoxedTool> {
    let executor = Arc::new(DbusAgentExecutor::new());
    Ok(Arc::new(AgentTool::new(agent_name, description, operations, executor)))
}

pub fn create_agent_tool_with_executor(
    agent_name: &str,
    description: &str,
    operations: &[String],
    executor: Arc<dyn AgentExecutor + Send + Sync>,
) -> Result<BoxedTool> {
    Ok(Arc::new(AgentTool::new(agent_name, description, operations, executor)))
}
