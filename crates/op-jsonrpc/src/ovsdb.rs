//! OVSDB JSON-RPC client for Open vSwitch integration
//!
//! Direct JSON-RPC client for /var/run/openvswitch/db.sock

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tracing::{debug, info};

/// OVSDB JSON-RPC client
pub struct OvsdbClient {
    socket_path: String,
    timeout: Duration,
}

impl OvsdbClient {
    /// Create a new OVSDB client with default socket path
    pub fn new() -> Self {
        Self {
            socket_path: "/var/run/openvswitch/db.sock".to_string(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Create with a custom socket path
    pub fn with_socket(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Set timeout for RPC calls
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Send a JSON-RPC request and get response
    async fn rpc_call(&self, method: &str, params: Value) -> Result<Value> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .context("Failed to connect to OVSDB socket")?;

        let request = json!({
            "method": method,
            "params": params,
            "id": 0
        });

        let request_str = serde_json::to_string(&request)?;
        debug!("OVSDB request: {}", request_str);

        stream.write_all(request_str.as_bytes()).await?;
        stream.write_all(b"\n").await?;

        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();

        tokio::time::timeout(self.timeout, reader.read_line(&mut response_line))
            .await
            .context("OVSDB response timeout")??;

        debug!("OVSDB response: {}", response_line.trim());

        let response: Value = serde_json::from_str(&response_line)?;

        if let Some(error) = response.get("error") {
            if !error.is_null() {
                return Err(anyhow::anyhow!("OVSDB error: {}", error));
            }
        }

        Ok(response["result"].clone())
    }

    /// List all databases
    pub async fn list_dbs(&self) -> Result<Vec<String>> {
        let result = self.rpc_call("list_dbs", json!([])).await?;
        Ok(serde_json::from_value(result)?)
    }

    /// Get schema for a database
    pub async fn get_schema(&self, db: &str) -> Result<Value> {
        self.rpc_call("get_schema", json!([db])).await
    }

    /// Execute a transaction
    pub async fn transact(&self, db: &str, operations: Value) -> Result<Value> {
        let mut params = vec![json!(db)];
        if let Some(ops_array) = operations.as_array() {
            for op in ops_array {
                params.push(op.clone());
            }
        }
        self.rpc_call("transact", json!(params)).await
    }

    /// Create a bridge
    pub async fn create_bridge(&self, name: &str) -> Result<()> {
        let bridge_uuid = format!("bridge-{}", name);
        let port_uuid = format!("port-{}", name);
        let iface_uuid = format!("iface-{}", name);

        let operations = json!([
            {
                "op": "insert",
                "table": "Bridge",
                "row": {
                    "name": name,
                    "ports": ["set", [["named-uuid", port_uuid]]]
                },
                "uuid-name": bridge_uuid
            },
            {
                "op": "insert",
                "table": "Port",
                "row": {
                    "name": name,
                    "interfaces": ["set", [["named-uuid", iface_uuid]]]
                },
                "uuid-name": port_uuid
            },
            {
                "op": "insert",
                "table": "Interface",
                "row": {
                    "name": name,
                    "type": "internal"
                },
                "uuid-name": iface_uuid
            },
            {
                "op": "mutate",
                "table": "Open_vSwitch",
                "where": [],
                "mutations": [
                    ["bridges", "insert", ["set", [["named-uuid", bridge_uuid]]]]
                ]
            }
        ]);

        self.transact("Open_vSwitch", operations).await?;
        info!("Created OVS bridge: {}", name);
        Ok(())
    }

    /// Delete a bridge
    pub async fn delete_bridge(&self, name: &str) -> Result<()> {
        let bridge_uuid = self.find_bridge_uuid(name).await?;

        let operations = json!([
            {
                "op": "mutate",
                "table": "Open_vSwitch",
                "where": [],
                "mutations": [
                    ["bridges", "delete", ["uuid", &bridge_uuid]]
                ]
            },
            {
                "op": "delete",
                "table": "Bridge",
                "where": [["_uuid", "==", ["uuid", &bridge_uuid]]]
            }
        ]);

        self.transact("Open_vSwitch", operations).await?;
        info!("Deleted OVS bridge: {}", name);
        Ok(())
    }

    /// Add a port to a bridge
    pub async fn add_port(&self, bridge: &str, port: &str) -> Result<()> {
        let bridge_uuid = self.find_bridge_uuid(bridge).await?;
        let port_uuid = format!("port-{}", port);
        let iface_uuid = format!("iface-{}", port);

        let operations = json!([
            {
                "op": "insert",
                "table": "Port",
                "row": {
                    "name": port,
                    "interfaces": ["set", [["named-uuid", iface_uuid]]]
                },
                "uuid-name": port_uuid
            },
            {
                "op": "insert",
                "table": "Interface",
                "row": {
                    "name": port
                },
                "uuid-name": iface_uuid
            },
            {
                "op": "mutate",
                "table": "Bridge",
                "where": [["_uuid", "==", ["uuid", &bridge_uuid]]],
                "mutations": [
                    ["ports", "insert", ["set", [["named-uuid", port_uuid]]]]
                ]
            }
        ]);

        self.transact("Open_vSwitch", operations).await?;
        info!("Added port {} to bridge {}", port, bridge);
        Ok(())
    }

    /// List all bridges
    pub async fn list_bridges(&self) -> Result<Vec<String>> {
        let operations = json!([{
            "op": "select",
            "table": "Bridge",
            "where": [],
            "columns": ["name"]
        }]);

        let result = self.transact("Open_vSwitch", operations).await?;

        let mut bridges = Vec::new();
        if let Some(rows) = result[0]["rows"].as_array() {
            for row in rows {
                if let Some(name) = row["name"].as_str() {
                    bridges.push(name.to_string());
                }
            }
        }

        Ok(bridges)
    }

    /// List ports on a bridge
    pub async fn list_ports(&self, bridge: &str) -> Result<Vec<String>> {
        let bridge_uuid = self.find_bridge_uuid(bridge).await?;

        let operations = json!([{
            "op": "select",
            "table": "Bridge",
            "where": [["_uuid", "==", ["uuid", &bridge_uuid]]],
            "columns": ["ports"]
        }]);

        let result = self.transact("Open_vSwitch", operations).await?;

        let mut port_uuids = Vec::new();
        if let Some(rows) = result[0]["rows"].as_array() {
            if let Some(first_row) = rows.first() {
                if let Some(ports) = first_row["ports"].as_array() {
                    if ports.len() == 2 && ports[0] == "set" {
                        if let Some(port_set) = ports[1].as_array() {
                            for port in port_set {
                                if let Some(uuid_array) = port.as_array() {
                                    if uuid_array.len() == 2 && uuid_array[0] == "uuid" {
                                        port_uuids
                                            .push(uuid_array[1].as_str().unwrap().to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Get port names
        let mut port_names = Vec::new();
        for port_uuid in port_uuids {
            let ops = json!([{
                "op": "select",
                "table": "Port",
                "where": [["_uuid", "==", ["uuid", &port_uuid]]],
                "columns": ["name"]
            }]);

            let result = self.transact("Open_vSwitch", ops).await?;
            if let Some(rows) = result[0]["rows"].as_array() {
                if let Some(first_row) = rows.first() {
                    if let Some(name) = first_row["name"].as_str() {
                        port_names.push(name.to_string());
                    }
                }
            }
        }

        Ok(port_names)
    }

    /// Check if a bridge exists
    pub async fn bridge_exists(&self, name: &str) -> Result<bool> {
        match self.find_bridge_uuid(name).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Get bridge info
    pub async fn get_bridge_info(&self, name: &str) -> Result<Value> {
        let bridge_uuid = self.find_bridge_uuid(name).await?;

        let operations = json!([{
            "op": "select",
            "table": "Bridge",
            "where": [["_uuid", "==", ["uuid", &bridge_uuid]]],
            "columns": []
        }]);

        let result = self.transact("Open_vSwitch", operations).await?;
        Ok(result[0]["rows"][0].clone())
    }

    /// Dump entire database
    pub async fn dump_db(&self, db: &str) -> Result<Value> {
        let schema = self.get_schema(db).await?;
        let tables = schema
            .get("tables")
            .and_then(|v| v.as_object())
            .ok_or_else(|| anyhow::anyhow!("Invalid schema: missing tables"))?;

        let mut ops = Vec::new();
        let mut order = Vec::new();

        for (name, _) in tables {
            ops.push(json!({
                "op": "select",
                "table": name,
                "where": []
            }));
            order.push(name.clone());
        }

        let result = self.transact(db, json!(ops)).await?;

        let mut out = serde_json::Map::new();
        for (i, name) in order.into_iter().enumerate() {
            let rows = result
                .get(i)
                .and_then(|r| r.get("rows"))
                .cloned()
                .unwrap_or_else(|| json!([]));
            out.insert(name, rows);
        }

        Ok(Value::Object(out))
    }

    /// Find bridge UUID by name
    async fn find_bridge_uuid(&self, name: &str) -> Result<String> {
        let operations = json!([{
            "op": "select",
            "table": "Bridge",
            "where": [["name", "==", name]],
            "columns": ["_uuid"]
        }]);

        let result = self.transact("Open_vSwitch", operations).await?;

        if let Some(rows) = result[0]["rows"].as_array() {
            if let Some(first_row) = rows.first() {
                if let Some(uuid_array) = first_row["_uuid"].as_array() {
                    if uuid_array.len() == 2 && uuid_array[0] == "uuid" {
                        return Ok(uuid_array[1].as_str().unwrap().to_string());
                    }
                }
            }
        }

        Err(anyhow::anyhow!("Bridge '{}' not found", name))
    }
}

impl Default for OvsdbClient {
    fn default() -> Self {
        Self::new()
    }
}
