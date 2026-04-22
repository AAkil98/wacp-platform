use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use wacp_types::WorkspaceId;

use crate::error::StorageError;
use crate::traits::SnapshotStorage;

const CHECKSUM_LEN: usize = 32;

/// Filesystem-backed snapshot storage.
///
/// Layout:
///   <dir>/ws-<id>.snapshot           — per-workspace snapshots
///   <dir>/system-<seq>.snapshot      — system snapshots (zero-padded seq)
///   <dir>/system-latest.snapshot     — symlink to most recent system snapshot
///
/// File format: 32-byte SHA-256 checksum + raw payload.
pub struct FileSnapshotStorage {
    dir: PathBuf,
}

impl FileSnapshotStorage {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn workspace_path(&self, workspace_id: &WorkspaceId) -> PathBuf {
        self.dir.join(format!("ws-{}.snapshot", workspace_id))
    }

    fn system_path(&self, sequence: u64) -> PathBuf {
        self.dir.join(format!("system-{:012}.snapshot", sequence))
    }

    fn latest_link(&self) -> PathBuf {
        self.dir.join("system-latest.snapshot")
    }

    fn write_with_checksum(path: &PathBuf, data: &[u8]) -> Result<(), StorageError> {
        let checksum = Sha256::digest(data);
        let mut buf = Vec::with_capacity(CHECKSUM_LEN + data.len());
        buf.extend_from_slice(&checksum);
        buf.extend_from_slice(data);
        fs::write(path, &buf).map_err(|e| StorageError::Io(e.to_string()))
    }

    fn read_with_checksum(path: &PathBuf) -> Result<Option<Vec<u8>>, StorageError> {
        let raw = match fs::read(path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(StorageError::Io(e.to_string())),
        };

        if raw.len() < CHECKSUM_LEN {
            return Err(StorageError::Corruption(format!(
                "snapshot too small: {} bytes",
                raw.len()
            )));
        }

        let stored_checksum = &raw[..CHECKSUM_LEN];
        let payload = &raw[CHECKSUM_LEN..];
        let computed = Sha256::digest(payload);

        if stored_checksum != computed.as_slice() {
            return Err(StorageError::Corruption(format!(
                "snapshot checksum mismatch in {}",
                path.display()
            )));
        }

        Ok(Some(payload.to_vec()))
    }
}

impl SnapshotStorage for FileSnapshotStorage {
    fn write_workspace(&self, workspace_id: &WorkspaceId, data: &[u8]) -> Result<(), StorageError> {
        let path = self.workspace_path(workspace_id);
        Self::write_with_checksum(&path, data)
    }

    fn read_workspace(&self, workspace_id: &WorkspaceId) -> Result<Option<Vec<u8>>, StorageError> {
        let path = self.workspace_path(workspace_id);
        Self::read_with_checksum(&path)
    }

