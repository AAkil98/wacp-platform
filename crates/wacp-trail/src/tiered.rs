use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::StorageError;

/// Storage tier for a trail segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentTier {
    Hot,
    Warm,
    Cold,
}

/// Metadata for a trail segment.
#[derive(Debug, Clone)]
pub struct SegmentInfo {
    pub id: u64,
    pub tier: SegmentTier,
    pub path: PathBuf,
    pub created: SystemTime,
    pub size_bytes: u64,
}

/// Manages tier transitions for trail segments.
///
/// Hot tier: active + recent sealed segments (uncompressed).
/// Warm tier: older segments compressed with zstd.
/// Cold tier: segments moved to external storage (optional).
pub struct TierManager {
    trail_dir: PathBuf,
    warm_dir: PathBuf,
    cold_destination: Option<PathBuf>,
    hot_limit: u32,
    warm_retention_days: u32,
    /// Sequence number of the latest snapshot anchor.
    /// Segments containing entries at or after this sequence stay hot.
    snapshot_anchor: Option<u64>,
}

impl TierManager {
    pub fn new(
        trail_dir: PathBuf,
        hot_limit: u32,
        warm_retention_days: u32,
        cold_destination: Option<PathBuf>,
    ) -> Result<Self, StorageError> {
        let warm_dir = trail_dir.join("warm");
        fs::create_dir_all(&warm_dir).map_err(|e| StorageError::Io(e.to_string()))?;

        if let Some(ref cold) = cold_destination {
            fs::create_dir_all(cold).map_err(|e| StorageError::Io(e.to_string()))?;
        }

        Ok(Self {
            trail_dir,
            warm_dir,
            cold_destination,
            hot_limit,
            warm_retention_days,
            snapshot_anchor: None,
        })
    }

    /// Update the snapshot anchor (recovery window boundary).
    pub fn set_snapshot_anchor(&mut self, sequence: u64) {
        self.snapshot_anchor = Some(sequence);
    }

    /// List sealed hot-tier segments (not the active segment).
    /// Segments are identified by their numeric ID extracted from filename.
    pub fn list_hot_segments(&self) -> Result<Vec<SegmentInfo>, StorageError> {
        self.list_segments_in(&self.trail_dir, SegmentTier::Hot, "segment-", ".trail")
    }

    /// List warm-tier segments.
    pub fn list_warm_segments(&self) -> Result<Vec<SegmentInfo>, StorageError> {
        self.list_segments_in(&self.warm_dir, SegmentTier::Warm, "segment-", ".trail.zst")
    }

    /// Transition eligible hot segments to warm tier (compress with zstd).
    /// Returns the number of segments transitioned.
    pub fn transition_hot_to_warm(&self) -> Result<usize, StorageError> {
        let mut hot = self.list_hot_segments()?;
        if hot.len() <= self.hot_limit as usize {
            return Ok(0);
        }

        // Sort by id ascending — oldest first.
        hot.sort_by_key(|s| s.id);

        let excess = hot.len() - self.hot_limit as usize;
        let mut transitioned = 0;

        for seg in hot.iter().take(excess) {
            // Recovery window check: don't move segments at or after snapshot anchor.
            if let Some(anchor) = self.snapshot_anchor
                && seg.id >= anchor
            {
                continue;
            }

            let warm_path = self
                .warm_dir
                .join(format!("segment-{:06}.trail.zst", seg.id));
            compress_file(&seg.path, &warm_path)?;
            fs::remove_file(&seg.path).map_err(|e| StorageError::Io(e.to_string()))?;
            transitioned += 1;
        }

        Ok(transitioned)
    }

    /// Transition aged warm segments to cold tier.
    /// Returns the number of segments transitioned.
    pub fn transition_warm_to_cold(&self) -> Result<usize, StorageError> {
        let cold_dir = match &self.cold_destination {
            Some(dir) => dir,
            None => return Ok(0),
        };

        let warm = self.list_warm_segments()?;
        let now = SystemTime::now();
        let retention = std::time::Duration::from_secs(self.warm_retention_days as u64 * 86400);
        let mut transitioned = 0;

        for seg in &warm {
            let age = now.duration_since(seg.created).unwrap_or_default();
            if age > retention {
                let dest = cold_dir.join(seg.path.file_name().unwrap());
                fs::rename(&seg.path, &dest).map_err(|e| StorageError::Io(e.to_string()))?;
                transitioned += 1;
            }
        }

        Ok(transitioned)
    }

    fn list_segments_in(
        &self,
        dir: &Path,
        tier: SegmentTier,
        prefix: &str,
        suffix: &str,
    ) -> Result<Vec<SegmentInfo>, StorageError> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(StorageError::Io(e.to_string())),
        };

        let mut segments = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| StorageError::Io(e.to_string()))?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if let Some(id_str) = name_str.strip_prefix(prefix).and_then(|s| s.strip_suffix(suffix))
                && let Ok(id) = id_str.parse::<u64>()
            {
                let meta = entry.metadata().map_err(|e| StorageError::Io(e.to_string()))?;
                segments.push(SegmentInfo {
                    id,
                    tier,
                    path: entry.path(),
                    created: meta.created().unwrap_or(SystemTime::UNIX_EPOCH),
                    size_bytes: meta.len(),
                });
            }
        }

        Ok(segments)
    }
}

