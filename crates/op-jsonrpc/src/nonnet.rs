//! NonNet database - OVSDB-like interface for non-network plugin state
//!
//! Provides a read-only, OVSDB-compatible JSON-RPC interface over Unix socket
//! for querying non-network plugin state.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::protocol::{error_codes, JsonRpcRequest, JsonRpcResponse};

/// NonNet database state
pub struct NonNetDb {
    state: Arc<RwLock<NonNetState>>,
}

/// Internal state structure
#[derive(Default)]
struct NonNetState {
    tables: HashMap<String, Vec<Value>>,
    schema: Value,
}

impl NonNetDb {
    /// Create a new NonNet database
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(NonNetState::default())),
        }
    }

    /// Set the tables/schema from plugin state
    pub async fn load_from_plugins(&self, plugins: &HashMap<String, Value>) {
        let mut state = self.state.write().await;

        // Build schema and tables from plugin state
        let mut schema_tables = serde_json::Map::new();
        let mut tables = HashMap::new();

        for (name, value) in plugins {
            // Skip network plugin
            if name == "net" {
                continue;
            }

            // Infer columns from the value structure
            let columns = infer_columns(value);
            schema_tables.insert(name.clone(), json!({"columns": columns}));

            // Convert value to rows
            let rows = value_to_rows(value);
            tables.insert(name.clone(), rows);
        }

        state.schema = json!({"tables": Value::Object(schema_tables)});
        state.tables = tables;

        debug!("NonNet DB loaded {} tables", state.tables.len());
    }

    /// Update a specific table
    pub async fn update_table(&self, name: &str, rows: Vec<Value>) {
        let mut state = self.state.write().await;
        state.tables.insert(name.to_string(), rows);
    }

    /// Run the JSON-RPC server on a Unix socket
    pub async fn run_server(&self, socket_path: &str) -> Result<()> {
        let path = Path::new(socket_path);

        // Create parent directory if needed
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir).await.ok();
        }

        // Remove existing socket
        if path.exists() {
            tokio::fs::remove_file(path).await.ok();
        }

        let listener = UnixListener::bind(path).context("Failed to bind NonNet socket")?;

        info!("NonNet JSON-RPC server listening on {}", socket_path);

        loop {
            let (stream, _) = listener.accept().await?;
            let state = Arc::clone(&self.state);

            tokio::spawn(async move {
                if let Err(e) = handle_connection(state, stream).await {
                    warn!("NonNet connection error: {}", e);
                }
            });
        }
    }

    /// Handle a single JSON-RPC request
    pub async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let state = self.state.read().await;
        handle_method(&state, request)
    }
}

impl Default for NonNetDb {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle a client connection
async fn handle_connection(state: Arc<RwLock<NonNetState>>, stream: UnixStream) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(value) => {
                let state = state.read().await;
                match serde_json::from_value::<JsonRpcRequest>(value.clone()) {
                    Ok(request) => handle_method(&state, request),
                    Err(e) => JsonRpcResponse::error(
                        value.get("id").cloned().unwrap_or(Value::Null),
                        error_codes::INVALID_REQUEST,
                        format!("Invalid request: {}", e),
                    ),
                }
            }
            Err(e) => JsonRpcResponse::error(
                Value::Null,
                error_codes::PARSE_ERROR,
                format!("Parse error: {}", e),
            ),
        };

        let response_str = serde_json::to_string(&response)?;
        writer.write_all(response_str.as_bytes()).await?;
        writer.write_all(b"\n").await?;

        line.clear();
    }

    Ok(())
}

/// Handle a JSON-RPC method call
fn handle_method(state: &NonNetState, request: JsonRpcRequest) -> JsonRpcResponse {
    let result = match request.method.as_str() {
        "list_dbs" => json!(["OpNonNet"]),

        "get_schema" => state.schema.clone(),

        "transact" => {
            // params: [db, ops...]
            let params = request.params.as_array();
            if let Some(params) = params {
                if params.is_empty() {
                    return JsonRpcResponse::error(
                        request.id,
                        error_codes::INVALID_PARAMS,
                        "Missing database name",
                    );
                }

                let db = params[0].as_str().unwrap_or("");
                if db != "OpNonNet" {
                    return JsonRpcResponse::error(
                        request.id,
                        error_codes::NOT_FOUND,
                        format!("Unknown database: {}", db),
                    );
                }

                // Process operations
                let ops = &params[1..];
                let mut results = Vec::new();

                for op in ops {
                    let op_type = op.get("op").and_then(|v| v.as_str()).unwrap_or("");

                    match op_type {
                        "select" => {
                            let table = op.get("table").and_then(|v| v.as_str()).unwrap_or("");
                            let rows = state.tables.get(table).cloned().unwrap_or_default();
                            results.push(json!({"rows": rows}));
                        }
                        "insert" | "update" | "delete" | "mutate" => {
                            // Read-only database
                            results.push(json!({"error": "Read-only database"}));
                        }
                        _ => {
                            results
                                .push(json!({"error": format!("Unknown operation: {}", op_type)}));
                        }
                    }
                }

                json!(results)
            } else {
                json!({"error": "Invalid params"})
            }
        }

        "echo" => request.params.clone(),

        _ => {
            return JsonRpcResponse::error(
                request.id,
                error_codes::METHOD_NOT_FOUND,
                format!("Unknown method: {}", request.method),
            );
        }
    };

    JsonRpcResponse::success(request.id, result)
}

/// Infer column types from a value
fn infer_columns(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut cols = serde_json::Map::new();
            for (k, v) in map {
                cols.insert(k.clone(), json!({"type": infer_type(v)}));
            }
            Value::Object(cols)
        }
        Value::Array(arr) => {
            if let Some(first) = arr.first() {
                infer_columns(first)
            } else {
                json!({})
            }
        }
        _ => json!({"value": {"type": infer_type(value)}}),
    }
}

/// Infer the type of a value
fn infer_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "integer",
        Value::String(_) => "string",
        Value::Array(_) => "set",
        Value::Object(_) => "map",
    }
}

/// Convert a value to table rows
fn value_to_rows(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(arr) => arr.clone(),
        Value::Object(map) => {
            // Check if there's an array field
            for (_, v) in map {
                if let Value::Array(arr) = v {
                    return arr.clone();
                }
            }
            // Return single row
            vec![value.clone()]
        }
        _ => vec![value.clone()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_nonnet_db_creation() {
        let db = NonNetDb::new();
        let mut plugins = HashMap::new();
        plugins.insert(
            "systemd".to_string(),
            json!({
                "units": ["nginx.service", "ssh.service"]
            }),
        );

        db.load_from_plugins(&plugins).await;

        let request = JsonRpcRequest::new("list_dbs", json!([]));
        let response = db.handle_request(request).await;

        assert!(response.result.is_some());
    }
}
