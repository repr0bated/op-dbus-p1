//! Tool Registry - Unified Introspection Point for MCP
//!
//! # Purpose
//! Provides dynamic tool management and consolidated system introspection.
//! Eliminates tight coupling by allowing tools to be registered dynamically
//! without hardcoding in the main MCP server.
//!
//! # Unified Introspection
//! The `get_introspection()` method returns a comprehensive view including:
//! - **Native Tools**: All registered MCP tools (system, network, file operations)
//! - **Plugin-Derived Tools**: Auto-converted StatePlugins (systemd, network, lxc, keyring, etc.)
//!   Each plugin becomes three tools:
//!   - `plugin_{name}_query` - Query current state
//!   - `plugin_{name}_diff` - Calculate state diff
//!   - `plugin_{name}_apply` - Apply state changes
//! - **Workflows**: code_review, test_generation, deployment workflows
//! - **State Plugins**: Metadata about plugin capabilities (rollback, checkpoints, etc.)
//!
//! This consolidation means AI agents and web UIs only need to call:
//! ```no_run
//! let introspection = tool_registry.get_introspection().await;
//! // Contains everything needed to understand system capabilities
//! ```
//!
//! Previously would require:
//! - Querying D-Bus introspection cache separately
//! - Checking plugin registry separately
//! - Checking workflow manager separately
//!
//! Now it's all in one place.

use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Tool trait that all MCP tools must implement
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool name
    fn name(&self) -> &str;

    /// Get tool description
    fn description(&self) -> &str;

    /// Get the JSON schema for input validation
    fn input_schema(&self) -> Value;

    /// Execute the tool with given parameters
    async fn execute(&self, params: Value) -> Result<ToolResult>;

    /// Validate parameters before execution
    async fn validate(&self, _params: &Value) -> Result<()> {
        // Default implementation - can be overridden
        Ok(())
    }

    /// Get tool metadata
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: "general".to_string(),
            tags: vec![],
            author: None,
            version: "1.0.0".to_string(),
            security_level: SecurityLevel::Low, // Default to low security
            requires_auth: false, // Default to no auth required
        }
    }
}

/// Result from tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl ToolResult {
    /// Create a success result with a single content item
    pub fn success(content: ToolContent) -> Self {
        Self {
            content: vec![content],
            metadata: None,
        }
    }

    /// Create a success result with multiple content items
    pub fn success_multi(content: Vec<ToolContent>) -> Self {
        Self {
            content,
            metadata: None,
        }
    }

    /// Create an error result
    pub fn error(message: &str) -> Self {
        Self {
            content: vec![ToolContent::error(message)],
            metadata: None,
        }
    }

    /// Add metadata to the result
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
    pub data: Option<Value>,
}

impl ToolContent {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content_type: "text".to_string(),
            text: Some(text.into()),
            data: None,
        }
    }

    pub fn json(data: Value) -> Self {
        Self {
            content_type: "json".to_string(),
            text: None,
            data: Some(data),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content_type: "error".to_string(),
            text: Some(message.into()),
            data: None,
        }
    }
}

/// Security level for tool operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecurityLevel {
    /// Basic read-only operations (file reading, status checks)
    Low,
    /// Standard operations (service status, network info)
    Medium,
    /// Sensitive operations (service management, network config)
    High,
    /// Critical system operations (system shutdown, firewall rules)
    Critical,
}

/// Tool metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub author: Option<String>,
    pub version: String,
    pub security_level: SecurityLevel,
    pub requires_auth: bool,
}

/// System summary for AI analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSummary {
    pub timestamp: u64,
    pub services: Vec<ServiceStatus>,
    pub network_interfaces: Vec<NetworkInterface>,
    pub system_load: SystemLoad,
    pub available_tools: Vec<String>,
    pub running_agents: Vec<String>,
}

/// Service status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub status: String, // "running", "stopped", "failed", etc.
    pub description: Option<String>,
    pub pid: Option<u32>,
    pub memory_usage: Option<u64>, // in bytes
}

/// Network interface information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub ip_addresses: Vec<String>,
    pub mac_address: Option<String>,
    pub status: String, // "up", "down"
    pub rx_bytes: Option<u64>,
    pub tx_bytes: Option<u64>,
}

