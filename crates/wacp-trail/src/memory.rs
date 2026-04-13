use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use wacp_types::WorkspaceId;

use crate::error::StorageError;
use crate::traits::*;

// ── In-memory trail storage ──

/// In-memory trail storage for testing.
pub struct InMemoryTrailStorage {
    /// Segments: each is a list of (entry_bytes, chain_hash).
    segments: Vec<Vec<(Vec<u8>, [u8; 32])>>,
    sequence_counter: u64,
    chain_head: [u8; 32],
    total_bytes: u64,
}

impl InMemoryTrailStorage {
    pub fn new() -> Self {
        Self {
            segments: vec![vec![]], // start with one empty segment
            sequence_counter: 0,
            chain_head: [0u8; 32],
            total_bytes: 0,
        }
    }
}

impl Default for InMemoryTrailStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl TrailStorage for InMemoryTrailStorage {
    fn append(
        &mut self,
        entry: &[u8],
        chain_hash: &[u8; 32],
    ) -> Result<WriteReceipt, StorageError> {
        let seg_idx = self.segments.len() - 1;
        let segment = self.segments.last_mut().unwrap();
        let offset = segment.len() as u64;

        segment.push((entry.to_vec(), *chain_hash));
        self.sequence_counter += 1;
        self.chain_head = *chain_hash;
        self.total_bytes += entry.len() as u64;

        Ok(WriteReceipt {
            segment: SegmentId(seg_idx as u64),
            offset,
            sequence: self.sequence_counter,
        })
    }

    fn read(&self, segment: SegmentId, offset: u64, _length: u32) -> Result<Vec<u8>, StorageError> {
        let seg = self
            .segments
            .get(segment.0 as usize)
            .ok_or(StorageError::SegmentNotFound(segment.0))?;
        let (entry, _) = seg.get(offset as usize).ok_or(StorageError::NotFound)?;
        Ok(entry.clone())
    }

    fn rotate(&mut self) -> Result<SegmentId, StorageError> {
        self.segments.push(vec![]);
        Ok(SegmentId((self.segments.len() - 1) as u64))
    }

    fn scan(
        &self,
        segment: SegmentId,
        from_offset: u64,
    ) -> Result<Vec<ScannedEntry>, StorageError> {
        let seg = self
            .segments
            .get(segment.0 as usize)
            .ok_or(StorageError::SegmentNotFound(segment.0))?;
        Ok(seg
            .iter()
            .enumerate()
            .skip(from_offset as usize)
            .map(|(i, (entry, hash))| (i as u64, entry.clone(), *hash))
            .collect())
    }

    fn metadata(&self) -> StorageMetadata {
        StorageMetadata {
            current_segment: SegmentId((self.segments.len() - 1) as u64),
            total_entries: self.sequence_counter,
            chain_head: self.chain_head,
            total_bytes: self.total_bytes,
        }
    }
}

// ── In-memory checkpoint storage ──

/// In-memory checkpoint storage for testing.
pub struct InMemoryCheckpointStorage {
    store: Mutex<HashMap<[u8; 32], Vec<u8>>>,
}

impl InMemoryCheckpointStorage {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<[u8; 32], Vec<u8>>> {
        self.store.lock().unwrap()
    }
}

impl Default for InMemoryCheckpointStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointStorage for InMemoryCheckpointStorage {
    fn store(&self, content_hash: &[u8; 32], payload: &[u8]) -> Result<bool, StorageError> {
        let mut store = self.lock();
        if store.contains_key(content_hash) {
            Ok(false)
        } else {
            store.insert(*content_hash, payload.to_vec());
            Ok(true)
        }
    }

    fn read(&self, content_hash: &[u8; 32]) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.lock().get(content_hash).cloned())
    }

    fn exists(&self, content_hash: &[u8; 32]) -> Result<bool, StorageError> {
        Ok(self.lock().contains_key(content_hash))
    }

    fn delete(&self, content_hash: &[u8; 32]) -> Result<bool, StorageError> {
        Ok(self.lock().remove(content_hash).is_some())
    }
}

// ── In-memory snapshot storage ──

/// In-memory snapshot storage for testing.
pub struct InMemorySnapshotStorage {
    workspaces: Mutex<HashMap<String, Vec<u8>>>,
    systems: Mutex<Vec<(u64, Vec<u8>)>>,
}

impl InMemorySnapshotStorage {
    pub fn new() -> Self {
        Self {
            workspaces: Mutex::new(HashMap::new()),
            systems: Mutex::new(Vec::new()),
        }
    }
}

impl Default for InMemorySnapshotStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotStorage for InMemorySnapshotStorage {
    fn write_workspace(&self, workspace_id: &WorkspaceId, data: &[u8]) -> Result<(), StorageError> {
        self.workspaces
            .lock()
            .unwrap()
            .insert(workspace_id.to_string(), data.to_vec());
        Ok(())
    }

    fn read_workspace(&self, workspace_id: &WorkspaceId) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self
            .workspaces
            .lock()
            .unwrap()
            .get(workspace_id.as_ref())
            .cloned())
    }

    fn delete_workspace(&self, workspace_id: &WorkspaceId) -> Result<(), StorageError> {
        self.workspaces
            .lock()
            .unwrap()
            .remove(workspace_id.as_ref());
        Ok(())
    }

    fn write_system(&self, sequence: u64, data: &[u8]) -> Result<(), StorageError> {
        self.systems.lock().unwrap().push((sequence, data.to_vec()));
        Ok(())
    }

    fn read_latest_system(&self) -> Result<Option<(u64, Vec<u8>)>, StorageError> {
        Ok(self.systems.lock().unwrap().last().cloned())
    }
}
