//! BTRFS snapshot management with automatic rotation
//!
//! Manages cache snapshots with configurable retention policy

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct SnapshotConfig {
    /// Base path for snapshots (e.g., /var/lib/op-dbus/@cache-snapshots)
    pub snapshot_dir: PathBuf,

    /// Maximum number of snapshots to keep (default: 24)
    pub max_snapshots: usize,

    /// Snapshot name prefix (default: "cache")
    pub prefix: String,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            snapshot_dir: PathBuf::from("/var/lib/op-dbus/@cache-snapshots"),
            max_snapshots: 24, // Keep 24 hourly snapshots = 1 day
            prefix: "cache".to_string(),
        }
    }
}

pub struct SnapshotManager {
    config: SnapshotConfig,
    source_subvol: PathBuf,
}

impl SnapshotManager {
    /// Create new snapshot manager
    pub fn new(source_subvol: PathBuf, config: SnapshotConfig) -> Self {
        Self {
            config,
            source_subvol,
        }
    }

    /// Create snapshot with automatic rotation
    pub async fn create_snapshot(&self) -> Result<PathBuf> {
        // Create snapshot directory if it doesn't exist
        tokio::fs::create_dir_all(&self.config.snapshot_dir).await?;

        // Generate snapshot name with timestamp
        let timestamp = chrono::Utc::now().format("%Y-%m-%d-%H:%M:%S");
        let snapshot_name = format!("{}@{}", self.config.prefix, timestamp);
        let snapshot_path = self.config.snapshot_dir.join(&snapshot_name);

        log::info!("Creating BTRFS snapshot: {}", snapshot_name);

        // Create readonly snapshot
        let output = Command::new("btrfs")
            .args(["subvolume", "snapshot", "-r"])
            .arg(&self.source_subvol)
            .arg(&snapshot_path)
            .output()
            .await
            .context("Failed to execute btrfs snapshot command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create snapshot: {}", stderr);
        }

        log::info!("Created snapshot: {}", snapshot_path.display());

        // Rotate old snapshots
        self.rotate_snapshots().await?;

        Ok(snapshot_path)
    }

    /// List all snapshots for this cache
    pub async fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>> {
        let mut snapshots = Vec::new();

        if !self.config.snapshot_dir.exists() {
            return Ok(snapshots);
        }

        let mut entries = tokio::fs::read_dir(&self.config.snapshot_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Filter by prefix
            if !name_str.starts_with(&format!("{}@", self.config.prefix)) {
                continue;
            }

            let path = entry.path();
            let metadata = tokio::fs::metadata(&path).await?;

            // Parse timestamp from name
            if let Some(timestamp_str) = name_str.strip_prefix(&format!("{}@", self.config.prefix))
            {
                snapshots.push(SnapshotInfo {
                    name: name_str.to_string(),
                    path: path.clone(),
                    created: metadata.created().ok(),
                    timestamp_str: timestamp_str.to_string(),
                });
            }
        }

        // Sort by timestamp (oldest first)
        snapshots.sort_by(|a, b| a.timestamp_str.cmp(&b.timestamp_str));

        Ok(snapshots)
    }

    /// Rotate snapshots, keeping only max_snapshots
    async fn rotate_snapshots(&self) -> Result<()> {
        let snapshots = self.list_snapshots().await?;

        if snapshots.len() <= self.config.max_snapshots {
            log::debug!(
                "Snapshot count {} within limit {}",
                snapshots.len(),
                self.config.max_snapshots
            );
            return Ok(());
        }

        // Calculate how many to delete
        let to_delete = snapshots.len() - self.config.max_snapshots;

        log::info!(
            "Rotating snapshots: {} total, keeping {}, deleting {}",
            snapshots.len(),
            self.config.max_snapshots,
            to_delete
        );

        // Delete oldest snapshots
        for snapshot in snapshots.iter().take(to_delete) {
            log::info!("Deleting old snapshot: {}", snapshot.name);
            self.delete_snapshot(&snapshot.path).await?;
        }

        Ok(())
    }

    /// Delete a specific snapshot
    pub async fn delete_snapshot(&self, snapshot_path: &Path) -> Result<()> {
        log::debug!("Deleting snapshot: {}", snapshot_path.display());

        let output = Command::new("btrfs")
            .args(["subvolume", "delete"])
            .arg(snapshot_path)
            .output()
            .await
            .context("Failed to execute btrfs delete command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to delete snapshot: {}", stderr);
        }

        Ok(())
    }

    /// Delete all snapshots
    pub async fn delete_all_snapshots(&self) -> Result<usize> {
        let snapshots = self.list_snapshots().await?;
        let count = snapshots.len();

        for snapshot in snapshots {
            self.delete_snapshot(&snapshot.path).await?;
        }

        log::info!("Deleted {} snapshots", count);
        Ok(count)
    }

    /// Get oldest snapshot
    #[allow(dead_code)]
    pub async fn oldest_snapshot(&self) -> Result<Option<SnapshotInfo>> {
        let snapshots = self.list_snapshots().await?;
        Ok(snapshots.into_iter().next())
    }

    /// Get newest snapshot
    #[allow(dead_code)]
    pub async fn newest_snapshot(&self) -> Result<Option<SnapshotInfo>> {
        let snapshots = self.list_snapshots().await?;
        Ok(snapshots.into_iter().last())
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub name: String,
    pub path: PathBuf,
    #[allow(dead_code)]
    pub created: Option<std::time::SystemTime>,
    pub timestamp_str: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_config_defaults() {
        let config = SnapshotConfig::default();
        assert_eq!(config.max_snapshots, 24);
        assert_eq!(config.prefix, "cache");
    }
}
