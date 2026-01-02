//! Agent Tool - Creates tools from agent specifications
//!
//! Wraps agent operations as MCP-compatible tools.
//! **ACTUALLY EXECUTES** via D-Bus, not placeholder responses.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, info};

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
        let operation = input
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required field: operation"))?;

        if !self.operations.is_empty() && !self.operations.contains(&operation.to_string()) {
            return Err(anyhow::anyhow!(
                "Unknown operation: {}. Valid operations: {:?}",
                operation,
                self.operations
            ));
        }

        let path = input.get("path").and_then(|v| v.as_str());
        let args = input.get("args").cloned();

        // Extract agent name from tool name (remove "agent_" prefix)
        let agent_name = self.name.strip_prefix("agent_").unwrap_or(&self.name);

        info!(
            agent = %agent_name,
            operation = %operation,
            path = ?path,
            "Executing agent operation"
        );

        self.executor
            .execute_operation(agent_name, operation, path, args)
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

        // Connect to D-Bus
        let connection = match self.bus_type {
            op_core::BusType::System => Connection::system().await?,
            op_core::BusType::Session => Connection::session().await?,
        };

        let service_name = Self::to_service_name(agent_name);
        let object_path = Self::to_object_path(agent_name);

        debug!(
            service = %service_name,
            path = %object_path,
            "D-Bus call target"
        );

        // Create proxy and call Execute method
        let proxy: zbus::Proxy = zbus::proxy::Builder::new(&connection)
            .destination(service_name.as_str())?
            .path(object_path.as_str())?
            .interface("org.dbusmcp.Agent")?
            .build()
            .await?;

        // Call the Execute method
        let result: String = proxy.call("Execute", &(task_json,)).await.map_err(|e| {
            error!(error = %e, "D-Bus call failed");
            anyhow::anyhow!("D-Bus call failed: {}", e)
        })?;

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
