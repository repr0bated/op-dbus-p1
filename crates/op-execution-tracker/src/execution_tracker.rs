use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tracing::{info, warn, instrument};
use anyhow::Result;
use async_trait::async_trait;

use crate::execution_context::{ExecutionContext, ExecutionStatus, ExecutionResult};
use crate::metrics::ExecutionMetrics;
use crate::telemetry::ExecutionTelemetry;

/// Event emitted when execution state changes
#[derive(Clone, Debug)]
pub enum ExecutionEvent {
    Started(ExecutionContext),
    Completed(String, ExecutionResult), // execution_id, result
    StatusUpdated(String, ExecutionStatus), // execution_id, new_status
}

/// Execution tracker for monitoring tool executions
#[derive(Clone)]
pub struct ExecutionTracker {
    /// Active executions
    active_executions: Arc<RwLock<std::collections::HashMap<String, ExecutionContext>>>,

    /// Completed executions (recent history)
    completed_executions: Arc<RwLock<std::collections::HashMap<String, ExecutionContext>>>,

    /// Metrics collector
    metrics: Arc<ExecutionMetrics>,

    /// Telemetry service
    telemetry: Arc<ExecutionTelemetry>,

    /// Maximum history size
    max_history: usize,

    /// Event broadcaster
    event_sender: broadcast::Sender<ExecutionEvent>,
}

impl ExecutionTracker {
    /// Create new execution tracker
    pub fn new(metrics: Arc<ExecutionMetrics>, telemetry: Arc<ExecutionTelemetry>) -> Self {
        let (tx, _) = broadcast::channel(1000); // Buffer up to 1000 events
        Self {
            active_executions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            completed_executions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            metrics,
            telemetry,
            max_history: 1000, // Keep 1000 recent executions
            event_sender: tx,
        }
    }

    /// Create with custom history size
    pub fn with_history_size(
        metrics: Arc<ExecutionMetrics>,
        telemetry: Arc<ExecutionTelemetry>,
        max_history: usize,
    ) -> Self {
        let (tx, _) = broadcast::channel(1000);
        Self {
            active_executions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            completed_executions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            metrics,
            telemetry,
            max_history,
            event_sender: tx,
        }
    }

    /// Subscribe to execution events
    pub fn subscribe(&self) -> broadcast::Receiver<ExecutionEvent> {
        self.event_sender.subscribe()
    }

    /// Track new execution
    #[instrument(skip(self), fields(execution_id, tool_name))]
    pub async fn track_execution(&self, context: ExecutionContext) -> Result<String> {
        let execution_id = context.execution_id.clone();

        // Record metrics
        self.metrics.execution_started(&context.tool_name);

        // Start telemetry span
        self.telemetry.start_execution_span(&context);

        // Notify subscribers
        let _ = self.event_sender.send(ExecutionEvent::Started(context.clone()));

        // Store execution context
        let tool_name = context.tool_name.clone();
        let mut active = self.active_executions.write().await;
        active.insert(execution_id.clone(), context);

        info!(execution_id = %execution_id, tool_name = %tool_name, "Tracking new execution");

        Ok(execution_id)
    }

    /// Update execution status
    #[instrument(skip(self), fields(execution_id, new_status))]
    pub async fn update_status(
        &self,
        execution_id: &str,
        new_status: ExecutionStatus,
    ) -> Result<()> {
        let mut active = self.active_executions.write().await;

        if let Some(context) = active.get_mut(execution_id) {
            // Update status
            context.update_status(new_status.clone());

            // Notify subscribers
            let _ = self.event_sender.send(ExecutionEvent::StatusUpdated(execution_id.to_string(), new_status.clone()));

            // Record metrics
            self.metrics.status_updated(&context.tool_name, &new_status.to_string());

            info!(execution_id = %execution_id, new_status = ?new_status, "Execution status updated");

            Ok(())
        } else {
            warn!(execution_id = %execution_id, "Execution not found for status update");
            Err(anyhow::anyhow!("Execution {} not found", execution_id))
        }
    }