    fn delete_workspace(&self, workspace_id: &WorkspaceId) -> Result<(), StorageError> {
        let path = self.workspace_path(workspace_id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Io(e.to_string())),
        }
    }

    fn write_system(&self, sequence: u64, data: &[u8]) -> Result<(), StorageError> {
        let path = self.system_path(sequence);
        Self::write_with_checksum(&path, data)?;

        // Update latest symlink.
        let link = self.latest_link();
        let _ = fs::remove_file(&link); // ignore if missing
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&path, &link)
                .map_err(|e| StorageError::Io(e.to_string()))?;
        }
        #[cfg(not(unix))]
        {
            // Fallback: copy the file for non-unix.
            fs::copy(&path, &link).map_err(|e| StorageError::Io(e.to_string()))?;
        }

        Ok(())
    }

    fn read_latest_system(&self) -> Result<Option<(u64, Vec<u8>)>, StorageError> {
        // Scan for system-*.snapshot files, find the one with highest sequence.
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(StorageError::Io(e.to_string())),
        };

        let mut latest: Option<(u64, PathBuf)> = None;

        for entry in entries {
            let entry = entry.map_err(|e| StorageError::Io(e.to_string()))?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if let Some(seq_str) = name_str
                .strip_prefix("system-")
                .and_then(|s| s.strip_suffix(".snapshot"))
            {
                if seq_str == "latest" {
                    continue;
                }
                if let Ok(seq) = seq_str.parse::<u64>() {
                    match &latest {
                        Some((prev_seq, _)) if seq > *prev_seq => {
                            latest = Some((seq, entry.path()));
                        }
                        None => {
                            latest = Some((seq, entry.path()));
                        }
                        _ => {}
                    }
                }
            }
        }

        match latest {
            Some((seq, path)) => {
                let data = Self::read_with_checksum(&path)?;
                Ok(data.map(|d| (seq, d)))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_storage() -> (tempfile::TempDir, FileSnapshotStorage) {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileSnapshotStorage::new(dir.path().to_path_buf());
        (dir, storage)
    }

    #[test]
    fn workspace_write_read() {
        let (_dir, storage) = test_storage();
        let ws = WorkspaceId::from("ws-1");
        let data = b"workspace state data";

        storage.write_workspace(&ws, data).unwrap();
        let read = storage.read_workspace(&ws).unwrap();
        assert_eq!(read, Some(data.to_vec()));
    }

    #[test]
    fn workspace_overwrite() {
        let (_dir, storage) = test_storage();
        let ws = WorkspaceId::from("ws-1");

        storage.write_workspace(&ws, b"v1").unwrap();
        storage.write_workspace(&ws, b"v2").unwrap();
        let read = storage.read_workspace(&ws).unwrap();
        assert_eq!(read, Some(b"v2".to_vec()));
    }

    #[test]
    fn workspace_delete() {
        let (_dir, storage) = test_storage();
        let ws = WorkspaceId::from("ws-1");

        storage.write_workspace(&ws, b"data").unwrap();
        storage.delete_workspace(&ws).unwrap();
        assert_eq!(storage.read_workspace(&ws).unwrap(), None);
    }

    #[test]
    fn workspace_not_found() {
        let (_dir, storage) = test_storage();
        let ws = WorkspaceId::from("ws-nonexistent");
        assert_eq!(storage.read_workspace(&ws).unwrap(), None);
    }

    #[test]
    fn system_write_read() {
        let (_dir, storage) = test_storage();
        let data = b"system snapshot payload";

        storage.write_system(10000, data).unwrap();
        let latest = storage.read_latest_system().unwrap();
        assert_eq!(latest, Some((10000, data.to_vec())));
    }

    #[test]
    fn system_latest_returns_highest_sequence() {
        let (_dir, storage) = test_storage();

        storage.write_system(5000, b"snap-5000").unwrap();
        storage.write_system(10000, b"snap-10000").unwrap();
        storage.write_system(7000, b"snap-7000").unwrap();

        let (seq, data) = storage.read_latest_system().unwrap().unwrap();
        assert_eq!(seq, 10000);
        assert_eq!(data, b"snap-10000");
    }

    #[test]
    fn system_checksum_corruption() {
        let (_dir, storage) = test_storage();
        storage.write_system(100, b"valid data").unwrap();

        // Corrupt the file by changing a byte in the payload.
        let path = storage.system_path(100);
        let mut raw = fs::read(&path).unwrap();
        if let Some(last) = raw.last_mut() {
            *last ^= 0xFF;
        }
        fs::write(&path, &raw).unwrap();

        let result = storage.read_latest_system();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("checksum"));
    }

    #[test]
    fn system_no_snapshots() {
        let (_dir, storage) = test_storage();
        assert_eq!(storage.read_latest_system().unwrap(), None);
    }

    // ── Phase 18b.1: Snapshot error paths ──

    #[test]
    fn snapshot_partial_file_detected() {
        let (dir, storage) = test_storage();
        let ws = WorkspaceId::from("ws-partial");

        // Write a file shorter than CHECKSUM_LEN (32 bytes).
        let path = dir.path().join("ws-ws-partial.snapshot");
        fs::write(&path, b"only 10 bytes!").unwrap();

        let result = storage.read_workspace(&ws);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("too small"), "got: {err}");
    }

    #[test]
    fn snapshot_zero_byte_file_detected() {
        let (dir, storage) = test_storage();
        let ws = WorkspaceId::from("ws-empty");

        let path = dir.path().join("ws-ws-empty.snapshot");
        fs::write(&path, b"").unwrap();

        let result = storage.read_workspace(&ws);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too small"));
    }

    #[test]
    fn snapshot_exactly_checksum_size_wrong_hash() {
        let (dir, storage) = test_storage();
        let ws = WorkspaceId::from("ws-exact");

        // 32 bytes: all zeros (checksum for empty payload is NOT all zeros).
        let path = dir.path().join("ws-ws-exact.snapshot");
        fs::write(&path, [0u8; 32]).unwrap();

        let result = storage.read_workspace(&ws);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("checksum"));
    }

    #[test]
    fn system_snapshot_partial_file() {
        let (dir, storage) = test_storage();

        // Write a system snapshot file that's too small.
        let path = dir.path().join("system-000000000099.snapshot");
        fs::write(&path, b"short").unwrap();

        let result = storage.read_latest_system();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too small"));
    }

    #[test]
    fn workspace_delete_nonexistent_is_ok() {
        let (_dir, storage) = test_storage();
        let ws = WorkspaceId::from("ws-ghost");
        // Deleting a non-existent workspace should not error.
        assert!(storage.delete_workspace(&ws).is_ok());
    }
}
