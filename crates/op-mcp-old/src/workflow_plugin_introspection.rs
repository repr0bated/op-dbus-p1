//! Introspection of Workflows and Plugins for AI Context
//!
//! Provides comprehensive information about available workflows and plugins
//! so the AI can understand what operations are available in the system.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Information about an available workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInfo {
    pub name: String,
    pub description: String,
    pub initial_state: String,
    pub final_states: Vec<String>,
    pub nodes: Vec<WorkflowNodeInfo>,
    pub transitions: Vec<WorkflowTransition>,
}

/// Information about a workflow node/step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNodeInfo {
    pub node_type: String,
    pub name: String,
    pub description: String,
    pub inputs: Vec<NodeParameter>,
    pub outputs: Vec<NodeParameter>,
    pub language: Option<String>, // For code review nodes
}

/// Parameter for workflow nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeParameter {
    pub name: String,
    pub param_type: String,
    pub required: bool,
    pub description: Option<String>,
}

/// Workflow state transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTransition {
    pub from_state: String,
    pub to_state: String,
    pub condition: String,
    pub description: Option<String>,
}

/// Information about a plugin capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub capabilities: PluginCapabilities,
    pub managed_resources: Vec<String>,
}

/// Plugin capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapabilities {
    pub can_query_state: bool,
    pub can_apply_state: bool,
    pub can_rollback: bool,
    pub can_create_checkpoints: bool,
    pub supports_diffs: bool,
    pub supports_verification: bool,
}

/// System introspection including workflows and plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPluginIntrospection {
    pub timestamp: u64,
    pub workflows: Vec<WorkflowInfo>,
    pub plugins: Vec<PluginInfo>,
    pub total_workflows: usize,
    pub available_plugins: usize,
    pub unavailable_plugins: usize,
}

impl WorkflowPluginIntrospection {
    /// Create a comprehensive introspection from available workflows and plugins
    pub fn new() -> Self {
        Self {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            workflows: Self::get_available_workflows(),
            plugins: Self::get_available_plugins(),
            total_workflows: 0,
            available_plugins: 0,
            unavailable_plugins: 0,
        }
    }

