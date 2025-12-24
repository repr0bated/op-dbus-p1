use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Command;
use uuid::Uuid;
use zbus::{connection::Builder, interface, object_server::SignalEmitter};
use std::io::{self, BufRead, Write};

// Security configuration
const FORBIDDEN_CHARS: &[char] = &[
    '$', '`', ';', '&', '|', '>', '<', '(', ')', '{', '}', '\n', '\r',
];
const MAX_TARGET_LENGTH: usize = 256;
const MAX_COUNT: u32 = 20;

#[derive(Debug, Deserialize, serde::Serialize)]
struct NetworkTask {
    #[serde(rename = "type")]
    task_type: String,
    operation: String, // ping, interfaces, connections, ports, route
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    count: Option<u32>,
}

pub struct NetworkAgent {
    agent_id: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct McpRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct McpResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct McpError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

struct NetworkMcpServer {
    agent: NetworkAgent,
}

impl NetworkMcpServer {
    fn new(agent_id: String) -> Self {
        Self {
            agent: NetworkAgent { agent_id },
        }
    }

    fn get_network_tools() -> Vec<Value> {
        vec![
            json!({
                "name": "network_interfaces",
                "description": "List all network interfaces with their configuration and status",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }),
            json!({
                "name": "network_ping",
                "description": "Ping a target host to test connectivity",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "target": {
                            "type": "string",
                            "description": "Target host to ping (IP address or hostname)"
                        },
                        "count": {
                            "type": "number",
                            "description": "Number of ping packets to send (default: 4, max: 20)",
                            "default": 4,
                            "maximum": 20
                        }
                    },
                    "required": ["target"]
                }
            }),
            json!({
                "name": "network_route_table",
                "description": "Display the current routing table",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }),
            json!({
                "name": "network_connections",
                "description": "Show active network connections",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }),
            json!({
                "name": "network_traceroute",
                "description": "Perform traceroute to a target host",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "target": {
                            "type": "string",
                            "description": "Target host for traceroute (IP address or hostname)"
                        }
                    },
                    "required": ["target"]
                }
            })
        ]
    }

    async fn handle_mcp_request(&self, request: McpRequest) -> Result<McpResponse, Box<dyn std::error::Error>> {
        let response = match request.method.as_str() {
            "initialize" => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {
                            "listChanged": false
                        }
                    },
                    "serverInfo": {
                        "name": "Network Engineer MCP Server (mcpo)",
                        "version": "1.0.0"
                    }
                })),
                error: None,
            },
            "tools/list" => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(json!({
                    "tools": Self::get_network_tools()
                })),
                error: None,
            },
            "tools/call" => {
                let params = request.params.unwrap_or(json!({}));
                let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let default_args = json!({});
                let arguments = params.get("arguments").unwrap_or(&default_args);

                match self.call_tool(tool_name, arguments).await {
                    Ok(result) => McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({ "content": result })),
                        error: None,
                    },
                    Err(e) => McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(McpError {
                            code: -32000,
                            message: format!("Tool execution failed: {}", e),
                            data: None,
                        }),
                    },
                }
            }
            _ => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError {
                    code: -32601,
                    message: "Method not found".to_string(),
                    data: None,
                }),
            },
        };

        Ok(response)
    }

    async fn call_tool(&self, tool_name: &str, arguments: &Value) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let task = match tool_name {
            "network_interfaces" => NetworkTask {
                task_type: "network".to_string(),
                operation: "interfaces".to_string(),
                target: None,
                count: None,
            },
            "network_ping" => NetworkTask {
                task_type: "network".to_string(),
                operation: "ping".to_string(),
                target: arguments.get("target").and_then(|v| v.as_str()).map(|s| s.to_string()),
                count: arguments.get("count").and_then(|v| v.as_u64()).map(|n| n as u32),
            },
            "network_route_table" => NetworkTask {
                task_type: "network".to_string(),
                operation: "route".to_string(),
                target: None,
                count: None,
            },
            "network_connections" => NetworkTask {
                task_type: "network".to_string(),
                operation: "connections".to_string(),
                target: None,
                count: None,
            },
            "network_traceroute" => NetworkTask {
                task_type: "network".to_string(),
                operation: "traceroute".to_string(),
                target: arguments.get("target").and_then(|v| v.as_str()).map(|s| s.to_string()),
                count: None,
            },
            _ => return Err(format!("Unknown tool: {}", tool_name).into()),
        };

        let task_json = serde_json::to_string(&task)?;
        let result = self.agent.execute(task_json).await?;
        Ok(vec![json!({ "type": "text", "text": result })])
    }
}

#[interface(name = "org.dbusmcp.Agent.Network")]
impl NetworkAgent {
    /// Execute a network operation task
    async fn execute(&self, task_json: String) -> zbus::fdo::Result<String> {
        println!("[{}] Received task: {}", self.agent_id, task_json);

        let task: NetworkTask = match serde_json::from_str(&task_json) {
            Ok(t) => t,
            Err(e) => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Failed to parse task: {}",
                    e
                )));
            }
        };

        if task.task_type != "network" {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Unknown task type: {}",
                task.task_type
            )));
        }

        println!("[{}] Network operation: {}", self.agent_id, task.operation);

        let result = match task.operation.as_str() {
            "ping" => self.ping(task.target.as_deref(), task.count),
            "interfaces" => self.list_interfaces(),
            "connections" => self.list_connections(),
            "ports" => self.list_ports(),
            "route" => self.show_routes(),
            "dns" => self.check_dns(),
            _ => Err(format!("Unknown network operation: {}", task.operation)),
        };

        match result {
            Ok(data) => {
                let response = serde_json::json!({
                    "success": true,
                    "operation": task.operation,
                    "data": data,
                });
                Ok(response.to_string())
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e)),
        }
    }

    /// Get agent status
    async fn get_status(&self) -> zbus::fdo::Result<String> {
        Ok(format!("Network agent {} is running", self.agent_id))
    }

    /// Signal emitted when task completes
    #[zbus(signal)]
    async fn task_completed(signal_emitter: &SignalEmitter<'_>, result: String)
        -> zbus::Result<()>;
}

