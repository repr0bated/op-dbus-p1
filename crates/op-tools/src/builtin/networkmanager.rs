//! NetworkManager Tools - D-Bus based

use async_trait::async_trait;
use serde_json::{json, Value};
use op_core::Tool;

pub struct NetworkManagerTool {
    name: String,
    description: String,
}

impl NetworkManagerTool {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

#[async_trait]
impl Tool for NetworkManagerTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    async fn execute(&self, args: Value) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        match self.name.as_str() {
            "nm_list_devices" => {
                match op_dbus::networkmanager::list_devices().await {
                    Ok(devices) => Ok(json!({"devices": devices})),
                    Err(e) => Err(format!("Failed to list devices: {}", e).into())
                }
            }
            "nm_get_device" => {
                let device = args.get("device").and_then(|d| d.as_str())
                    .ok_or("Missing device name")?;
                match op_dbus::networkmanager::get_device(device).await {
                    Ok(info) => Ok(info),
                    Err(e) => Err(format!("Failed to get device: {}", e).into())
                }
            }
            "nm_list_connections" => {
                match op_dbus::networkmanager::list_connections().await {
                    Ok(connections) => Ok(json!({"connections": connections})),
                    Err(e) => Err(format!("Failed to list connections: {}", e).into())
                }
            }
            _ => Ok(json!({"error": "Not implemented"}))
        }
    }
}
