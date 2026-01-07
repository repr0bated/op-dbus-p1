//! Agent Tool - Creates tools from agent specifications
//!
//! Wraps agent operations as MCP-compatible tools.
//! **ACTUALLY EXECUTES** via D-Bus, not placeholder responses.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::tool::{BoxedTool, Tool};

/// Agent tool that wraps agent operations
pub struct AgentTool {
    name: String,
    agent_name: String,
    description: String,
    operations: Vec<String>,
    /// Role category for MCP splitting (language, infrastructure, database, etc.)
    role_category: String,
    #[allow(dead_code)]
    config: Value,
    executor: Arc<dyn AgentExecutor + Send + Sync>,
}

/// Trait for executing agent operations
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Execute an agent operation
    async fn execute_operation(
        &self,
        agent_name: &str,
        operation: &str,
        path: Option<&str>,
        args: Option<Value>,
    ) -> Result<Value>;
}

impl AgentTool {
    pub fn new(
        agent_name: &str,
        description: &str,
        operations: &[String],
        config: Value,
        executor: Arc<dyn AgentExecutor + Send + Sync>,
    ) -> Self {
        Self {
            name: format!("agent_{}", agent_name.replace('-', "_")),
            agent_name: agent_name.to_string(),
            description: description.to_string(),
            operations: operations.to_vec(),
            role_category: "agent".to_string(), // Default
            config,
            executor,
        }
    }

    /// Create with specific role category for MCP splitting
    pub fn with_category(
        agent_name: &str,
        description: &str,
        operations: &[String],
        role_category: &str,
        config: Value,
        executor: Arc<dyn AgentExecutor + Send + Sync>,
    ) -> Self {
        Self {
            name: format!("agent_{}", agent_name.replace('-', "_")),
            agent_name: agent_name.to_string(),
            description: description.to_string(),
            operations: operations.to_vec(),
            role_category: role_category.to_string(),
            config,
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
        // Special schema for sequential_thinking to satisfy Gemini requirements
        if self.agent_name == "sequential_thinking" || self.agent_name == "sequential-thinking" {
            return serde_json::json!({
                "type": "object",
                "properties": {
                    "thought": {
                        "type": "string",
                        "description": "The current thought or reasoning step"
                    },
                    "operation": {
                        "type": "string",
                        "description": "Operation to perform",
                        "enum": ["think", "plan", "analyze", "conclude"]
                    },
                    "step": {
                        "type": "integer",
                        "description": "Current step number"
                    },
                    "total_steps": {
                        "type": "integer",
                        "description": "Total estimated steps"
                    }
                },
                "required": ["thought", "operation", "step", "total_steps"]
            });
        }

        if self.operations.is_empty() {
            return serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "description": "Operation to perform"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional path argument"
                    },
                    "args": {
                        "type": "object",
                        "description": "Additional arguments"
                    }
                },
                "required": ["operation"]
            });
        }
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": self.operations,
                    "description": "Operation to perform"
                },
                "path": {
                    "type": "string",
                    "description": "Optional path argument"
                },
                "args": {
                    "type": "object",
                    "description": "Additional arguments"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        // Handle special case for sequential_thinking agent - accept "thought" as operation content
        let (operation, args) = if self.agent_name == "sequential_thinking" || self.agent_name == "sequential-thinking" {
            // Extract fields regardless of how they are passed
            let thought = input.get("thought").and_then(|v| v.as_str());
            let op = input.get("operation").and_then(|v| v.as_str()).unwrap_or("think");
            let step = input.get("step").and_then(|v| v.as_u64());
            let total_steps = input.get("total_steps").and_then(|v| v.as_u64());

            // Build args object
            let mut args_map = serde_json::Map::new();
            if let Some(t) = thought { args_map.insert("thought".to_string(), serde_json::Value::String(t.to_string())); }
            if let Some(s) = step { args_map.insert("step".to_string(), serde_json::json!(s)); }
            if let Some(ts) = total_steps { args_map.insert("total_steps".to_string(), serde_json::json!(ts)); }
            
            // Merge explicit args if present
            if let Some(explicit_args) = input.get("args").and_then(|v| v.as_object()) {
                for (k, v) in explicit_args {
                    args_map.insert(k.clone(), v.clone());
                }
            }

            (op.to_string(), Some(serde_json::Value::Object(args_map)))
        } else {
            let op = input
                .get("operation")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing required field: operation"))?;
            (op.to_string(), input.get("args").cloned())
        };

        if !self.operations.is_empty() && !self.operations.contains(&operation) {
            // For sequential_thinking, accept "think" even if not in operations list
            if !(self.agent_name.contains("sequential_thinking") && operation == "think") {
                return Err(anyhow::anyhow!(
                    "Unknown operation: {}. Valid operations: {:?}",
                    operation,
                    self.operations
                ));
            }
        }

        let path = input.get("path").and_then(|v| v.as_str());

        // Extract agent name from tool name (remove "agent_" prefix)
        let agent_name = self.name.strip_prefix("agent_").unwrap_or(&self.name);

        info!(
            agent = %agent_name,
            operation = %operation,
            path = ?path,
            "Executing agent operation"
        );

        self.executor
            .execute_operation(agent_name, &operation, path, args)
            .await
    }

    fn category(&self) -> &str {
        &self.role_category
    }

    fn namespace(&self) -> &str {
        if is_control_agent(&self.agent_name) {
            "control-agent"
        } else {
            "agent"
        }
    }

    fn tags(&self) -> Vec<String> {
        vec![
            "agent".to_string(),
            self.role_category.clone(),
            self.agent_name.clone(),
        ]
    }
}

