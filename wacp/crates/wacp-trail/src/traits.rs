use wacp_types::WorkspaceId;

use crate::error::StorageError;

/// Identifies a trail segment file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegmentId(pub u64);

impl std::fmt::Display for SegmentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Receipt from a successful trail write.
#[derive(Debug, Clone)]
pub struct WriteReceipt {
    pub segment: SegmentId,
    pub offset: u64,
    pub sequence: u64,
}

/// Trail store metadata.
#[derive(Debug, Clone)]
pub struct StorageMetadata {
    pub current_segment: SegmentId,
    pub total_entries: u64,
    pub chain_head: [u8; 32],
    pub total_bytes: u64,
}

/// A scanned entry: (offset, entry_bytes, chain_hash).
pub type ScannedEntry = (u64, Vec<u8>, [u8; 32]);

/// Append-only trail storage backend (storage spec §11).
pub trait TrailStorage: Send + Sync {
    /// Append an entry. Implementation MUST fsync before returning.
    fn append(&mut self, entry: &[u8], chain_hash: &[u8; 32])
    -> Result<WriteReceipt, StorageError>;

    /// Read an entry by segment and byte offset.
    fn read(&self, segment: SegmentId, offset: u64, length: u32) -> Result<Vec<u8>, StorageError>;

    /// Rotate the active segment.
    fn rotate(&mut self) -> Result<SegmentId, StorageError>;

    /// Scan a segment from a byte offset, yielding (offset, entry, hash).
    fn scan(&self, segment: SegmentId, from_offset: u64)
    -> Result<Vec<ScannedEntry>, StorageError>;

    /// Current metadata.
    fn metadata(&self) -> StorageMetadata;
}

/// Content-addressable checkpoint storage (storage spec §11).
pub trait CheckpointStorage: Send + Sync {
    /// Store a payload. Returns true if new, false if deduplicated.
    fn store(&self, content_hash: &[u8; 32], payload: &[u8]) -> Result<bool, StorageError>;

    /// Read a payload by content hash. None if not found.
    fn read(&self, content_hash: &[u8; 32]) -> Result<Option<Vec<u8>>, StorageError>;

    /// Check existence without reading.
    fn exists(&self, content_hash: &[u8; 32]) -> Result<bool, StorageError>;

    /// Delete a payload.
    fn delete(&self, content_hash: &[u8; 32]) -> Result<bool, StorageError>;
}

/// Workspace and system snapshot storage (storage spec §11).
pub trait SnapshotStorage: Send + Sync {
    fn write_workspace(&self, workspace_id: &WorkspaceId, data: &[u8]) -> Result<(), StorageError>;

    fn read_workspace(&self, workspace_id: &WorkspaceId) -> Result<Option<Vec<u8>>, StorageError>;

    fn delete_workspace(&self, workspace_id: &WorkspaceId) -> Result<(), StorageError>;

    fn write_system(&self, sequence: u64, data: &[u8]) -> Result<(), StorageError>;

    /// Read the most recent system snapshot. Returns (sequence, data).
    fn read_latest_system(&self) -> Result<Option<(u64, Vec<u8>)>, StorageError>;
}
