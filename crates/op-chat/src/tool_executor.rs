//! Tool Executor with Tracking
//!
//! Wraps tool execution with accountability tracking.
//! Every tool call is logged and tracked for audit purposes.

use anyhow::Result;
use op_core::{ExecutionTracker, ExecutionContext, ExecutionResult};
use op_tools::ToolRegistry;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};
use chrono::Utc;

/// Tool executor with built-in tracking
pub struct TrackedToolExecutor {
    registry: Arc<ToolRegistry>,
    tracker: Arc<ExecutionTracker>,
}

impl TrackedToolExecutor {
    /// Create a new tracked executor
    pub fn new(registry: Arc<ToolRegistry>, tracker: Arc<ExecutionTracker>) -> Self {
        Self { registry, tracker }
    }

    /// Execute a tool with full tracking
    pub async fn execute(
        &self,
        tool_name: &str,
        arguments: Value,
        initiated_by: Option<String>,
    ) -> Result<TrackedResult> {
        // Create execution context
        let mut context = ExecutionContext::new(tool_name);
        
        // Store input arguments and initiator in metadata
        let mut metadata = serde_json::Map::new();
        metadata.insert("arguments".to_string(), arguments.clone());
        if let Some(initiator) = initiated_by {
            metadata.insert("initiated_by".to_string(), Value::String(initiator));
        }
        context.set_metadata(Value::Object(metadata));

        // Start tracking
        let execution_id = self.tracker.track_execution(context).await?;

        info!(
            execution_id = %execution_id,
            tool = %tool_name,
            "Starting tool execution"
        );

        let start_time = Instant::now();

        // Get tool
        let tool = self.registry.get(tool_name).await
            .ok_or_else(|| anyhow::anyhow!("Tool '{}' not found", tool_name));

        let execution_result = match tool {
            Ok(t) => {
                // Execute
                match t.execute(arguments.clone()).await {
                    Ok(val) => {
                        let duration = start_time.elapsed().as_millis() as u64;
                        ExecutionResult {
                            success: true,
                            result: Some(val),
                            error: None,
                            duration_ms: duration,
                            finished_at: Utc::now(),
                        }
                    },
                    Err(e) => {
                        let duration = start_time.elapsed().as_millis() as u64;
                        ExecutionResult {
                            success: false,
                            result: None,
                            error: Some(e.to_string()),
                            duration_ms: duration,
                            finished_at: Utc::now(),
                        }
                    }
                }
            },
            Err(e) => {
                let duration = start_time.elapsed().as_millis() as u64;
                ExecutionResult {
                    success: false,
                    result: None,
                    error: Some(e.to_string()),
                    duration_ms: duration,
                    finished_at: Utc::now(),
                }
            }
        };

        // Complete tracking
        self.tracker.complete_execution(&execution_id, execution_result.clone()).await?;

        Ok(TrackedResult {
            result: execution_result,
            execution_id,
        })
    }

    /// Execute multiple tools in sequence
    pub async fn execute_sequence(
        &self,
        tools: Vec<(String, Value)>,
        initiated_by: Option<String>,
    ) -> Vec<TrackedResult> {
        let mut results = Vec::new();

        for (tool_name, arguments) in tools {
            let result = self
                .execute(&tool_name, arguments, initiated_by.clone())
                .await;

            match result {
                Ok(tracked) => {
                    let should_continue = tracked.success();
                    results.push(tracked);
                    if !should_continue {
                        warn!("Stopping sequence due to failed execution");
                        break;
                    }
                }
                Err(e) => {
                    error!(error = %e, tool = %tool_name, "Execution error");
                    break;
                }
            }
        }

        results
    }

    /// Get execution history
    pub async fn get_history(&self, limit: usize) -> Vec<ExecutionContext> {
        self.tracker.list_recent_completed(limit).await
    }

    /// Get execution statistics
    pub async fn get_stats(&self) -> Value {
        // Return JSON metrics directly
        self.tracker.get_metrics().get_metrics_json().await.unwrap_or(serde_json::json!({"error": "Failed to get metrics"}))
    }

    /// Get tracker reference
    pub fn tracker(&self) -> &Arc<ExecutionTracker> {
        &self.tracker
    }

    /// Get registry reference
    pub fn registry(&self) -> &Arc<ToolRegistry> {
        &self.registry
    }
}

/// Result with tracking information

#[derive(Debug)]

pub struct TrackedResult {

    /// The actual tool result

    pub result: ExecutionResult,

    /// Execution ID for audit trail

    pub execution_id: String,

}



impl TrackedResult {

    pub fn success(&self) -> bool {

        self.result.success

    }



    pub fn content(&self) -> &Option<Value> {

        &self.result.result

    }



    pub fn error(&self) -> Option<&String> {

        self.result.error.as_ref()

    }

}
