//! Rtnetlink tools - native network interface and route management
//!
//! These tools provide direct access to Linux network configuration via rtnetlink,
//! avoiding CLI tools like `ip`, `ifconfig`, etc.

use crate::Tool;
use crate::ToolRegistry;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;
use anyhow::Result;

/// Tool to list all network interfaces
pub struct RtnetlinkListInterfacesTool;

#[async_trait]
impl Tool for RtnetlinkListInterfacesTool {
    fn name(&self) -> &str {
        "list_network_interfaces"
    }

    fn description(&self) -> &str {
        "List all network interfaces with their details (name, MAC, MTU, state, addresses) using native rtnetlink. Equivalent to 'ip addr show' but without CLI."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "filter_state": {
                    "type": "string",
                    "description": "Optional: filter by state ('up' or 'down')",
                    "enum": ["up", "down"]
                }
            },
            "required": []
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["rtnetlink".to_string(), "network".to_string(), "interfaces".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        info!("Listing network interfaces via rtnetlink");

        let filter_state = input.get("filter_state").and_then(|v| v.as_str());

        match op_network::rtnetlink::list_interfaces().await {
            Ok(mut interfaces) => {
                // Apply filters
                if let Some(state) = filter_state {
                    interfaces.retain(|iface| iface.state == state);
                }

                let count = interfaces.len();
                Ok(json!({
                    "protocol": "rtnetlink",
                    "count": count,
                    "interfaces": interfaces
                }))
            }
            Err(e) => {
                // Fallback to `ip -j addr show`
                use tokio::process::Command;
                
                info!("Native rtnetlink failed ({}), trying 'ip' command fallback", e);
                
                let output = Command::new("ip")
                    .args(&["-j", "addr", "show"])
                    .output()
                    .await;
                    
                match output {
                    Ok(out) if out.status.success() => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let mut interfaces: Value = serde_json::from_str(&stdout)
                            .map_err(|je| anyhow::anyhow!("Failed to parse ip command output: {}", je))?;
                        
                        // Basic filtering if it's an array
                        if let Some(arr) = interfaces.as_array_mut() {
                            if let Some(state) = filter_state {
                                let state_upper = state.to_uppercase();
                                arr.retain(|iface| {
                                    iface.get("operstate")
                                        .and_then(|s| s.as_str())
                                        .map(|s| s == state_upper)
                                        .unwrap_or(false)
                                });
                            }
                        }
                        
                        Ok(json!({
                            "protocol": "cli_fallback",
                            "interfaces": interfaces,
                            "native_error": e.to_string()
                        }))
                    }
                    _ => Err(anyhow::anyhow!("Failed to list interfaces (native: {}, cli: failed)", e)),
                }
            }
        }
    }
}

/// Tool to get the default route
pub struct RtnetlinkGetDefaultRouteTool;

#[async_trait]
impl Tool for RtnetlinkGetDefaultRouteTool {
    fn name(&self) -> &str {
        "rtnetlink_get_default_route"
    }

    fn description(&self) -> &str {
        "Get the default IPv4 route (gateway and interface) using native rtnetlink. Equivalent to 'ip route show default' but without CLI."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["rtnetlink".to_string(), "network".to_string(), "route".to_string()]
    }

    async fn execute(&self, _input: Value) -> Result<Value> {
        info!("Getting default route via rtnetlink");

        match op_network::rtnetlink::get_default_route().await {
            Ok(Some(route)) => Ok(json!({
                "protocol": "rtnetlink",
                "found": true,
                "route": route
            })),
            Ok(None) => Ok(json!({
                "protocol": "rtnetlink",
                "found": false,
                "message": "No default route configured"
            })),
            Err(e) => Err(anyhow::anyhow!("Failed to get default route: {}", e)),
        }
    }
}

/// Tool to add an IP address to an interface
pub struct RtnetlinkAddAddressTool;

#[async_trait]
impl Tool for RtnetlinkAddAddressTool {
    fn name(&self) -> &str {
        "rtnetlink_add_address"
    }

