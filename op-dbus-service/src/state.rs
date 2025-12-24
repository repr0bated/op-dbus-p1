use zbus::interface;
use std::sync::Arc;
use op_state::StateManager;
use op_state::manager::DesiredState;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct StateInterface {
    manager: Arc<StateManager>,
}

impl StateInterface {
    pub fn new(manager: Arc<StateManager>) -> Self {
        Self { manager }
    }
}

#[interface(name = "org.op_dbus.State")]
impl StateInterface {
    /// Get the state of a specific plugin as a JSON string
    async fn get_state(&self, plugin_name: String) -> zbus::fdo::Result<String> {
        match self.manager.query_plugin_state(&plugin_name).await {
            Ok(state) => Ok(state.to_string()),
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// Get the state of all plugins as a JSON string
    async fn get_all_state(&self) -> zbus::fdo::Result<String> {
        match self.manager.query_current_state().await {
            Ok(state) => Ok(serde_json::to_string(&state).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?),
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// Set the state of a specific plugin using a JSON string value
    async fn set_state(&self, plugin_name: String, state_json: String) -> zbus::fdo::Result<String> {
        let value: Value = serde_json::from_str(&state_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("Invalid JSON: {}", e)))?;
        
        let mut plugins = HashMap::new();
        plugins.insert(plugin_name.clone(), value);
        
        let desired = DesiredState {
            version: 1, 
            plugins,
        };

        match self.manager.apply_state_single_plugin(desired, &plugin_name).await {
            Ok(report) => Ok(serde_json::to_string(&report).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?),
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// Set the state of the whole system (multiple plugins) using a DesiredState JSON string
    async fn set_all_state(&self, state_json: String) -> zbus::fdo::Result<String> {
        let desired: DesiredState = serde_json::from_str(&state_json)
             .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("Invalid JSON (expected DesiredState structure): {}", e)))?;

        match self.manager.apply_state(desired).await {
            Ok(report) => Ok(serde_json::to_string(&report).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?),
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// Apply state from a JSON file path
    async fn apply_from_file(&self, path: String) -> zbus::fdo::Result<String> {
        let path = PathBuf::from(path);
        match self.manager.load_desired_state(&path).await {
            Ok(desired) => {
                match self.manager.apply_state(desired).await {
                    Ok(report) => Ok(serde_json::to_string(&report).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?),
                    Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
                }
            }
            Err(e) => Err(zbus::fdo::Error::Failed(format!("Failed to load state file: {}", e))),
        }
    }

    /// Apply state for a specific plugin from a JSON file path
    async fn apply_plugin_from_file(&self, plugin_name: String, path: String) -> zbus::fdo::Result<String> {
        let path = PathBuf::from(path);
        match self.manager.load_desired_state(&path).await {
            Ok(desired) => {
                match self.manager.apply_state_single_plugin(desired, &plugin_name).await {
                    Ok(report) => Ok(serde_json::to_string(&report).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?),
                    Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
                }
            }
            Err(e) => Err(zbus::fdo::Error::Failed(format!("Failed to load state file: {}", e))),
        }
    }
}
