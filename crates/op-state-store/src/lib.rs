//! OP State Store - Execution State Tracking and Job Ledger
//!
//! Provides persistent storage for execution jobs with state transitions:
//! REQUESTED → DISPATCHED → RUNNING → COMPLETED/FAILED
//!
//! Features:
//! - SQLite persistent storage
//! - Redis real-time stream
//! - Prometheus metrics
//! - Plugin schema registry
//! - Disaster recovery export/import
//! - OpenTelemetry tracing integration

pub mod disaster_recovery;
pub mod error;
pub mod execution_job;
pub mod metrics;
pub mod plugin_schema;
pub mod redis_stream;
pub mod sqlite_store;
pub mod state_store;

pub use disaster_recovery::{
    DisasterRecoveryExport, HostInfo, PluginStateExport, RestoreResult, SystemDependency,
    get_global_dependencies, get_plugin_dependencies,
};
pub use error::StateStoreError;
pub use execution_job::{ExecutionJob, ExecutionResult, ExecutionStatus};
pub use plugin_schema::{PluginSchema, SchemaRegistry, ValidationResult as SchemaValidationResult};
pub use redis_stream::RedisStream;
pub use sqlite_store::SqliteStore;
pub use state_store::StateStore;