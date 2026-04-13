use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::error::StorageError;
use crate::traits::CheckpointStorage;

/// Content-addressable filesystem checkpoint store (storage spec §5).
pub struct FileCheckpointStorage {
    dir: PathBuf,
}

impl FileCheckpointStorage {
    pub fn open(dir: PathBuf) -> Result<Self, StorageError> {
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn blob_path(&self, hash: &[u8; 32]) -> PathBuf {
        let hex = hex_encode(hash);
        let shard = &hex[..2];
        self.dir.join(shard).join(format!("{hex}.blob"))
    }
}

impl CheckpointStorage for FileCheckpointStorage {
    fn store(&self, content_hash: &[u8; 32], payload: &[u8]) -> Result<bool, StorageError> {
        let path = self.blob_path(content_hash);
        if path.exists() {
            return Ok(false); // deduplicated
        }

        // Create shard directory.
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::File::create(&path)?;
        file.write_all(payload)?;
        file.sync_all()?;
        Ok(true)
    }

    fn read(&self, content_hash: &[u8; 32]) -> Result<Option<Vec<u8>>, StorageError> {
        let path = self.blob_path(content_hash);
        if !path.exists() {
            return Ok(None);
        }

        let data = fs::read(&path)?;

        // Integrity verification on read.
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let computed: [u8; 32] = hasher.finalize().into();
        if computed != *content_hash {
            return Err(StorageError::Corruption(format!(
                "checkpoint hash mismatch: expected {}, computed {}",
                hex_encode(content_hash),
                hex_encode(&computed),
            )));
        }

        Ok(Some(data))
    }

    fn exists(&self, content_hash: &[u8; 32]) -> Result<bool, StorageError> {
        Ok(self.blob_path(content_hash).exists())
    }

    fn delete(&self, content_hash: &[u8; 32]) -> Result<bool, StorageError> {
        let path = self.blob_path(content_hash);
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(path)?;
        Ok(true)
    }
}

fn hex_encode(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