fn is_control_agent(agent_name: &str) -> bool {
    matches!(
        agent_name,
        "executor" | "file" | "monitor" | "network" | "packagekit" | "systemd"
    )
}

/// D-Bus agent executor - ACTUALLY calls agents via D-Bus
pub struct DbusAgentExecutor {
    bus_type: op_core::BusType,
}

impl DbusAgentExecutor {
    pub fn new() -> Self {
        let bus_type = std::env::var("OP_AGENT_BUS")
            .ok()
            .as_deref()
            .map(|value| value.to_lowercase())
            .and_then(|value| match value.as_str() {
                "system" => Some(op_core::BusType::System),
                "session" => Some(op_core::BusType::Session),
                _ => None,
            })
            .unwrap_or(op_core::BusType::System);

        Self {
            bus_type,
        }
    }

    #[allow(dead_code)]
    pub fn with_bus_type(bus_type: op_core::BusType) -> Self {
        Self { bus_type }
    }

    /// Convert agent name to D-Bus service name
    fn to_service_name(agent_name: &str) -> String {
        let pascal = agent_name
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

    /// Convert agent name to D-Bus object path
    fn to_object_path(agent_name: &str) -> String {
        let pascal = agent_name
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

    /// Check if an error indicates the D-Bus service is unavailable
    fn is_service_unavailable_error(error: &zbus::Error) -> bool {
        let error_str = error.to_string().to_lowercase();
        
        // Check for common D-Bus service unavailable patterns
        error_str.contains("serviceunknown")
            || error_str.contains("name has no owner")
            || error_str.contains("namehasnoowner")
            || error_str.contains("not found")
            || error_str.contains("does not exist")
            || error_str.contains("service unknown")
            || error_str.contains("no such")
            || error_str.contains("connection refused")
            || error_str.contains("not available")
            || matches!(error, zbus::Error::NameTaken | zbus::Error::Address(_))
    }
}

impl Default for DbusAgentExecutor {
    fn default() -> Self {
        Self::new()
    }
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
        use zbus::Connection;

        let service_name = Self::to_service_name(agent_name);
        let object_path = Self::to_object_path(agent_name);

        // Build task JSON for the agent
        // Convert args to string if present (agents expect args as string, not object)
        let args_str = args.and_then(|v| {
            if v.is_null() {
                None
            } else {
                Some(serde_json::to_string(&v).ok()?)
            }
        });

        let task = serde_json::json!({
            "type": agent_name.replace('_', "-"),
            "operation": operation,
            "path": path,
            "args": args_str
        });

        let task_json = serde_json::to_string(&task)?;

        debug!(
            agent = %agent_name,
            task = %task_json,
            "Calling agent via D-Bus"
        );

        // Connect to D-Bus - handle connection failure gracefully
        let connection = match self.bus_type {
            op_core::BusType::System => {
                match Connection::system().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        warn!(agent = %agent_name, error = %e, "Failed to connect to system D-Bus");
                        return Ok(serde_json::json!({
                            "available": false,
                            "agent": agent_name,
                            "operation": operation,
                            "error": format!("D-Bus connection failed: {}", e),
                            "message": "Agent service is not available (D-Bus connection failed)"
                        }));
                    }
                }
            }
            op_core::BusType::Session => {
                match Connection::session().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        warn!(agent = %agent_name, error = %e, "Failed to connect to session D-Bus");
                        return Ok(serde_json::json!({
                            "available": false,
                            "agent": agent_name,
                            "operation": operation,
                            "error": format!("D-Bus connection failed: {}", e),
                            "message": "Agent service is not available (D-Bus connection failed)"
                        }));
                    }
                }
            }
        };

        debug!(
            service = %service_name,
            path = %object_path,
            "D-Bus call target"
        );

        // Create proxy - handle build failure gracefully
        let proxy: zbus::Proxy = match zbus::proxy::Builder::new(&connection)
            .destination(service_name.as_str())
            .and_then(|b| b.path(object_path.as_str()))
            .and_then(|b| b.interface("org.dbusmcp.Agent"))
        {
            Ok(builder) => {
                match builder.build().await {
                    Ok(p) => p,
                    Err(e) => {
                        if Self::is_service_unavailable_error(&e) {
                            warn!(agent = %agent_name, service = %service_name, "Agent service not available on D-Bus");
                            return Ok(serde_json::json!({
                                "available": false,
                                "agent": agent_name,
                                "service": service_name,
                                "operation": operation,
                                "error": format!("Service not found: {}", e),
                                "message": format!("Agent '{}' is not running or not registered on D-Bus", agent_name)
                            }));
                        }
                        error!(error = %e, "D-Bus proxy build failed");
                        return Err(anyhow::anyhow!("D-Bus proxy build failed: {}", e));
                    }
                }
            }
            Err(e) => {
                warn!(agent = %agent_name, error = %e, "Failed to build D-Bus proxy");
                return Ok(serde_json::json!({
                    "available": false,
                    "agent": agent_name,
                    "operation": operation,
                    "error": format!("Proxy configuration error: {}", e),
                    "message": "Agent service is not available (proxy configuration failed)"
                }));
            }
        };

        // Call the Execute method - handle service unavailable gracefully
        let result: String = match proxy.call("Execute", &(task_json,)).await {
            Ok(r) => r,
            Err(e) => {
                if Self::is_service_unavailable_error(&e) {
                    warn!(
                        agent = %agent_name,
                        service = %service_name,
                        error = %e,
                        "Agent D-Bus service not available"
                    );
                    return Ok(serde_json::json!({
                        "available": false,
                        "agent": agent_name,
                        "service": service_name,
                        "operation": operation,
                        "error": e.to_string(),
                        "message": format!("Agent '{}' is not running. The D-Bus service '{}' is not registered.", agent_name, service_name)
                    }));
                }
                // For other errors, still return gracefully but log as error
                error!(error = %e, agent = %agent_name, "D-Bus call failed");
                return Ok(serde_json::json!({
                    "available": false,
                    "agent": agent_name,
                    "service": service_name,
                    "operation": operation,
                    "error": e.to_string(),
                    "message": format!("D-Bus call to agent '{}' failed: {}", agent_name, e)
                }));
            }
        };

        // Parse result JSON
        let parsed: Value = serde_json::from_str(&result).map_err(|e| {
            error!(error = %e, result = %result, "Failed to parse agent response");
            anyhow::anyhow!("Failed to parse agent response: {}", e)
        })?;

        info!(
            agent = %agent_name,
            operation = %operation,
            success = %parsed.get("success").and_then(|v| v.as_bool()).unwrap_or(false),
            "Agent operation completed"
        );

        Ok(parsed)
    }
}