    fn description(&self) -> &str {
        "Add an IPv4 address to a network interface using native rtnetlink. Equivalent to 'ip addr add' but without CLI."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "interface": {
                    "type": "string",
                    "description": "Interface name (e.g., 'eth0', 'ens1')"
                },
                "address": {
                    "type": "string",
                    "description": "IPv4 address to add (e.g., '10.0.0.1')"
                },
                "prefix_len": {
                    "type": "integer",
                    "description": "Prefix length / CIDR (e.g., 24 for /24, 32 for single host)"
                }
            },
            "required": ["interface", "address", "prefix_len"]
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["rtnetlink".to_string(), "network".to_string(), "address".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        // Accept both "interface" and "iface" for compatibility
        let interface = input
            .get("interface")
            .or_else(|| input.get("iface"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: interface"))?;
        let address = input
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: address"))?;
        // Accept both "prefix_len" and "prefix" for compatibility
        let prefix_len = input
            .get("prefix_len")
            .or_else(|| input.get("prefix"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: prefix_len"))? as u8;

        info!(
            "Adding address {}/{} to {} via rtnetlink",
            address, prefix_len, interface
        );

        match op_network::rtnetlink::add_ipv4_address(interface, address, prefix_len).await {
            Ok(()) => Ok(json!({
                "protocol": "rtnetlink",
                "success": true,
                "interface": interface,
                "address": address,
                "prefix_len": prefix_len,
                "message": format!("Added {}/{} to {}", address, prefix_len, interface)
            })),
            Err(e) => Err(anyhow::anyhow!("Failed to add address: {}", e)),
        }
    }
}

/// Tool to bring an interface up
pub struct RtnetlinkLinkUpTool;

#[async_trait]
impl Tool for RtnetlinkLinkUpTool {
    fn name(&self) -> &str {
        "rtnetlink_link_up"
    }

    fn description(&self) -> &str {
        "Bring a network interface up using native rtnetlink. Equivalent to 'ip link set up' but without CLI."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "interface": {
                    "type": "string",
                    "description": "Interface name to bring up (e.g., 'eth0', 'ens1')"
                }
            },
            "required": ["interface"]
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["rtnetlink".to_string(), "network".to_string(), "link".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let interface = input
            .get("interface")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: interface"))?;

        info!("Bringing interface {} up via rtnetlink", interface);

        match op_network::rtnetlink::link_up(interface).await {
            Ok(()) => Ok(json!({
                "protocol": "rtnetlink",
                "success": true,
                "interface": interface,
                "state": "up",
                "message": format!("Interface {} is now up", interface)
            })),
            Err(e) => Err(anyhow::anyhow!("Failed to bring interface up: {}", e)),
        }
    }
}

/// Tool to bring an interface down
pub struct RtnetlinkLinkDownTool;

#[async_trait]
impl Tool for RtnetlinkLinkDownTool {
    fn name(&self) -> &str {
        "rtnetlink_link_down"
    }

    fn description(&self) -> &str {
        "Bring a network interface down using native rtnetlink. Equivalent to 'ip link set down' but without CLI."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "interface": {
                    "type": "string",
                    "description": "Interface name to bring down (e.g., 'eth0', 'ens1')"
                }
            },
            "required": ["interface"]
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["rtnetlink".to_string(), "network".to_string(), "link".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let interface = input
            .get("interface")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: interface"))?;

        info!("Bringing interface {} down via rtnetlink", interface);

        match op_network::rtnetlink::link_down(interface).await {
            Ok(()) => Ok(json!({
                "protocol": "rtnetlink",
                "success": true,
                "interface": interface,
                "state": "down",
                "message": format!("Interface {} is now down", interface)
            })),
            Err(e) => Err(anyhow::anyhow!("Failed to bring interface down: {}", e)),
        }
    }
}

/// Register all rtnetlink tools
pub async fn register_rtnetlink_tools(
    registry: &ToolRegistry,
) -> Result<()> {
    registry.register_tool(Arc::new(RtnetlinkListInterfacesTool)).await?;
    registry.register_tool(Arc::new(RtnetlinkGetDefaultRouteTool)).await?;
    registry.register_tool(Arc::new(RtnetlinkAddAddressTool)).await?;
    registry.register_tool(Arc::new(RtnetlinkLinkUpTool)).await?;
    registry.register_tool(Arc::new(RtnetlinkLinkDownTool)).await?;
    info!("Registered 5 rtnetlink tools");
    Ok(())
}
