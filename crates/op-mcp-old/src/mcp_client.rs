//! MCP Client - Connect to and introspect other MCP servers
//! Discovers hosted MCP servers and exposes their tools individually
//! Supports both local process-based servers and remote HTTP/HTTPS servers

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::RwLock;

/// Configuration for a hosted MCP server (local process-based)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedMcpConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub disabled: bool,
}

/// Configuration for a remote HTTP/HTTPS MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteMcpConfig {
    pub name: String,
    pub url: String, // HTTPS URL with port and path, e.g., "https://example.com:8443/mcp"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>, // Optional: Authorization: Bearer YOUR_TOKEN
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>, // Optional: X-API-Key: YOUR_KEY
    #[serde(default)]
    pub disabled: bool,
}

/// Information about a discovered MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedMcpInfo {
    pub name: String,
    pub status: McpServerStatus,
    pub tools: Vec<McpToolInfo>,
    pub server_info: Option<Value>,
    pub capabilities: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum McpServerStatus {
    Connected,
    Disconnected,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub server_name: String, // Which MCP server this tool belongs to
}

/// MCP Client - manages connections to hosted MCP servers (both local and remote)
pub struct McpClient {
    configs: Arc<RwLock<HashMap<String, HostedMcpConfig>>>,
    servers: Arc<RwLock<HashMap<String, HostedMcpServer>>>,
    remote_configs: Arc<RwLock<HashMap<String, RemoteMcpConfig>>>,
    remote_servers: Arc<RwLock<HashMap<String, RemoteMcpServer>>>,
    http_client: Client,
}

/// Active connection to a hosted MCP server
struct HostedMcpServer {
    name: String,
    process: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    request_id: Arc<tokio::sync::Mutex<u64>>,
    pending_requests: Arc<tokio::sync::Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>>,
}

impl HostedMcpServer {
    fn new(name: String) -> Self {
        Self {
            name,
            process: None,
            stdin: None,
            stdout: None,
            request_id: Arc::new(tokio::sync::Mutex::new(0)),
            pending_requests: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    async fn connect(&mut self, config: &HostedMcpConfig) -> Result<()> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        cmd.envs(&config.env);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .context(format!("Failed to spawn MCP server: {}", config.command))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout"))?;

        self.process = Some(child);
        self.stdin = Some(stdin);
        self.stdout = Some(BufReader::new(stdout));

        // Send initialize request
        let init_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "op-dbus-mcp-client",
                    "version": "1.0.0"
                }
            }
        });

        self.send_request(init_request).await?;

        Ok(())
    }

    async fn send_request(&mut self, request: Value) -> Result<Value> {
        let mut req_id = self.request_id.lock().await;
        let id = *req_id;
        *req_id += 1;
        drop(req_id);

        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id, tx);
        }

        // Update request ID in JSON
        let request_obj = request
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Request must be an object"))?
            .clone();
        let mut request_obj = request_obj;
        request_obj.insert("id".to_string(), json!(id));
        let request = json!(request_obj);

        let request_str = serde_json::to_string(&request)?;
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(request_str.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }

        // Read response
        let response =
            tokio::time::timeout(std::time::Duration::from_secs(10), self.read_response())
                .await??;

        {
            let mut pending = self.pending_requests.lock().await;
            pending.remove(&id);
        }

        Ok(response)
    }

    async fn read_response(&mut self) -> Result<Value> {
        if let Some(ref mut stdout) = self.stdout {
            let mut line = String::new();
            stdout.read_line(&mut line).await?;
            let response: Value = serde_json::from_str(&line)?;
            Ok(response)
        } else {
            Err(anyhow::anyhow!("No stdout available"))
        }
    }

    async fn list_tools(&mut self) -> Result<Vec<McpToolInfo>> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {}
        });

        let response = self.send_request(request).await?;

        if let Some(result) = response.get("result") {
            if let Some(tools) = result.get("tools").and_then(|t| t.as_array()) {
                let tool_infos: Result<Vec<McpToolInfo>> = tools
                    .iter()
                    .map(|tool| {
                        Ok(McpToolInfo {
                            name: tool["name"]
                                .as_str()
                                .ok_or_else(|| anyhow::anyhow!("Missing tool name"))?
                                .to_string(),
                            description: tool["description"].as_str().unwrap_or("").to_string(),
                            input_schema: tool["inputSchema"].clone(),
                            server_name: self.name.clone(),
                        })
                    })
                    .collect();

                return tool_infos;
            }
        }

        Ok(vec![])
    }

    async fn disconnect(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill().await;
        }
        self.stdin = None;
        self.stdout = None;
    }
}

