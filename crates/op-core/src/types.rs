//! Core types for op-dbus-v2 system

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

/// D-Bus bus type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BusType {
    /// System bus
    System,
    /// Session bus
    Session,
    /// Custom bus
    Custom(String),
}

impl std::fmt::Display for BusType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BusType::System => write!(f, "system"),
            BusType::Session => write!(f, "session"),
            BusType::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the tool execution was successful
    pub success: bool,
    /// Result content
    pub content: Value,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Unique execution ID
    pub execution_id: Uuid,
}

/// Tool execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    /// Tool name
    pub name: String,
    /// Tool arguments
    pub arguments: Value,
    /// Optional execution context
    pub context: Option<HashMap<String, Value>>,
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Input schema for the tool
    pub input_schema: Value,
    /// Tool category
    pub category: String,
    /// Tool tags
    pub tags: Vec<String>,
    /// Security level
    pub security_level: SecurityLevel,
}

/// Security level for tool execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityLevel {
    /// Low security - basic tools
    Low,
    /// Medium security - system tools
    Medium,
    /// High security - privileged tools
    High,
    /// Critical security - dangerous tools
    Critical,
}

/// D-Bus service information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Service name
    pub name: String,
    /// Bus type
    pub bus_type: BusType,
    /// Whether the service is currently active
    pub active: bool,
}

/// D-Bus interface information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceInfo {
    /// Interface name
    pub name: String,
    /// Methods available on this interface
    pub methods: Vec<MethodInfo>,
    /// Signals available on this interface
    pub signals: Vec<SignalInfo>,
    /// Properties available on this interface
    pub properties: Vec<PropertyInfo>,
}

/// D-Bus method information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodInfo {
    /// Method name
    pub name: String,
    /// Input signature
    pub input_signature: String,
    /// Output signature
    pub output_signature: String,
}

/// D-Bus signal information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalInfo {
    /// Signal name
    pub name: String,
    /// Signal signature
    pub signature: String,
}

/// D-Bus property information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyInfo {
    /// Property name
    pub name: String,
    /// Property type
    pub property_type: String,
    /// Whether the property is readable
    pub readable: bool,
    /// Whether the property is writable
    pub writable: bool,
}