use serde::{Deserialize, Serialize};

use crate::enums::{EnvelopeOrigin, EnvelopePriority, EnvelopeState};
use crate::id::{EnvelopeId, WorkspaceId};

/// The unit of communication (PROTOCOL.md §4.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub id: EnvelopeId,
    pub from_workspace: WorkspaceId,
    pub to_workspace: WorkspaceId,
    /// Open set: "directive", "feedback", "query", or taxonomy-registered.
    pub envelope_type: String,
    pub payload: Vec<u8>,
    pub in_reply_to: Option<EnvelopeId>,
    /// Placeholder — replaced by HLC timestamp from wacp-clock.
    pub timestamp: u64,
    pub priority: EnvelopePriority,
    /// Runtime-assigned, immutable.
    pub origin: EnvelopeOrigin,
    pub state: EnvelopeState,
}