impl McpClient {
    pub fn new() -> Self {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
            servers: Arc::new(RwLock::new(HashMap::new())),
            remote_configs: Arc::new(RwLock::new(HashMap::new())),
            remote_servers: Arc::new(RwLock::new(HashMap::new())),
            http_client,
        }
    }

    /// Load MCP server configurations from a JSON file (MCP config format)
    pub async fn load_configs_from_file(&self, path: &str) -> Result<()> {
        let content = std::fs::read_to_string(path)
            .context(format!("Failed to read config file: {}", path))?;

        let config: Value = serde_json::from_str(&content)?;

        if let Some(servers) = config.get("mcpServers").and_then(|s| s.as_object()) {
            let mut configs = self.configs.write().await;

            for (name, server_config) in servers {
                if let Some(disabled) = server_config.get("disabled").and_then(|d| d.as_bool()) {
                    if disabled {
                        continue;
                    }
                }

                let config = HostedMcpConfig {
                    name: name.clone(),
                    command: server_config["command"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Missing command"))?
                        .to_string(),
                    args: server_config["args"]
                        .as_array()
                        .ok_or_else(|| anyhow::anyhow!("Missing args"))?
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect(),
                    env: server_config
                        .get("env")
                        .and_then(|e| e.as_object())
                        .map(|obj| {
                            obj.iter()
                                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                                .collect()
                        })
                        .unwrap_or_default(),
                    disabled: false,
                };

                configs.insert(name.clone(), config);
            }
        }

        Ok(())
    }

    /// Discover and connect to all configured MCP servers
    pub async fn discover_servers(&self) -> Result<Vec<HostedMcpInfo>> {
        let configs = self.configs.read().await.clone();
        let mut servers_info = Vec::new();
        let mut servers = self.servers.write().await;

        for (name, config) in configs {
            let mut server = HostedMcpServer::new(name.clone());

            match server.connect(&config).await {
                Ok(_) => {
                    // Get server info and tools
                    let tools = server.list_tools().await.unwrap_or_default();

                    servers_info.push(HostedMcpInfo {
                        name: name.clone(),
                        status: McpServerStatus::Connected,
                        tools,
                        server_info: None,
                        capabilities: None,
                    });

                    servers.insert(name, server);
                }
                Err(e) => {
                    servers_info.push(HostedMcpInfo {
                        name: name.clone(),
                        status: McpServerStatus::Error(e.to_string()),
                        tools: vec![],
                        server_info: None,
                        capabilities: None,
                    });
                }
            }
        }

        Ok(servers_info)
    }

    /// Get all tools from all connected MCP servers
    pub async fn get_all_tools(&self) -> Vec<McpToolInfo> {
        // Tools are stored in HostedMcpInfo, not in the server connection
        // This method should be called after discover_servers() which returns the tools
        vec![]
    }

    /// Call a tool on a specific hosted MCP server
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value> {
        let mut servers = self.servers.write().await;

        if let Some(server) = servers.get_mut(server_name) {
            let request = json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": tool_name,
                    "arguments": arguments
                }
            });

            server.send_request(request).await
        } else {
            Err(anyhow::anyhow!("Server not found: {}", server_name))
        }
    }

    /// Disconnect from all servers
    pub async fn disconnect_all(&self) {
        let mut servers = self.servers.write().await;
        for (_, mut server) in servers.drain() {
            server.disconnect().await;
        }
    }

    /// Write JSON configurations for popular MCP clients
    /// Each discovered MCP server is exposed individually (not monolithic)
    pub async fn write_client_configs(
        &self,
        base_dir: &str,
        discovered_servers: &[HostedMcpInfo],
    ) -> Result<()> {
        use std::fs;
        use std::path::PathBuf;

        // Popular clients to generate configs for
        let clients = vec![
            ("cursor", "Cursor"),
            ("vscode", "VS Code"),
            ("claude-desktop", "Claude Desktop"),
            ("aider", "Aider"),
        ];

        for (client_id, client_name) in clients {
            let client_dir = PathBuf::from(base_dir).join(client_id);
            fs::create_dir_all(&client_dir)
                .context(format!("Failed to create directory for {}", client_name))?;

            let config_path = client_dir.join("mcp.json");

            // Build config with all discovered servers as individual entries
            let mut mcp_servers = serde_json::Map::new();

            // Add the main op-dbus server
            mcp_servers.insert(
                "operation-dbus".to_string(),
                json!({
                    "command": "target/release/op-dbus",
                    "args": ["mcp"],
                    "env": {},
                    "disabled": false
                }),
            );

            // Add each discovered hosted MCP server individually
            for server_info in discovered_servers {
                if server_info.status == McpServerStatus::Connected {
                    // Get the original config for this server
                    let configs = self.configs.read().await;
                    if let Some(config) = configs.get(&server_info.name) {
                        let mut server_entry = json!({
                            "command": config.command,
                            "args": config.args,
                            "disabled": false
                        });

                        // Add environment variables if present
                        if !config.env.is_empty() {
                            server_entry["env"] = json!(config.env);
                        }

                        mcp_servers.insert(server_info.name.clone(), server_entry);
                    }
                }
            }

            let full_config = json!({
                "mcpServers": mcp_servers
            });

            let json_str = serde_json::to_string_pretty(&full_config)?;
            fs::write(&config_path, json_str)
                .context(format!("Failed to write config for {}", client_name))?;

            eprintln!(
                "✓ Generated {} config: {}",
                client_name,
                config_path.display()
            );
        }

        Ok(())
    }

    /// Write individual tool configurations - one JSON file per tool from each server
    /// This allows clients to see tools individually, not just as monolithic servers
    pub async fn write_individual_tool_configs(
        &self,
        base_dir: &str,
        discovered_servers: &[HostedMcpInfo],
    ) -> Result<()> {
        use std::fs;
        use std::path::PathBuf;

        let tools_dir = PathBuf::from(base_dir).join("tools");
        fs::create_dir_all(&tools_dir).context("Failed to create tools directory")?;

        for server_info in discovered_servers {
            if server_info.status == McpServerStatus::Connected && !server_info.tools.is_empty() {
                let server_tools_dir = tools_dir.join(&server_info.name);
                fs::create_dir_all(&server_tools_dir).context(format!(
                    "Failed to create tools dir for {}",
                    server_info.name
                ))?;

                // Write a config file for each tool
                for tool in &server_info.tools {
                    let tool_config = json!({
                        "server": server_info.name,
                        "tool": {
                            "name": tool.name,
                            "description": tool.description,
                            "inputSchema": tool.input_schema
                        },
                        "config": {
                            "command": self.get_server_command(&server_info.name).await?,
                            "args": self.get_server_args(&server_info.name).await?,
                            "env": self.get_server_env(&server_info.name).await?
                        }
                    });

                    let tool_file = server_tools_dir.join(format!("{}.json", tool.name));
                    let json_str = serde_json::to_string_pretty(&tool_config)?;
                    fs::write(&tool_file, json_str)
                        .context(format!("Failed to write tool config: {}", tool.name))?;
                }

                eprintln!(
                    "✓ Wrote {} tools from {}",
                    server_info.tools.len(),
                    server_info.name
                );
            }
        }

        Ok(())
    }

    async fn get_server_command(&self, server_name: &str) -> Result<String> {
        let configs = self.configs.read().await;
        Ok(configs
            .get(server_name)
            .map(|c| c.command.clone())
            .unwrap_or_default())
    }

    async fn get_server_args(&self, server_name: &str) -> Result<Vec<String>> {
        let configs = self.configs.read().await;
        Ok(configs
            .get(server_name)
            .map(|c| c.args.clone())
            .unwrap_or_default())
    }

    async fn get_server_env(&self, server_name: &str) -> Result<HashMap<String, String>> {
        let configs = self.configs.read().await;
        Ok(configs
            .get(server_name)
            .map(|c| c.env.clone())
            .unwrap_or_default())
    }
}

