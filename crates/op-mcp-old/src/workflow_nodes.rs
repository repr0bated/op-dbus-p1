//! Workflow Node Discovery and Registration
//! Exposes plugins, services, and agents as workflow nodes

use op_state::plugin::StatePlugin;
use op_state::manager::StateManager;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// A workflow node that can be used in flow-based workflows
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    /// Unique node identifier
    pub id: String,
    
    /// Display name
    pub name: String,
    
    /// Node type: "plugin", "service", "agent", "dbus-method", etc.
    pub node_type: String,
    
    /// Category for grouping in palette
    pub category: String,
    
    /// Icon/emoji for visual representation
    pub icon: String,
    
    /// Description of what this node does
    pub description: String,
    
    /// Input ports (data this node expects)
    pub inputs: Vec<NodePort>,
    
    /// Output ports (data this node produces)
    pub outputs: Vec<NodePort>,
    
    /// Configuration schema (JSON schema for node properties)
    pub config_schema: Value,
    
    /// Default configuration
    pub default_config: Value,
    
    /// Metadata (plugin name, service name, etc.)
    pub metadata: HashMap<String, Value>,
}

/// A port on a workflow node (input or output)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePort {
    /// Port identifier
    pub id: String,
    
    /// Display name
    pub name: String,
    
    /// Data type (e.g., "string", "number", "object", "state")
    pub data_type: String,
    
    /// Whether this port is required
    pub required: bool,
    
    /// Description
    pub description: Option<String>,
}

/// Discover all available workflow nodes
pub async fn discover_workflow_nodes(
    state_manager: Option<&StateManager>,
) -> Result<Vec<WorkflowNode>, Box<dyn std::error::Error>> {
    let mut nodes = Vec::new();
    
    // 1. Discover plugins as nodes
    if let Some(manager) = state_manager {
        nodes.extend(discover_plugin_nodes(manager).await?);
    }
    
    // 2. Discover D-Bus services as nodes (via introspection cache)
    nodes.extend(discover_dbus_service_nodes().await?);
    
    // 3. Discover MCP agents as nodes
    nodes.extend(discover_agent_nodes().await?);
    
    Ok(nodes)
}

/// Convert plugins to workflow nodes
async fn discover_plugin_nodes(
    manager: &StateManager,
) -> Result<Vec<WorkflowNode>, Box<dyn std::error::Error>> {
    let mut nodes = Vec::new();
    
    // Get all registered plugins
    let plugins = manager.list_plugins();
    
    for plugin in plugins {
        let node = WorkflowNode {
            id: format!("plugin:{}", plugin.name()),
            name: plugin.name().to_string(),
            node_type: "plugin".to_string(),
            category: "Plugins".to_string(),
            icon: "🔌".to_string(),
            description: format!("State management plugin: {}", plugin.name()),
            inputs: vec![
                NodePort {
                    id: "desired_state".to_string(),
                    name: "Desired State".to_string(),
                    data_type: "object".to_string(),
                    required: false,
                    description: Some("Desired state configuration".to_string()),
                },
            ],
            outputs: vec![
                NodePort {
                    id: "current_state".to_string(),
                    name: "Current State".to_string(),
                    data_type: "object".to_string(),
                    required: false,
                    description: Some("Current system state".to_string()),
                },
                NodePort {
                    id: "diff".to_string(),
                    name: "State Diff".to_string(),
                    data_type: "object".to_string(),
                    required: false,
                    description: Some("Calculated state difference".to_string()),
                },
                NodePort {
                    id: "apply_result".to_string(),
                    name: "Apply Result".to_string(),
                    data_type: "object".to_string(),
                    required: false,
                    description: Some("Result of applying state changes".to_string()),
                },
            ],
            config_schema: json!({
                "type": "object",
                "properties": {
                    "plugin_name": {
                        "type": "string",
                        "default": plugin.name()
                    }
                }
            }),
            default_config: json!({
                "plugin_name": plugin.name()
            }),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("plugin_name".to_string(), json!(plugin.name()));
                meta.insert("version".to_string(), json!(plugin.version()));
                meta.insert("available".to_string(), json!(plugin.is_available()));
                meta
            },
        };
        
        nodes.push(node);
    }
    
    Ok(nodes)
}

