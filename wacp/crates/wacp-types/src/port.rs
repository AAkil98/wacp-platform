use serde::{Deserialize, Serialize};

use crate::enums::PortRightType;
use crate::id::WorkspaceId;

/// A port right governing workspace-to-workspace communication (PROTOCOL.md §5.6).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortRight {
    pub right_type: PortRightType,
    pub holder: WorkspaceId,
    pub target: WorkspaceId,
}
