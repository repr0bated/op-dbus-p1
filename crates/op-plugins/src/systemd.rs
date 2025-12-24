//! Systemd plugin for service management
//!
//! This plugin manages systemd services using systemctl.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

/// Systemd plugin for service management
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemdPlugin {
    /// Services to manage (if empty, checks common services)
    #[serde(default)]
    pub services: Vec<String>,
}

/// Service state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    pub name: String,
    pub active_state: String,
    pub sub_state: String,
    pub load_state: String,
}

/// Desired service state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesiredServiceState {
    pub name: String,
    pub active_state: Option<String>,
    pub enabled: Option<bool>,
}

impl SystemdPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get current state of services
    pub async fn get_state(&self) -> Result<Value> {
        let services_to_check = if self.services.is_empty() {
            // Default to checking common services
            vec!["dbus", "NetworkManager", "sshd", "systemd-resolved"]
        } else {
            self.services.iter().map(|s| s.as_str()).collect()
        };

        let mut states = Vec::new();

        for service in services_to_check {
            let output = tokio::process::Command::new("systemctl")
                .arg("show")
                .arg(service)
                .arg("--property=ActiveState,SubState,LoadState")
                .output()
                .await?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut active = "unknown".to_string();
                let mut sub = "unknown".to_string();
                let mut load = "unknown".to_string();

                for line in stdout.lines() {
                    if let Some((key, value)) = line.split_once('=') {
                        match key {
                            "ActiveState" => active = value.to_string(),
                            "SubState" => sub = value.to_string(),
                            "LoadState" => load = value.to_string(),
                            _ => {}
                        }
                    }
                }

                states.push(ServiceState {
                    name: service.to_string(),
                    active_state: active,
                    sub_state: sub,
                    load_state: load,
                });
            }
        }

        Ok(json!({
            "services": states
        }))
    }

    /// Apply desired state
    pub async fn apply_state(&self, desired: Value) -> Result<()> {
        let desired_obj = desired.as_object().context("Desired state must be an object")?;

        if let Some(services) = desired_obj.get("services").and_then(|v| v.as_array()) {
            for service_val in services {
                let name = service_val.get("name").and_then(|v| v.as_str())
                    .context("Service name missing")?;
                let desired_active = service_val.get("active_state").and_then(|v| v.as_str());
                let desired_enabled = service_val.get("enabled").and_then(|v| v.as_bool());

                // Handle active state
                if let Some(state) = desired_active {
                    match state {
                        "active" => {
                            self.manage_service(name, "start").await?;
                        }
                        "inactive" => {
                            self.manage_service(name, "stop").await?;
                        }
                        "restarting" | "reloading" => {
                            self.manage_service(name, "restart").await?;
                        }
                        _ => {}
                    }
                }

                // Handle enabled state
                if let Some(enabled) = desired_enabled {
                    if enabled {
                        self.manage_service(name, "enable").await?;
                    } else {
                        self.manage_service(name, "disable").await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Start a service
    pub async fn start_service(&self, name: &str) -> Result<()> {
        self.manage_service(name, "start").await
    }

    /// Stop a service
    pub async fn stop_service(&self, name: &str) -> Result<()> {
        self.manage_service(name, "stop").await
    }

    /// Restart a service
    pub async fn restart_service(&self, name: &str) -> Result<()> {
        self.manage_service(name, "restart").await
    }

    /// Reload a service
    pub async fn reload_service(&self, name: &str) -> Result<()> {
        self.manage_service(name, "reload").await
    }

    /// Enable a service
    pub async fn enable_service(&self, name: &str) -> Result<()> {
        self.manage_service(name, "enable").await
    }

    /// Disable a service
    pub async fn disable_service(&self, name: &str) -> Result<()> {
        self.manage_service(name, "disable").await
    }

    /// Get status of a specific service
    pub async fn get_service_status(&self, name: &str) -> Result<ServiceState> {
        let output = tokio::process::Command::new("systemctl")
            .arg("show")
            .arg(name)
            .arg("--property=ActiveState,SubState,LoadState")
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to get status for service {}", name));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut active = "unknown".to_string();
        let mut sub = "unknown".to_string();
        let mut load = "unknown".to_string();

        for line in stdout.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "ActiveState" => active = value.to_string(),
                    "SubState" => sub = value.to_string(),
                    "LoadState" => load = value.to_string(),
                    _ => {}
                }
            }
        }

        Ok(ServiceState {
            name: name.to_string(),
            active_state: active,
            sub_state: sub,
            load_state: load,
        })
    }

    /// List all services
    pub async fn list_services(&self) -> Result<Vec<String>> {
        let output = tokio::process::Command::new("systemctl")
            .arg("list-units")
            .arg("--type=service")
            .arg("--no-pager")
            .arg("--plain")
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to list services"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let services: Vec<String> = stdout
            .lines()
            .skip(1) // Skip header
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if !parts.is_empty() {
                    Some(parts[0].to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok(services)
    }

    async fn manage_service(&self, name: &str, action: &str) -> Result<()> {
        info!("Systemd: {} {}", action, name);

        let output = tokio::process::Command::new("systemctl")
            .arg(action)
            .arg(name)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to {} service {}: {}", action, name, stderr));
        }

        info!("✓ Systemd: {} {} complete", action, name);
        Ok(())
    }
}