/// System load information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemLoad {
    pub cpu_usage: f64, // percentage
    pub memory_usage: f64, // percentage
    pub disk_usage: f64, // percentage
    pub uptime_seconds: u64,
}

/// Detailed service information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub status: String,
    pub description: Option<String>,
    pub unit_file: Option<String>,
    pub loaded: bool,
    pub active: bool,
    pub sub_state: Option<String>,
    pub pid: Option<u32>,
    pub memory_usage: Option<u64>,
    pub cpu_usage: Option<f64>,
    pub start_time: Option<u64>,
    pub logs: Vec<String>, // recent log entries
}

/// Tool factory for creating tool instances
#[async_trait]
pub trait ToolFactory: Send + Sync {
    /// Create a new tool instance
    async fn create_tool(&self) -> Result<Box<dyn Tool>>;

    /// Get the tool name this factory creates
    fn tool_name(&self) -> &str;
}

/// Tool registry for managing all tools
pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, Arc<Box<dyn Tool>>>>>,
    factories: Arc<RwLock<HashMap<String, Box<dyn ToolFactory>>>>,
    categories: Arc<RwLock<HashMap<String, Vec<String>>>>,
    middleware: Arc<RwLock<Vec<Box<dyn ToolMiddleware>>>>,
}

/// Security context for authentication
#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub authenticated: bool,
    pub permissions: Vec<String>,
}

/// Middleware for tool execution
#[async_trait]
pub trait ToolMiddleware: Send + Sync {
    /// Called before tool execution
    async fn before_execute(&self, tool_name: &str, params: &Value) -> Result<()>;

    /// Called after tool execution
    async fn after_execute(&self, tool_name: &str, params: &Value, result: &Result<ToolResult>);
}

/// Security validation middleware
pub struct SecurityMiddleware {
    security_context: Arc<RwLock<SecurityContext>>,
}

impl SecurityMiddleware {
    pub fn new() -> Self {
        Self {
            security_context: Arc::new(RwLock::new(SecurityContext {
                user_id: None,
                session_id: None,
                authenticated: false,
                permissions: vec![],
            })),
        }
    }

    pub async fn set_security_context(&self, context: SecurityContext) {
        let mut ctx = self.security_context.write().await;
        *ctx = context;
    }

    pub async fn get_security_context(&self) -> SecurityContext {
        self.security_context.read().await.clone()
    }
}

#[async_trait]
impl ToolMiddleware for SecurityMiddleware {
    async fn before_execute(&self, tool_name: &str, params: &Value) -> Result<()> {
        let ctx = self.security_context.read().await;

        // Get tool metadata (we need to find the tool to get its metadata)
        // For now, use basic security checks based on tool name
        let security_level = self.infer_security_level(tool_name);

        // Check authentication requirements
        match security_level {
            SecurityLevel::Low => {
                // No special requirements for low security operations
            }
            SecurityLevel::Medium => {
                if !ctx.authenticated {
                    return Err(anyhow::anyhow!("Authentication required for medium security operation: {}", tool_name));
                }
            }
            SecurityLevel::High => {
                if !ctx.authenticated {
                    return Err(anyhow::anyhow!("Authentication required for high security operation: {}", tool_name));
                }
                // Check for admin permission
                if !ctx.permissions.contains(&"admin".to_string()) {
                    return Err(anyhow::anyhow!("Admin permission required for high security operation: {}", tool_name));
                }
            }
            SecurityLevel::Critical => {
                if !ctx.authenticated {
                    return Err(anyhow::anyhow!("Authentication required for critical security operation: {}", tool_name));
                }
                // Check for super-admin permission
                if !ctx.permissions.contains(&"super-admin".to_string()) {
                    return Err(anyhow::anyhow!("Super-admin permission required for critical operation: {}", tool_name));
                }
                // Additional validation for critical operations
                self.validate_critical_operation(tool_name, params)?;
            }
        }

        log::info!("Security check passed for tool '{}' at level {:?}", tool_name, security_level);
        Ok(())
    }

