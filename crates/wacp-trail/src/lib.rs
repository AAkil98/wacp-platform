//! WACP — Append-only trail storage.
//!
//! Three storage domains: trail entries (append-only log), checkpoint payloads
//! (content-addressable store), workspace snapshots. Each backed by traits
//! with in-memory and filesystem implementations.

pub mod chain;
pub mod error;
pub mod fs_checkpoint;
pub mod fs_trail;
pub mod index;
pub mod memory;
pub mod traits;

pub use chain::{compute_chain_hash, verify_chain, verify_chain_link, ChainHash, ChainVerificationError};
pub use error::StorageError;
pub use fs_checkpoint::FileCheckpointStorage;
pub use fs_trail::{FileTrailConfig, FileTrailStorage};
pub use index::{IndexEntry, QueryResult, TrailIndex, TrailQuery};
pub use memory::{InMemoryCheckpointStorage, InMemorySnapshotStorage, InMemoryTrailStorage};
pub use traits::{
    CheckpointStorage, ScannedEntry, SegmentId, SnapshotStorage, StorageMetadata, TrailStorage,
    WriteReceipt,
};

#[cfg(test)]
mod tests;