impl NetworkAgent {
    fn validate_target(&self, target: &str) -> Result<(), String> {
        if target.len() > MAX_TARGET_LENGTH {
            return Err(format!(
                "Target exceeds maximum length of {} characters",
                MAX_TARGET_LENGTH
            ));
        }

        if target.trim().is_empty() {
            return Err("Target cannot be empty".to_string());
        }

        for forbidden_char in FORBIDDEN_CHARS {
            if target.contains(*forbidden_char) {
                return Err(format!(
                    "Target contains forbidden character: {:?}",
                    forbidden_char
                ));
            }
        }

        Ok(())
    }

    fn ping(&self, target: Option<&str>, count: Option<u32>) -> Result<String, String> {
        let target = target.ok_or("Target is required for ping operation")?;
        self.validate_target(target)?;
        let count = count.unwrap_or(4).min(MAX_COUNT);

        let output = Command::new("ping")
            .arg("-c")
            .arg(count.to_string())
            .arg(target)
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                if out.status.success() {
                    Ok(stdout.to_string())
                } else {
                    Err(format!("Ping failed: {}", stderr))
                }
            }
            Err(e) => Err(format!("Failed to execute ping: {}", e)),
        }
    }

    fn list_interfaces(&self) -> Result<String, String> {
        let output = Command::new("ip").arg("addr").output();

        match output {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Err(e) => Err(format!("Failed to list interfaces: {}", e)),
        }
    }

    fn list_connections(&self) -> Result<String, String> {
        let output = Command::new("ss").arg("-tuln").output();

        match output {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Err(e) => Err(format!("Failed to list connections: {}", e)),
        }
    }

    fn list_ports(&self) -> Result<String, String> {
        let output = Command::new("ss").arg("-tulnp").output();

        match output {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Err(e) => Err(format!("Failed to list ports: {}", e)),
        }
    }

    fn show_routes(&self) -> Result<String, String> {
        let output = Command::new("ip").arg("route").output();

        match output {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Err(e) => Err(format!("Failed to show routes: {}", e)),
        }
    }

    fn check_dns(&self) -> Result<String, String> {
        let output = Command::new("resolvectl").arg("status").output();

        match output {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Err(e) => {
                // Fallback to old systemd-resolve
                let output_fallback = Command::new("systemd-resolve").arg("--status").output();
                match output_fallback {
                    Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
                    Err(_) => Err(format!("Failed to check DNS: {}", e)),
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Check if we should run as MCP server
    if args.len() > 1 && args[1] == "--mcp" {
        run_mcp_server().await
    } else {
        run_dbus_service(args).await
    }
}

async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    let agent_id = "network-engineer".to_string();
    println!("Starting Network Engineer MCP Server (mcpo): {}", agent_id);

    let server = NetworkMcpServer::new(agent_id.clone());
    println!("Network Engineer MCP Server '{}' ready", agent_id);
    println!("Serving network engineering tools via MCP protocol");
    println!("Available tools: network_interfaces, network_ping, network_route_table, network_connections, network_traceroute");

    // MCP server mode - read from stdin, write to stdout
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<McpRequest>(&line) {
            Ok(request) => {
                match server.handle_mcp_request(request).await {
                    Ok(response) => {
                        let response_json = serde_json::to_string(&response)?;
                        writeln!(stdout, "{}", response_json)?;
                        stdout.flush()?;
                    }
                    Err(e) => {
                        let error_response = McpResponse {
                            jsonrpc: "2.0".to_string(),
                            id: None,
                            result: None,
                            error: Some(McpError {
                                code: -32000,
                                message: format!("Internal error: {}", e),
                                data: None,
                            }),
                        };
                        let error_json = serde_json::to_string(&error_response)?;
                        writeln!(stdout, "{}", error_json)?;
                        stdout.flush()?;
                    }
                }
            }
            Err(e) => {
                let error_response = McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: None,
                    result: None,
                    error: Some(McpError {
                        code: -32700,
                        message: format!("Parse error: {}", e),
                        data: None,
                    }),
                };
                let error_json = serde_json::to_string(&error_response)?;
                writeln!(stdout, "{}", error_json)?;
                stdout.flush()?;
            }
        }
    }

    Ok(())
}

async fn run_dbus_service(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let agent_id = if args.len() > 1 {
        args[1].clone()
    } else {
        format!("network-{}", Uuid::new_v4().to_string()[..8].to_string())
    };

    println!("Starting Network Agent: {}", agent_id);

    let agent = NetworkAgent {
        agent_id: agent_id.clone(),
    };

    let path = format!("/org/dbusmcp/Agent/Network/{}", agent_id.replace('-', "_"));
    let service_name = format!("org.dbusmcp.Agent.Network.{}", agent_id.replace('-', "_"));

    let _conn = Builder::system()?
        .name(service_name.as_str())?
        .serve_at(path.as_str(), agent)?
        .build()
        .await?;

    println!("Network agent {} ready on D-Bus", agent_id);
    println!("Service: {}", service_name);
    println!("Path: {}", path);

    // Keep running
    std::future::pending::<()>().await;

    Ok(())
}
