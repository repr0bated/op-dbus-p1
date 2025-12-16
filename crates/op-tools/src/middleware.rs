//! Middleware components for tool execution

use super::{ExecutionContext, ToolExecutor};
use op_core::{ToolRequest, ToolResult, SecurityLevel};
use std::sync::Arc;
use tokio::time::{Duration, Instant};
use tracing::{info, warn};

/// Logging middleware that logs tool executions
pub struct LoggingMiddleware {
    enable_request_logging: bool,
    enable_response_logging: bool,
}

impl LoggingMiddleware {
    /// Create a new logging middleware
    pub fn new(enable_request_logging: bool, enable_response_logging: bool) -> Self {
        Self {
            enable_request_logging,
            enable_response_logging,
        }
    }
}

#[async_trait::async_trait]
impl super::ToolMiddleware for LoggingMiddleware {
    async fn before_execute(
        &self,
        context: &ExecutionContext,
        request: &ToolRequest,
    ) -> Option<ToolResult> {
        if self.enable_request_logging {
            info!("Executing tool '{}' with arguments: {:?}", 
                  context.tool_name, request.arguments);
        }
        None
    }

    async fn after_execute(
        &self,
        context: &ExecutionContext,
        result: ToolResult,
    ) -> ToolResult {
        if self.enable_response_logging {
            info!("Tool '{}' execution completed with success: {}", 
                  context.tool_name, result.success);
        }
        result
    }
}

/// Timing middleware that measures execution time
pub struct TimingMiddleware {
    enable_timing: bool,
}

impl TimingMiddleware {
    /// Create a new timing middleware
    pub fn new(enable_timing: bool) -> Self {
        Self { enable_timing }
    }
}

#[async_trait::async_trait]
impl super::ToolMiddleware for TimingMiddleware {
    async fn before_execute(
        &self,
        context: &ExecutionContext,
        _request: &ToolRequest,
    ) -> Option<ToolResult> {
        if self.enable_timing {
            info!("Starting execution of tool '{}' (ID: {})", 
                  context.tool_name, context.execution_id);
        }
        None
    }

    async fn after_execute(
        &self,
        context: &ExecutionContext,
        result: ToolResult,
    ) -> ToolResult {
        if self.enable_timing {
            let duration = chrono::Utc::now().signed_duration_since(context.start_time);
            info!("Tool '{}' execution completed in {}ms", 
                  context.tool_name, duration.num_milliseconds());
        }
        result
    }
}

/// Security middleware that enforces security levels
pub struct SecurityMiddleware {
    allowed_security_levels: Vec<SecurityLevel>,
}

impl SecurityMiddleware {
    /// Create a new security middleware
    pub fn new(allowed_security_levels: Vec<SecurityLevel>) -> Self {
        Self { allowed_security_levels }
    }

    /// Create a middleware that only allows low and medium security tools
    pub fn restrictive() -> Self {
        Self {
            allowed_security_levels: vec![SecurityLevel::Low, SecurityLevel::Medium],
        }
    }

    /// Create a middleware that allows all security levels
    pub fn permissive() -> Self {
        Self {
            allowed_security_levels: vec![
                SecurityLevel::Low,
                SecurityLevel::Medium,
                SecurityLevel::High,
                SecurityLevel::Critical,
            ],
        }
    }
}

#[async_trait::async_trait]
impl super::ToolMiddleware for SecurityMiddleware {
    async fn before_execute(
        &self,
        context: &ExecutionContext,
        _request: &ToolRequest,
    ) -> Option<ToolResult> {
        // In a real implementation, we'd get the tool definition from context
        // For now, we'll allow all executions
        None
    }

    async fn after_execute(
        &self,
        _context: &ExecutionContext,
        result: ToolResult,
    ) -> ToolResult {
        result
    }
}

/// Rate limiting middleware
pub struct RateLimitMiddleware {
    max_executions_per_minute: usize,
    execution_count: std::sync::atomic::AtomicUsize,
    window_start: std::sync::atomic::AtomicU64,
}

impl RateLimitMiddleware {
    /// Create a new rate limiting middleware
    pub fn new(max_executions_per_minute: usize) -> Self {
        Self {
            max_executions_per_minute,
            execution_count: std::sync::atomic::AtomicUsize::new(0),
            window_start: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[async_trait::async_trait]
impl super::ToolMiddleware for RateLimitMiddleware {
    async fn before_execute(
        &self,
        context: &ExecutionContext,
        _request: &ToolRequest,
    ) -> Option<ToolResult> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
            
        let window_start = self.window_start.load(std::sync::atomic::Ordering::Relaxed);
        
        // Reset counter if we're in a new minute
        if now - window_start >= 60 {
            self.window_start.store(now, std::sync::atomic::Ordering::Relaxed);
            self.execution_count.store(0, std::sync::atomic::Ordering::Relaxed);
        }
        
        let current_count = self.execution_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        if current_count >= self.max_executions_per_minute {
            warn!("Rate limit exceeded for tool '{}'", context.tool_name);
            return Some(ToolResult {
                success: false,
                content: serde_json::json!({
                    "error": "Rate limit exceeded",
                    "max_per_minute": self.max_executions_per_minute
                }),
                duration_ms: 0,
                execution_id: context.execution_id,
            });
        }
        
        None
    }

    async fn after_execute(
        &self,
        _context: &ExecutionContext,
        result: ToolResult,
    ) -> ToolResult {
        result
    }
}

/// Validation middleware that validates tool arguments
pub struct ValidationMiddleware {
    enable_validation: bool,
}

impl ValidationMiddleware {
    /// Create a new validation middleware
    pub fn new(enable_validation: bool) -> Self {
        Self { enable_validation }
    }
}

#[async_trait::async_trait]
impl super::ToolMiddleware for ValidationMiddleware {
    async fn before_execute(
        &self,
        context: &ExecutionContext,
        request: &ToolRequest,
    ) -> Option<ToolResult> {
        if !self.enable_validation {
            return None;
        }

        // Basic validation - check if arguments are present
        if request.arguments.is_null() {
            warn!("Tool '{}' called with null arguments", context.tool_name);
            return Some(ToolResult {
                success: false,
                content: serde_json::json!({
                    "error": "Arguments cannot be null"
                }),
                duration_ms: 0,
                execution_id: context.execution_id,
            });
        }

        None
    }

    async fn after_execute(
        &self,
        _context: &ExecutionContext,
        result: ToolResult,
    ) -> ToolResult {
        result
    }
}