//! Lazy tool factory implementations for dynamic tool loading
//!
//! Provides factories that can create tool instances on-demand without
//! loading them all at startup.

use crate::builtin::dbus_hybrid::DbusMethodTool;
use crate::registry::{ToolDefinition, ToolFactory};
use crate::tool::BoxedTool;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Factory for creating D-Bus method tools lazily
pub struct DbusToolFactory {
    /// Tool name (cached)
    pub tool_name: String,
    /// Service name (e.g., "org.freedesktop.systemd1")
    pub service: String,
    /// Object path
    pub path: String,
    /// Interface name
    pub interface: String,
    /// Method name
    pub method: String,
    /// Input signature
    pub input_signature: String,
    /// Output signature
    pub output_signature: String,
    /// Bus type (session or system)
    pub bus_type: BusType,
    /// Tool description
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusType {
    Session,
    System,
}

#[async_trait]
impl ToolFactory for DbusToolFactory {
    fn tool_name(&self) -> &str {
        &self.tool_name
    }

    async fn create(&self) -> Result<BoxedTool> {
        use crate::builtin::dbus_hybrid::create_dbus_method_tool;

        let use_system_bus = matches!(self.bus_type, BusType::System);

        let tool = create_dbus_method_tool(
            &self.service,
            &self.path,
            &self.interface,
            &self.method,
            &self.input_signature,
            &self.output_signature,
            use_system_bus,
        )?;

        Ok(tool)
    }

    fn definition(&self) -> ToolDefinition {
        let tool_name = format!(
            "dbus_{}_{}",
            self.interface.replace('.', "_").to_lowercase(),
            self.method.to_lowercase()
        );

        let input_schema = DbusMethodTool::generate_schema_from_signature(&self.input_signature);

        ToolDefinition {
            name: tool_name,
            description: self.description.clone(),
            input_schema,
            category: "dbus".to_string(),
            tags: vec!["dbus".to_string(), self.service.clone(), self.interface.clone()],
        }
    }
}

impl DbusToolFactory {
    fn generate_input_schema(&self) -> Value {
        // Generate JSON schema from D-Bus signature
        let properties = self.signature_to_schema(&self.input_signature);
        
        serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": self.get_required_params(&self.input_signature)
        })
    }
    
    fn signature_to_schema(&self, signature: &str) -> HashMap<String, Value> {
        let mut properties = HashMap::new();
        let mut param_idx = 0;
        
        for c in signature.chars() {
            let (name, schema) = match c {
                's' => (
                    format!("param{}", param_idx),
                    serde_json::json!({"type": "string"})
                ),
                'i' | 'n' => (
                    format!("param{}", param_idx),
                    serde_json::json!({"type": "integer"})
                ),
                'u' | 'q' | 't' | 'x' => (
                    format!("param{}", param_idx),
                    serde_json::json!({"type": "integer", "minimum": 0})
                ),
                'b' => (
                    format!("param{}", param_idx),
                    serde_json::json!({"type": "boolean"})
                ),
                'd' => (
                    format!("param{}", param_idx),
                    serde_json::json!({"type": "number"})
                ),
                'o' => (
                    format!("param{}", param_idx),
                    serde_json::json!({"type": "string", "description": "D-Bus object path"})
                ),
                'a' | '(' | ')' | '{' | '}' | 'v' => continue, // Complex types handled separately
                _ => continue,
            };
            
            properties.insert(name, schema);
            param_idx += 1;
        }
        
        properties
    }
    
    fn get_required_params(&self, signature: &str) -> Vec<String> {
        let mut required = Vec::new();
        let mut param_idx = 0;
        
        for c in signature.chars() {
            match c {
                's' | 'i' | 'n' | 'u' | 'q' | 't' | 'x' | 'b' | 'd' | 'o' => {
                    required.push(format!("param{}", param_idx));
                    param_idx += 1;
                }
                _ => {}
            }
        }
        
        required
    }
}

/// Factory for creating agent tools lazily
pub struct AgentToolFactory {
    /// Agent name
    pub agent_name: String,
    /// Agent description
    pub description: String,
    /// Operations supported by this agent
    pub operations: Vec<String>,
    /// Agent configuration
    pub config: Value,
}

#[async_trait]
impl ToolFactory for AgentToolFactory {
    fn tool_name(&self) -> &str {
        &self.agent_name // Use agent_name as tool name prefix or similar
    }

    async fn create(&self) -> Result<BoxedTool> {
        use crate::builtin::agent_tool::create_agent_tool;

        let tool = create_agent_tool(
            &self.agent_name,
            &self.description,
            &self.operations,
            self.config.clone(),
        )?;

        Ok(tool)
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: format!("agent_{}", self.agent_name.replace('-', "_")),
            description: self.description.clone(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Task description for the agent"
                    }
                },
                "required": ["task"]
            }),
            category: "agents".to_string(),
            tags: vec!["agent".to_string(), self.agent_name.clone()],
        }
    }
}

