//! op-cache: BTRFS-based caching with NUMA awareness
//!
//! Features:
//! - BTRFS subvolume cache management
//! - NUMA-aware memory allocation
//! - Snapshot management for rollback

pub mod btrfs_cache;
pub mod numa;
pub mod snapshot_manager;

pub use btrfs_cache::BtrfsCache;
pub use numa::{NumaNode, NumaTopology};
pub use snapshot_manager::SnapshotManager;

/// Prelude for convenient imports
pub mod prelude {
    pub use super::btrfs_cache::BtrfsCache;
    pub use super::numa::{NumaNode, NumaTopology};
    pub use super::snapshot_manager::SnapshotManager;
}
