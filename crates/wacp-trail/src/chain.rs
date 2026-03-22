use sha2::{Digest, Sha256};
use std::fmt;

/// SHA-256 chain hash — links each trail entry to the previous one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChainHash(pub [u8; 32]);

impl ChainHash {
    /// The genesis hash: all zeros. Used as the "previous" for the first entry.
    pub const GENESIS: Self = Self([0u8; 32]);
}

impl fmt::Display for ChainHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl AsRef<[u8; 32]> for ChainHash {
    fn as_ref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<[u8; 32]> for ChainHash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// Compute `SHA-256(previous || entry_bytes)`.
pub fn compute_chain_hash(previous: &ChainHash, entry_bytes: &[u8]) -> ChainHash {
    let mut hasher = Sha256::new();
    hasher.update(previous.0);
    hasher.update(entry_bytes);
    let result = hasher.finalize();
    ChainHash(result.into())
}

/// Verify a single chain link.
pub fn verify_chain_link(
    previous: &ChainHash,
    entry_bytes: &[u8],
    expected: &ChainHash,
) -> bool {
    compute_chain_hash(previous, entry_bytes) == *expected
}

/// Error from chain verification.
#[derive(Debug, Clone)]
pub struct ChainVerificationError {
    pub index: usize,
    pub expected: ChainHash,
    pub computed: ChainHash,
}

impl fmt::Display for ChainVerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "chain broken at index {}: expected {}, computed {}",
            self.index, self.expected, self.computed
        )
    }
}

impl std::error::Error for ChainVerificationError {}

/// Verify a full chain from genesis.
/// Each element is (entry_bytes, chain_hash).
pub fn verify_chain(
    entries: &[(Vec<u8>, ChainHash)],
) -> Result<(), ChainVerificationError> {
    let mut previous = ChainHash::GENESIS;
    for (i, (entry_bytes, expected)) in entries.iter().enumerate() {
        let computed = compute_chain_hash(&previous, entry_bytes);
        if computed != *expected {
            return Err(ChainVerificationError {
                index: i,
                expected: *expected,
                computed,
            });
        }
        previous = *expected;
    }
    Ok(())
}
