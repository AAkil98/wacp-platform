use serde::{Deserialize, Serialize};

use crate::enums::{CheckpointStatus, Confidence};
use crate::id::{CheckpointId, WorkspaceId};
use crate::resource::ResourceUsage;

/// The unit of progress (PROTOCOL.md §7.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub workspace_id: WorkspaceId,
    /// Open set: "artifact", "observation", or taxonomy-registered.
    pub checkpoint_type: String,
    pub payload: Vec<u8>,
    /// SHA-256 hex of payload.
    pub content_hash: String,
    pub intent: String,
    pub parent_checkpoint: Option<CheckpointId>,
    pub status: CheckpointStatus,
    pub confidence: Confidence,
    /// Placeholder — replaced by HLC timestamp from wacp-clock.
    pub timestamp: u64,
    pub resource_usage: Option<ResourceUsage>,
}
