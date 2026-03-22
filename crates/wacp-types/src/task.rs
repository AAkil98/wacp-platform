use serde::{Deserialize, Serialize};

use crate::enums::TaskStatus;
use crate::id::{CheckpointId, TaskId, WorkspaceId};

/// The unit of work (PROTOCOL.md §4.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub description: String,
    pub depends_on: Vec<TaskId>,
    pub parent_task: Option<TaskId>,
    pub status: TaskStatus,
    pub workspace_ref: Option<WorkspaceId>,
    pub workspace_history: Vec<WorkspaceId>,
    pub checkpoint_ref: Option<CheckpointId>,
}
