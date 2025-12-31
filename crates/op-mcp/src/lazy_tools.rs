//! Lazy Tool Loading Bridge for MCP Server
//!
//! This module bridges the MCP server with the lazy tool loading system,
//! providing:
//! - On-demand tool loading
//! - Context-based tool filtering
//! - LRU caching integration
//! - Discovery system integration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

use op_tools::{
    builtin::{create_networkmanager_tools, create_ovs_tools, create_systemd_tools},
    discovery::{
        AgentDiscoverySource, BuiltinToolSource, DbusDiscoverySource, DiscoveryStats,
        PluginDiscoverySource, ToolDiscoverySource, ToolDiscoverySystem,
    },
    registry::{LruConfig, RegistryStats, ToolDefinition, ToolRegistry},
    tool::Tool,
    BoxedTool,
};

/// Configuration for lazy tool loading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazyToolConfig {
    /// Maximum tools to keep loaded
    pub max_loaded_tools: usize,
    /// Minimum idle time before eviction (seconds)
    pub min_idle_secs: u64,
    /// Enable D-Bus runtime discovery
    pub enable_dbus_discovery: bool,
    /// Enable plugin discovery
    pub enable_plugin_discovery: bool,
    /// Enable agent discovery
    pub enable_agent_discovery: bool,
    /// Preload essential tools
    pub preload_essential: bool,
}

impl Default for LazyToolConfig {
    fn default() -> Self {
        Self {
            max_loaded_tools: 50,
            min_idle_secs: 300,
            enable_dbus_discovery: true,
            enable_plugin_discovery: true,
            enable_agent_discovery: true,
            preload_essential: true,
        }
    }
}

/// Lazy tool manager for MCP server
pub struct LazyToolManager {
    registry: Arc<ToolRegistry>,
    discovery: Arc<ToolDiscoverySystem>,
    config: LazyToolConfig,
}

impl LazyToolManager {
    /// Create a new lazy tool manager with default config
    pub async fn new() -> Result<Self> {
        Self::with_config(LazyToolConfig::default()).await
    }

    /// Create a new lazy tool manager with custom config
    pub async fn with_config(config: LazyToolConfig) -> Result<Self> {
        // Create registry with LRU caching
        let lru_config = LruConfig {
            max_loaded_tools: config.max_loaded_tools,
            min_idle_time: std::time::Duration::from_secs(config.min_idle_secs),
            hot_threshold: 10,
            eviction_check_interval: 10,
        };
        let registry = Arc::new(ToolRegistry::with_config(lru_config));

        // Create discovery system
        let discovery = Arc::new(ToolDiscoverySystem::new());

        let manager = Self {
            registry,
            discovery,
            config,
        };

        // Initialize discovery sources
        manager.initialize_discovery().await?;

        // Preload essential tools if configured
        if manager.config.preload_essential {
            manager.preload_essential_tools().await?;
        }

        Ok(manager)
    }

    /// Initialize discovery sources based on config
    async fn initialize_discovery(&self) -> Result<()> {
        // Always register built-in tools
        let builtin_definitions = self.collect_builtin_definitions();
        self.discovery
            .register_source(Arc::new(BuiltinToolSource::new(builtin_definitions)))
            .await;
        info!("Registered built-in tool discovery source");

        // D-Bus discovery (runtime introspection)
        if self.config.enable_dbus_discovery {
            let dbus_source = DbusDiscoverySource::system();
            self.discovery.register_source(Arc::new(dbus_source)).await;
            info!("Registered D-Bus discovery source");
        }

        // Plugin discovery
        if self.config.enable_plugin_discovery {
            let plugin_source = PluginDiscoverySource::default();
            self.discovery
                .register_source(Arc::new(plugin_source))
                .await;
            info!("Registered plugin discovery source");
        }

        // Agent discovery
        if self.config.enable_agent_discovery {
            let agent_source = AgentDiscoverySource::default();
            self.discovery
                .register_source(Arc::new(agent_source))
                .await;
            info!("Registered agent discovery source");
        }

        // Initial cache population
        self.discovery.start_background_refresh().await;

        Ok(())
    }

    /// Collect definitions from built-in tools
    fn collect_builtin_definitions(&self) -> Vec<ToolDefinition> {
        let mut definitions = Vec::new();

        // Systemd tools
        for tool in create_systemd_tools() {
            definitions.push(ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
                category: "dbus".to_string(),
                tags: vec!["dbus".to_string(), "systemd".to_string()],
            });
        }

