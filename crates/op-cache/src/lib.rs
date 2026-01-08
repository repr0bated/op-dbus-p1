//! op-cache: BTRFS-based caching with NUMA awareness and agent orchestration
//!
//! Features:
//! - BTRFS subvolume cache management
//! - NUMA-aware memory allocation
//! - Snapshot management for rollback
//! - Agent registry with capabilities
//! - Workstack orchestration with caching
//! - gRPC services

pub mod agent;
pub mod btrfs_cache;
pub mod numa;
pub mod orchestrator;
pub mod snapshot_manager;
pub mod workstack_cache;

#[cfg(feature = "grpc")]
pub mod grpc;

pub use agent::{Agent, AgentRegistry, Capability};
pub use btrfs_cache::BtrfsCache;
pub use numa::{NumaNode, NumaTopology};
pub use orchestrator::Orchestrator;
pub use snapshot_manager::SnapshotManager;
pub use workstack_cache::WorkstackCache;

/// Prelude for convenient imports
pub mod prelude {
    pub use super::agent::{Agent, AgentRegistry, Capability};
    pub use super::btrfs_cache::BtrfsCache;
    pub use super::numa::{NumaNode, NumaTopology};
    pub use super::orchestrator::Orchestrator;
    pub use super::snapshot_manager::SnapshotManager;
    pub use super::workstack_cache::WorkstackCache;
}
