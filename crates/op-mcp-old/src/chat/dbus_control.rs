// Generic D-Bus → MCP Bridge
// Takes any D-Bus service and exposes it as MCP tools
// Moved to chat module for tighter integration

use crate::introspection_parser;

use introspection_parser::{IntrospectionParser, MethodInfo};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use zbus::Connection;

#[derive(Debug, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Debug, Serialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
}

pub struct DbusMcpBridge {
    service_name: String,
    connection: Connection,
    methods: Vec<MethodInfo>,
    mcp_name: String,
    object_path: String,
    use_system_bus: bool,
}

impl DbusMcpBridge {
    pub async fn new(
        service_name: String,
        use_system_bus: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let connection = if use_system_bus {
            Connection::system().await?
        } else {
            Connection::session().await?
        };

        // Determine the correct D-Bus path for introspection
        // Pattern: org.freedesktop.systemd1 -> /org/freedesktop/systemd1
        let path = if service_name.starts_with("org.") || service_name.starts_with("com.") {
            format!("/{}", service_name.replace('.', "/"))
        } else {
            "/".to_string()
        };

        eprintln!(
            "Introspecting {} at path {} (bus: {})",
            service_name,
            path,
            if use_system_bus { "system" } else { "session" }
        );

        // Introspect using busctl command
        let mut cmd = std::process::Command::new("busctl");
        if use_system_bus {
            cmd.arg("--system");
        } else {
            cmd.arg("--user");
        }

        let output = cmd
            .arg("introspect")
            .arg("--xml-interface")
            .arg(&service_name)
            .arg(&path)
            .output()?;

        let xml = if output.status.success() {
            String::from_utf8_lossy(&output.stdout).to_string()
        } else {
            eprintln!("Warning: Could not introspect {} at {}", service_name, path);
            String::new()
        };

        let data = IntrospectionParser::parse_xml(&xml);

        let methods = if !data.interfaces.is_empty() {
            data.interfaces[0].methods.clone()
        } else {
            Vec::new()
        };

        let mcp_name = Self::service_to_mcp_name(&service_name);

        eprintln!("Bridge initialized for: {}", service_name);
        eprintln!("  MCP name: {}", mcp_name);
        eprintln!("  Object path: {}", path);
        eprintln!("  Methods discovered: {}", methods.len());

        Ok(Self {
            service_name,
            connection,
            methods,
            mcp_name,
            object_path: path,
            use_system_bus,
        })
    }

    fn service_to_mcp_name(service: &str) -> String {
        service
            .replace("org.freedesktop.", "")
            .replace("org.dbusmcp.", "")
            .replace('.', "_")
            .to_lowercase()
    }