        // NetworkManager tools
        for tool in create_networkmanager_tools() {
            definitions.push(ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
                category: "dbus".to_string(),
                tags: vec!["dbus".to_string(), "networkmanager".to_string()],
            });
        }

        // OVS tools (via OVSDB JSON-RPC, NOT CLI)
        for tool in create_ovs_tools() {
            definitions.push(ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
                category: "ovs".to_string(),
                tags: vec!["ovs".to_string(), "networking".to_string()],
            });
        }

        definitions
    }

    /// Preload essential tools
    async fn preload_essential_tools(&self) -> Result<()> {
        info!("Preloading essential tools...");

        // Preload systemd tools (most commonly used)
        for tool in create_systemd_tools() {
            let name: Arc<str> = Arc::from(tool.name());
            let definition = ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
                category: "dbus".to_string(),
                tags: vec!["essential".to_string()],
            };

            if let Err(e) = self.registry.register(name.clone(), tool, definition).await {
                warn!("Failed to preload tool {}: {}", name, e);
            } else {
                debug!("Preloaded: {}", name);
            }
        }

        // Preload OVS tools (critical for networking operations)
        for tool in create_ovs_tools() {
            let name: Arc<str> = Arc::from(tool.name());
            let definition = ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
                category: "ovs".to_string(),
                tags: vec!["essential".to_string(), "ovs".to_string()],
            };

            if let Err(e) = self.registry.register(name.clone(), tool, definition).await {
                warn!("Failed to preload OVS tool {}: {}", name, e);
            } else {
                debug!("Preloaded OVS: {}", name);
            }
        }

        Ok(())
    }

    /// Get a tool by name, loading it if necessary
    pub async fn get_tool(&self, name: &str) -> Option<BoxedTool> {
        // First try the registry (already loaded)
        if let Some(tool) = self.registry.get(name).await {
            return Some(tool);
        }

        // Check if tool exists in discovery
        let definition = self.discovery.get_tool_definition(name).await?;

        // Load based on category
        match definition.category.as_str() {
            "dbus" => self.load_dbus_tool(name).await,
            "ovs" => self.load_ovs_tool(name).await,
            "agent" => self.load_agent_tool(name, &definition).await,
            "plugin" => self.load_plugin_tool(name, &definition).await,
            _ => {
                warn!("Unknown tool category: {}", definition.category);
                None
            }
        }
    }

    /// Load a D-Bus tool on demand
    async fn load_dbus_tool(&self, name: &str) -> Option<BoxedTool> {
        // Find in predefined sets
        for tool in create_systemd_tools()
            .into_iter()
            .chain(create_networkmanager_tools())
        {
            if tool.name() == name {
                let arc_name: Arc<str> = Arc::from(name);
                let definition = ToolDefinition {
                    name: name.to_string(),
                    description: tool.description().to_string(),
                    input_schema: tool.input_schema(),
                    category: "dbus".to_string(),
                    tags: vec![],
                };

                if self
                    .registry
                    .register(arc_name, tool.clone(), definition)
                    .await
                    .is_ok()
                {
                    debug!("Loaded D-Bus tool: {}", name);
                    return Some(tool);
                }
            }
        }
        None
    }

    /// Load an OVS tool on demand (via OVSDB JSON-RPC)
    async fn load_ovs_tool(&self, name: &str) -> Option<BoxedTool> {
        for tool in create_ovs_tools() {
            if tool.name() == name {
                let arc_name: Arc<str> = Arc::from(name);
                let definition = ToolDefinition {
                    name: name.to_string(),
                    description: tool.description().to_string(),
                    input_schema: tool.input_schema(),
                    category: "ovs".to_string(),
                    tags: vec!["ovs".to_string(), "networking".to_string()],
                };

                if self
                    .registry
                    .register(arc_name, tool.clone(), definition)
                    .await
                    .is_ok()
                {
                    debug!("Loaded OVS tool: {}", name);
                    return Some(tool);
                }
            }
        }
        None
    }

    /// Load an agent tool on demand
    async fn load_agent_tool(&self, name: &str, _definition: &ToolDefinition) -> Option<BoxedTool> {
        // Agent tools would be created via op-agents
        // For now, return None (would integrate with AgentToolFactory)
        debug!("Agent tool loading not yet implemented: {}", name);
        None
    }

    /// Load a plugin tool on demand
    async fn load_plugin_tool(
        &self,
        name: &str,
        _definition: &ToolDefinition,
    ) -> Option<BoxedTool> {
        // Plugin tools would be created via op-state plugins
        // For now, return None (would integrate with PluginStateToolFactory)
        debug!("Plugin tool loading not yet implemented: {}", name);
        None
    }

    /// List all available tools (from discovery)
    pub async fn list_all_tools(&self) -> Vec<ToolDefinition> {
        self.discovery
            .get_all_tool_definitions()
            .await
            .unwrap_or_default()
    }

    /// List currently loaded tools
    pub async fn list_loaded_tools(&self) -> Vec<ToolDefinition> {
        self.registry.list_loaded().await
    }

    /// Search for tools
    pub async fn search_tools(
        &self,
        query: &str,
        category: Option<&str>,
        tags: Option<&[String]>,
    ) -> Vec<ToolDefinition> {
        self.discovery.search_tools(query, category, tags).await
    }

    /// Get tools relevant to a context
    pub async fn get_context_relevant_tools(&self, context: &str) -> Vec<ToolDefinition> {
        // Simple context-based filtering
        let context_lower = context.to_lowercase();

        let all_tools = self.list_all_tools().await;

        all_tools
            .into_iter()
            .filter(|t| {
                // Match against name, description, or tags
                t.name.to_lowercase().contains(&context_lower)
                    || t.description.to_lowercase().contains(&context_lower)
                    || t.tags.iter().any(|tag| tag.to_lowercase().contains(&context_lower))
            })
            .take(25) // LLM context limit
            .collect()
    }

    /// Get statistics
    pub async fn stats(&self) -> (RegistryStats, DiscoveryStats) {
        let registry_stats = self.registry.stats().await;
        let discovery_stats = self.discovery.stats().await;
        (registry_stats, discovery_stats)
    }

    /// Force refresh discovery cache
    pub async fn refresh_discovery(&self) -> Result<()> {
        self.discovery.force_refresh().await
    }

    /// Get the underlying registry
    pub fn registry(&self) -> Arc<ToolRegistry> {
        Arc::clone(&self.registry)
    }

    /// Get the underlying discovery system
    pub fn discovery(&self) -> Arc<ToolDiscoverySystem> {
        Arc::clone(&self.discovery)
    }
}

