//! Core traits for op-dbus-v2 system

use crate::{ToolDefinition, ToolRequest, ToolResult};
use std::future::Future;
use std::pin::Pin;

/// Tool trait - defines the interface for all tools
pub trait Tool: Send + Sync {
    /// Get the tool definition
    fn definition(&self) -> ToolDefinition;
    
    /// Execute the tool with the given request
    fn execute(&self, request: ToolRequest) -> Pin<Box<dyn Future<Output = ToolResult> + Send>>;
}

/// Tool registry trait - defines the interface for tool registries
pub trait ToolRegistry: Send + Sync {
    /// Register a tool
    fn register_tool(&self, tool: Box<dyn Tool>) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
    
    /// Unregister a tool by name
    fn unregister_tool(&self, name: &str) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
    
    /// Get a tool by name
    fn get_tool(&self, name: &str) -> Pin<Box<dyn Future<Output = Option<Box<dyn Tool>>> + Send>>;
    
    /// List all registered tools
    fn list_tools(&self) -> Pin<Box<dyn Future<Output = Vec<ToolDefinition>> + Send>>;
    
    /// Get tools by category
    fn get_tools_by_category(&self, category: &str) -> Pin<Box<dyn Future<Output = Vec<ToolDefinition>> + Send>>;
}

/// D-Bus introspection trait
pub trait DbusIntrospector: Send + Sync {
    /// List services on a bus
    fn list_services(&self, bus_type: crate::BusType) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<crate::ServiceInfo>>> + Send>>;
    
    /// Get interfaces for a service
    fn get_interfaces(&self, service_name: &str, bus_type: crate::BusType) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<crate::InterfaceInfo>>> + Send>>;
    
    /// Get methods for an interface
    fn get_methods(&self, service_name: &str, interface_name: &str, bus_type: crate::BusType) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<crate::MethodInfo>>> + Send>>;
}