/// Remote HTTP/HTTPS MCP server connection
struct RemoteMcpServer {
    name: String,
    url: String,
    client: Client,
    bearer_token: Option<String>,
    api_key: Option<String>,
    request_id: Arc<tokio::sync::Mutex<u64>>,
    initialized: Arc<tokio::sync::Mutex<bool>>,
}

impl RemoteMcpServer {
    fn new(name: String, config: &RemoteMcpConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            name,
            url: config.url.clone(),
            client,
            bearer_token: config.bearer_token.clone(),
            api_key: config.api_key.clone(),
            request_id: Arc::new(tokio::sync::Mutex::new(0)),
            initialized: Arc::new(tokio::sync::Mutex::new(false)),
        })
    }

    async fn initialize(&self) -> Result<Value> {
        let mut initialized = self.initialized.lock().await;
        if *initialized {
            return Ok(json!({"initialized": true}));
        }

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "op-dbus-remote-mcp-client",
                    "version": "1.0.0"
                }
            }
        });

        let response = self.send_request(request).await?;
        *initialized = true;
        Ok(response)
    }

    async fn send_request(&self, request: Value) -> Result<Value> {
        let mut req_id = self.request_id.lock().await;
        let id = *req_id;
        *req_id += 1;
        drop(req_id);

        // Update request ID
        let mut request_obj = request
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Request must be an object"))?
            .clone();
        request_obj.insert("id".to_string(), json!(id));
        let request = json!(request_obj);

        // Build request with optional authentication headers
        let mut req_builder = self.client.post(&self.url)
            .json(&request);

        // Add Bearer token if provided (optional)
        if let Some(ref token) = self.bearer_token {
            if !token.is_empty() {
                req_builder = req_builder.header("Authorization", format!("Bearer {}", token));
            }
        }

        // Add API key if provided (optional)
        if let Some(ref key) = self.api_key {
            if !key.is_empty() {
                req_builder = req_builder.header("X-API-Key", key);
            }
        }

        let response = req_builder
            .send()
            .await
            .context("Failed to send HTTP request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "HTTP error: {} {}",
                response.status(),
                response.status().canonical_reason().unwrap_or("Unknown")
            ));
        }

        let response_json: Value = response
            .json()
            .await
            .context("Failed to parse response JSON")?;

        // Check for JSON-RPC error
        if let Some(error) = response_json.get("error") {
            let error_msg = error.get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("MCP error: {}", error_msg));
        }

        Ok(response_json.get("result").cloned().unwrap_or(json!({})))
    }

    async fn list_tools(&self) -> Result<Vec<McpToolInfo>> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {}
        });

        let response = self.send_request(request).await?;

        if let Some(tools) = response.get("tools").and_then(|t| t.as_array()) {
            let tool_infos: Result<Vec<McpToolInfo>> = tools
                .iter()
                .map(|tool| {
                    Ok(McpToolInfo {
                        name: tool["name"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("Missing tool name"))?
                            .to_string(),
                        description: tool["description"].as_str().unwrap_or("").to_string(),
                        input_schema: tool.get("inputSchema").cloned().unwrap_or(json!({})),
                        server_name: self.name.clone(),
                    })
                })
                .collect();

            return tool_infos;
        }

        Ok(vec![])
    }

    async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        self.send_request(request).await
    }
}