/// MCP tool list response with pagination
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolListResponse {
    pub tools: Vec<McpToolInfo>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
}

/// MCP tool info
#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

impl From<ToolDefinition> for McpToolInfo {
    fn from(def: ToolDefinition) -> Self {
        Self {
            name: def.name,
            description: def.description,
            input_schema: def.input_schema,
        }
    }
}

/// Get paginated tool list for MCP
pub async fn get_mcp_tool_list(
    manager: &LazyToolManager,
    offset: usize,
    limit: usize,
    context: Option<&str>,
) -> ToolListResponse {
    let tools = if let Some(ctx) = context {
        manager.get_context_relevant_tools(ctx).await
    } else {
        manager.list_all_tools().await
    };

    let total = tools.len();
    let paginated: Vec<McpToolInfo> = tools
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(McpToolInfo::from)
        .collect();

    let has_more = offset + paginated.len() < total;

    ToolListResponse {
        tools: paginated,
        total,
        offset,
        limit,
        has_more,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lazy_tool_manager_creation() {
        let config = LazyToolConfig {
            enable_dbus_discovery: false, // Disable for test
            enable_plugin_discovery: false,
            enable_agent_discovery: false,
            preload_essential: true,
            ..Default::default()
        };

        let manager = LazyToolManager::with_config(config).await.unwrap();

        // Should have preloaded tools
        let loaded = manager.list_loaded_tools().await;
        assert!(!loaded.is_empty());
    }

    #[tokio::test]
    async fn test_tool_listing() {
        let config = LazyToolConfig {
            enable_dbus_discovery: false,
            enable_plugin_discovery: false,
            enable_agent_discovery: false,
            preload_essential: true,
            ..Default::default()
        };

        let manager = LazyToolManager::with_config(config).await.unwrap();

        let all_tools = manager.list_all_tools().await;
        assert!(!all_tools.is_empty());
    }

    #[tokio::test]
    async fn test_context_filtering() {
        let config = LazyToolConfig {
            enable_dbus_discovery: false,
            enable_plugin_discovery: false,
            enable_agent_discovery: false,
            preload_essential: true,
            ..Default::default()
        };

        let manager = LazyToolManager::with_config(config).await.unwrap();

        let systemd_tools = manager.get_context_relevant_tools("systemd").await;
        assert!(systemd_tools.iter().any(|t| t.name.contains("systemd")));
    }
}