    /// Get all available workflows in the system
    fn get_available_workflows() -> Vec<WorkflowInfo> {
        vec![
            WorkflowInfo {
                name: "code_review".to_string(),
                description: "Automated code review and analysis workflow".to_string(),
                initial_state: "start".to_string(),
                final_states: vec!["success".to_string(), "failure".to_string()],
                nodes: vec![
                    WorkflowNodeInfo {
                        node_type: "code_review".to_string(),
                        name: "Code Review Node".to_string(),
                        description: "Analyzes code for issues and best practices".to_string(),
                        inputs: vec![
                            NodeParameter {
                                name: "code".to_string(),
                                param_type: "string".to_string(),
                                required: true,
                                description: Some("Source code to review".to_string()),
                            },
                            NodeParameter {
                                name: "language".to_string(),
                                param_type: "string".to_string(),
                                required: true,
                                description: Some("Programming language (rust, python, etc)".to_string()),
                            },
                        ],
                        outputs: vec![
                            NodeParameter {
                                name: "analysis".to_string(),
                                param_type: "object".to_string(),
                                required: true,
                                description: Some("Code analysis results".to_string()),
                            },
                        ],
                        language: Some("polyglot".to_string()),
                    },
                    WorkflowNodeInfo {
                        node_type: "test_generation".to_string(),
                        name: "Test Generation Node".to_string(),
                        description: "Generates tests for the reviewed code".to_string(),
                        inputs: vec![
                            NodeParameter {
                                name: "code".to_string(),
                                param_type: "string".to_string(),
                                required: true,
                                description: Some("Code to generate tests for".to_string()),
                            },
                            NodeParameter {
                                name: "test_framework".to_string(),
                                param_type: "string".to_string(),
                                required: false,
                                description: Some("Testing framework preference".to_string()),
                            },
                        ],
                        outputs: vec![
                            NodeParameter {
                                name: "tests".to_string(),
                                param_type: "string".to_string(),
                                required: true,
                                description: Some("Generated test code".to_string()),
                            },
                        ],
                        language: None,
                    },
                ],
                transitions: vec![
                    WorkflowTransition {
                        from_state: "start".to_string(),
                        to_state: "code_analyzed".to_string(),
                        condition: "code_review_complete".to_string(),
                        description: Some("Code review finished successfully".to_string()),
                    },
                    WorkflowTransition {
                        from_state: "code_analyzed".to_string(),
                        to_state: "tests_generated".to_string(),
                        condition: "tests_created".to_string(),
                        description: Some("Tests have been generated".to_string()),
                    },
                    WorkflowTransition {
                        from_state: "tests_generated".to_string(),
                        to_state: "success".to_string(),
                        condition: "all_complete".to_string(),
                        description: Some("Workflow completed successfully".to_string()),
                    },
                ],
            },
            WorkflowInfo {
                name: "deployment".to_string(),
                description: "Application deployment workflow with rollback capability".to_string(),
                initial_state: "start".to_string(),
                final_states: vec!["deployed".to_string(), "rolled_back".to_string()],
                nodes: vec![
                    WorkflowNodeInfo {
                        node_type: "checkpoint".to_string(),
                        name: "Create Checkpoint".to_string(),
                        description: "Creates rollback checkpoint before deployment".to_string(),
                        inputs: vec![],
                        outputs: vec![
                            NodeParameter {
                                name: "checkpoint_id".to_string(),
                                param_type: "string".to_string(),
                                required: true,
                                description: Some("ID of created checkpoint".to_string()),
                            },
                        ],
                        language: None,
                    },
                ],
                transitions: vec![
                    WorkflowTransition {
                        from_state: "start".to_string(),
                        to_state: "checkpoint_created".to_string(),
                        condition: "checkpoint_ready".to_string(),
                        description: None,
                    },
                ],
            },
        ]
    }

