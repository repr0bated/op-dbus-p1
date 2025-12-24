//! Bridge between Plugin Registry and Tool Registry
//! Auto-creates plugins from introspection and exposes them as MCP tools

use crate::tool_registry::{DynamicToolBuilder, ToolContent, ToolRegistry, ToolResult};
use op_state::manager::StateManager;
use op_state::plugin::StatePlugin;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::sync::Arc;

/// Bridge that connects plugin registry to tool registry
pub struct PluginToolBridge {
    state_manager: Arc<StateManager>,
    tool_registry: Arc<ToolRegistry>,
}

impl PluginToolBridge {
    pub fn new(state_manager: Arc<StateManager>, tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            state_manager,
            tool_registry,
        }
    }

    /// Auto-create plugins from introspection and register them as tools
    pub async fn auto_discover_and_register(&self) -> Result<()> {
        log::info!("Auto-discovering plugins from D-Bus introspection...");

        // Discover and register auto-generated plugins
        #[cfg(feature = "mcp")]
        {
            self.state_manager
                .discover_and_register_auto_plugins()
                .await
                .context("Failed to discover auto plugins")?;
        }

        // Register all plugins as tools
        self.register_plugins_as_tools().await?;

        Ok(())
    }

    /// Register all plugins from plugin registry as tools in tool registry
    pub async fn register_plugins_as_tools(&self) -> Result<()> {
        log::info!("Registering plugins as MCP tools...");

        // Get all registered plugins
        let plugin_names = self.state_manager.list_plugin_names().await;
        let plugin_count = plugin_names.len();

        for plugin_name in plugin_names {
            if let Some(plugin) = self.state_manager.get_plugin(&plugin_name).await {
                self.register_plugin_as_tool(&plugin_name, plugin).await?;
            }
        }

        log::info!("Registered {} plugins as MCP tools", plugin_count);

        Ok(())
    }

    /// Register a single plugin as an MCP tool
    async fn register_plugin_as_tool(
        &self,
        plugin_name: &str,
        plugin: Arc<dyn StatePlugin>,
    ) -> Result<()> {
        let tool_name = format!("plugin_{}", plugin_name);

        // Create tool for querying plugin state
        let query_tool = DynamicToolBuilder::new(&format!("{}_query", tool_name))
            .description(format!("Query current state from {} plugin", plugin_name))
            .schema(json!({
                "type": "object",
                "properties": {},
                "required": []
            }))
            .handler({
                let plugin = plugin.clone();
                let plugin_name = plugin_name.to_string();
                move |_params| {
                    let plugin = plugin.clone();
                    let plugin_name = plugin_name.clone();
                    Box::pin(async move {
                        match plugin.query_current_state().await {
                            Ok(state) => Ok(ToolResult {
                                content: vec![ToolContent::json(state)],
                                metadata: Some(json!({
                                    "plugin": plugin_name,
                                    "operation": "query"
                                })),
                            }),
                            Err(e) => Ok(ToolResult {
                                content: vec![ToolContent::error(&format!(
                                    "Failed to query plugin state: {}",
                                    e
                                ))],
                                metadata: None,
                            }),
                        }
                    })
                }
            })
            .build();

        self.tool_registry
            .register_tool(Box::new(query_tool))
            .await?;

        // Create tool for calculating diff
        let diff_tool = DynamicToolBuilder::new(&format!("{}_diff", tool_name))
            .description(format!("Calculate state diff for {} plugin", plugin_name))
            .schema(json!({
                "type": "object",
                "properties": {
                    "desired_state": {
                        "type": "object",
                        "description": "Desired state configuration"
                    }
                },
                "required": ["desired_state"]
            }))
            .handler({
                let plugin = plugin.clone();
                let plugin_name = plugin_name.to_string();
                move |params| {
                    let plugin = plugin.clone();
                    let plugin_name = plugin_name.clone();
                    Box::pin(async move {
                        let desired = params
                            .get("desired_state")
                            .ok_or_else(|| anyhow::anyhow!("Missing desired_state"))?
                            .clone();

                        let current = plugin.query_current_state().await?;
                        let diff = plugin.calculate_diff(&current, &desired).await?;

                        Ok(ToolResult {
                            content: vec![ToolContent::json(json!({
                                "diff": diff,
                                "current": current,
                                "desired": desired
                            }))],
                            metadata: Some(json!({
                                "plugin": plugin_name,
                                "operation": "diff"
                            })),
                        })
                    })
                }
            })
            .build();

        self.tool_registry
            .register_tool(Box::new(diff_tool))
            .await?;

        // Create tool for applying state
        let apply_tool = DynamicToolBuilder::new(&format!("{}_apply", tool_name))
            .description(format!("Apply state changes for {} plugin", plugin_name))
            .schema(json!({
                "type": "object",
                "properties": {
                    "desired_state": {
                        "type": "object",
                        "description": "Desired state configuration"
                    }
                },
                "required": ["desired_state"]
            }))
            .handler({
                let plugin = plugin.clone();
                let plugin_name = plugin_name.to_string();
                move |params| {
                    let plugin = plugin.clone();
                    let plugin_name = plugin_name.clone();
                    Box::pin(async move {
                        let desired = params
                            .get("desired_state")
                            .ok_or_else(|| anyhow::anyhow!("Missing desired_state"))?
                            .clone();

                        let current = plugin.query_current_state().await?;
                        let diff = plugin.calculate_diff(&current, &desired).await?;
                        let result = plugin.apply_state(&diff).await?;

                        Ok(ToolResult {
                            content: vec![ToolContent::json(json!({
                                "result": result,
                                "diff": diff
                            }))],
                            metadata: Some(json!({
                                "plugin": plugin_name,
                                "operation": "apply"
                            })),
                        })
                    })
                }
            })
            .build();

        self.tool_registry
            .register_tool(Box::new(apply_tool))
            .await?;

        log::info!("Registered plugin '{}' as MCP tools", plugin_name);

        Ok(())
    }

    /// Trigger plugin creation when introspection discovers a new service
    /// This is called by the orchestrator when introspection finds something new
    pub async fn on_introspection_discovery(&self, service_name: &str) -> Result<()> {
        log::info!(
            "Introspection discovered service: {}, creating plugin...",
            service_name
        );

        // Check if plugin already exists
        let plugin_name = Self::service_to_plugin_name(service_name);
        if self.state_manager.get_plugin(&plugin_name).await.is_some() {
            log::debug!("Plugin '{}' already exists, skipping", plugin_name);
            return Ok(());
        }

        // Create auto-generated plugin for this service
        #[cfg(feature = "mcp")]
        match op_state::auto_plugin::AutoGeneratedPlugin::new(service_name.to_string()).await {
            Ok(plugin) => {
                let plugin_arc = Arc::new(plugin);

                // Register plugin
                self.state_manager.register_plugin(plugin_arc.clone()).await;

                // Register as tool
                self.register_plugin_as_tool(&plugin_name, plugin_arc)
                    .await?;

                log::info!(
                    "✓ Created plugin '{}' from introspection discovery",
                    plugin_name
                );
            }
            Err(e) => {
                log::warn!("Failed to create plugin for {}: {}", service_name, e);
            }
        }
        #[cfg(not(feature = "mcp"))]
        {
            log::warn!("Auto-generated plugins require 'mcp' feature");
        }

        Ok(())
    }

    fn service_to_plugin_name(service: &str) -> String {
        service.split('.').last().unwrap_or(service).to_lowercase()
    }
}

/// Event listener that triggers plugin creation from introspection
pub struct IntrospectionPluginListener {
    bridge: Arc<PluginToolBridge>,
}

impl IntrospectionPluginListener {
    pub fn new(bridge: Arc<PluginToolBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait::async_trait]
impl crate::chat::orchestrator::EventListener for IntrospectionPluginListener {
    async fn on_agent_spawned(&self, _agent_id: &str, _agent_type: &str) {
        // Not relevant for plugin creation
    }

    async fn on_agent_died(&self, _agent_id: &str, _reason: &str) {
        // Not relevant for plugin creation
    }

    async fn on_task_completed(&self, _task_id: &str, result: &Value) {
        // Check if this task was an introspection task that discovered a new service
        if let Some(service_name) = result.get("discovered_service").and_then(|v| v.as_str()) {
            if let Err(e) = self.bridge.on_introspection_discovery(service_name).await {
                log::error!("Failed to create plugin from introspection: {}", e);
            }
        }
    }

    async fn on_error(&self, context: &str, error: &str) {
        log::error!("Error in {}: {}", context, error);
    }
}