/// In-process agent executor - for agents running in same process
#[allow(dead_code)]
pub struct InProcessAgentExecutor {
    agents: Arc<tokio::sync::RwLock<std::collections::HashMap<String, Box<dyn InProcessAgent + Send + Sync>>>>,
}

/// Trait for in-process agents
#[async_trait]
#[allow(dead_code)]
pub trait InProcessAgent: Send + Sync {
    async fn execute(&self, operation: &str, path: Option<&str>, args: Option<Value>) -> Result<Value>;
}

impl InProcessAgentExecutor {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            agents: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    #[allow(dead_code)]
    pub async fn register_agent(&self, name: &str, agent: Box<dyn InProcessAgent + Send + Sync>) {
        let mut agents = self.agents.write().await;
        agents.insert(name.to_string(), agent);
    }
}

impl Default for InProcessAgentExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentExecutor for InProcessAgentExecutor {
    async fn execute_operation(
        &self,
        agent_name: &str,
        operation: &str,
        path: Option<&str>,
        args: Option<Value>,
    ) -> Result<Value> {
        let agents = self.agents.read().await;
        let agent = agents
            .get(agent_name)
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", agent_name))?;

        agent.execute(operation, path, args).await
    }
}

/// Create an agent tool with D-Bus executor (default)
pub fn create_agent_tool(
    agent_name: &str,
    description: &str,
    operations: &[String],
    config: Value,
) -> Result<BoxedTool> {
    let executor = Arc::new(DbusAgentExecutor::new());
    Ok(Arc::new(AgentTool::new(
        agent_name,
        description,
        operations,
        config,
        executor,
    )))
}

