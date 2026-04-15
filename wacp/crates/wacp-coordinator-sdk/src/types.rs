use serde::{Deserialize, Serialize};
use wacp_types::ResourceBudget;

/// Definition for a task in the task graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    /// Task name (human-readable).
    pub name: String,
    /// Task description.
    pub description: String,
    /// IDs of tasks this depends on.
    pub depends_on: Vec<String>,
    /// Role for the workspace (e.g., "worker", "swe:implementer").
    pub role: String,
    /// Directive payload (the task assignment).
    pub directive_payload: Vec<u8>,
    /// Tool names to mount in the workspace.
    pub tools: Vec<String>,
}

/// Configuration for dispatching a workspace.
#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    /// Role for the workspace.
    pub role: String,
    /// Directive payload.
    pub directive_payload: Vec<u8>,
    /// Tool names to mount.
    pub tools: Vec<String>,
    /// Budget allocation for this workspace.
    pub budget: Option<ResourceBudget>,
}

/// Handle to a dispatched workspace.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceHandle {
    pub workspace_id: String,
    pub task_id: String,
}

/// View of a task's current state.
#[derive(Debug, Clone, Serialize)]
pub struct TaskView {
    pub task_id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub assigned_workspace: Option<String>,
    pub depends_on: Vec<String>,
}

/// Filter for signal streams.
#[derive(Debug, Clone, Default)]
pub struct SignalFilter {
    /// Filter by workspace ID. None = all workspaces.
    pub workspace_id: Option<String>,
    /// Filter by signal type. None = all types.
    pub signal_type: Option<String>,
}

/// A signal event from a child workspace.
#[derive(Debug, Clone, Serialize)]
pub struct SignalEvent {
    pub workspace_id: String,
    pub signal_type: String,
    pub reason: String,
    pub context: Vec<u8>,
    pub timestamp: u64,
}

/// Result of an integration pipeline.
#[derive(Debug, Clone, Serialize)]
pub struct IntegrationResult {
    /// "accepted", "revised", or "rejected"
    pub result: String,
    /// Details about the decision.
    pub detail: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_definition_constructable() {
        let task = TaskDefinition {
            name: "implement auth".into(),
            description: "Implement authentication module".into(),
            depends_on: vec!["plan-task-1".into()],
            role: "swe:implementer".into(),
            directive_payload: b"implement auth".to_vec(),
            tools: vec!["file_read".into(), "file_write".into()],
        };
        assert_eq!(task.name, "implement auth");
        assert_eq!(task.depends_on.len(), 1);
        assert_eq!(task.tools.len(), 2);
    }

    #[test]
    fn workspace_config_constructable() {
        let config = WorkspaceConfig {
            role: "worker".into(),
            directive_payload: b"do work".to_vec(),
            tools: vec![],
            budget: None,
        };
        assert_eq!(config.role, "worker");
    }

    #[test]
    fn signal_filter_default_is_unfiltered() {
        let filter = SignalFilter::default();
        assert!(filter.workspace_id.is_none());
        assert!(filter.signal_type.is_none());
    }

    #[test]
    fn workspace_handle_serde() {
        let handle = WorkspaceHandle {
            workspace_id: "ws-1".into(),
            task_id: "task-1".into(),
        };
        let json = serde_json::to_string(&handle).unwrap();
        assert!(json.contains("ws-1"));
    }

    #[test]
    fn integration_result_variants() {
        let accepted = IntegrationResult {
            result: "accepted".into(),
            detail: "all quality checks passed".into(),
        };
        assert_eq!(accepted.result, "accepted");

        let rejected = IntegrationResult {
            result: "rejected".into(),
            detail: "test coverage below threshold".into(),
        };
        assert_eq!(rejected.result, "rejected");
    }

    #[test]
    fn task_definition_serde_roundtrip() {
        let task = TaskDefinition {
            name: "test".into(),
            description: "a test".into(),
            depends_on: vec!["dep-1".into()],
            role: "worker".into(),
            directive_payload: b"payload".to_vec(),
            tools: vec!["tool_a".into()],
        };
        let json = serde_json::to_string(&task).unwrap();
        let parsed: TaskDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, task.name);
        assert_eq!(parsed.depends_on, task.depends_on);
    }
}
