//! Default plugin registry - auto-loads essential plugins
//!
//! This module defines which plugins are loaded by default when the system starts.
//! Plugins can be enabled/disabled via configuration.

use anyhow::Result;
use std::sync::Arc;
use op_state_store::StateStore;

use crate::state_plugins::{
    McpStatePlugin,
    NetStatePlugin,
    SystemdStatePlugin,
    OpenFlowPlugin,
    PrivacyRouterPlugin,
    NetmakerPlugin,
};

/// Plugin registry configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginRegistryConfig {
    /// Auto-load plugins on startup
    #[serde(default = "default_auto_load")]
    pub auto_load: Vec<String>,
    
    /// Plugin-specific configurations
    #[serde(default)]
    pub plugin_configs: std::collections::HashMap<String, serde_json::Value>,
}

fn default_auto_load() -> Vec<String> {
    vec![
        "mcp".to_string(),
        "systemd".to_string(),
        "net".to_string(),
        "openflow".to_string(),
    ]
}

impl Default for PluginRegistryConfig {
    fn default() -> Self {
        Self {
            auto_load: default_auto_load(),
            plugin_configs: std::collections::HashMap::new(),
        }
    }
}

/// Default plugin registry
pub struct DefaultPluginRegistry {
    config: PluginRegistryConfig,
    state_store: Arc<dyn StateStore>,
}

impl DefaultPluginRegistry {
    /// Create a new plugin registry
    pub fn new(state_store: Arc<dyn StateStore>) -> Self {
        Self {
            config: PluginRegistryConfig::default(),
            state_store,
        }
    }

    /// Create with custom configuration
    pub fn with_config(state_store: Arc<dyn StateStore>, config: PluginRegistryConfig) -> Self {
        Self {
            config,
            state_store,
        }
    }

    /// Load all auto-load plugins
    pub async fn load_default_plugins(&self) -> Result<Vec<Arc<dyn op_state::StatePlugin>>> {
        let mut plugins: Vec<Arc<dyn op_state::StatePlugin>> = Vec::new();

        for plugin_name in &self.config.auto_load {
            match self.load_plugin(plugin_name).await {
                Ok(plugin) => {
                    tracing::info!("✅ Loaded plugin: {}", plugin_name);
                    plugins.push(plugin);
                }
                Err(e) => {
                    tracing::warn!("⚠️ Failed to load plugin {}: {}", plugin_name, e);
                }
            }
        }

        tracing::info!("📦 Loaded {} plugins", plugins.len());
        Ok(plugins)
    }

    /// Load a specific plugin by name
    async fn load_plugin(&self, name: &str) -> Result<Arc<dyn op_state::StatePlugin>> {
        let plugin: Arc<dyn op_state::StatePlugin> = match name {
            "mcp" => {
                let config_path = self.get_plugin_config_path("mcp", "/etc/op-dbus/mcp-config.json");
                Arc::new(McpStatePlugin::new(self.state_store.clone(), config_path))
            }
            "systemd" => {
                Arc::new(SystemdStatePlugin::new())
            }
            "net" => {
                Arc::new(NetStatePlugin::new())
            }
            "openflow" => {
                Arc::new(OpenFlowPlugin::new())
            }
            "privacy_router" => {
                let config_path = self.get_plugin_config_path("privacy_router", "/etc/op-dbus/privacy-config.json");
                // In a real implementation, we would load from file. For now, use default.
                use crate::state_plugins::privacy_router::PrivacyRouterConfig;
                Arc::new(PrivacyRouterPlugin::new(PrivacyRouterConfig::default()))
            }
            "netmaker" => {
                use crate::state_plugins::netmaker::NetmakerConfig;
                Arc::new(NetmakerPlugin::new(NetmakerConfig::default()))
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown plugin: {}", name));
            }
        };

        Ok(plugin)
    }

    /// Get plugin-specific config value or default
    fn get_plugin_config_path(&self, plugin_name: &str, default: &str) -> String {
        self.config
            .plugin_configs
            .get(plugin_name)
            .and_then(|v| v.get("config_path"))
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    }

    /// Get list of available plugins
    pub fn available_plugins() -> Vec<&'static str> {
        vec![
            "mcp",
            "systemd",
            "net",
            "openflow",
            "privacy_router",
            "netmaker",
            "lxc",
            "packagekit",
        ]
    }

    /// Check if a plugin is enabled for auto-load
    pub fn is_auto_load(&self, plugin_name: &str) -> bool {
        self.config.auto_load.contains(&plugin_name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use op_state_store::SqliteStore;

    #[tokio::test]
    async fn test_default_plugin_registry() {
        let store = Arc::new(SqliteStore::new(":memory:").await.unwrap());
        let registry = DefaultPluginRegistry::new(store);

        // Check default auto-load plugins
        assert!(registry.is_auto_load("mcp"));
        assert!(registry.is_auto_load("systemd"));
        assert!(registry.is_auto_load("net"));
        assert!(registry.is_auto_load("openflow"));

        // Load plugins
        let plugins = registry.load_default_plugins().await.unwrap();
        assert!(!plugins.is_empty());
    }

    #[tokio::test]
    async fn test_custom_config() {
        let store = Arc::new(SqliteStore::new(":memory:").await.unwrap());
        
        let config = PluginRegistryConfig {
            auto_load: vec!["systemd".to_string()],
            plugin_configs: std::collections::HashMap::new(),
        };

        let registry = DefaultPluginRegistry::with_config(store, config);
        
        assert!(registry.is_auto_load("systemd"));
        assert!(!registry.is_auto_load("mcp"));
    }
}