    async fn after_execute(&self, tool_name: &str, _params: &Value, result: &Result<ToolResult>) {
        let security_level = self.infer_security_level(tool_name);

        // Enhanced audit logging for sensitive operations
        if security_level >= SecurityLevel::High {
            let ctx = self.security_context.read().await;
            let success = result.is_ok();
            let error_msg = result.as_ref().err().map(|e| e.to_string());

            log::warn!(
                "AUDIT: Tool '{}' executed by user {:?}, level {:?}, success: {}, error: {:?}",
                tool_name, ctx.user_id, security_level, success, error_msg
            );
        }
    }
}

impl SecurityMiddleware {
    fn infer_security_level(&self, tool_name: &str) -> SecurityLevel {
        match tool_name {
            // Low security - read-only operations
            "file_read" | "network_interfaces" | "process_list" => SecurityLevel::Low,

            // Medium security - status checks
            "systemd_status" => SecurityLevel::Medium,

            // High security - system management
            "create_ovs_bridge" | "systemd_control" | "network_config" => SecurityLevel::High,

            // Critical security - system-altering operations
            "exec_command" | "system_shutdown" | "firewall_rules" => SecurityLevel::Critical,

            // Default to medium for unknown tools
            _ => SecurityLevel::Medium,
        }
    }