/// Factory for creating plugin state tools lazily
pub struct PluginStateToolFactory {
    /// Plugin name
    pub plugin_name: String,
    /// Plugin description
    pub description: String,
    /// Operation type (query, diff, apply)
    pub operation: PluginOperation,
    /// Plugin capabilities
    pub capabilities: PluginCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginOperation {
    Query,
    Diff,
    Apply,
}

#[derive(Debug, Clone, Default)]
pub struct PluginCapabilities {
    pub supports_rollback: bool,
    pub supports_checkpoints: bool,
    pub supports_verification: bool,
    pub atomic_operations: bool,
}

#[async_trait]
impl ToolFactory for PluginStateToolFactory {
    fn tool_name(&self) -> &str {
        &self.plugin_name
    }

    async fn create(&self) -> Result<BoxedTool> {
        use crate::builtin::plugin_state_tool::create_plugin_state_tool;
        
        let tool = create_plugin_state_tool(
            &self.plugin_name,
            &self.description,
            self.operation,
            &self.capabilities,
        )?;
        
        Ok(tool)
    }

    fn definition(&self) -> ToolDefinition {
        let op_suffix = match self.operation {
            PluginOperation::Query => "query",
            PluginOperation::Diff => "diff",
            PluginOperation::Apply => "apply",
        };
        
        let (description, schema) = match self.operation {
            PluginOperation::Query => (
                format!("Query current state from {} plugin", self.plugin_name),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "filter": {
                            "type": "object",
                            "description": "Optional filter for state query"
                        }
                    }
                })
            ),
            PluginOperation::Diff => (
                format!("Calculate diff between current and desired state for {} plugin", self.plugin_name),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "desired_state": {
                            "type": "object",
                            "description": "Desired state configuration"
                        }
                    },
                    "required": ["desired_state"]
                })
            ),
            PluginOperation::Apply => (
                format!("Apply state changes for {} plugin", self.plugin_name),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "diff": {
                            "type": "object",
                            "description": "State diff to apply"
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, only simulate changes",
                            "default": false
                        }
                    },
                    "required": ["diff"]
                })
            ),
        };
        
        ToolDefinition {
            name: format!("{}_{}", self.plugin_name, op_suffix),
            description,
            input_schema: schema,
            category: "state".to_string(),
            tags: vec!["state".to_string(), "plugin".to_string(), self.plugin_name.clone()],
        }
    }
}

/// Composite factory that can create tools from multiple sources
pub struct CompositeToolFactory {
    factories: Arc<RwLock<HashMap<String, Box<dyn ToolFactory + Send + Sync>>>>,
}

impl CompositeToolFactory {
    pub fn new() -> Self {
        Self {
            factories: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn register(&self, name: &str, factory: Box<dyn ToolFactory + Send + Sync>) {
        let mut factories = self.factories.write().await;
        factories.insert(name.to_string(), factory);
    }
    
    pub async fn get_definition(&self, name: &str) -> Option<ToolDefinition> {
        let factories = self.factories.read().await;
        factories.get(name).map(|f| f.definition())
    }
    
    pub async fn create_tool(&self, name: &str) -> Result<BoxedTool> {
        let factories = self.factories.read().await;
        match factories.get(name) {
            Some(factory) => factory.create().await,
            None => anyhow::bail!("No factory registered for tool: {}", name),
        }
    }
    
    pub async fn list_definitions(&self) -> Vec<ToolDefinition> {
        let factories = self.factories.read().await;
        factories.values().map(|f| f.definition()).collect()
    }
}

impl Default for CompositeToolFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dbus_signature_to_schema() {
        let factory = DbusToolFactory {
            tool_name: "test_dbus_tool".to_string(),
            service: "org.example".to_string(),
            path: "/org/example".to_string(),
            interface: "org.example.Test".to_string(),
            method: "DoSomething".to_string(),
            input_signature: "sib".to_string(),
            output_signature: "s".to_string(),
            bus_type: BusType::Session,
            description: "Test tool".to_string(),
        };

        let schema = factory.signature_to_schema("sib");
        assert_eq!(schema.len(), 3);
        assert!(schema.contains_key("param0"));
        assert!(schema.contains_key("param1"));
        assert!(schema.contains_key("param2"));
    }

    #[test]
    fn test_plugin_tool_definition() {
        let factory = PluginStateToolFactory {
            plugin_name: "packagekit".to_string(),
            description: "Package management".to_string(),
            operation: PluginOperation::Query,
            capabilities: PluginCapabilities::default(),
        };

        let def = factory.definition();
        assert_eq!(def.name, "packagekit_query");
        assert_eq!(def.category, "state");
    }
}
