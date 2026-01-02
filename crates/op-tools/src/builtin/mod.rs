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
//! - **LXC Tools**: Native Proxmox REST API for LXC container management
//! - **Response Tools**: LLM response handling for anti-hallucination
//! - **Self Tools**: Git and code editing for the chatbot's own source code

mod dbus;
mod dbus_introspection;
mod error_reporting_tool;
mod ovs_tools;
mod openflow_tools;
mod lxc_tools;
mod packagekit;
mod rtnetlink_tools;
mod shell;
mod agent_tool;
mod file;
mod procfs;
pub mod response_tools;
pub mod self_tools;

// Re-exports
pub use agent_tool::{create_agent_tool, create_agent_tool_with_executor, AgentTool};
pub use file::{FileTool, SecureFileTool};
pub use procfs::{ProcFsReadTool, ProcFsWriteTool, SysFsReadTool, SysFsWriteTool};
pub use shell::register_shell_tools;
pub use ovs_tools::register_ovs_tools;
pub use lxc_tools::register_lxc_tools;
pub use self_tools::{create_self_tools, get_self_repo_system_context};

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
/// - Self tools (git, code editing for own source - if OP_SELF_REPO_PATH is set)
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

    // OpenFlow tools (native OpenFlow protocol)
    openflow_tools::register_openflow_tools(registry).await?;
    debug!("Registered OpenFlow tools");

    // Rtnetlink tools (native network interface management)
    rtnetlink_tools::register_rtnetlink_tools(registry).await?;
    debug!("Registered rtnetlink tools");

    // LXC tools (native Proxmox REST API)
    lxc_tools::register_lxc_tools(registry).await?;
    debug!("Registered LXC tools (native Proxmox API)");

    // Response tools (for anti-hallucination)
    for tool in response_tools::create_response_tools() {
        registry.register_tool(tool).await?;
    }
    registry.register_tool(Arc::new(error_reporting_tool::ReportInternalErrorTool)).await?;
    debug!("Registered response tools");

    // Self-repository tools (if OP_SELF_REPO_PATH is configured)
    if op_core::self_identity::is_self_repo_configured() {
        for tool in self_tools::create_self_tools() {
            registry.register_tool(tool).await?;
        }
        info!("Registered self-repository tools - chatbot can modify its own code");
    } else {
        debug!("Self-repository tools not registered (OP_SELF_REPO_PATH not set)");
    }

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
