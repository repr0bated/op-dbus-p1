//! Plugin registry with BTRFS cache and blockchain integration

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::auto_create::AutoPluginFactory;
use crate::plugin::{BoxedPlugin, PluginContext, PluginMetadata};
use crate::state::{DesiredState, StateChange};
use op_blockchain::{PluginFootprint, StreamingBlockchain};

/// Registered plugin with metadata
pub struct RegisteredPlugin {
    pub plugin: Arc<RwLock<BoxedPlugin>>,
    pub metadata: PluginMetadata,
    pub storage_path: PathBuf,
    pub enabled: bool,
    pub change_count: u64,
}

/// Plugin lifecycle event
#[derive(Debug, Clone)]
pub enum PluginEvent {
    PreRegister {
        name: String,
    },
    PostRegister {
        name: String,
    },
    PreUnregister {
        name: String,
    },
    PostUnregister {
        name: String,
    },
    StateChanged {
        plugin: String,
        changes: Vec<StateChange>,
    },
    Error {
        plugin: String,
        error: String,
    },
}

/// Hook handler type
pub type HookHandler = Arc<dyn Fn(&PluginEvent) -> Result<()> + Send + Sync>;

/// Plugin registry - manages all plugins with BTRFS storage and blockchain audit
pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<String, RegisteredPlugin>>>,
    base_path: PathBuf,
    blockchain: Option<Arc<RwLock<StreamingBlockchain>>>,
    hooks: Arc<RwLock<Vec<HookHandler>>>,
    auto_factory: AutoPluginFactory,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            base_path: base_path.as_ref().to_path_buf(),
            blockchain: None,
            hooks: Arc::new(RwLock::new(Vec::new())),
            auto_factory: AutoPluginFactory::new(),
        }
    }

    /// Create with blockchain integration
    pub fn with_blockchain(mut self, blockchain: Arc<RwLock<StreamingBlockchain>>) -> Self {
        self.blockchain = Some(blockchain);
        self
    }

    /// Register a hook for plugin events
    pub async fn register_hook(&self, handler: HookHandler) {
        let mut hooks = self.hooks.write().await;
        hooks.push(handler);
    }

    /// Emit an event to all hooks
    async fn emit_event(&self, event: PluginEvent) {
        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            if let Err(e) = hook(&event) {
                warn!("Hook error: {}", e);
            }
        }
    }

    /// Create BTRFS subvolume for plugin storage
    async fn create_plugin_subvolume(&self, name: &str) -> Result<PathBuf> {
        let storage_path = self.base_path.join("plugins").join(name);

        // Ensure parent exists
        if let Some(parent) = storage_path.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }

        // Try to create BTRFS subvolume
        let output = Command::new("btrfs")
            .args(["subvolume", "create"])
            .arg(&storage_path)
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => {
                info!(
                    "Created BTRFS subvolume for plugin '{}' at {:?}",
                    name, storage_path
                );
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("not a btrfs")
                    || stderr.contains("command not found")
                    || storage_path.exists()
                {
                    debug!("Using regular directory for plugin '{}'", name);
                    tokio::fs::create_dir_all(&storage_path).await?;
                } else if !storage_path.exists() {
                    tokio::fs::create_dir_all(&storage_path).await?;
                }
            }
            Err(_) => {
                tokio::fs::create_dir_all(&storage_path).await?;
            }
        }

        Ok(storage_path)
    }

    /// Record a footprint in the blockchain
    async fn record_footprint(&self, plugin_name: &str, operation: &str, data: &serde_json::Value) {
        if let Some(ref blockchain) = self.blockchain {
            let footprint = PluginFootprint::new(plugin_name, operation, data);
            let bc = blockchain.write().await;
            if let Err(e) = bc.add_footprint(footprint).await {
                warn!("Failed to record blockchain footprint: {}", e);
            }
        }
    }

    /// Register a new plugin
    pub async fn register(&self, mut plugin: BoxedPlugin) -> Result<()> {
        let name = plugin.name().to_string();

        self.emit_event(PluginEvent::PreRegister { name: name.clone() })
            .await;

        {
            let plugins = self.plugins.read().await;
            if plugins.contains_key(&name) {
                return Err(anyhow::anyhow!("Plugin '{}' is already registered", name));
            }
        }

        let storage_path = self.create_plugin_subvolume(&name).await?;

        let context = PluginContext {
            storage_path: storage_path.clone(),
            numa_node: None,
            config: serde_json::json!({}),
        };

        plugin
            .initialize(context)
            .await
            .context(format!("Failed to initialize plugin '{}'", name))?;

        let metadata = plugin.metadata();

        self.record_footprint(
            &name,
            "register",
            &serde_json::json!({
                "version": metadata.version,
                "description": metadata.description,
            }),
        )
        .await;

        {
            let mut plugins = self.plugins.write().await;
            plugins.insert(
                name.clone(),
                RegisteredPlugin {
                    plugin: Arc::new(RwLock::new(plugin)),
                    metadata,
                    storage_path,
                    enabled: true,
                    change_count: 0,
                },
            );
        }

        info!("Registered plugin: {}", name);
        self.emit_event(PluginEvent::PostRegister { name }).await;

        Ok(())
    }

    /// Unregister a plugin
    pub async fn unregister(&self, name: &str) -> Result<()> {
        self.emit_event(PluginEvent::PreUnregister {
            name: name.to_string(),
        })
        .await;

        let plugin = {
            let mut plugins = self.plugins.write().await;
            plugins
                .remove(name)
                .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", name))?
        };

        {
            let mut p = plugin.plugin.write().await;
            p.cleanup().await.ok();
        }

        self.record_footprint(
            name,
            "unregister",
            &serde_json::json!({"reason": "user_request"}),
        )
        .await;

        info!("Unregistered plugin: {}", name);
        self.emit_event(PluginEvent::PostUnregister {
            name: name.to_string(),
        })
        .await;

        Ok(())
    }

    /// Get a plugin by name
    pub async fn get(&self, name: &str) -> Option<Arc<RwLock<BoxedPlugin>>> {
        let plugins = self.plugins.read().await;
        plugins
            .get(name)
            .filter(|p| p.enabled)
            .map(|p| Arc::clone(&p.plugin))
    }

    /// Get or create a plugin (auto-creation if missing)
    pub async fn get_or_create(&self, name: &str) -> Result<Arc<RwLock<BoxedPlugin>>> {
        if let Some(plugin) = self.get(name).await {
            return Ok(plugin);
        }

        info!("Plugin '{}' not found, attempting auto-creation", name);
        let plugin = self.auto_factory.create_plugin(name).await?;
        self.register(plugin).await?;

        self.get(name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Failed to get auto-created plugin"))
    }

    /// List all registered plugins
    pub async fn list(&self) -> Vec<String> {
        let plugins = self.plugins.read().await;
        plugins.keys().cloned().collect()
    }

    /// Get all plugin metadata
    pub async fn list_metadata(&self) -> Vec<PluginMetadata> {
        let plugins = self.plugins.read().await;
        plugins.values().map(|p| p.metadata.clone()).collect()
    }

    /// Apply desired state to a plugin and record changes
    pub async fn apply_plugin_state(&self, name: &str) -> Result<Vec<StateChange>> {
        let plugin = self
            .get(name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", name))?;

        let changes = {
            let p = plugin.read().await;
            p.apply_state().await?
        };

        // Record each change in blockchain
        for change in &changes {
            self.record_footprint(
                name,
                &format!("{:?}", change.operation),
                &serde_json::json!({
                    "path": change.path,
                    "hash": change.hash,
                    "description": change.description,
                }),
            )
            .await;
        }

        // Update change count
        {
            let mut plugins = self.plugins.write().await;
            if let Some(registered) = plugins.get_mut(name) {
                registered.change_count += changes.len() as u64;
            }
        }

        // Emit event
        self.emit_event(PluginEvent::StateChanged {
            plugin: name.to_string(),
            changes: changes.clone(),
        })
        .await;

        Ok(changes)
    }

    /// Get diff for a plugin
    pub async fn diff_plugin(&self, name: &str) -> Result<Vec<StateChange>> {
        let plugin = self
            .get(name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", name))?;

        let p = plugin.read().await;
        p.diff().await
    }

    /// Set desired state for a plugin
    pub async fn set_desired_state(&self, name: &str, state: DesiredState) -> Result<()> {
        let plugin = self
            .get(name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", name))?;

        let p = plugin.read().await;
        p.set_desired_state(state).await?;

        self.record_footprint(
            name,
            "set_desired_state",
            &serde_json::json!({
                "hash": p.state_hash(),
            }),
        )
        .await;

        Ok(())
    }

    /// Enable a plugin
    pub async fn enable(&self, name: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        if let Some(p) = plugins.get_mut(name) {
            p.enabled = true;
            info!("Enabled plugin: {}", name);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Plugin '{}' not found", name))
        }
    }

    /// Disable a plugin
    pub async fn disable(&self, name: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        if let Some(p) = plugins.get_mut(name) {
            p.enabled = false;
            info!("Disabled plugin: {}", name);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Plugin '{}' not found", name))
        }
    }

    /// Get plugin count
    pub async fn count(&self) -> usize {
        let plugins = self.plugins.read().await;
        plugins.len()
    }
}

impl Clone for PluginRegistry {
    fn clone(&self) -> Self {
        Self {
            plugins: Arc::clone(&self.plugins),
            base_path: self.base_path.clone(),
            blockchain: self.blockchain.clone(),
            hooks: Arc::clone(&self.hooks),
            auto_factory: self.auto_factory.clone(),
        }
    }
}
