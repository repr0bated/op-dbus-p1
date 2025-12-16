//! Tool execution engine with middleware support

use op_core::{Tool, ToolRequest, ToolResult};
use std::sync::Arc;
use tracing::{info, warn, instrument};
use uuid::Uuid;
use chrono::Utc;

/// Tool executor trait
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute a tool with the given request
    async fn execute(&self, tool: Arc<dyn Tool>, request: ToolRequest) -> ToolResult;
}

/// Tool executor implementation with middleware support
pub struct ToolExecutorImpl {
    middleware: Vec<Arc<dyn ToolMiddleware>>,
}

impl ToolExecutorImpl {
    /// Create a new tool executor
    pub fn new(middleware: Vec<Arc<dyn ToolMiddleware>>) -> Self {
        Self { middleware }
    }

    /// Execute a tool through the middleware chain
    async fn execute_with_middleware(&self, tool: Arc<dyn Tool>, request: ToolRequest) -> ToolResult {
        let start_time = Utc::now();
        let execution_id = Uuid::new_v4();
        
        info!("Starting tool execution: {} (ID: {})", tool.definition().name, execution_id);

        // Create the execution context
        let context = ExecutionContext {
            execution_id,
            tool_name: tool.definition().name.clone(),
            start_time,
            middleware: self.middleware.clone(),
        };

        // Execute through middleware chain
        let result = self.execute_middleware_chain(context, tool, request).await;
        
        let duration = Utc::now().signed_duration_since(start_time);
        let duration_ms = duration.num_milliseconds() as u64;
        
        let final_result = ToolResult {
            duration_ms,
            ..result
        };
        
        info!("Tool execution completed: {} (ID: {}, duration: {}ms, success: {})", 
              tool.definition().name, execution_id, duration_ms, final_result.success);
        
        final_result
    }

    /// Execute through the middleware chain
    async fn execute_middleware_chain(
        &self,
        mut context: ExecutionContext,
        tool: Arc<dyn Tool>,
        request: ToolRequest,
    ) -> ToolResult {
        // Apply pre-execution middleware
        for middleware in &context.middleware {
            if let Some(result) = middleware.before_execute(&context, &request).await {
                return result;
            }
        }

        // Execute the actual tool
        let result = tool.execute(request).await;

        // Apply post-execution middleware
        let mut final_result = result;
        for middleware in &context.middleware {
            final_result = middleware.after_execute(&context, final_result).await;
        }

        final_result
    }
}

#[async_trait::async_trait]
impl ToolExecutor for ToolExecutorImpl {
    async fn execute(&self, tool: Arc<dyn Tool>, request: ToolRequest) -> ToolResult {
        self.execute_with_middleware(tool, request).await
    }
}

/// Execution context for middleware
#[derive(Clone)]
pub struct ExecutionContext {
    pub execution_id: Uuid,
    pub tool_name: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub middleware: Vec<Arc<dyn ToolMiddleware>>,
}

/// Tool middleware trait for adding cross-cutting concerns
#[async_trait::async_trait]
pub trait ToolMiddleware: Send + Sync {
    /// Called before tool execution
    /// Return Some(result) to short-circuit execution
    async fn before_execute(
        &self,
        context: &ExecutionContext,
        request: &ToolRequest,
    ) -> Option<ToolResult>;

    /// Called after tool execution
    async fn after_execute(
        &self,
        context: &ExecutionContext,
        result: ToolResult,
    ) -> ToolResult;
}

/// Simple tool executor without middleware
pub struct SimpleToolExecutor;

#[async_trait::async_trait]
impl ToolExecutor for SimpleToolExecutor {
    async fn execute(&self, tool: Arc<dyn Tool>, request: ToolRequest) -> ToolResult {
        let start_time = Utc::now();
        let result = tool.execute(request).await;
        let duration = Utc::now().signed_duration_since(start_time);
        
        ToolResult {
            duration_ms: duration.num_milliseconds() as u64,
            ..result
        }
    }
}

/// Tool execution statistics
#[derive(Debug, Clone)]
pub struct ExecutionStats {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub average_duration_ms: f64,
    pub total_duration_ms: u64,
}

impl Default for ExecutionStats {
    fn default() -> Self {
        Self {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            average_duration_ms: 0.0,
            total_duration_ms: 0,
        }
    }
}

/// Tool execution result with additional metadata
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub result: ToolResult,
    pub execution_id: Uuid,
    pub tool_name: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
}

impl ToolExecutionResult {
    /// Create a new execution result
    pub fn new(
        result: ToolResult,
        execution_id: Uuid,
        tool_name: String,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            result,
            execution_id,
            tool_name,
            start_time,
            end_time,
        }
    }
}