//! Built-in tools for the op-dbus-v2 system

use op_core::{Tool, ToolDefinition, ToolRequest, ToolResult, SecurityLevel, Result};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn};

/// Built-in echo tool
pub struct EchoTool;

impl EchoTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for EchoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "echo".to_string(),
            description: "Echo back the input text".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to echo back"
                    }
                },
                "required": ["text"]
            }),
            category: "utility".to_string(),
            tags: vec!["echo", "utility", "test".to_string()],
            security_level: SecurityLevel::Low,
        }
    }

    async fn execute(&self, request: ToolRequest) -> ToolResult {
        let execution_id = uuid::Uuid::new_v4();
        let start_time = std::time::Instant::now();

        info!("Echo tool executing");

        // Extract text from arguments
        let text = request.arguments.get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("No text provided");

        let result = ToolResult {
            success: true,
            content: serde_json::json!({
                "echoed_text": text,
                "original_input": request.arguments
            }),
            duration_ms: start_time.elapsed().as_millis() as u64,
            execution_id,
        };

        info!("Echo tool completed successfully");
        result
    }
}

/// Built-in system info tool
pub struct SystemInfoTool;

impl SystemInfoTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for SystemInfoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "system_info".to_string(),
            description: "Get system information".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "info_type": {
                        "type": "string",
                        "enum": ["os", "memory", "cpu", "all"],
                        "description": "Type of system info to retrieve"
                    }
                },
                "required": ["info_type"]
            }),
            category: "system".to_string(),
            tags: vec!["system", "info", "monitoring".to_string()],
            security_level: SecurityLevel::Medium,
        }
    }

    async fn execute(&self, request: ToolRequest) -> ToolResult {
        let execution_id = uuid::Uuid::new_v4();
        let start_time = std::time::Instant::now();

        info!("System info tool executing");

        let info_type = request.arguments.get("info_type")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let system_info = match info_type {
            "os" => get_os_info(),
            "memory" => get_memory_info(),
            "cpu" => get_cpu_info(),
            "all" | _ => get_all_system_info(),
        };

        let result = ToolResult {
            success: true,
            content: serde_json::json!({
                "info_type": info_type,
                "system_info": system_info
            }),
            duration_ms: start_time.elapsed().as_millis() as u64,
            execution_id,
        };

        info!("System info tool completed successfully");
        result
    }
}

/// Built-in calculation tool
pub struct CalculatorTool;

impl CalculatorTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for CalculatorTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "calculate".to_string(),
            description: "Perform basic mathematical calculations".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "expression": {
                        "type": "string",
                        "description": "Mathematical expression to evaluate"
                    },
                    "operation": {
                        "type": "string",
                        "enum": ["add", "subtract", "multiply", "divide"],
                        "description": "Operation to perform"
                    },
                    "a": {
                        "type": "number",
                        "description": "First operand"
                    },
                    "b": {
                        "type": "number",
                        "description": "Second operand"
                    }
                },
                "required": ["operation", "a", "b"]
            }),
            category: "utility".to_string(),
            tags: vec!["math", "calculation", "utility".to_string()],
            security_level: SecurityLevel::Low,
        }
    }

    async fn execute(&self, request: ToolRequest) -> ToolResult {
        let execution_id = uuid::Uuid::new_v4();
        let start_time = std::time::Instant::now();

        info!("Calculator tool executing");

        // Try expression-based calculation first
        if let Some(expression) = request.arguments.get("expression").and_then(|v| v.as_str()) {
            let result = evaluate_expression(expression, execution_id);
            return ToolResult {
                success: result.is_some(),
                content: serde_json::json!({
                    "expression": expression,
                    "result": result,
                    "method": "expression_evaluation"
                }),
                duration_ms: start_time.elapsed().as_millis() as u64,
                execution_id,
            };
        }

        // Fall back to operation-based calculation
        let operation = request.arguments.get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("add");
            
        let a = request.arguments.get("a")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
            
        let b = request.arguments.get("b")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let result = match operation {
            "add" => Some(a + b),
            "subtract" => Some(a - b),
            "multiply" => Some(a * b),
            "divide" => {
                if b != 0.0 {
                    Some(a / b)
                } else {
                    None
                }
            },
            _ => None,
        };

        let success = result.is_some();
        if !success {
            warn!("Invalid operation in calculator tool: {}", operation);
        }

        ToolResult {
            success,
            content: serde_json::json!({
                "operation": operation,
                "operands": [a, b],
                "result": result,
                "error": if success { None } else { Some("Invalid operation or division by zero") }
            }),
            duration_ms: start_time.elapsed().as_millis() as u64,
            execution_id,
        }
    }
}

