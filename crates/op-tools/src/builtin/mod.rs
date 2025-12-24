//! Built-in Tools
//!
//! This module provides built-in tool implementations.

// mod ovs;
// mod systemd;
// mod networkmanager;
mod agent_tool;
mod file;
mod procfs;
pub mod response_tools;
// mod system;
// mod plugin;

// pub use ovs::OvsTool;
// pub use systemd::SystemdTool;
// pub use networkmanager::NetworkManagerTool;
pub use agent_tool::{create_agent_tool, create_agent_tool_with_executor, AgentTool};
pub use file::FileTool;
pub use procfs::{ProcFsReadTool, ProcFsWriteTool, SysFsReadTool, SysFsWriteTool};
// pub use system::SystemTool;
// pub use plugin::PluginTool;

use crate::ToolRegistry;
use std::sync::Arc;

/// Register all built-in tools
pub async fn register_response_tools(registry: &ToolRegistry) -> anyhow::Result<()> {
    // File tools
    let _ = registry.register_tool(Arc::new(FileTool::new("file_read", "Read file content"))).await;
    let _ = registry.register_tool(Arc::new(FileTool::new("file_write", "Write file content"))).await;
    let _ = registry.register_tool(Arc::new(FileTool::new("file_list", "List directory"))).await;
    let _ = registry.register_tool(Arc::new(FileTool::new("file_exists", "Check if file exists"))).await;
    let _ = registry.register_tool(Arc::new(FileTool::new("file_stat", "Get file status"))).await;

    // /proc and /sys tools
    let _ = registry.register_tool(Arc::new(ProcFsReadTool::new())).await;
    let _ = registry.register_tool(Arc::new(SysFsReadTool::new())).await;
    let _ = registry.register_tool(Arc::new(ProcFsWriteTool::new())).await;
    let _ = registry.register_tool(Arc::new(SysFsWriteTool::new())).await;

    Ok(())
}
