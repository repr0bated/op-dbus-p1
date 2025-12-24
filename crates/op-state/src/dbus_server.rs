//! D-Bus server for system bus integration

use crate::manager::StateManager;
use crate::plugin::{StateAction, StateDiff};
use anyhow::Result;
use std::sync::Arc;
use zbus::{connection::Builder, interface};

/// D-Bus interface for the state manager
pub struct StateManagerDBus {
    state_manager: Arc<StateManager>,
}

#[interface(name = "org.opdbus.StateManager")]
impl StateManagerDBus {
    /// Apply state from JSON string
    async fn apply_state(&self, state_json: String) -> zbus::fdo::Result<String> {
        match serde_json::from_str(&state_json) {
            Ok(desired_state) => match self.state_manager.apply_state(desired_state).await {
                Ok(report) => Ok(format!("Applied successfully: {}", report.success)),
                Err(e) => Err(zbus::fdo::Error::Failed(format!("Apply failed: {}", e))),
            },
            Err(e) => Err(zbus::fdo::Error::InvalidArgs(format!(
                "Invalid JSON: {}",
                e
            ))),
        }
    }

    /// Query current state
    async fn query_state(&self) -> zbus::fdo::Result<String> {
        match self.state_manager.query_current_state().await {
            Ok(state) => match serde_json::to_string(&state) {
                Ok(json) => Ok(json),
                Err(e) => Err(zbus::fdo::Error::Failed(format!(
                    "Serialization failed: {}",
                    e
                ))),
            },
            Err(e) => Err(zbus::fdo::Error::Failed(format!("Query failed: {}", e))),
        }
    }

    /// Restore OpenFlow flows from state file (used after OVS restart)
    ///
    /// # Arguments
    /// * `state_file_path` - Optional path to state file (default: /etc/op-dbus/state.json)
    /// * `bridge_name` - Optional bridge filter (empty string = all bridges)
    ///
    /// # Returns
    /// Success message with count of restored flows
    async fn restore_flows(
        &self,
        state_file_path: String,
        bridge_name: String,
    ) -> zbus::fdo::Result<String> {
        use std::path::PathBuf;

        // Handle default state file path
        let state_path = if state_file_path.is_empty() {
            PathBuf::from("/etc/op-dbus/state.json")
        } else {
            PathBuf::from(state_file_path)
        };

        // Check if state file exists
        if !state_path.exists() {
            return Err(zbus::fdo::Error::Failed(format!(
                "State file not found: {}",
                state_path.display()
            )));
        }

        // Load desired state
        let desired_state = match self.state_manager.load_desired_state(&state_path).await {
            Ok(state) => state,
            Err(e) => {
                return Err(zbus::fdo::Error::Failed(format!(
                    "Failed to load state file: {}",
                    e
                )))
            }
        };

        // Check if openflow plugin state exists
        let openflow_state = match desired_state.plugins.get("openflow") {
            Some(state) => state,
            None => {
                return Err(zbus::fdo::Error::Failed(
                    "No 'openflow' plugin configuration in state file".to_string(),
                ))
            }
        };

        // Get the openflow plugin
        let openflow_plugin = match self.state_manager.get_plugin("openflow").await {
            Some(plugin) => plugin,
            None => {
                return Err(zbus::fdo::Error::Failed(
                    "OpenFlow plugin not registered".to_string(),
                ))
            }
        };

        // Query current state
        let current_state = match openflow_plugin.query_current_state().await {
            Ok(state) => state,
            Err(e) => {
                return Err(zbus::fdo::Error::Failed(format!(
                    "Failed to query current state: {}",
                    e
                )))
            }
        };

        // Calculate diff
        let diff = match openflow_plugin
            .calculate_diff(&current_state, openflow_state)
            .await
        {
            Ok(diff) => diff,
            Err(e) => {
                return Err(zbus::fdo::Error::Failed(format!(
                    "Failed to calculate diff: {}",
                    e
                )))
            }
        };

        // Filter for flow-only actions
        let flow_actions: Vec<StateAction> = diff
            .actions
            .iter()
            .filter(|action| match action {
                StateAction::Create { resource, .. } => {
                    resource.contains("flow/") || resource.contains("flows")
                }
                _ => false,
            })
            .cloned()
            .collect();

        if flow_actions.is_empty() {
            return Ok("No flows need to be restored".to_string());
        }

        // Filter by bridge if specified
        let filtered_actions: Vec<StateAction> = if !bridge_name.is_empty() {
            flow_actions
                .into_iter()
                .filter(|action| {
                    if let StateAction::Create { resource, .. } = action {
                        resource.contains(&bridge_name)
                    } else {
                        false
                    }
                })
                .collect()
        } else {
            flow_actions
        };

        if filtered_actions.is_empty() {
            return Ok(format!("No flows to restore for bridge: {}", bridge_name));
        }

        // Create filtered diff
        let flow_count = filtered_actions.len();
        let filtered_diff = StateDiff {
            plugin: diff.plugin.clone(),
            actions: filtered_actions.clone(),
            metadata: diff.metadata.clone(),
        };

        // Apply the restoration
        match openflow_plugin.apply_state(&filtered_diff).await {
            Ok(_) => Ok(format!("Successfully restored {} flows", flow_count)),
            Err(e) => Err(zbus::fdo::Error::Failed(format!(
                "Failed to restore flows: {}",
                e
            ))),
        }
    }
}

/// Start the system bus D-Bus service
pub async fn start_system_bus(state_manager: Arc<StateManager>) -> Result<()> {
    let interface = StateManagerDBus { state_manager };

    let _connection = Builder::system()?
        .name("org.opdbus")?
        .serve_at("/org/opdbus/state", interface)?
        .build()
        .await?;

    // Keep the connection alive
    std::future::pending::<()>().await;

    Ok(())
}
