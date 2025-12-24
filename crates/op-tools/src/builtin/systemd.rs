//! Systemd Tools - D-Bus based

use async_trait::async_trait;
use serde_json::{json, Value};
use op_core::Tool;

pub struct SystemdTool {
    name: String,
    description: String,
}

impl SystemdTool {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

#[async_trait]
impl Tool for SystemdTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        match self.name.as_str() {
            "systemd_list_units" => json!({
                "type": "object",
                "properties": {
                    "type_filter": {"type": "string", "description": "Filter by unit type (service, socket, etc.)"}
                }
            }),
            "systemd_get_unit_status" | "systemd_start_unit" | "systemd_stop_unit" | "systemd_restart_unit" => json!({
                "type": "object",
                "properties": {
                    "unit": {"type": "string", "description": "Unit name (e.g., nginx.service)"}
                },
                "required": ["unit"]
            }),
            _ => json!({"type": "object", "properties": {}})
        }
    }

    async fn execute(&self, args: Value) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        // Use op-dbus's systemd client
        match self.name.as_str() {
            "systemd_list_units" => {
                match op_dbus::systemd::list_units().await {
                    Ok(units) => Ok(json!({"units": units})),
                    Err(e) => Err(format!("Failed to list units: {}", e).into())
                }
            }
            "systemd_get_unit_status" => {
                let unit = args.get("unit").and_then(|u| u.as_str())
                    .ok_or("Missing unit name")?;
                match op_dbus::systemd::get_unit_status(unit).await {
                    Ok(status) => Ok(status),
                    Err(e) => Err(format!("Failed to get unit status: {}", e).into())
                }
            }
            "systemd_start_unit" => {
                let unit = args.get("unit").and_then(|u| u.as_str())
                    .ok_or("Missing unit name")?;
                match op_dbus::systemd::start_unit(unit).await {
                    Ok(_) => Ok(json!({"started": unit})),
                    Err(e) => Err(format!("Failed to start unit: {}", e).into())
                }
            }
            "systemd_stop_unit" => {
                let unit = args.get("unit").and_then(|u| u.as_str())
                    .ok_or("Missing unit name")?;
                match op_dbus::systemd::stop_unit(unit).await {
                    Ok(_) => Ok(json!({"stopped": unit})),
                    Err(e) => Err(format!("Failed to stop unit: {}", e).into())
                }
            }
            "systemd_restart_unit" => {
                let unit = args.get("unit").and_then(|u| u.as_str())
                    .ok_or("Missing unit name")?;
                match op_dbus::systemd::restart_unit(unit).await {
                    Ok(_) => Ok(json!({"restarted": unit})),
                    Err(e) => Err(format!("Failed to restart unit: {}", e).into())
                }
            }
            _ => Ok(json!({"error": "Not implemented"}))
        }
    }
}
