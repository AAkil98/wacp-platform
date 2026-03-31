pub mod e2e;

use std::collections::HashSet;

use tokio::sync::mpsc;
use wacp_coordinator::{Coordinator, DispatchRequest};
use wacp_types::*;
use wacp_workspace::{WorkspaceConfig, WorkspaceEvent};

/// Create a coordinator with an event channel. Returns root ID = "ws-root".
pub fn make_coordinator() -> (Coordinator, mpsc::Receiver<WorkspaceEvent>) {
    let (tx, rx) = mpsc::channel(256);
    let coord = Coordinator::new(
        WorkspaceId::from("ws-root"),
        UserId::from("system"),
        tx,
    );
    (coord, rx)
}

/// Build a worker workspace config under ws-root.
pub fn worker_config(id: &str, _task_id: &str) -> WorkspaceConfig {
    WorkspaceConfig {
        id: WorkspaceId::from(id),
        role: "worker".into(),
        base_role: BaseRole::Worker,
        parent: WorkspaceId::from("ws-root"),
        owner: UserId::from("system"),
        originator: Originator::System,
        directive: make_envelope(
            &format!("dir-{id}"),
            "ws-root",
            id,
            "directive",
        ),
        context: vec![],
        visibility: HashSet::new(),
        authority: HashSet::new(),
        delegate: false,
        budget: None,
    }
}

/// Build a minimal envelope.
pub fn make_envelope(id: &str, from: &str, to: &str, envelope_type: &str) -> Envelope {
    Envelope {
        id: EnvelopeId::from(id),
        from_workspace: WorkspaceId::from(from),
        to_workspace: WorkspaceId::from(to),
        envelope_type: envelope_type.into(),
        payload: b"payload".to_vec(),
        in_reply_to: None,
        timestamp: 0,
        priority: EnvelopePriority::Normal,
        origin: EnvelopeOrigin::Agent,
        state: EnvelopeState::Created,
    }
}

/// Build a task in Draft status.
pub fn make_task(id: &str, name: &str) -> Task {
    Task {
        id: TaskId::from(id),
        name: name.into(),
        description: String::new(),
        depends_on: vec![],
        parent_task: None,
        status: TaskStatus::Draft,
        workspace_ref: None,
        workspace_history: vec![],
        checkpoint_ref: None,
    }
}

/// Dispatch a workspace and return the workspace ID.
pub fn dispatch(
    coord: &mut Coordinator,
    ws_id: &str,
    task_id: &str,
) -> WorkspaceId {
    coord.dispatch(DispatchRequest {
        task_id: TaskId::from(task_id),
        config: worker_config(ws_id, task_id),
    });
    WorkspaceId::from(ws_id)
}

/// Drain events from the channel, forwarding each to the coordinator.
pub async fn drain_events(
    coord: &mut Coordinator,
    rx: &mut mpsc::Receiver<WorkspaceEvent>,
    max: usize,
    timeout_ms: u64,
) {
    for _ in 0..max {
        match tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            rx.recv(),
        )
        .await
        {
            Ok(Some(event)) => coord.handle_event(&event),
            _ => break,
        }
    }
}
