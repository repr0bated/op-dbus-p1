//! OVS Tools for Chat Interface
//!
//! These tools expose OVS operations to the LLM chat system.

use crate::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;
use anyhow::Result;
use crate::ToolRegistry;
use std::sync::Arc;

/// Tool to test tool execution (no network ops)
pub struct TestTool;

#[async_trait]
impl Tool for TestTool {
    fn name(&self) -> &str {
        "test_tool"
    }

    fn description(&self) -> &str {
        "Test tool execution without network operations"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    
    fn category(&self) -> &str {
        "test"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["test".to_string()]
    }

    async fn execute(&self, _input: Value) -> Result<Value> {
        // Simple delay to test async execution
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(json!({
            "message": "Test tool executed successfully",
            "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
        }))
    }
}

/// Tool to list OVS bridges (via OVSDB)
pub struct OvsListBridgesTool;

#[async_trait]
impl Tool for OvsListBridgesTool {
    fn name(&self) -> &str {
        "ovs_list_bridges"
    }

    fn description(&self) -> &str {
        "List all OVS bridges configured in OVSDB. LLM_EXCUSE_COUNTER: This proves we CAN interact with OVS."
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
        vec!["ovs".to_string(), "bridges".to_string()]
    }

    async fn execute(&self, _input: Value) -> Result<Value> {
        use op_network::OvsdbClient;
        use tokio::process::Command;

        // Try native client first
        match OvsdbClient::new().list_bridges().await {
            Ok(bridges) => Ok(json!({ "bridges": bridges, "method": "native_ovsdb" })),
            Err(e) => {
                // Fallback to CLI (service runs as root, no sudo needed)
                let output = Command::new("ovs-vsctl")
                    .arg("list-br")
                    .output()
                    .await;

                match output {
                    Ok(out) if out.status.success() => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let bridges: Vec<&str> = stdout.lines().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                        Ok(json!({ 
                            "bridges": bridges, 
                            "method": "cli_fallback",
                            "native_error": e.to_string()
                        }))
                    }
                    _ => {
                        // Return original error if fallback also failed
                        Err(e)
                    }
                }
            }
        }
    }
}

/// Tool to list kernel datapaths (via OVS Netlink)
pub struct OvsListDatapathsTool;

#[async_trait]
impl Tool for OvsListDatapathsTool {
    fn name(&self) -> &str {
        "ovs_list_datapaths"
    }

    fn description(&self) -> &str {
        "List OVS kernel datapaths via Generic Netlink. Requires root privileges. Shows kernel-level datapath info."
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
        vec!["ovs".to_string(), "datapaths".to_string(), "kernel".to_string()]
    }

    async fn execute(&self, _input: Value) -> Result<Value> {
        use op_network::OvsNetlinkClient;

        let mut client = OvsNetlinkClient::new().await
            .map_err(|e| anyhow::anyhow!("Failed to create netlink client: {} (requires root)", e))?;
            
        let dps = client.list_datapaths().await
            .map_err(|e| anyhow::anyhow!("Failed to list datapaths: {}", e))?;
            
        Ok(json!({ "datapaths": dps }))
    }
}

/// Tool to list vports on a datapath
pub struct OvsListVportsTool;

#[async_trait]
impl Tool for OvsListVportsTool {
    fn name(&self) -> &str {
        "ovs_list_vports"
    }

    fn description(&self) -> &str {
        "List vports on an OVS kernel datapath. Requires root."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "datapath": {
                    "type": "string",
                    "description": "Name of the datapath (e.g., 'ovs-system' or bridge name)"
                }
            },
            "required": ["datapath"]
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["ovs".to_string(), "vports".to_string(), "kernel".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        use op_network::OvsNetlinkClient;

        let dp_name = input
            .get("datapath")
            .and_then(|v| v.as_str())
            .unwrap_or("ovs-system");

        let mut client = OvsNetlinkClient::new().await
            .map_err(|e| anyhow::anyhow!("Failed to create netlink client: {}", e))?;
            
        let vports = client.list_vports(dp_name).await
            .map_err(|e| anyhow::anyhow!("Failed to list vports: {}", e))?;
            
        Ok(json!({ "datapath": dp_name, "vports": vports }))
    }
}

/// Tool to show OVS capabilities
pub struct OvsCapabilitiesTool;

#[async_trait]
impl Tool for OvsCapabilitiesTool {
    fn name(&self) -> &str {
        "ovs_capabilities"
    }

    fn description(&self) -> &str {
        "Detect and report OVS capabilities. Shows what OVS operations are available on this system. Use this to know what you CAN do."
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
        vec!["ovs".to_string(), "capabilities".to_string(), "detection".to_string()]
    }

