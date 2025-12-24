//! OVS Tools - OVSDB JSON-RPC based

use async_trait::async_trait;
use serde_json::{json, Value};
use op_core::Tool;

pub struct OvsTool {
    name: String,
    description: String,
}

impl OvsTool {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

#[async_trait]
impl Tool for OvsTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        match self.name.as_str() {
            "ovs_list_bridges" => json!({
                "type": "object",
                "properties": {}
            }),
            "ovs_create_bridge" => json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Bridge name"}
                },
                "required": ["name"]
            }),
            "ovs_delete_bridge" => json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Bridge name"}
                },
                "required": ["name"]
            }),
            "ovs_add_port" => json!({
                "type": "object",
                "properties": {
                    "bridge": {"type": "string", "description": "Bridge name"},
                    "port": {"type": "string", "description": "Port name"}
                },
                "required": ["bridge", "port"]
            }),
            _ => json!({"type": "object", "properties": {}})
        }
    }

    async fn execute(&self, args: Value) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        // Use op-network's OVSDB client
        match self.name.as_str() {
            "ovs_list_bridges" => {
                match op_network::ovsdb::list_bridges().await {
                    Ok(bridges) => Ok(json!({"bridges": bridges})),
                    Err(e) => Err(format!("Failed to list bridges: {}", e).into())
                }
            }
            "ovs_create_bridge" => {
                let name = args.get("name").and_then(|n| n.as_str())
                    .ok_or("Missing bridge name")?;
                match op_network::ovsdb::create_bridge(name).await {
                    Ok(_) => Ok(json!({"created": name})),
                    Err(e) => Err(format!("Failed to create bridge: {}", e).into())
                }
            }
            "ovs_delete_bridge" => {
                let name = args.get("name").and_then(|n| n.as_str())
                    .ok_or("Missing bridge name")?;
                match op_network::ovsdb::delete_bridge(name).await {
                    Ok(_) => Ok(json!({"deleted": name})),
                    Err(e) => Err(format!("Failed to delete bridge: {}", e).into())
                }
            }
            "ovs_list_ports" => {
                let bridge = args.get("bridge").and_then(|b| b.as_str())
                    .ok_or("Missing bridge name")?;
                match op_network::ovsdb::list_ports(bridge).await {
                    Ok(ports) => Ok(json!({"bridge": bridge, "ports": ports})),
                    Err(e) => Err(format!("Failed to list ports: {}", e).into())
                }
            }
            "ovs_add_port" => {
                let bridge = args.get("bridge").and_then(|b| b.as_str())
                    .ok_or("Missing bridge name")?;
                let port = args.get("port").and_then(|p| p.as_str())
                    .ok_or("Missing port name")?;
                match op_network::ovsdb::add_port(bridge, port).await {
                    Ok(_) => Ok(json!({"bridge": bridge, "port_added": port})),
                    Err(e) => Err(format!("Failed to add port: {}", e).into())
                }
            }
            _ => Ok(json!({"error": "Not implemented"}))
        }
    }
}