impl McpClient {
    /// Configure a remote HTTP/HTTPS MCP server
    pub async fn configure_remote_server(&self, config: RemoteMcpConfig) -> Result<()> {
        let mut remote_configs = self.remote_configs.write().await;
        remote_configs.insert(config.name.clone(), config.clone());
        drop(remote_configs);

        // Create and initialize remote server connection
        let server = RemoteMcpServer::new(config.name.clone(), &config)?;
        server.initialize().await?;

        let mut remote_servers = self.remote_servers.write().await;
        remote_servers.insert(config.name.clone(), server);

        Ok(())
    }

    /// Get all remote server configurations
    pub async fn get_remote_configs(&self) -> Vec<RemoteMcpConfig> {
        let configs = self.remote_configs.read().await;
        configs.values().cloned().collect()
    }

    /// Remove a remote server configuration
    pub async fn remove_remote_server(&self, name: &str) {
        let mut configs = self.remote_configs.write().await;
        configs.remove(name);
        let mut servers = self.remote_servers.write().await;
        servers.remove(name);
    }

    /// List tools from a remote server
    pub async fn list_remote_tools(&self, server_name: &str) -> Result<Vec<McpToolInfo>> {
        let servers = self.remote_servers.read().await;
        if let Some(server) = servers.get(server_name) {
            server.list_tools().await
        } else {
            Err(anyhow::anyhow!("Remote server not found: {}", server_name))
        }
    }

    /// Call a tool on a remote server
    pub async fn call_remote_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value> {
        let servers = self.remote_servers.read().await;
        if let Some(server) = servers.get(server_name) {
            server.call_tool(tool_name, arguments).await
        } else {
            Err(anyhow::anyhow!("Remote server not found: {}", server_name))
        }
    }
}

impl Default for McpClient {
    fn default() -> Self {
        Self::new()
    }
}