    /// Get all available state plugins
    fn get_available_plugins() -> Vec<PluginInfo> {
        vec![
            PluginInfo {
                name: "systemd".to_string(),
                version: "1.0.0".to_string(),
                description: "systemd service and unit management".to_string(),
                available: true,
                unavailable_reason: None,
                capabilities: PluginCapabilities {
                    can_query_state: true,
                    can_apply_state: true,
                    can_rollback: true,
                    can_create_checkpoints: true,
                    supports_diffs: true,
                    supports_verification: true,
                },
                managed_resources: vec![
                    "systemd.services".to_string(),
                    "systemd.units".to_string(),
                    "systemd.targets".to_string(),
                ],
            },
            PluginInfo {
                name: "network".to_string(),
                version: "1.0.0".to_string(),
                description: "Network interface and routing management".to_string(),
                available: true,
                unavailable_reason: None,
                capabilities: PluginCapabilities {
                    can_query_state: true,
                    can_apply_state: true,
                    can_rollback: true,
                    can_create_checkpoints: true,
                    supports_diffs: true,
                    supports_verification: true,
                },
                managed_resources: vec![
                    "network.interfaces".to_string(),
                    "network.routes".to_string(),
                    "network.dns".to_string(),
                ],
            },
            PluginInfo {
                name: "packagekit".to_string(),
                version: "1.0.0".to_string(),
                description: "Package management (install, update, remove)".to_string(),
                available: true,
                unavailable_reason: None,
                capabilities: PluginCapabilities {
                    can_query_state: true,
                    can_apply_state: true,
                    can_rollback: false,
                    can_create_checkpoints: true,
                    supports_diffs: true,
                    supports_verification: true,
                },
                managed_resources: vec![
                    "packages.installed".to_string(),
                    "packages.available".to_string(),
                    "packages.updates".to_string(),
                ],
            },
            PluginInfo {
                name: "login1".to_string(),
                version: "1.0.0".to_string(),
                description: "User login session and power management".to_string(),
                available: true,
                unavailable_reason: None,
                capabilities: PluginCapabilities {
                    can_query_state: true,
                    can_apply_state: true,
                    can_rollback: false,
                    can_create_checkpoints: false,
                    supports_diffs: true,
                    supports_verification: true,
                },
                managed_resources: vec![
                    "sessions.active".to_string(),
                    "users.logged_in".to_string(),
                    "power.state".to_string(),
                ],
            },
            PluginInfo {
                name: "keyring".to_string(),
                version: "1.0.0".to_string(),
                description: "Secret/credential management".to_string(),
                available: true,
                unavailable_reason: None,
                capabilities: PluginCapabilities {
                    can_query_state: true,
                    can_apply_state: true,
                    can_rollback: true,
                    can_create_checkpoints: true,
                    supports_diffs: false,
                    supports_verification: true,
                },
                managed_resources: vec![
                    "secrets.stored".to_string(),
                    "credentials.cached".to_string(),
                ],
            },
            PluginInfo {
                name: "lxc".to_string(),
                version: "1.0.0".to_string(),
                description: "LXC container lifecycle management".to_string(),
                available: true,
                unavailable_reason: None,
                capabilities: PluginCapabilities {
                    can_query_state: true,
                    can_apply_state: true,
                    can_rollback: true,
                    can_create_checkpoints: true,
                    supports_diffs: true,
                    supports_verification: true,
                },
                managed_resources: vec![
                    "containers.active".to_string(),
                    "containers.config".to_string(),
                    "containers.resources".to_string(),
                ],
            },
            PluginInfo {
                name: "openflow".to_string(),
                version: "1.0.0".to_string(),
                description: "OpenFlow network programming (experimental)".to_string(),
                available: cfg!(feature = "openflow"),
                unavailable_reason: if cfg!(feature = "openflow") {
                    None
                } else {
                    Some("Requires 'openflow' feature flag".to_string())
                },
                capabilities: PluginCapabilities {
                    can_query_state: true,
                    can_apply_state: true,
                    can_rollback: true,
                    can_create_checkpoints: true,
                    supports_diffs: true,
                    supports_verification: true,
                },
                managed_resources: vec![
                    "openflow.switches".to_string(),
                    "openflow.flows".to_string(),
                    "openflow.ports".to_string(),
                ],
            },
        ]
    }

    /// Convert to JSON for AI context
    pub fn to_json(&self) -> Value {
        json!(self)
    }

    /// Get a summary suitable for AI chat context
    pub fn to_chat_context(&self) -> String {
        let mut context = format!(
            "System has {} workflows and {} state plugins ({} available, {} unavailable).\n\n",
            self.workflows.len(),
            self.plugins.len(),
            self.plugins.iter().filter(|p| p.available).count(),
            self.plugins.iter().filter(|p| !p.available).count()
        );

        context.push_str("Available Workflows:\n");
        for workflow in &self.workflows {
            context.push_str(&format!(
                "- {}: {} (states: {:?})\n",
                workflow.name, workflow.description, workflow.final_states
            ));
        }

        context.push_str("\nAvailable Plugins:\n");
        for plugin in &self.plugins {
            if plugin.available {
                context.push_str(&format!(
                    "- {}: {} (resources: {})\n",
                    plugin.name,
                    plugin.description,
                    plugin.managed_resources.join(", ")
                ));
            }
        }

        context
    }
}

impl Default for WorkflowPluginIntrospection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_introspection_creation() {
        let introspection = WorkflowPluginIntrospection::new();
        assert!(!introspection.workflows.is_empty());
        assert!(!introspection.plugins.is_empty());
    }

    #[test]
    fn test_chat_context_generation() {
        let introspection = WorkflowPluginIntrospection::new();
        let context = introspection.to_chat_context();
        assert!(context.contains("workflows"));
        assert!(context.contains("plugins"));
    }
}