/// Convert D-Bus services to workflow nodes
async fn discover_dbus_service_nodes() -> Result<Vec<WorkflowNode>, Box<dyn std::error::Error>> {
    let mut nodes = Vec::new();
    
    // Use introspection cache to discover services
    let cache_path = std::env::var("OPDBUS_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/var/lib/op-dbus/cache"))
        .join("introspection.db");
    
    if let Ok(cache) = crate::introspection_cache::IntrospectionCache::new(&cache_path) {
        // Get cache stats to find services
        if let Ok(stats) = cache.get_stats() {
            // For each service, create nodes for:
            // 1. Service node (can call any method)
            // 2. Individual method nodes (for specific methods)
            
            // Note: In a full implementation, we'd query the cache for all services
            // and create nodes dynamically. For now, we create generic service nodes.
        }
    }
    
    // Create generic D-Bus service node
    nodes.push(WorkflowNode {
        id: "dbus:service".to_string(),
        name: "D-Bus Service".to_string(),
        node_type: "dbus-service".to_string(),
        category: "D-Bus Services".to_string(),
        icon: "📞".to_string(),
        description: "Call methods on a D-Bus service".to_string(),
        inputs: vec![
            NodePort {
                id: "service".to_string(),
                name: "Service Name".to_string(),
                data_type: "string".to_string(),
                required: true,
                description: Some("D-Bus service name (e.g., org.freedesktop.systemd1)".to_string()),
            },
            NodePort {
                id: "path".to_string(),
                name: "Object Path".to_string(),
                data_type: "string".to_string(),
                required: true,
                description: Some("Object path (e.g., /org/freedesktop/systemd1)".to_string()),
            },
            NodePort {
                id: "method".to_string(),
                name: "Method Name".to_string(),
                data_type: "string".to_string(),
                required: true,
                description: Some("Method to call".to_string()),
            },
            NodePort {
                id: "args".to_string(),
                name: "Arguments".to_string(),
                data_type: "array".to_string(),
                required: false,
                description: Some("Method arguments".to_string()),
            },
        ],
        outputs: vec![
            NodePort {
                id: "result".to_string(),
                name: "Method Result".to_string(),
                data_type: "object".to_string(),
                required: false,
                description: Some("Result from D-Bus method call".to_string()),
            },
        ],
        config_schema: json!({
            "type": "object",
            "properties": {
                "service": {"type": "string"},
                "path": {"type": "string"},
                "interface": {"type": "string"},
                "method": {"type": "string"}
            },
            "required": ["service", "path", "method"]
        }),
        default_config: json!({}),
        metadata: HashMap::new(),
    });
    
    Ok(nodes)
}

/// Convert MCP agents to workflow nodes
async fn discover_agent_nodes() -> Result<Vec<WorkflowNode>, Box<dyn std::error::Error>> {
    let mut nodes = Vec::new();
    
    // Agent types from agent_registry
    let agent_types = vec![
        ("executor", "Command Executor", "⚡", "Execute whitelisted commands"),
        ("systemd", "Systemd Controller", "🔧", "Manage systemd services"),
        ("file", "File Manager", "📁", "File operations"),
        ("network", "Network Manager", "🌐", "Network configuration"),
        ("packagekit", "Package Manager", "📦", "Package management"),
        ("monitor", "System Monitor", "📊", "System metrics"),
    ];
    
    for (agent_type, name, icon, description) in agent_types {
        nodes.push(WorkflowNode {
            id: format!("agent:{}", agent_type),
            name: name.to_string(),
            node_type: "agent".to_string(),
            category: "Agents".to_string(),
            icon: icon.to_string(),
            description: description.to_string(),
            inputs: vec![
                NodePort {
                    id: "task".to_string(),
                    name: "Task".to_string(),
                    data_type: "object".to_string(),
                    required: true,
                    description: Some(format!("Task for {} agent", agent_type)),
                },
            ],
            outputs: vec![
                NodePort {
                    id: "result".to_string(),
                    name: "Result".to_string(),
                    data_type: "object".to_string(),
                    required: false,
                    description: Some("Agent execution result".to_string()),
                },
            ],
            config_schema: json!({
                "type": "object",
                "properties": {
                    "agent_type": {
                        "type": "string",
                        "default": agent_type
                    }
                }
            }),
            default_config: json!({
                "agent_type": agent_type
            }),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("agent_type".to_string(), json!(agent_type));
                meta
            },
        });
    }
    
    Ok(nodes)
}

/// Get workflow node by ID
pub async fn get_workflow_node(
    node_id: &str,
    state_manager: Option<&StateManager>,
) -> Result<Option<WorkflowNode>, Box<dyn std::error::Error>> {
    let nodes = discover_workflow_nodes(state_manager).await?;
    Ok(nodes.into_iter().find(|n| n.id == node_id))
}

/// Get nodes by category
pub async fn get_nodes_by_category(
    category: &str,
    state_manager: Option<&StateManager>,
) -> Result<Vec<WorkflowNode>, Box<dyn std::error::Error>> {
    let nodes = discover_workflow_nodes(state_manager).await?;
    Ok(nodes.into_iter().filter(|n| n.category == category).collect())
}