    fn validate_critical_operation(&self, tool_name: &str, params: &Value) -> Result<()> {
        match tool_name {
            "exec_command" => {
                // Validate command is in whitelist
                if let Some(command) = params.get("command").and_then(|c| c.as_str()) {
                    let allowed_commands = ["systemctl", "journalctl", "ip", "ovs-vsctl"];
                    if !allowed_commands.contains(&command) {
                        return Err(anyhow::anyhow!("Command '{}' not in allowed list for critical operations", command));
                    }
                }
            }
            "system_shutdown" => {
                // Require explicit confirmation parameter
                if !params.get("confirmed").and_then(|c| c.as_bool()).unwrap_or(false) {
                    return Err(anyhow::anyhow!("System shutdown requires explicit confirmation"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            factories: Arc::new(RwLock::new(HashMap::new())),
            categories: Arc::new(RwLock::new(HashMap::new())),
            middleware: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a tool instance
    pub async fn register_tool(&self, tool: Box<dyn Tool>) -> Result<()> {
        let name = tool.name().to_string();
        let metadata = tool.metadata();

        let mut tools = self.tools.write().await;
        if tools.contains_key(&name) {
            bail!("Tool '{}' is already registered", name);
        }

        tools.insert(name.clone(), Arc::new(tool));

        // Add to category
        let mut categories = self.categories.write().await;
        categories
            .entry(metadata.category)
            .or_insert_with(Vec::new)
            .push(name);

        Ok(())
    }

    /// Register a tool factory
    pub async fn register_factory(&self, factory: Box<dyn ToolFactory>) -> Result<()> {
        let name = factory.tool_name().to_string();

        let mut factories = self.factories.write().await;
        if factories.contains_key(&name) {
            bail!("Tool factory '{}' is already registered", name);
        }

        factories.insert(name, factory);
        Ok(())
    }

    /// Register middleware
    pub async fn add_middleware(&self, middleware: Box<dyn ToolMiddleware>) {
        let mut middlewares = self.middleware.write().await;
        middlewares.push(middleware);
    }

    /// Get a tool by name
    pub async fn get_tool(&self, name: &str) -> Option<Arc<Box<dyn Tool>>> {
        let tools = self.tools.read().await;
        tools.get(name).cloned()
    }

    /// Execute a tool
    pub async fn execute_tool(&self, name: &str, params: Value) -> Result<ToolResult> {
        // Try to get existing tool
        let tool = if let Some(tool) = self.get_tool(name).await {
            tool
        } else {
            // Try to create from factory
            let factories = self.factories.read().await;
            if let Some(factory) = factories.get(name) {
                let new_tool = factory.create_tool().await?;
                drop(factories);

                // Register the new tool
                self.register_tool(new_tool).await?;
                self.get_tool(name)
                    .await
                    .ok_or_else(|| anyhow::anyhow!("Failed to register tool"))?
            } else {
                bail!("Tool '{}' not found", name);
            }
        };

        // Call before middleware
        let middlewares = self.middleware.read().await;
        for mw in middlewares.iter() {
            mw.before_execute(name, &params).await?;
        }

        // Validate parameters
        tool.validate(&params).await?;

        // Execute tool
        let result = tool.execute(params.clone()).await;

        // Call after middleware
        for mw in middlewares.iter() {
            mw.after_execute(name, &params, &result).await;
        }

        result
    }

    /// List all registered tools
    pub async fn list_tools(&self) -> Vec<ToolInfo> {
        let tools = self.tools.read().await;
        tools
            .values()
            .map(|tool| ToolInfo {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
                metadata: tool.metadata(),
            })
            .collect()
    }

    /// List tools by category
    pub async fn list_tools_by_category(&self, category: &str) -> Vec<String> {
        let categories = self.categories.read().await;
        categories.get(category).cloned().unwrap_or_default()
    }

    /// Get all categories
    pub async fn list_categories(&self) -> Vec<String> {
        let categories = self.categories.read().await;
        categories.keys().cloned().collect()
    }

    /// Get comprehensive unified introspection including tools, workflows, and plugins
    /// This is the single introspection point for all system capabilities (tools + state plugins + workflows)
    pub async fn get_introspection(&self) -> Value {
        let tools = self.list_tools().await;
        let categories = self.list_categories().await;

        let tools_json: Vec<Value> = tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "schema": tool.input_schema,
                    "security_level": tool.metadata.security_level,
                    "requires_auth": tool.metadata.requires_auth,
                    "category": tool.metadata.category,
                })
            })
            .collect();

        // Get workflow and plugin introspection from the dedicated module
        // This ensures workflows and plugins are always included in the unified view
        #[cfg(any(feature = "mcp", feature = "web"))]
        let (workflows, plugins) = {
            use crate::workflow_plugin_introspection::WorkflowPluginIntrospection;
            let wp = WorkflowPluginIntrospection::new();
            (
                Some(serde_json::json!(wp.workflows)),
                Some(serde_json::json!(wp.plugins)),
            )
        };

        #[cfg(not(any(feature = "mcp", feature = "web")))]
        let (workflows, plugins) = (None, None);

        // Build the unified introspection response
        let mut introspection = serde_json::json!({
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            "type": "unified_system_introspection",
            "total_tools": tools.len(),
            "categories": categories,
            "tools": tools_json,
        });

        // Add workflows if available
        if let Some(w) = workflows {
            introspection["workflows"] = w;
        }

        // Add plugins if available
        if let Some(p) = plugins {
            introspection["state_plugins"] = p;
        }

        introspection
    }
}

/// Tool information for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    pub metadata: ToolMetadata,
}

/// Example logging middleware
pub struct LoggingMiddleware;

#[async_trait]
impl ToolMiddleware for LoggingMiddleware {
    async fn before_execute(&self, tool_name: &str, params: &Value) -> Result<()> {
        log::info!("Executing tool '{}' with params: {:?}", tool_name, params);
        Ok(())
    }