/// Built-in file operation tool
pub struct FileOpTool;

impl FileOpTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for FileOpTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_op".to_string(),
            description: "Perform basic file operations".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["read", "write", "list"],
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
            }),
            category: "filesystem".to_string(),
            tags: vec!["file", "filesystem", "io".to_string()],
            security_level: SecurityLevel::Medium,
        }
    }

    async fn execute(&self, request: ToolRequest) -> ToolResult {
        let execution_id = uuid::Uuid::new_v4();
        let start_time = std::time::Instant::now();

        info!("File operation tool executing");

        let operation = request.arguments.get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("read");
            
        let path = request.arguments.get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if path.is_empty() {
            return ToolResult {
                success: false,
                content: serde_json::json!({
                    "error": "Path cannot be empty"
                }),
                duration_ms: start_time.elapsed().as_millis() as u64,
                execution_id,
            };
        }

        // Note: This is a simplified implementation for demo purposes
        // In a real implementation, you'd want proper error handling and security checks
        let result = match operation {
            "read" => {
                // Mock read operation
                serde_json::json!({
                    "operation": "read",
                    "path": path,
                    "content": format!("Mock content of file: {}", path),
                    "size": 1024,
                    "modified": chrono::Utc::now().to_rfc3339()
                })
            },
            "write" => {
                let content = request.arguments.get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                serde_json::json!({
                    "operation": "write",
                    "path": path,
                    "bytes_written": content.len(),
                    "success": true
                })
            },
            "list" => {
                serde_json::json!({
                    "operation": "list",
                    "path": path,
                    "files": ["file1.txt", "file2.txt", "directory1/"],
                    "total_items": 3
                })
            },
            _ => {
                serde_json::json!({
                    "error": "Unknown operation",
                    "operation": operation
                })
            }
        };

        ToolResult {
            success: operation != "unknown",
            content: result,
            duration_ms: start_time.elapsed().as_millis() as u64,
            execution_id,
        }
    }
}

/// Register all built-in tools
pub fn register_builtin_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(EchoTool::new()),
        Box::new(SystemInfoTool::new()),
        Box::new(CalculatorTool::new()),
        Box::new(FileOpTool::new()),
    ]
}

/// Helper functions for system information
fn get_os_info() -> Value {
    serde_json::json!({
        "platform": std::env::consts::OS,
        "architecture": std::env::consts::ARCH,
        "family": std::env::consts::FAMILY
    })
}

fn get_memory_info() -> Value {
    // In a real implementation, you'd get actual memory info
    serde_json::json!({
        "total": "8GB",
        "available": "6GB",
        "used": "2GB"
    })
}

fn get_cpu_info() -> Value {
    serde_json::json!({
        "cores": num_cpus::get(),
        "model": "Unknown CPU",
        "frequency": "2.4GHz"
    })
}

fn get_all_system_info() -> Value {
    serde_json::json!({
        "os": get_os_info(),
        "memory": get_memory_info(),
        "cpu": get_cpu_info(),
        "hostname": std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_string()),
        "user": std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
    })
}

/// Simple expression evaluator (very basic implementation)
fn evaluate_expression(expression: &str, execution_id: uuid::Uuid) -> Option<f64> {
    // This is a very basic implementation - in reality you'd want a proper parser
    // For demo purposes, we'll just try to evaluate simple expressions
    
    // Remove whitespace
    let expr: String = expression.chars().filter(|c| !c.is_whitespace()).collect();
    
    // Very basic parsing - just handle single operations
    if let Some(pos) = expr.find('+') {
        let (left, right) = expr.split_at(pos);
        if let (Ok(a), Ok(b)) = (left.parse::<f64>(), right.parse::<f64>()) {
            return Some(a + b);
        }
    }
    
    if let Some(pos) = expr.find('-') {
        let (left, right) = expr.split_at(pos);
        if let (Ok(a), Ok(b)) = (left.parse::<f64>(), right.parse::<f64>()) {
            return Some(a - b);
        }
    }
    
    if let Some(pos) = expr.find('*') {
        let (left, right) = expr.split_at(pos);
        if let (Ok(a), Ok(b)) = (left.parse::<f64>(), right.parse::<f64>()) {
            return Some(a * b);
        }
    }
    
    if let Some(pos) = expr.find('/') {
        let (left, right) = expr.split_at(pos);
        if let (Ok(a), Ok(b)) = (left.parse::<f64>(), right.parse::<f64>()) {
            if b != 0.0 {
                return Some(a / b);
            }
        }
    }
    
    // Try to parse as a single number
    expr.parse::<f64>().ok()
}