    async fn execute(&self, _input: Value) -> Result<Value> {
        use op_network::OvsCapabilities;

        let caps = OvsCapabilities::detect().await;
        let llm_context = caps.to_llm_context();

        Ok(json!({
            "capabilities": caps,
            "llm_context": llm_context
        }))
    }
}

/// Tool to dump kernel flows
pub struct OvsDumpFlowsTool;

#[async_trait]
impl Tool for OvsDumpFlowsTool {
    fn name(&self) -> &str {
        "ovs_dump_flows"
    }

    fn description(&self) -> &str {
        "Dump kernel flow table for a datapath. Shows flows cached in kernel. Requires root. LLM_EXCUSE_COUNTER: Yes, we CAN see kernel flows."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "datapath": {
                    "type": "string",
                    "description": "Datapath name (default: ovs-system)"
                }
            },
            "required": []
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["ovs".to_string(), "flows".to_string(), "kernel".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        use op_network::OvsNetlinkClient;

        let dp_name = input
            .get("datapath")
            .and_then(|v| v.as_str())
            .unwrap_or("ovs-system");

        let mut client = OvsNetlinkClient::new().await
            .map_err(|e| anyhow::anyhow!("Failed to create netlink client: {}", e))?;
            
        let flows = client.dump_flows(dp_name).await
            .map_err(|e| anyhow::anyhow!("Failed to dump flows: {}", e))?;
            
        Ok(json!({
            "datapath": dp_name,
            "flow_count": flows.len(),
            "flows": flows
        }))
    }
}

// =============================================================================
// OVSDB WRITE OPERATIONS - Bridge/Port Management via JSON-RPC
// =============================================================================

/// Tool to create an OVS bridge
pub struct OvsCreateBridgeTool;

#[async_trait]
impl Tool for OvsCreateBridgeTool {
    fn name(&self) -> &str {
        "ovs_create_bridge"
    }

    fn description(&self) -> &str {
        "Create a new OVS bridge via OVSDB JSON-RPC. This creates the bridge in the Open vSwitch database and the kernel datapath. The bridge will have an internal port with the same name automatically created."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the bridge to create (e.g., 'br0', 'ovsbr1')"
                }
            },
            "required": ["name"]
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["ovs".to_string(), "bridge".to_string(), "create".to_string(), "write".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        use op_network::OvsdbClient;

        let bridge_name = input.get("name").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: name"))?;

        let client = OvsdbClient::new();

        // Check if bridge already exists
        let bridges = client.list_bridges().await
            .map_err(|e| anyhow::anyhow!("Failed to check existing bridges: {}", e))?;
            
        if bridges.contains(&bridge_name.to_string()) {
            return Err(anyhow::anyhow!("Bridge '{}' already exists", bridge_name));
        }

        client.create_bridge(bridge_name).await
            .map_err(|e| anyhow::anyhow!("Failed to create bridge: {}", e))?;
            
        // POST-EXECUTION VERIFICATION: Check if bridge was actually created
        let bridges_after = client.list_bridges().await
            .map_err(|e| anyhow::anyhow!("Bridge creation succeeded but verification failed: {}", e))?;
            
        if bridges_after.contains(&bridge_name.to_string()) {
            Ok(json!({
                "success": true,
                "bridge": bridge_name,
                "message": format!("Bridge '{}' created and verified successfully", bridge_name),
                "verification": "Bridge found in OVSDB after creation"
            }))
        } else {
            Err(anyhow::anyhow!("Bridge creation claimed success but '{}' not found in OVSDB - possible hallucination", bridge_name))
        }
    }
}

/// Tool to delete an OVS bridge
pub struct OvsDeleteBridgeTool;

#[async_trait]
impl Tool for OvsDeleteBridgeTool {
    fn name(&self) -> &str {
        "ovs_delete_bridge"
    }

    fn description(&self) -> &str {
        "Delete an OVS bridge via OVSDB JSON-RPC. This removes the bridge from the database and kernel. WARNING: This will disconnect any ports attached to the bridge."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the bridge to delete"
                }
            },
            "required": ["name"]
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["ovs".to_string(), "bridge".to_string(), "delete".to_string(), "write".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        use op_network::OvsdbClient;

        let bridge_name = input.get("name").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: name"))?;

        let client = OvsdbClient::new();

        client.delete_bridge(bridge_name).await
            .map_err(|e| anyhow::anyhow!("Failed to delete bridge: {}", e))?;
            
        Ok(json!({
            "success": true,
            "bridge": bridge_name,
            "message": format!("Bridge '{}' deleted successfully", bridge_name)
        }))
    }
}

/// Tool to add a port to an OVS bridge
pub struct OvsAddPortTool;