    async fn after_execute(&self, tool_name: &str, _params: &Value, result: &Result<ToolResult>) {
        match result {
            Ok(_) => log::info!("Tool '{}' executed successfully", tool_name),
            Err(e) => log::error!("Tool '{}' failed: {}", tool_name, e),
        }
    }
}

/// Example audit middleware
pub struct AuditMiddleware {
    audit_log: Arc<RwLock<Vec<AuditEntry>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub tool_name: String,
    pub params: Value,
    pub success: bool,
    pub error: Option<String>,
}

impl AuditMiddleware {
    pub fn new() -> Self {
        Self {
            audit_log: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn get_audit_log(&self) -> Vec<AuditEntry> {
        let log = self.audit_log.read().await;
        log.clone()
    }
}

#[async_trait]
impl ToolMiddleware for AuditMiddleware {
    async fn before_execute(&self, _tool_name: &str, _params: &Value) -> Result<()> {
        Ok(())
    }

    async fn after_execute(&self, tool_name: &str, params: &Value, result: &Result<ToolResult>) {
        let entry = AuditEntry {
            timestamp: chrono::Utc::now(),
            tool_name: tool_name.to_string(),
            params: params.clone(),
            success: result.is_ok(),
            error: result.as_ref().err().map(|e| e.to_string()),
        };

        let mut log = self.audit_log.write().await;
        log.push(entry);

        // Keep only last 1000 entries
        if log.len() > 1000 {
            let drain_to = log.len().saturating_sub(1000);
            log.drain(0..drain_to);
        }
    }
}

/// Helper macro to implement tools
#[macro_export]
macro_rules! impl_tool {
    ($name:ident, $tool_name:expr, $description:expr, $schema:expr) => {
        #[async_trait]
        impl Tool for $name {
            fn name(&self) -> &str {
                $tool_name
            }

            fn description(&self) -> &str {
                $description
            }

            fn input_schema(&self) -> Value {
                $schema
            }

            async fn execute(&self, params: Value) -> Result<ToolResult> {
                self.execute_impl(params).await
            }
        }
    };
}

/// Example tool implementation
pub struct SystemdStatusTool;

impl SystemdStatusTool {
    async fn execute_impl(&self, params: Value) -> Result<ToolResult> {
        let service = params["service"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'service' parameter"))?;

        // In real implementation, would query systemd
        let status = format!("Service '{}' is running", service);

        Ok(ToolResult {
            content: vec![ToolContent::text(status)],
            metadata: None,
        })
    }
}

impl_tool!(
    SystemdStatusTool,
    "systemd_status",
    "Get the status of a systemd service",
    json!({
        "type": "object",
        "properties": {
            "service": {
                "type": "string",
                "description": "Name of the systemd service"
            }
        },
        "required": ["service"]
    })
);

use std::future::Future;
/// Dynamic tool builder for runtime tool creation
use std::pin::Pin;

pub struct DynamicToolBuilder {
    name: String,
    description: String,
    schema: Value,
    security_level: SecurityLevel,
    requires_auth: bool,
    handler: Arc<
        dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<ToolResult>> + Send>> + Send + Sync,
    >,
}

impl DynamicToolBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            schema: json!({}),
            security_level: SecurityLevel::Low, // Default to low security
            requires_auth: false, // Default to no auth required
            handler: Arc::new(|_| {
                Box::pin(async {
                    Ok(ToolResult {
                        content: vec![ToolContent::text("Not implemented")],
                        metadata: None,
                    })
                })
            }),
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn schema(mut self, schema: Value) -> Self {
        self.schema = schema;
        self
    }

    pub fn security_level(mut self, level: SecurityLevel) -> Self {
        self.security_level = level;
        self
    }

    pub fn requires_auth(mut self, requires: bool) -> Self {
        self.requires_auth = requires;
        self
    }

    pub fn handler<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ToolResult>> + Send + 'static,
    {
        self.handler = Arc::new(move |params| Box::pin(handler(params)));
        self
    }

    pub fn build(self) -> DynamicTool {
        let metadata = ToolMetadata {
            name: self.name.clone(),
            description: self.description.clone(),
            category: "dynamic".to_string(),
            tags: vec![],
            author: None,
            version: "1.0.0".to_string(),
            security_level: self.security_level,
            requires_auth: self.requires_auth,
        };

        DynamicTool {
            name: self.name,
            description: self.description,
            schema: self.schema,
            metadata,
            handler: self.handler,
        }
    }
}

pub struct DynamicTool {
    name: String,
    description: String,
    schema: Value,
    metadata: ToolMetadata,
    handler: Arc<
        dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<ToolResult>> + Send>> + Send + Sync,
    >,
}

#[async_trait]
impl Tool for DynamicTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        self.schema.clone()
    }

