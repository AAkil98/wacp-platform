use serde::{Deserialize, Serialize};

use crate::enums::ErrorCategory;

/// Structured protocol error (protocol-interface spec §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub category: ErrorCategory,
    /// Protocol section reference (e.g., "§5.5").
    pub rule: String,
    pub message: String,
    /// Echoed client_request_id.
    pub request_id: Option<String>,
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} ({}): {}", self.category, self.rule, self.message)
    }
}

impl std::error::Error for ProtocolError {}