#[async_trait]
impl Tool for OvsAddPortTool {
    fn name(&self) -> &str {
        "ovs_add_port"
    }

    fn description(&self) -> &str {
        "Add a port (network interface) to an OVS bridge via OVSDB JSON-RPC. This attaches an existing network interface to the bridge, allowing traffic to flow through OVS."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "bridge": {
                    "type": "string",
                    "description": "Name of the bridge to add the port to"
                },
                "port": {
                    "type": "string",
                    "description": "Name of the port/interface to add (e.g., 'eth0', 'veth1')"
                }
            },
            "required": ["bridge", "port"]
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["ovs".to_string(), "port".to_string(), "add".to_string(), "write".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        use op_network::OvsdbClient;

        let bridge_name = input.get("bridge").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: bridge"))?;
            
        let port_name = input.get("port").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: port"))?;

        let client = OvsdbClient::new();

        client.add_port(bridge_name, port_name).await
            .map_err(|e| anyhow::anyhow!("Failed to add port: {}", e))?;
            
        Ok(json!({
            "success": true,
            "bridge": bridge_name,
            "port": port_name,
            "message": format!("Port '{}' added to bridge '{}' successfully", port_name, bridge_name)
        }))
    }
}

/// Tool to list ports on an OVS bridge
pub struct OvsListPortsTool;

#[async_trait]
impl Tool for OvsListPortsTool {
    fn name(&self) -> &str {
        "ovs_list_ports"
    }

    fn description(&self) -> &str {
        "List all ports attached to an OVS bridge via OVSDB JSON-RPC. Shows the network interfaces connected to the specified bridge."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "bridge": {
                    "type": "string",
                    "description": "Name of the bridge to list ports for"
                }
            },
            "required": ["bridge"]
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["ovs".to_string(), "port".to_string(), "list".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        use op_network::OvsdbClient;

        let bridge_name = input.get("bridge").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: bridge"))?;

        let client = OvsdbClient::new();

        let ports = client.list_bridge_ports(bridge_name).await
            .map_err(|e| anyhow::anyhow!("Failed to list ports: {}", e))?;
            
        Ok(json!({
            "bridge": bridge_name,
            "ports": ports,
            "port_count": ports.len()
        }))
    }
}

pub async fn register_ovs_tools(registry: &ToolRegistry) -> Result<()> {
    registry.register_tool(Arc::new(OvsListBridgesTool)).await?;
    registry.register_tool(Arc::new(OvsListPortsTool)).await?;
    registry.register_tool(Arc::new(OvsCapabilitiesTool)).await?;
    registry.register_tool(Arc::new(OvsCreateBridgeTool)).await?;
    registry.register_tool(Arc::new(OvsDeleteBridgeTool)).await?;
    registry.register_tool(Arc::new(OvsAddPortTool)).await?;
    registry.register_tool(Arc::new(OvsListDatapathsTool)).await?;
    registry.register_tool(Arc::new(OvsListVportsTool)).await?;
    registry.register_tool(Arc::new(OvsDumpFlowsTool)).await?;
    Ok(())
}

/// Tool to get detailed bridge info
pub struct OvsGetBridgeInfoTool;

#[async_trait]
impl Tool for OvsGetBridgeInfoTool {
    fn name(&self) -> &str {
        "ovs_get_bridge_info"
    }

    fn description(&self) -> &str {
        "Get detailed information about an OVS bridge from OVSDB. Returns all properties including controller, protocols, datapath type, and other configuration."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "bridge": {
                    "type": "string",
                    "description": "Name of the bridge to get info for"
                }
            },
            "required": ["bridge"]
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["ovs".to_string(), "bridge".to_string(), "info".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        use op_network::OvsdbClient;

        let bridge_name = input.get("bridge").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: bridge"))?;

        let client = OvsdbClient::new();

        let info = client.get_bridge_info(bridge_name).await
            .map_err(|e| anyhow::anyhow!("Failed to get bridge info: {}", e))?;
            
        Ok(json!({
            "bridge": bridge_name,
            "info": info
        }))
    }
}

/// Tool to check if OVS is available
pub struct OvsCheckAvailableTool;

#[async_trait]
impl Tool for OvsCheckAvailableTool {
    fn name(&self) -> &str {
        "ovs_check_available"
    }

    fn description(&self) -> &str {
        "Check if Open vSwitch is available and running on this system. Verifies OVSDB socket connectivity and returns available databases. Use this first to confirm OVS operations will work."
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
        vec!["ovs".to_string(), "check".to_string(), "status".to_string()]
    }