    fn metadata(&self) -> ToolMetadata {
        self.metadata.clone()
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        (self.handler)(params).await
    }
}

impl DynamicTool {
    /// Get mutable reference to metadata (for security level configuration)
    pub fn metadata_mut(&mut self) -> &mut ToolMetadata {
        &mut self.metadata
    }
}

/// Tool Registry Service - provides tool management for MCP servers
pub struct ToolRegistryService {
    registry: Arc<ToolRegistry>,
}

impl ToolRegistryService {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    /// Get system summary for AI analysis (stub - introspection handled separately)
    pub async fn get_introspection_summary(&self) -> Result<SystemSummary> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Get service statuses (simplified - would query systemd in real implementation)
        let services = vec![
            ServiceStatus {
                name: "dbus-mcp".to_string(),
                status: "running".to_string(),
                description: Some("D-Bus MCP Server".to_string()),
                pid: Some(1234),
                memory_usage: Some(50 * 1024 * 1024), // 50MB
            },
            ServiceStatus {
                name: "systemd-networkd".to_string(),
                status: "running".to_string(),
                description: Some("Network service daemon".to_string()),
                pid: Some(567),
                memory_usage: Some(20 * 1024 * 1024), // 20MB
            },
        ];

        // Get network interfaces (simplified - would query netlink in real implementation)
        let network_interfaces = vec![
            NetworkInterface {
                name: "eth0".to_string(),
                ip_addresses: vec!["192.168.1.100/24".to_string()],
                mac_address: Some("aa:bb:cc:dd:ee:ff".to_string()),
                status: "up".to_string(),
                rx_bytes: Some(1_000_000),
                tx_bytes: Some(500_000),
            },
            NetworkInterface {
                name: "lo".to_string(),
                ip_addresses: vec!["127.0.0.1/8".to_string()],
                mac_address: None,
                status: "up".to_string(),
                rx_bytes: Some(100_000),
                tx_bytes: Some(100_000),
            },
        ];

        // Get system load (simplified - would query /proc in real implementation)
        let system_load = SystemLoad {
            cpu_usage: 15.5,
            memory_usage: 45.2,
            disk_usage: 67.8,
            uptime_seconds: 86400, // 1 day
        };

        // Get available tools
        let available_tools = self.registry.list_tools().await
            .into_iter()
            .map(|tool| tool.name)
            .collect();

        // Running agents (would be populated from agent registry in real implementation)
        let running_agents = vec!["rust_pro".to_string(), "network_monitor".to_string()];

        Ok(SystemSummary {
            timestamp,
            services,
            network_interfaces,
            system_load,
            available_tools,
            running_agents,
        })
    }

    /// Get detailed service information
    pub async fn get_service_details(&self, service_name: &str) -> Result<ServiceInfo> {
        // In real implementation, this would query systemd for detailed service info
        // For now, return mock data based on service name
        match service_name {
            "dbus-mcp" => Ok(ServiceInfo {
                name: "dbus-mcp".to_string(),
                status: "running".to_string(),
                description: Some("D-Bus MCP Server for system orchestration".to_string()),
                unit_file: Some("/usr/lib/systemd/system/dbus-mcp.service".to_string()),
                loaded: true,
                active: true,
                sub_state: Some("running".to_string()),
                pid: Some(1234),
                memory_usage: Some(50 * 1024 * 1024),
                cpu_usage: Some(2.1),
                start_time: Some(1609459200), // 2021-01-01
                logs: vec![
                    "[INFO] Service started".to_string(),
                    "[INFO] Connected to D-Bus".to_string(),
                    "[INFO] Tool registry initialized".to_string(),
                ],
            }),
            "systemd-networkd" => Ok(ServiceInfo {
                name: "systemd-networkd".to_string(),
                status: "running".to_string(),
                description: Some("Network service daemon".to_string()),
                unit_file: Some("/usr/lib/systemd/system/systemd-networkd.service".to_string()),
                loaded: true,
                active: true,
                sub_state: Some("running".to_string()),
                pid: Some(567),
                memory_usage: Some(20 * 1024 * 1024),
                cpu_usage: Some(1.5),
                start_time: Some(1609459200),
                logs: vec![
                    "[INFO] Network configuration loaded".to_string(),
                    "[INFO] Interface eth0 configured".to_string(),
                ],
            }),
            _ => Err(anyhow::anyhow!("Service '{}' not found", service_name)),
        }
    }

    /// Get access to the underlying tool registry
    pub fn registry(&self) -> &Arc<ToolRegistry> {
        &self.registry
    }
}
