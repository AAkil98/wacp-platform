use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::error::StorageError;
use crate::tiered::TierManager;

/// Configuration for the compaction task.
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    pub hot_limit: u32,
    pub warm_retention_days: u32,
    /// None = indefinite (never delete cold segments).
    pub cold_retention_days: Option<u64>,
    pub compaction_interval_minutes: u32,
}

/// Background compaction task.
///
/// Runs periodically to manage tier transitions and cleanup:
/// 1. Hot → warm compression (excess sealed segments)
/// 2. Warm → cold movement (aged segments)
/// 3. Cold retention enforcement (delete segments beyond retention)
///
/// Every deletion is logged before execution (no-silent-deletion invariant).
/// In the full system, deletion events are recorded as trail entries.
/// This implementation provides the mechanics; trail recording is wired
/// by the coordinator when driving compaction.
pub struct CompactionTask {
    tier_manager: TierManager,
    config: CompactionConfig,
    cold_dir: Option<PathBuf>,
}

impl CompactionTask {
    pub fn new(
        trail_dir: PathBuf,
        cold_destination: Option<PathBuf>,
        config: CompactionConfig,
    ) -> Result<Self, StorageError> {
        let tier_manager = TierManager::new(
            trail_dir,
            config.hot_limit,
            config.warm_retention_days,
            cold_destination.clone(),
        )?;

        Ok(Self {
            tier_manager,
            config,
            cold_dir: cold_destination,
        })
    }

    /// Update the snapshot anchor for recovery window enforcement.
    pub fn set_snapshot_anchor(&mut self, sequence: u64) {
        self.tier_manager.set_snapshot_anchor(sequence);
    }

    /// Execute one compaction cycle. Returns a summary of actions taken.
    pub fn run_once(&self) -> Result<CompactionSummary, StorageError> {
        let hot_to_warm = self.tier_manager.transition_hot_to_warm()?;
        let warm_to_cold = self.tier_manager.transition_warm_to_cold()?;
        let cold_deleted = self.cleanup_cold_segments()?;

        Ok(CompactionSummary {
            hot_to_warm,
            warm_to_cold,
            cold_deleted,
        })
    }

    /// Delete cold segments beyond the retention window.
    fn cleanup_cold_segments(&self) -> Result<usize, StorageError> {
        let retention_days = match self.config.cold_retention_days {
            Some(days) => days,
            None => return Ok(0), // indefinite retention
        };

        let cold_dir = match &self.cold_dir {
            Some(dir) => dir,
            None => return Ok(0),
        };

        let entries = match fs::read_dir(cold_dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(StorageError::Io(e.to_string())),
        };

        let now = SystemTime::now();
        let retention = std::time::Duration::from_secs(retention_days * 86400);
        let mut deleted = 0;

        for entry in entries {
            let entry = entry.map_err(|e| StorageError::Io(e.to_string()))?;
            let meta = entry
                .metadata()
                .map_err(|e| StorageError::Io(e.to_string()))?;
            let created = meta.created().unwrap_or(SystemTime::UNIX_EPOCH);
            let age = now.duration_since(created).unwrap_or_default();

            if age > retention {
                // No-silent-deletion: in the full system, a trail_segment_deleted
                // entry is recorded before this remove. The caller is responsible
                // for recording the trail entry.
                fs::remove_file(entry.path()).map_err(|e| StorageError::Io(e.to_string()))?;
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    /// Merge small warm segments into larger ones.
    /// Segments smaller than `threshold_bytes` are candidates for merging.
    /// Returns the number of merge operations performed.
    pub fn merge_warm_segments(&self, threshold_bytes: u64) -> Result<usize, StorageError> {
        let warm = self.tier_manager.list_warm_segments()?;
        let mut small: Vec<_> = warm
            .iter()
            .filter(|s| s.size_bytes < threshold_bytes)
            .collect();

        if small.len() < 2 {
            return Ok(0);
        }

        // readdir order is filesystem-dependent; sort by id so the merged
        // segment keeps the lower id and warm-tier ordering stays monotonic.
        small.sort_by_key(|s| s.id);

        // Merge pairs of consecutive small segments.
        let mut merged = 0;
        let mut i = 0;
        while i + 1 < small.len() {
            let a = &small[i];
            let b = &small[i + 1];

            // Read both compressed files, decompress, concatenate, recompress.
            let data_a = fs::read(&a.path).map_err(|e| StorageError::Io(e.to_string()))?;
            let data_b = fs::read(&b.path).map_err(|e| StorageError::Io(e.to_string()))?;

            let dec_a =
                zstd::decode_all(data_a.as_slice()).map_err(|e| StorageError::Io(e.to_string()))?;
            let dec_b =
                zstd::decode_all(data_b.as_slice()).map_err(|e| StorageError::Io(e.to_string()))?;

            let mut combined = dec_a;
            combined.extend_from_slice(&dec_b);

            let compressed = zstd::encode_all(combined.as_slice(), 3)
                .map_err(|e| StorageError::Io(e.to_string()))?;

            // Write merged file with lower id, remove both originals.
            fs::write(&a.path, &compressed).map_err(|e| StorageError::Io(e.to_string()))?;
            fs::remove_file(&b.path).map_err(|e| StorageError::Io(e.to_string()))?;

            merged += 1;
            i += 2;
        }

        Ok(merged)
    }
}

/// Summary of a single compaction cycle.
#[derive(Debug, Default)]
pub struct CompactionSummary {
    pub hot_to_warm: usize,
    pub warm_to_cold: usize,
    pub cold_deleted: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(
        dir: &std::path::Path,
        hot_limit: u32,
        cold_retention: Option<u64>,
    ) -> CompactionTask {
        let cold_dir = dir.join("cold");
        CompactionTask::new(
            dir.join("trail"),
            Some(cold_dir),
            CompactionConfig {
                hot_limit,
                warm_retention_days: 0, // immediate warm→cold
                cold_retention_days: cold_retention,
                compaction_interval_minutes: 60,
            },
        )
        .unwrap()
    }

    #[test]
    fn compaction_empty_trail() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("trail")).unwrap();
        let task = make_task(dir.path(), 10, None);
        let summary = task.run_once().unwrap();
        assert_eq!(summary.hot_to_warm, 0);
        assert_eq!(summary.warm_to_cold, 0);
        assert_eq!(summary.cold_deleted, 0);
    }

    #[test]
    fn compaction_triggers_hot_warm() {
        let dir = tempfile::tempdir().unwrap();
        let trail_dir = dir.path().join("trail");
        fs::create_dir_all(&trail_dir).unwrap();

        // Create 5 hot segments.
        for i in 0..5u64 {
            fs::write(
                trail_dir.join(format!("segment-{i:06}.trail")),
                format!("data {i}"),
            )
            .unwrap();
        }

        let task = make_task(dir.path(), 3, None);
        let summary = task.run_once().unwrap();
        assert_eq!(summary.hot_to_warm, 2);
    }

    #[test]
    fn compaction_cold_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let trail_dir = dir.path().join("trail");
        let cold_dir = dir.path().join("cold");
        fs::create_dir_all(&trail_dir).unwrap();
        fs::create_dir_all(&cold_dir).unwrap();

        // Create a cold segment file (it will have "now" as creation time,
        // and with 0 retention days it should be deleted).
        fs::write(cold_dir.join("segment-000001.trail.zst"), b"old data").unwrap();

        let task = CompactionTask::new(
            trail_dir,
            Some(cold_dir.clone()),
            CompactionConfig {
                hot_limit: 10,
                warm_retention_days: 90,
                cold_retention_days: Some(0), // 0 days = delete immediately
                compaction_interval_minutes: 60,
            },
        )
        .unwrap();

        let summary = task.run_once().unwrap();
        assert_eq!(summary.cold_deleted, 1);
        assert!(!cold_dir.join("segment-000001.trail.zst").exists());
    }

