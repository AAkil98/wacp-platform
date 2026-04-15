use serde::{Deserialize, Serialize};

use crate::enums::SignalType;
use crate::id::WorkspaceId;

/// The unit of notification (PROTOCOL.md §4.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    pub signal_type: SignalType,
    pub workspace_id: WorkspaceId,
    /// Placeholder — replaced by HLC timestamp from wacp-clock.
    pub timestamp: u64,
    /// Required for Blocked and Failed.
    pub reason: Option<String>,
    /// Required for Escalation.
    pub context: Option<Vec<u8>>,
}
