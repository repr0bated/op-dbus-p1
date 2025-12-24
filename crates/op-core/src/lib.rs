//! op-core: Core types and utilities for op-dbus-v2
//!
//! This crate provides the foundational types used across all op-dbus components:
//! - Tool definitions and requests/responses
//! - D-Bus type abstractions
//! - Common error types
//! - Execution tracking

pub mod types;
pub mod error;
pub mod connection;
pub mod message;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export execution tracking from op-execution-tracker
pub use op_execution_tracker as execution_tracker; // Re-export module
pub use op_execution_tracker::{
    ExecutionContext, ExecutionResult, ExecutionStatus, ExecutionTracker,
    metrics::ExecutionMetrics, telemetry::ExecutionTelemetry,
};

// Note: ExecutionTrackerTrait removed due to compatibility issues

// Re-export types for convenience
pub use types::*;
pub use error::{Error, Result};

// Re-export connection types
pub use connection::DbusConnection;

/// Tool definition for MCP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Unique tool name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// JSON Schema for input parameters
    pub input_schema: serde_json::Value,
    /// Optional category for grouping
    #[serde(default)]
    pub category: Option<String>,
    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ToolDefinition {
    pub fn new(name: &str, description: &str, input_schema: serde_json::Value) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            category: None,
            tags: Vec::new(),
        }
    }

    pub fn with_category(mut self, category: &str) -> Self {
        self.category = Some(category.to_string());
        self
    }

    pub fn with_tags(mut self, tags: Vec<&str>) -> Self {
        self.tags = tags.into_iter().map(String::from).collect();
        self
    }
}

/// Tool execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    /// Request ID for correlation
    pub id: String,
    /// Tool name to execute
    pub name: String,
    /// Input arguments
    pub arguments: serde_json::Value,
    /// Timeout in milliseconds
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Optional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl ToolRequest {
    pub fn new(name: &str, arguments: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            arguments,
            timeout_ms: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_id(mut self, id: &str) -> Self {
        self.id = id.to_string();
        self
    }

    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Request ID this result corresponds to
    pub id: String,
    /// Whether execution was successful
    pub success: bool,
    /// Result content (JSON)
    pub content: serde_json::Value,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Execution duration in milliseconds
    #[serde(default)]
    pub duration_ms: u64,
    /// Execution ID for tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
}

impl ToolResult {
    pub fn success(id: &str, content: serde_json::Value, duration_ms: u64) -> Self {
        Self {
            id: id.to_string(),
            success: true,
            content,
            error: None,
            duration_ms,
            execution_id: None,
        }
    }

    pub fn error(id: &str, error: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            id: id.to_string(),
            success: false,
            content: serde_json::Value::Null,
            error: Some(error.into()),
            duration_ms,
            execution_id: None,
        }
    }

    pub fn with_execution_id(mut self, execution_id: &str) -> Self {
        self.execution_id = Some(execution_id.to_string());
        self
    }
}

/// Prelude for convenient imports
pub mod prelude {
    pub use super::{
        ToolDefinition, ToolRequest, ToolResult,
        ExecutionContext, ExecutionResult, ExecutionStatus, ExecutionTracker,
        Error, Result,
    };
    pub use super::types::*;
}
