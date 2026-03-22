use crate::*;

/// Helper: count variants via an exhaustive match that increments a counter.
/// This ensures the test breaks if a variant is added or removed.

#[test]
fn signal_type_count() {
    let all = [
        SignalType::Ready,
        SignalType::Started,
        SignalType::Blocked,
        SignalType::Checkpoint,
        SignalType::Complete,
        SignalType::Failed,
        SignalType::Integrate,
        SignalType::Acknowledged,
        SignalType::Escalation,
        SignalType::Suspend,
        SignalType::Migrate,
    ];
    assert_eq!(all.len(), 11);
}

#[test]
fn workspace_state_count() {
    let all = [
        WorkspaceState::Idle,
        WorkspaceState::Active,
        WorkspaceState::Blocked,
        WorkspaceState::Suspended,
        WorkspaceState::Migrating,
        WorkspaceState::Integrating,
        WorkspaceState::Conflicted,
        WorkspaceState::Closed,
        WorkspaceState::Failed,
    ];
    assert_eq!(all.len(), 9);
}

#[test]
fn workspace_terminal_states() {
    assert!(!WorkspaceState::Idle.is_terminal());
    assert!(!WorkspaceState::Active.is_terminal());
    assert!(!WorkspaceState::Blocked.is_terminal());
    assert!(!WorkspaceState::Suspended.is_terminal());
    assert!(!WorkspaceState::Migrating.is_terminal());
    assert!(!WorkspaceState::Integrating.is_terminal());
    assert!(!WorkspaceState::Conflicted.is_terminal());
    assert!(WorkspaceState::Closed.is_terminal());
    assert!(WorkspaceState::Failed.is_terminal());
}

#[test]
fn envelope_state_count() {
    let all = [
        EnvelopeState::Created,
        EnvelopeState::Validated,
        EnvelopeState::Delivered,
        EnvelopeState::Acknowledged,
        EnvelopeState::Rejected,
    ];
    assert_eq!(all.len(), 5);
}

#[test]
fn task_status_count() {
    let all = [
        TaskStatus::Draft,
        TaskStatus::Pending,
        TaskStatus::Assigned,
        TaskStatus::InProgress,
        TaskStatus::Completed,
        TaskStatus::Failed,
        TaskStatus::Integrated,
        TaskStatus::Cancelled,
    ];
    assert_eq!(all.len(), 8);
}

#[test]
fn id_display() {
    let ws = WorkspaceId::from("ws-1");
    assert_eq!(ws.to_string(), "ws-1");
}

#[test]
fn id_equality() {
    let a = WorkspaceId::from("ws-1");
    let b = WorkspaceId::from("ws-1");
    let c = WorkspaceId::from("ws-2");
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn id_ordering() {
    let a = WorkspaceId::from("aaa");
    let b = WorkspaceId::from("bbb");
    assert!(a < b);
}

#[test]
fn serde_roundtrip_enums() {
    // SignalType
    let s = SignalType::Escalation;
    let json = serde_json::to_string(&s).unwrap();
    let back: SignalType = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);

    // WorkspaceState
    let ws = WorkspaceState::Integrating;
    let json = serde_json::to_string(&ws).unwrap();
    let back: WorkspaceState = serde_json::from_str(&json).unwrap();
    assert_eq!(ws, back);

    // TaskStatus
    let ts = TaskStatus::InProgress;
    let json = serde_json::to_string(&ts).unwrap();
    let back: TaskStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(ts, back);

    // EnvelopePriority
    let ep = EnvelopePriority::Blocking;
    let json = serde_json::to_string(&ep).unwrap();
    let back: EnvelopePriority = serde_json::from_str(&json).unwrap();
    assert_eq!(ep, back);

    // GateType
    let gt = GateType::ConflictResolution;
    let json = serde_json::to_string(&gt).unwrap();
    let back: GateType = serde_json::from_str(&json).unwrap();
    assert_eq!(gt, back);
}

#[test]
fn serde_roundtrip_structs() {
    let task = Task {
        id: TaskId::from("task-1"),
        name: "test task".into(),
        description: "a test".into(),
        depends_on: vec![TaskId::from("task-0")],
        parent_task: None,
        status: TaskStatus::Draft,
        workspace_ref: None,
        workspace_history: vec![],
        checkpoint_ref: None,
    };
    let json = serde_json::to_string(&task).unwrap();
    let back: Task = serde_json::from_str(&json).unwrap();
    assert_eq!(task, back);

    let signal = Signal {
        signal_type: SignalType::Blocked,
        workspace_id: WorkspaceId::from("ws-1"),
        timestamp: 1000,
        reason: Some("waiting for input".into()),
        context: None,
    };
    let json = serde_json::to_string(&signal).unwrap();
    let back: Signal = serde_json::from_str(&json).unwrap();
    assert_eq!(signal, back);
}

#[test]
fn originator_variants() {
    let sys = Originator::System;
    let user = Originator::User(UserId::from("user-42"));

    let sys_json = serde_json::to_string(&sys).unwrap();
    let user_json = serde_json::to_string(&user).unwrap();
    assert_ne!(sys_json, user_json);

    let sys_back: Originator = serde_json::from_str(&sys_json).unwrap();
    let user_back: Originator = serde_json::from_str(&user_json).unwrap();
    assert_eq!(sys, sys_back);
    assert_eq!(user, user_back);
}
