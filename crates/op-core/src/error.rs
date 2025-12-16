//! Error types for op-dbus-v2 system

use thiserror::Error;

/// Core error type
#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Tool not found: {name}")]
    ToolNotFound { name: String },
    
    #[error("D-Bus error: {message}")]
    DbusError { message: String },
    
    #[error("Invalid input: {message}")]
    InvalidInput { message: String },
    
    #[error("Execution error: {message}")]
    ExecutionError { message: String },
    
    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },
    
    #[error("Network error: {message}")]
    NetworkError { message: String },
    
    #[error("Permission denied: {operation}")]
    PermissionDenied { operation: String },
    
    #[error("Timeout: {operation}")]
    Timeout { operation: String },
    
    #[error("Serialization error: {source}")]
    SerializationError { source: serde_json::Error },
    
    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl CoreError {
    /// Create a tool not found error
    pub fn tool_not_found(name: impl Into<String>) -> Self {
        Self::ToolNotFound { name: name.into() }
    }
    
    /// Create a D-Bus error
    pub fn dbus_error(message: impl Into<String>) -> Self {
        Self::DbusError { message: message.into() }
    }
    
    /// Create an invalid input error
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput { message: message.into() }
    }
    
    /// Create an execution error
    pub fn execution_error(message: impl Into<String>) -> Self {
        Self::ExecutionError { message: message.into() }
    }
    
    /// Create a configuration error
    pub fn configuration_error(message: impl Into<String>) -> Self {
        Self::ConfigurationError { message: message.into() }
    }
    
    /// Create a network error
    pub fn network_error(message: impl Into<String>) -> Self {
        Self::NetworkError { message: message.into() }
    }
    
    /// Create a permission denied error
    pub fn permission_denied(operation: impl Into<String>) -> Self {
        Self::PermissionDenied { operation: operation.into() }
    }
    
    /// Create a timeout error
    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::Timeout { operation: operation.into() }
    }
    
    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal { message: message.into() }
    }
}

/// Result type alias
pub type Result<T> = std::result::Result<T, CoreError>;

impl From<serde_json::Error> for CoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::SerializationError { source: error }
    }
}

impl From<anyhow::Error> for CoreError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal {
            message: error.to_string(),
        }
    }
}