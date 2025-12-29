//! Built-in Tools
//!
//! This module provides built-in tool implementations with security controls.
//!
//! ## Security
//!
//! All tools in this module use the centralized SecurityValidator for:
//! - Command allowlist validation
//! - Path validation (read/write)
//! - Rate limiting
//! - Input sanitization
//!
//! ## Tools
//!
//! - **File Tools**: Secure file read/write/list with path validation
//! - **Shell Tools**: Secure command execution with allowlist
//! - **ProcFs/SysFs Tools**: Read-only access to /proc and /sys
//! - **D-Bus Tools**: Native protocol access to system services
//! - **OVS Tools**: Native OVSDB JSON-RPC for Open vSwitch
//! - **Response Tools**: LLM response handling for anti-hallucination

mod dbus;
mod dbus_introspection;
mod ovs_tools;
mod packagekit;
mod shell;
mod agent_tool;
mod file;
mod procfs;
mod git_tool;
pub mod response_tools;

// Re-exports
pub use agent_tool::{create_agent_tool, create_agent_tool_with_executor, AgentTool};
pub use file::{FileTool, SecureFileTool};
pub use procfs::{ProcFsReadTool, ProcFsWriteTool, SysFsReadTool, SysFsWriteTool};
pub use shell::register_shell_tools;
pub use ovs_tools::register_ovs_tools;

use crate::ToolRegistry;
use std::sync::Arc;
use tracing::{debug, info};

/// Register all built-in tools with the registry
///
/// This registers:
/// - Secure file tools (file_read, file_write, file_list, file_exists, file_stat)
/// - Secure shell tools (shell_execute, shell_execute_batch)
/// - ProcFs/SysFs tools
/// - D-Bus tools (systemd, introspection)
/// - OVS tools (native OVSDB)
/// - Response tools (respond_to_user, cannot_perform, request_clarification)
pub async fn register_response_tools(registry: &ToolRegistry) -> anyhow::Result<()> {
    info!("Registering built-in tools with security controls");

    // Secure file tools
    registry.register_tool(Arc::new(SecureFileTool::read())).await?;
    registry.register_tool(Arc::new(SecureFileTool::write())).await?;
    registry.register_tool(Arc::new(SecureFileTool::list())).await?;
    registry.register_tool(Arc::new(SecureFileTool::exists())).await?;
    registry.register_tool(Arc::new(SecureFileTool::stat())).await?;
    debug!("Registered secure file tools");

    // ProcFs and SysFs tools (read-only system info)
    registry.register_tool(Arc::new(ProcFsReadTool::new())).await?;
    registry.register_tool(Arc::new(SysFsReadTool::new())).await?;
    registry.register_tool(Arc::new(ProcFsWriteTool::new())).await?;
    registry.register_tool(Arc::new(SysFsWriteTool::new())).await?;
    debug!("Registered procfs/sysfs tools");

    // Secure shell tools
    shell::register_shell_tools(registry).await?;
    debug!("Registered secure shell tools");

    // D-Bus tools (native protocol access)
    dbus::register_dbus_tools(registry).await?;
    dbus_introspection::register_dbus_introspection_tools(registry).await?;
    debug!("Registered D-Bus tools");

    // PackageKit tools
    packagekit::register_packagekit_tools(registry).await?;
    debug!("Registered PackageKit tools");

    // OVS tools (native OVSDB JSON-RPC)
    ovs_tools::register_ovs_tools(registry).await?;
    debug!("Registered OVS tools");

    // Response tools (for anti-hallucination)
    for tool in response_tools::create_response_tools() {
        registry.register_tool(tool).await?;
    }
    debug!("Registered response tools");

    // Git tools (structured git operations)
    git_tool::register_git_tools(registry).await?;
    debug!("Registered git tools");

    info!("Built-in tool registration complete");
    Ok(())
}

/// Register only essential tools (for minimal/restricted mode)
pub async fn register_essential_tools(registry: &ToolRegistry) -> anyhow::Result<()> {
    info!("Registering essential tools only (restricted mode)");

    // Only file read tools
    registry.register_tool(Arc::new(SecureFileTool::read())).await?;
    registry.register_tool(Arc::new(SecureFileTool::list())).await?;
    registry.register_tool(Arc::new(SecureFileTool::exists())).await?;
    registry.register_tool(Arc::new(SecureFileTool::stat())).await?;

    // Response tools are always needed
    for tool in response_tools::create_response_tools() {
        registry.register_tool(tool).await?;
    }

    info!("Essential tool registration complete");
    Ok(())
}