/// Create an agent tool with custom executor
pub fn create_agent_tool_with_executor(
    agent_name: &str,
    description: &str,
    operations: &[String],
    config: Value,
    executor: Arc<dyn AgentExecutor + Send + Sync>,
) -> Result<BoxedTool> {
    Ok(Arc::new(AgentTool::new(
        agent_name,
        description,
        operations,
        config,
        executor,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAgentExecutor;

    #[async_trait]
    impl AgentExecutor for MockAgentExecutor {
        async fn execute_operation(
            &self,
            agent_name: &str,
            operation: &str,
            path: Option<&str>,
            _args: Option<Value>,
        ) -> Result<Value> {
            Ok(serde_json::json!({
                "success": true,
                "agent": agent_name,
                "operation": operation,
                "path": path,
                "executed": true
            }))
        }
    }

    #[test]
    fn test_service_name_conversion() {
        assert_eq!(
            DbusAgentExecutor::to_service_name("python_pro"),
            "org.dbusmcp.Agent.PythonPro"
        );
        assert_eq!(
            DbusAgentExecutor::to_service_name("rust_pro"),
            "org.dbusmcp.Agent.RustPro"
        );
    }

    #[tokio::test]
    async fn test_agent_tool_execution() {
        let executor = Arc::new(MockAgentExecutor);
        let tool = AgentTool::new(
            "test-agent",
            "Test agent",
            &["build".to_string(), "test".to_string()],
            serde_json::json!({}),
            executor,
        );

        let result = tool
            .execute(serde_json::json!({
                "operation": "build",
                "path": "/tmp/project"
            }))
            .await
            .unwrap();

        assert_eq!(
            result.get("operation").and_then(|v| v.as_str()),
            Some("build")
        );
        assert_eq!(
            result.get("executed").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[tokio::test]
    async fn test_invalid_operation() {
        let executor = Arc::new(MockAgentExecutor);
        let tool = AgentTool::new(
            "test-agent",
            "Test agent",
            &["build".to_string()],
            serde_json::json!({}),
            executor,
        );

        let result = tool
            .execute(serde_json::json!({
                "operation": "invalid_op"
            }))
            .await;

        assert!(result.is_err());
    }
}