    async fn execute(&self, _input: Value) -> Result<Value> {
        use op_network::OvsdbClient;

        let client = OvsdbClient::new();

        // Try to list databases to verify connectivity
        match client.list_dbs().await {
            Ok(dbs) => {
                // Also try to list bridges
                let bridges = client.list_bridges().await.unwrap_or_default();

                Ok(json!({
                    "available": true,
                    "socket": "/var/run/openvswitch/db.sock",
                    "databases": dbs,
                    "bridges": bridges,
                    "message": "Open vSwitch is available and responding"
                }))
            }
            Err(e) => Ok(json!({
                "available": false,
                "socket": "/var/run/openvswitch/db.sock",
                "error": e.to_string(),
                "message": "Open vSwitch is not available or not running"
            })),
        }
    }
}

/// Tool to set a bridge property
pub struct OvsSetBridgePropertyTool;

#[async_trait]
impl Tool for OvsSetBridgePropertyTool {
    fn name(&self) -> &str {
        "ovs_set_bridge_property"
    }

    fn description(&self) -> &str {
        "Set a property on an OVS bridge via OVSDB JSON-RPC. Supported properties: datapath_type (system/netdev), fail_mode (secure/standalone), stp_enable, mcast_snooping_enable."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "bridge": {
                    "type": "string",
                    "description": "Name of the bridge"
                },
                "property": {
                    "type": "string",
                    "description": "Property name (datapath_type, fail_mode, stp_enable, mcast_snooping_enable)"
                },
                "value": {
                    "type": "string",
                    "description": "Property value"
                }
            },
            "required": ["bridge", "property", "value"]
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["ovs".to_string(), "bridge".to_string(), "property".to_string(), "write".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        use op_network::OvsdbClient;

        let bridge_name = input.get("bridge").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: bridge"))?;
            
        let property = input.get("property").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: property"))?;
            
        let value = input.get("value").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: value"))?;

        let client = OvsdbClient::new();

        client.set_bridge_property(bridge_name, property, value).await
            .map_err(|e| anyhow::anyhow!("Failed to set bridge property: {}", e))?;
            
        Ok(json!({
            "success": true,
            "bridge": bridge_name,
            "property": property,
            "value": value,
            "message": format!("Set {}={} on bridge '{}'", property, value, bridge_name)
        }))
    }
}

/// Tool to delete a port from an OVS bridge
pub struct OvsDeletePortTool;

#[async_trait]
impl Tool for OvsDeletePortTool {
    fn name(&self) -> &str {
        "ovs_delete_port"
    }

    fn description(&self) -> &str {
        "Delete a port from an OVS bridge via OVSDB JSON-RPC. This removes the port and its interface from the bridge."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "bridge": {
                    "type": "string",
                    "description": "Name of the bridge"
                },
                "port": {
                    "type": "string",
                    "description": "Name of the port to delete"
                }
            },
            "required": ["bridge", "port"]
        })
    }
    
    fn category(&self) -> &str {
        "networking"
    }
    
    fn tags(&self) -> Vec<String> {
        vec!["ovs".to_string(), "port".to_string(), "delete".to_string(), "write".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        use op_network::OvsdbClient;

        let bridge_name = input.get("bridge").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: bridge"))?;
            
        let port_name = input.get("port").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: port"))?;

        let client = OvsdbClient::new();

        client.delete_port(bridge_name, port_name).await
            .map_err(|e| anyhow::anyhow!("Failed to delete port: {}", e))?;
            
        Ok(json!({
            "success": true,
            "bridge": bridge_name,
            "port": port_name,
            "message": format!("Port '{}' deleted from bridge '{}'", port_name, bridge_name)
        }))
    }
}

/// Create all OVS tools
pub fn create_ovs_tools() -> Vec<std::sync::Arc<dyn Tool>> {
    vec![
        // Test tool (for debugging tool execution)
        std::sync::Arc::new(TestTool),
        // Read operations
        std::sync::Arc::new(OvsCheckAvailableTool),
        std::sync::Arc::new(OvsListBridgesTool),
        std::sync::Arc::new(OvsListPortsTool),
        std::sync::Arc::new(OvsGetBridgeInfoTool),
        std::sync::Arc::new(OvsListDatapathsTool),
        std::sync::Arc::new(OvsListVportsTool),
        std::sync::Arc::new(OvsCapabilitiesTool),
        std::sync::Arc::new(OvsDumpFlowsTool),
        // Write operations
        std::sync::Arc::new(OvsCreateBridgeTool),
        std::sync::Arc::new(OvsDeleteBridgeTool),
        std::sync::Arc::new(OvsAddPortTool),
        std::sync::Arc::new(OvsDeletePortTool),
        std::sync::Arc::new(OvsSetBridgePropertyTool),
    ]
}