    /// Complete execution with result
    #[instrument(skip(self), fields(execution_id, success))]
    pub async fn complete_execution(
        &self,
        execution_id: &str,
        result: ExecutionResult,
    ) -> Result<()> {
        let mut active = self.active_executions.write().await;
        let mut completed = self.completed_executions.write().await;

        if let Some(mut context) = active.remove(execution_id) {
            // Update final status
            context.update_status(if result.success {
                ExecutionStatus::Completed
            } else {
                ExecutionStatus::Failed
            });

            // Notify subscribers
            let _ = self.event_sender.send(ExecutionEvent::Completed(execution_id.to_string(), result.clone()));

            // Store result in metadata
            let mut metadata = context.metadata.as_object().cloned().unwrap_or_default();
            metadata.insert("result".to_string(), serde_json::to_value(result.clone()).unwrap());
            context.set_metadata(serde_json::Value::Object(metadata));

            // Record metrics
            if result.success {
                self.metrics.execution_succeeded(&context.tool_name, result.duration_ms);
            } else {
                self.metrics.execution_failed(&context.tool_name);
            }

            // End telemetry span
            self.telemetry.end_execution_span(&context, &result);

            // Store in completed executions
            completed.insert(execution_id.to_string(), context);

            // Trim history if needed
            if completed.len() > self.max_history {
                let keys: Vec<_> = completed.keys().take(completed.len() - self.max_history).cloned().collect();
                for key in keys {
                    completed.remove(&key);
                }
            }

            info!(execution_id = %execution_id, success = result.success, duration_ms = result.duration_ms, "Execution completed");

            Ok(())
        } else {
            warn!(execution_id = %execution_id, "Execution not found for completion");
            Err(anyhow::anyhow!("Execution {} not found", execution_id))
        }
    }

    /// Get execution context
    pub async fn get_execution(&self, execution_id: &str) -> Option<ExecutionContext> {
        // Check active executions first
        {
            let active = self.active_executions.read().await;
            if let Some(context) = active.get(execution_id) {
                return Some(context.clone());
            }
        }

        // Check completed executions
        {
            let completed = self.completed_executions.read().await;
            if let Some(context) = completed.get(execution_id) {
                return Some(context.clone());
            }
        }

        None
    }

    /// List active executions
    pub async fn list_active_executions(&self) -> Vec<ExecutionContext> {
        let active = self.active_executions.read().await;
        active.values().cloned().collect()
    }

    /// List recent completed executions
    pub async fn list_recent_completed(&self, limit: usize) -> Vec<ExecutionContext> {
        let completed = self.completed_executions.read().await;
        completed
            .values()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get metrics snapshot
    pub fn get_metrics(&self) -> Arc<ExecutionMetrics> {
        Arc::clone(&self.metrics)
    }
}

/// Execution tracker trait for integration
#[async_trait]
pub trait ExecutionTrackerTrait: Send + Sync {
    /// Track new execution
    async fn track_execution(&self, context: ExecutionContext) -> Result<String>;

    /// Update execution status
    async fn update_status(&self, execution_id: &str, status: ExecutionStatus) -> Result<()>;

    /// Complete execution
    async fn complete_execution(&self, execution_id: &str, result: ExecutionResult) -> Result<()>;

    /// Get execution context
    async fn get_execution(&self, execution_id: &str) -> Option<ExecutionContext>;
}

#[async_trait]
impl ExecutionTrackerTrait for ExecutionTracker {
    async fn track_execution(&self, context: ExecutionContext) -> Result<String> {
        self.track_execution(context).await
    }

    async fn update_status(&self, execution_id: &str, status: ExecutionStatus) -> Result<()> {
        self.update_status(execution_id, status).await
    }

    async fn complete_execution(&self, execution_id: &str, result: ExecutionResult) -> Result<()> {
        self.complete_execution(execution_id, result).await
    }

    async fn get_execution(&self, execution_id: &str) -> Option<ExecutionContext> {
        self.get_execution(execution_id).await
    }
}
