use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

/// 80-bit Hybrid Logical Clock timestamp.
///
/// 64-bit physical time (microseconds since Unix epoch) + 16-bit logical counter.
/// Big-endian 10-byte encoding preserves lexicographic ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Timestamp {
    physical_us: u64,
    logical: u16,
}

impl Timestamp {
    /// The zero timestamp.
    pub const ZERO: Self = Self {
        physical_us: 0,
        logical: 0,
    };

    /// Create a new timestamp.
    pub fn new(physical_us: u64, logical: u16) -> Self {
        Self {
            physical_us,
            logical,
        }
    }

    /// Physical component in microseconds since epoch.
    pub fn physical_us(&self) -> u64 {
        self.physical_us
    }

    /// Logical counter.
    pub fn logical(&self) -> u16 {
        self.logical
    }

    /// Encode as 10-byte big-endian. Lexicographic byte order matches temporal order.
    pub fn to_bytes(&self) -> [u8; 10] {
        let mut buf = [0u8; 10];
        buf[..8].copy_from_slice(&self.physical_us.to_be_bytes());
        buf[8..10].copy_from_slice(&self.logical.to_be_bytes());
        buf
    }

    /// Decode from 10-byte big-endian.
    pub fn from_bytes(bytes: [u8; 10]) -> Self {
        let physical_us = u64::from_be_bytes(bytes[..8].try_into().unwrap());
        let logical = u16::from_be_bytes(bytes[8..10].try_into().unwrap());
        Self {
            physical_us,
            logical,
        }
    }

    /// Next logical tick. If logical is saturated, advances physical by 1.
    pub fn succ(&self) -> Self {
        if self.logical == u16::MAX {
            Self {
                physical_us: self.physical_us + 1,
                logical: 0,
            }
        } else {
            Self {
                physical_us: self.physical_us,
                logical: self.logical + 1,
            }
        }
    }
}

impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.physical_us
            .cmp(&other.physical_us)
            .then(self.logical.cmp(&other.logical))
    }
}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.physical_us, self.logical)
    }
}
