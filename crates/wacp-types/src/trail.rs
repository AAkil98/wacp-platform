use serde::{Deserialize, Serialize};

use crate::id::{TrailEntryId, WorkspaceId};

/// A single trail entry (PROTOCOL.md §9.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrailEntry {
    pub id: TrailEntryId,
    /// Placeholder — replaced by HLC timestamp from wacp-clock.
    pub timestamp: u64,
    /// None for system-level events.
    pub workspace_id: Option<WorkspaceId>,
    /// Role name, "protocol", or user_id.
    pub actor: String,
    pub event_type: String,
    pub body: Vec<u8>,
    pub sequence_number: u64,
    /// SHA-256 hash chain link.
    pub chain_hash: Vec<u8>,
}
