//! OP Execution Tracker - Lightweight Execution Monitoring Layer
//!
//! Complements existing state management by providing:
//! - Execution acknowledgment protocol
//! - Real-time execution tracking
//! - Integration with existing workflow/orchestration states
//! - Observability without duplicating state management

pub mod execution_context;
pub mod execution_tracker;
pub mod metrics;
pub mod telemetry;

pub use execution_context::{ExecutionContext, ExecutionStatus, ExecutionResult};
pub use execution_tracker::{ExecutionTracker, ExecutionEvent};
pub use metrics::ExecutionMetrics;
pub use telemetry::ExecutionTelemetry;