/// Compress a file with zstd (level 3 — good balance of speed and ratio).
pub fn compress_file(src: &Path, dst: &Path) -> Result<(), StorageError> {
    let input = fs::read(src).map_err(|e| StorageError::Io(e.to_string()))?;
    let compressed =
        zstd::encode_all(input.as_slice(), 3).map_err(|e| StorageError::Io(e.to_string()))?;
    fs::write(dst, &compressed).map_err(|e| StorageError::Io(e.to_string()))
}

/// Decompress a zstd-compressed file.
pub fn decompress_file(src: &Path, dst: &Path) -> Result<(), StorageError> {
    let compressed = fs::read(src).map_err(|e| StorageError::Io(e.to_string()))?;
    let mut decoder = zstd::Decoder::new(compressed.as_slice())
        .map_err(|e| StorageError::Io(e.to_string()))?;
    let mut output = Vec::new();
    decoder
        .read_to_end(&mut output)
        .map_err(|e| StorageError::Io(e.to_string()))?;
    let mut file =
        fs::File::create(dst).map_err(|e| StorageError::Io(e.to_string()))?;
    file.write_all(&output)
        .map_err(|e| StorageError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_decompress_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("input.trail");
        let compressed = dir.path().join("input.trail.zst");
        let decompressed = dir.path().join("output.trail");

        let data = b"trail segment data with lots of repeated content for compression ";
        let mut big_data = Vec::new();
        for _ in 0..100 {
            big_data.extend_from_slice(data);
        }
        fs::write(&src, &big_data).unwrap();

        compress_file(&src, &compressed).unwrap();
        assert!(compressed.exists());
        // Compressed should be smaller than original.
        assert!(fs::metadata(&compressed).unwrap().len() < big_data.len() as u64);

        decompress_file(&compressed, &decompressed).unwrap();
        let result = fs::read(&decompressed).unwrap();
        assert_eq!(result, big_data);
    }

    #[test]
    fn hot_to_warm_respects_limit() {
        let dir = tempfile::tempdir().unwrap();
        let trail_dir = dir.path().to_path_buf();

        // Create 5 sealed segment files.
        for i in 0..5u64 {
            let path = trail_dir.join(format!("segment-{i:06}.trail"));
            fs::write(&path, format!("segment {i} data")).unwrap();
        }

        let mgr = TierManager::new(trail_dir, 3, 90, None).unwrap();
        let moved = mgr.transition_hot_to_warm().unwrap();
        // 5 segments, limit 3 → 2 should move.
        assert_eq!(moved, 2);
        assert_eq!(mgr.list_hot_segments().unwrap().len(), 3);
        assert_eq!(mgr.list_warm_segments().unwrap().len(), 2);
    }

    #[test]
    fn recovery_window_prevents_transition() {
        let dir = tempfile::tempdir().unwrap();
        let trail_dir = dir.path().to_path_buf();

        // Create 5 segments.
        for i in 0..5u64 {
            let path = trail_dir.join(format!("segment-{i:06}.trail"));
            fs::write(&path, format!("data {i}")).unwrap();
        }

        let mut mgr = TierManager::new(trail_dir, 2, 90, None).unwrap();
        // Set snapshot anchor at segment 2 — segments 2+ must stay hot.
        mgr.set_snapshot_anchor(2);

        let moved = mgr.transition_hot_to_warm().unwrap();
        // Only segments 0 and 1 can move (before anchor). Segments 2,3,4 must stay.
        assert_eq!(moved, 2);
        let hot = mgr.list_hot_segments().unwrap();
        assert!(hot.iter().all(|s| s.id >= 2));
    }

    #[test]
    fn warm_to_cold_moves_file() {
        let dir = tempfile::tempdir().unwrap();
        let trail_dir = dir.path().join("trail");
        let cold_dir = dir.path().join("cold");
        fs::create_dir_all(&trail_dir).unwrap();

        let mgr = TierManager::new(trail_dir, 10, 0, Some(cold_dir.clone())).unwrap();
        // 0 retention days → everything moves immediately.

        // Create a warm segment.
        let warm_path = mgr.warm_dir.join("segment-000001.trail.zst");
        fs::write(&warm_path, b"compressed data").unwrap();

        let moved = mgr.transition_warm_to_cold().unwrap();
        assert_eq!(moved, 1);
        assert!(cold_dir.join("segment-000001.trail.zst").exists());
        assert!(!warm_path.exists());
    }

    #[test]
    fn no_cold_destination_skips() {
        let dir = tempfile::tempdir().unwrap();
        let trail_dir = dir.path().to_path_buf();

        let mgr = TierManager::new(trail_dir, 10, 0, None).unwrap();
        let moved = mgr.transition_warm_to_cold().unwrap();
        assert_eq!(moved, 0);
    }

    #[test]
    fn segment_listing() {
        let dir = tempfile::tempdir().unwrap();
        let trail_dir = dir.path().to_path_buf();

        // Create hot segments.
        for i in 0..3u64 {
            fs::write(
                trail_dir.join(format!("segment-{i:06}.trail")),
                "data",
            )
            .unwrap();
        }

        let mgr = TierManager::new(trail_dir, 10, 90, None).unwrap();
        let hot = mgr.list_hot_segments().unwrap();
        assert_eq!(hot.len(), 3);
        assert!(hot.iter().all(|s| s.tier == SegmentTier::Hot));
    }
}