    #[test]
    fn compaction_indefinite_retention() {
        let dir = tempfile::tempdir().unwrap();
        let trail_dir = dir.path().join("trail");
        let cold_dir = dir.path().join("cold");
        fs::create_dir_all(&trail_dir).unwrap();
        fs::create_dir_all(&cold_dir).unwrap();

        fs::write(cold_dir.join("segment-000001.trail.zst"), b"data").unwrap();

        let task = CompactionTask::new(
            trail_dir,
            Some(cold_dir.clone()),
            CompactionConfig {
                hot_limit: 10,
                warm_retention_days: 90,
                cold_retention_days: None, // indefinite
                compaction_interval_minutes: 60,
            },
        )
        .unwrap();

        let summary = task.run_once().unwrap();
        assert_eq!(summary.cold_deleted, 0);
        assert!(cold_dir.join("segment-000001.trail.zst").exists());
    }

    #[test]
    fn merge_warm_combines_files() {
        let dir = tempfile::tempdir().unwrap();
        let trail_dir = dir.path().join("trail");
        fs::create_dir_all(&trail_dir).unwrap();

        let task = make_task(dir.path(), 10, None);

        // Create two small warm segments.
        let warm_dir = trail_dir.join("warm");
        let data_a = zstd::encode_all(b"aaaa".as_slice(), 3).unwrap();
        let data_b = zstd::encode_all(b"bbbb".as_slice(), 3).unwrap();
        fs::write(warm_dir.join("segment-000001.trail.zst"), &data_a).unwrap();
        fs::write(warm_dir.join("segment-000002.trail.zst"), &data_b).unwrap();

        let merged = task.merge_warm_segments(1_000_000).unwrap();
        assert_eq!(merged, 1);

        // First file should contain merged data, second should be gone.
        assert!(warm_dir.join("segment-000001.trail.zst").exists());
        assert!(!warm_dir.join("segment-000002.trail.zst").exists());

        // Decompress and verify combined content.
        let combined_compressed = fs::read(warm_dir.join("segment-000001.trail.zst")).unwrap();
        let combined = zstd::decode_all(combined_compressed.as_slice()).unwrap();
        assert_eq!(combined, b"aaaabbbb");
    }
}
