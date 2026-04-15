/// Storage-level errors. No protocol semantics — the trail writer translates these.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(String),

    #[error("corruption detected: {0}")]
    Corruption(String),

    #[error("capacity exceeded")]
    CapacityExceeded,

    #[error("segment not found: {0}")]
    SegmentNotFound(u64),

    #[error("not found")]
    NotFound,
}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        StorageError::Io(e.to_string())
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(e: rusqlite::Error) -> Self {
        StorageError::Io(e.to_string())
    }
}
