//! OP State Store - Execution State Tracking and Job Ledger
//!
//! Provides persistent storage for execution jobs with state transitions:
//! REQUESTED → DISPATCHED → RUNNING → COMPLETED/FAILED
//!
//! Features:
//! - SQLite persistent storage
//! - Redis real-time stream
//! - Prometheus metrics
//! - OpenTelemetry tracing integration

pub mod error;
pub mod execution_job;
pub mod metrics;
pub mod redis_stream;
pub mod sqlite_store;
pub mod state_store;

pub use execution_job::{ExecutionJob, ExecutionStatus, ExecutionResult};
pub use state_store::StateStore;
pub use error::StateStoreError;