    fn method_to_tool_name(&self, method: &str) -> String {
        // Convert CamelCase to snake_case
        let mut result = String::new();
        for (i, c) in method.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        }
        result
    }

    pub async fn handle_request(&self, request: McpRequest) -> McpResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id),
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            _ => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                }),
            },
        }
    }

    fn handle_initialize(&self, id: Option<Value>) -> McpResponse {
        McpResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": format!("dbus-{}", self.mcp_name),
                    "version": "0.1.0"
                }
            })),
            error: None,
        }
    }

    fn handle_tools_list(&self, id: Option<Value>) -> McpResponse {
        let tools: Vec<Value> = self
            .methods
            .iter()
            .map(|method| {
                let tool_name = self.method_to_tool_name(&method.name);

                // Build input schema from method inputs
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();

                for input in &method.inputs {
                    properties.insert(
                        input.name.clone(),
                        json!({
                            "type": IntrospectionParser::dbus_type_to_mcp_schema(&input.type_sig).get("type").unwrap(),
                            "description": format!("{} ({})", input.name, input.type_name)
                        }),
                    );
                    required.push(input.name.clone());
                }

                json!({
                    "name": tool_name,
                    "description": format!("Call D-Bus method: {}.{}", self.service_name, method.name),
                    "inputSchema": {
                        "type": "object",
                        "properties": properties,
                        "required": required
                    }
                })
            })
            .collect();

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "tools": tools
            })),
            error: None,
        }
    }

    async fn handle_tools_call(&self, id: Option<Value>, params: Option<Value>) -> McpResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(McpError {
                        code: -32602,
                        message: "Missing params".to_string(),
                    }),
                };
            }
        };

        let tool_name = params["name"].as_str().unwrap_or("");
        let args = &params["arguments"];

        // Find the method
        let method = self
            .methods
            .iter()
            .find(|m| self.method_to_tool_name(&m.name) == tool_name);

        if method.is_none() {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(McpError {
                    code: -32602,
                    message: format!("Unknown tool: {}", tool_name),
                }),
            };
        }

        let method = method.unwrap();

        // Call D-Bus method
        match self.call_dbus_method(&method.name, args).await {
            Ok(result) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "content": [{
                        "type": "text",
                        "text": format!("D-Bus call successful:\n{}", result)
                    }]
                })),
                error: None,
            },
            Err(e) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(McpError {
                    code: -32603,
                    message: format!("D-Bus call failed: {}", e),
                }),
            },
        }
    }

    async fn call_dbus_method(
        &self,
        method: &str,
        args: &Value,
    ) -> Result<String, Box<dyn std::error::Error>> {
        eprintln!("Calling D-Bus: {}.{}", self.service_name, method);
        eprintln!("  Args: {}", args);

        // Find method info to get interface name
        let method_info = self.methods.iter().find(|m| m.name == method);

        if method_info.is_none() {
            return Err(format!("Method {} not found in introspection data", method).into());
        }

        // Use busctl to call the method
        // busctl --user call <service> <path> <interface> <method> [signature] [args...]
        let mut cmd = std::process::Command::new("busctl");
        if self.use_system_bus {
            cmd.arg("--system");
        } else {
            cmd.arg("--user");
        }
        cmd.arg("call")
            .arg(&self.service_name)
            .arg(&self.object_path);

        // We need to determine the interface from introspection data
        // For now, use a common pattern or make it configurable
        // Most services use their service name as interface prefix
        let interface = if self.service_name.contains("freedesktop") {
            // Common FreeDesktop services
            format!("{}.Manager", self.service_name)
        } else if self.service_name.contains("dbusmcp") {
            // Our own services
            format!(
                "{}.Agent",
                self.service_name
                    .trim_end_matches(char::is_numeric)
                    .trim_end_matches('.')
            )
        } else {
            // Default: use service name as interface
            self.service_name.clone()
        };

        cmd.arg(&interface).arg(method);

        // Convert JSON args to D-Bus signature if provided
        if let Some(obj) = args.as_object() {
            if !obj.is_empty() {
                // Build signature and arguments from method inputs
                let method_info = method_info.unwrap();
                let mut signature = String::new();
                let mut arg_values = Vec::new();

                for input in &method_info.inputs {
                    signature.push_str(&input.type_sig);

                    // Get the argument value from JSON
                    if let Some(val) = obj.get(&input.name) {
                        arg_values.push(Self::json_to_dbus_arg(val, &input.type_sig)?);
                    }
                }

                if !signature.is_empty() {
                    cmd.arg(&signature);
                    for arg_val in arg_values {
                        cmd.arg(arg_val);
                    }
                }
            }
        }

        eprintln!("  Command: {:?}", cmd);

        let output = cmd.output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let mut result = String::new();
            if !stdout.is_empty() {
                result.push_str("Output:\n");
                result.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !result.is_empty() {
                    result.push_str("\n");
                }
                result.push_str("Info:\n");
                result.push_str(&stderr);
            }

            Ok(result)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("D-Bus call failed: {}", stderr).into())
        }
    }

    fn json_to_dbus_arg(
        value: &Value,
        type_sig: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        match type_sig {
            "s" => Ok(value.as_str().unwrap_or("").to_string()),
            "i" | "u" | "x" | "t" => Ok(value.as_i64().unwrap_or(0).to_string()),
            "d" => Ok(value.as_f64().unwrap_or(0.0).to_string()),
            "b" => Ok(if value.as_bool().unwrap_or(false) {
                "true"
            } else {
                "false"
            }
            .to_string()),
            _ => Ok(format!("{}", value)), // Fallback to JSON representation
        }
    }
}
