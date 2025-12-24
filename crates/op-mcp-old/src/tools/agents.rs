//! MCP Tools for Embedded Agent Access
//! Exposes agent capabilities directly as MCP tools for chatbot access

use crate::tool_registry::{DynamicToolBuilder, ToolContent, ToolResult};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use zbus::Connection;

/// Register all agent tools with the tool registry
pub async fn register_agent_tools(registry: &crate::tool_registry::ToolRegistry) -> Result<()> {
    // Executor Agent Tool
    register_executor_tool(registry).await?;
    
    // Systemd Agent Tool
    register_systemd_tool(registry).await?;
    
    // File Agent Tool
    register_file_tool(registry).await?;
    
    // Network Agent Tool
    register_network_tool(registry).await?;
    
    // PackageKit Agent Tool
    register_packagekit_tool(registry).await?;
    
    // Monitor Agent Tool
    register_monitor_tool(registry).await?;
    
    // Orchestrator: Spawn agents
    register_spawn_agent_tool(registry).await?;
    
    Ok(())
}

/// Executor Agent: Execute whitelisted commands
async fn register_executor_tool(registry: &crate::tool_registry::ToolRegistry) -> Result<()> {
    let tool = DynamicToolBuilder::new("agent_executor_execute")
        .description("Execute a whitelisted command via executor agent")
        .schema(json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Command to execute (must be in whitelist)"
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Command arguments"
                },
                "timeout": {
                    "type": "number",
                    "description": "Timeout in seconds (default: 30, max: 300)"
                }
            },
            "required": ["command"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let command = params["command"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing command parameter"))?
                    .to_string();
                
                let args: Vec<String> = params["args"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                
                let timeout = params["timeout"]
                    .as_u64()
                    .unwrap_or(30)
                    .min(300);
                
                // Call executor agent via D-Bus
                let result = call_agent_dbus(
                    "org.dbusmcp.Agent.Executor",
                    "/org/dbusmcp/Agent/Executor",
                    "Execute",
                    json!({
                        "type": "execute",
                        "command": command,
                        "args": args,
                        "timeout": timeout
                    }),
                ).await?;
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(result)],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Systemd Agent: Manage systemd services
async fn register_systemd_tool(registry: &crate::tool_registry::ToolRegistry) -> Result<()> {
    let tool = DynamicToolBuilder::new("agent_systemd_manage")
        .description("Manage systemd services via systemd agent (start, stop, restart, status, enable, disable)")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {
                    "type": "string",
                    "description": "Systemd service name (e.g., nginx.service)"
                },
                "action": {
                    "type": "string",
                    "enum": ["start", "stop", "restart", "status", "enable", "disable", "is-active", "is-enabled"],
                    "description": "Action to perform on the service"
                }
            },
            "required": ["service", "action"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing service parameter"))?
                    .to_string();
                
                let action = params["action"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing action parameter"))?
                    .to_string();
                
                // Call systemd agent via D-Bus
                let result = call_agent_dbus(
                    "org.dbusmcp.Agent.Systemd",
                    "/org/dbusmcp/Agent/Systemd",
                    "Execute",
                    json!({
                        "type": "systemd",
                        "service": service,
                        "action": action
                    }),
                ).await?;
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(result)],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// File Agent: File operations
async fn register_file_tool(registry: &crate::tool_registry::ToolRegistry) -> Result<()> {
    let tool = DynamicToolBuilder::new("agent_file_operation")
        .description("Perform file operations via file agent (read, write, delete, list)")
        .schema(json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["read", "write", "delete", "list", "stat"],
                    "description": "File operation to perform"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write (for write operation)"
                }
            },
            "required": ["operation", "path"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let operation = params["operation"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing operation parameter"))?
                    .to_string();
                
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing path parameter"))?
                    .to_string();
                
                let mut task = json!({
                    "type": "file",
                    "operation": operation,
                    "path": path
                });
                
                if let Some(content) = params.get("content") {
                    task["content"] = content.clone();
                }
                
                // Call file agent via D-Bus
                let result = call_agent_dbus(
                    "org.dbusmcp.Agent.File",
                    "/org/dbusmcp/Agent/File",
                    "Execute",
                    task,
                ).await?;
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(result)],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Network Agent: Network operations
async fn register_network_tool(registry: &crate::tool_registry::ToolRegistry) -> Result<()> {
    let tool = DynamicToolBuilder::new("agent_network_operation")
        .description("Perform network operations via network agent (list interfaces, get status, configure)")
        .schema(json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["list", "status", "info", "configure"],
                    "description": "Network operation to perform"
                },
                "interface": {
                    "type": "string",
                    "description": "Network interface name (optional, for specific operations)"
                }
            },
            "required": ["operation"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let operation = params["operation"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing operation parameter"))?
                    .to_string();
                
                let mut task = json!({
                    "type": "network",
                    "operation": operation
                });
                
                if let Some(interface) = params.get("interface") {
                    task["interface"] = interface.clone();
                }
                
                // Call network agent via D-Bus
                let result = call_agent_dbus(
                    "org.dbusmcp.Agent.Network",
                    "/org/dbusmcp/Agent/Network",
                    "Execute",
                    task,
                ).await?;
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(result)],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// PackageKit Agent: Package management
async fn register_packagekit_tool(registry: &crate::tool_registry::ToolRegistry) -> Result<()> {
    let tool = DynamicToolBuilder::new("agent_packagekit_manage")
        .description("Manage packages via PackageKit agent (install, remove, update, search, get-details)")
        .schema(json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["install", "remove", "update", "search", "get-details", "list-installed"],
                    "description": "Package operation"
                },
                "package": {
                    "type": "string",
                    "description": "Package name"
                }
            },
            "required": ["operation"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let operation = params["operation"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing operation parameter"))?
                    .to_string();
                
                let mut task = json!({
                    "type": "packagekit",
                    "operation": operation
                });
                
                if let Some(package) = params.get("package") {
                    task["package"] = package.clone();
                }
                
                // Call packagekit agent via D-Bus
                let result = call_agent_dbus(
                    "org.dbusmcp.Agent.PackageKit",
                    "/org/dbusmcp/Agent/PackageKit",
                    "Execute",
                    task,
                ).await?;
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(result)],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Monitor Agent: System monitoring
async fn register_monitor_tool(registry: &crate::tool_registry::ToolRegistry) -> Result<()> {
    let tool = DynamicToolBuilder::new("agent_monitor_metrics")
        .description("Get system metrics via monitor agent (cpu, memory, disk, network, processes)")
        .schema(json!({
            "type": "object",
            "properties": {
                "metric": {
                    "type": "string",
                    "enum": ["cpu", "memory", "disk", "network", "processes", "all"],
                    "description": "Metric to retrieve"
                }
            },
            "required": ["metric"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let metric = params["metric"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing metric parameter"))?
                    .to_string();
                
                let task = json!({
                    "type": "monitor",
                    "metric": metric
                });
                
                // Call monitor agent via D-Bus
                let result = call_agent_dbus(
                    "org.dbusmcp.Agent.Monitor",
                    "/org/dbusmcp/Agent/Monitor",
                    "Execute",
                    task,
                ).await?;
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(result)],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Orchestrator: Spawn agents
async fn register_spawn_agent_tool(registry: &crate::tool_registry::ToolRegistry) -> Result<()> {
    let tool = DynamicToolBuilder::new("spawn_agent")
        .description("Spawn a new agent instance via the orchestrator (e.g., rust-pro, systemd, network, executor, file, monitor)")
        .schema(json!({
            "type": "object",
            "properties": {
                "agent_type": {
                    "type": "string",
                    "description": "Type of agent to spawn (rust-pro, systemd, network, executor, file, monitor, etc.)",
                    "enum": ["rust-pro", "systemd", "network", "executor", "file", "monitor", "c-pro", "cpp-pro", "golang-pro", "python-pro", "javascript-pro"]
                },
                "config": {
                    "type": "object",
                    "description": "Optional configuration for the agent (JSON object)"
                }
            },
            "required": ["agent_type"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let agent_type = params["agent_type"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing agent_type parameter"))?
                    .to_string();
                
                let config_json = if let Some(config) = params.get("config") {
                    serde_json::to_string(config)?
                } else {
                    String::new()
                };
                
                // Call orchestrator via D-Bus
                let connection = Connection::system().await?;
                
                let proxy = zbus::Proxy::new(
                    &connection,
                    "org.dbusmcp.Orchestrator",
                    "/org/dbusmcp/Orchestrator",
                    "org.dbusmcp.Orchestrator",
                ).await?;
                
                let agent_id: String = proxy.call("SpawnAgent", &(agent_type.clone(), config_json)).await?;
                
                let result = json!({
                    "success": true,
                    "agent_id": agent_id,
                    "agent_type": agent_type,
                    "message": format!("Successfully spawned {} agent", agent_type)
                });
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(result)],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Helper: Call agent via D-Bus
async fn call_agent_dbus(
    service: &str,
    path: &str,
    method: &str,
    task: Value,
) -> Result<Value> {
    let connection = Connection::session().await?;
    
    let proxy = zbus::Proxy::new(
        &connection,
        service,
        path,
        service,
    ).await?;
    
    let task_json = serde_json::to_string(&task)?;
    let result: String = proxy.call(method, &(task_json,)).await?;
    
    // Parse result JSON
    let parsed: Value = serde_json::from_str(&result)?;
    Ok(parsed)
}

