//! Application State Management
//!
//! Reactive state for the entire application using Leptos signals.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Global application state
#[derive(Clone, Debug)]
pub struct AppState {
    /// Chat messages history
    pub messages: VecDeque<ChatMessage>,
    /// Available tools from backend
    pub tools: Vec<ToolInfo>,
    /// System status
    pub system_status: Option<SystemStatus>,
    /// Current model being used
    pub current_model: String,
    /// Current provider
    pub current_provider: String,
    /// Connection status
    pub connected: bool,
    /// Loading state
    pub loading: bool,
    /// Error message if any
    pub error: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            messages: VecDeque::with_capacity(100),
            tools: Vec::new(),
            system_status: None,
            current_model: "default".to_string(),
            current_provider: "unknown".to_string(),
            connected: false,
            loading: false,
            error: None,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// A chat message
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: String,
    pub tools_executed: Vec<String>,
    pub tool_results: Vec<ToolResultInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Tool information from backend
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    pub input_schema: serde_json::Value,
}

/// Tool execution result
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolResultInfo {
    pub tool_name: String,
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// System status information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SystemStatus {
    pub services: Vec<ServiceStatus>,
    pub network: NetworkStatus,
    pub ovs: Option<OvsStatus>,
    pub system_info: SystemInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub active_state: String,
    pub sub_state: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub interfaces: Vec<InterfaceInfo>,
    pub connections: Vec<ConnectionInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InterfaceInfo {
    pub name: String,
    pub state: String,
    pub ip_addresses: Vec<String>,
    pub mac_address: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub name: String,
    pub conn_type: String,
    pub state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OvsStatus {
    pub available: bool,
    pub bridges: Vec<OvsBridge>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OvsBridge {
    pub name: String,
    pub uuid: String,
    pub ports: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SystemInfo {
    pub hostname: String,
    pub kernel: String,
    pub uptime: String,
    pub load_average: [f64; 3],
    pub memory_used_percent: f64,
    pub cpu_count: usize,
}
