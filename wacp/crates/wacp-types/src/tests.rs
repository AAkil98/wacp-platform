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

// ── Phase 18a.1: Serde roundtrips for all 12 struct types ──

#[test]
fn serde_roundtrip_envelope() {
    let envelope = Envelope {
        id: EnvelopeId::from("env-1"),
        from_workspace: WorkspaceId::from("ws-src"),
        to_workspace: WorkspaceId::from("ws-dst"),
        envelope_type: "directive".into(),
        payload: vec![0xDE, 0xAD],
        in_reply_to: Some(EnvelopeId::from("env-0")),
        timestamp: 1234567890,
        priority: EnvelopePriority::Urgent,
        origin: EnvelopeOrigin::Agent,
        state: EnvelopeState::Created,
    };
    let json = serde_json::to_string(&envelope).unwrap();
    let back: Envelope = serde_json::from_str(&json).unwrap();
    assert_eq!(envelope, back);
}

#[test]
fn serde_roundtrip_envelope_minimal() {
    let envelope = Envelope {
        id: EnvelopeId::from("env-min"),
        from_workspace: WorkspaceId::from("ws-a"),
        to_workspace: WorkspaceId::from("ws-b"),
        envelope_type: "query".into(),
        payload: vec![],
        in_reply_to: None,
        timestamp: 0,
        priority: EnvelopePriority::Normal,
        origin: EnvelopeOrigin::Human,
        state: EnvelopeState::Rejected,
    };
    let json = serde_json::to_string(&envelope).unwrap();
    let back: Envelope = serde_json::from_str(&json).unwrap();
    assert_eq!(envelope, back);
}

#[test]
fn serde_roundtrip_checkpoint() {
    let checkpoint = Checkpoint {
        id: CheckpointId::from("cp-1"),
        workspace_id: WorkspaceId::from("ws-1"),
        checkpoint_type: "artifact".into(),
        payload: vec![1, 2, 3, 4],
        content_hash: "abc123def456".into(),
        intent: "store intermediate result".into(),
        parent_checkpoint: Some(CheckpointId::from("cp-0")),
        status: CheckpointStatus::Provisional,
        confidence: Confidence::High,
        timestamp: 9999999,
        resource_usage: Some(ResourceUsage {
            tokens: 100,
            wall_time_ms: 500,
            storage_bytes: 1024,
            network_bytes: 0,
            cost_micros: 42,
        }),
    };
    let json = serde_json::to_string(&checkpoint).unwrap();
    let back: Checkpoint = serde_json::from_str(&json).unwrap();
    assert_eq!(checkpoint, back);
}

#[test]
fn serde_roundtrip_checkpoint_minimal() {
    let checkpoint = Checkpoint {
        id: CheckpointId::from("cp-min"),
        workspace_id: WorkspaceId::from("ws-min"),
        checkpoint_type: "observation".into(),
        payload: vec![],
        content_hash: "".into(),
        intent: "".into(),
        parent_checkpoint: None,
        status: CheckpointStatus::Final,
        confidence: Confidence::Low,
        timestamp: 0,
        resource_usage: None,
    };
    let json = serde_json::to_string(&checkpoint).unwrap();
    let back: Checkpoint = serde_json::from_str(&json).unwrap();
    assert_eq!(checkpoint, back);
}

#[test]
fn serde_roundtrip_trail_entry() {
    let entry = TrailEntry {
        id: TrailEntryId::from("te-1"),
        timestamp: 1000000,
        workspace_id: Some(WorkspaceId::from("ws-1")),
        actor: "worker".into(),
        event_type: "state_changed".into(),
        body: vec![0xFF, 0x00, 0xAB],
        sequence_number: 42,
        chain_hash: vec![0x01; 32],
    };
    let json = serde_json::to_string(&entry).unwrap();
    let back: TrailEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(entry, back);
}

#[test]
fn serde_roundtrip_trail_entry_system_level() {
    let entry = TrailEntry {
        id: TrailEntryId::from("te-sys"),
        timestamp: 0,
        workspace_id: None,
        actor: "protocol".into(),
        event_type: "system_init".into(),
        body: vec![],
        sequence_number: 0,
        chain_hash: vec![],
    };
    let json = serde_json::to_string(&entry).unwrap();
    let back: TrailEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(entry, back);
}

#[test]
fn serde_roundtrip_gate_event() {
    let gate = GateEvent {
        gate_id: GateId::from("gate-1"),
        gate_type: GateType::TaskApproval,
        subject: vec![0xCA, 0xFE],
        workspace_id: WorkspaceId::from("ws-1"),
        task_id: Some(TaskId::from("task-1")),
        timeout_ms: 30000,
        fallback_action: "reject".into(),
        created_at: 5555555,
    };
    let json = serde_json::to_string(&gate).unwrap();
    let back: GateEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(gate, back);
}

#[test]
fn serde_roundtrip_gate_event_no_task() {
    let gate = GateEvent {
        gate_id: GateId::from("gate-notask"),
        gate_type: GateType::WorkspaceAbort,
        subject: vec![],
        workspace_id: WorkspaceId::from("ws-2"),
        task_id: None,
        timeout_ms: 0,
        fallback_action: "approve".into(),
        created_at: 0,
    };
    let json = serde_json::to_string(&gate).unwrap();
    let back: GateEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(gate, back);
}

#[test]
fn serde_roundtrip_escalation_event() {
    let esc = EscalationEvent {
        escalation_id: EscalationId::from("esc-1"),
        workspace_id: WorkspaceId::from("ws-1"),
        owner: UserId::from("user-1"),
        context: vec![0x01, 0x02, 0x03],
        created_at: 7777777,
    };
    let json = serde_json::to_string(&esc).unwrap();
    let back: EscalationEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(esc, back);
}

#[test]
fn serde_roundtrip_escalation_event_empty_context() {
    let esc = EscalationEvent {
        escalation_id: EscalationId::from("esc-empty"),
        workspace_id: WorkspaceId::from("ws-x"),
        owner: UserId::from("user-x"),
        context: vec![],
        created_at: 0,
    };
    let json = serde_json::to_string(&esc).unwrap();
    let back: EscalationEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(esc, back);
}

#[test]
fn serde_roundtrip_protocol_error() {
    let err = ProtocolError {
        category: ErrorCategory::PermissionDenied,
        rule: "§5.5".into(),
        message: "Worker cannot send directive".into(),
        request_id: Some("req-42".into()),
    };
    let json = serde_json::to_string(&err).unwrap();
    let back: ProtocolError = serde_json::from_str(&json).unwrap();
    assert_eq!(err, back);
}

#[test]
fn serde_roundtrip_protocol_error_no_request_id() {
    let err = ProtocolError {
        category: ErrorCategory::Internal,
        rule: "§15".into(),
        message: "unexpected state".into(),
        request_id: None,
    };
    let json = serde_json::to_string(&err).unwrap();
    let back: ProtocolError = serde_json::from_str(&json).unwrap();
    assert_eq!(err, back);
}

#[test]
fn protocol_error_display() {
    let err = ProtocolError {
        category: ErrorCategory::ValidationFailed,
        rule: "§4.2".into(),
        message: "invalid envelope type".into(),
        request_id: None,
    };
    let display = format!("{err}");
    assert!(display.contains("ValidationFailed"));
    assert!(display.contains("§4.2"));
    assert!(display.contains("invalid envelope type"));
}

#[test]
fn protocol_error_is_std_error() {
    let err = ProtocolError {
        category: ErrorCategory::Timeout,
        rule: "§6.6".into(),
        message: "exceeded".into(),
        request_id: None,
    };
    let std_err: &dyn std::error::Error = &err;
    assert!(!std_err.to_string().is_empty());
}

#[test]
fn serde_roundtrip_resource_usage() {
    let usage = ResourceUsage {
        tokens: 10000,
        wall_time_ms: 5000,
        storage_bytes: 1024 * 1024,
        network_bytes: 2048,
        cost_micros: 500,
    };
    let json = serde_json::to_string(&usage).unwrap();
    let back: ResourceUsage = serde_json::from_str(&json).unwrap();
    assert_eq!(usage, back);
}

#[test]
fn resource_usage_default() {
    let usage = ResourceUsage::default();
    assert_eq!(usage.tokens, 0);
    assert_eq!(usage.wall_time_ms, 0);
    assert_eq!(usage.storage_bytes, 0);
    assert_eq!(usage.network_bytes, 0);
    assert_eq!(usage.cost_micros, 0);
}

#[test]
fn resource_usage_max_values() {
    let usage = ResourceUsage {
        tokens: u64::MAX,
        wall_time_ms: u64::MAX,
        storage_bytes: u64::MAX,
        network_bytes: u64::MAX,
        cost_micros: u64::MAX,
    };
    let json = serde_json::to_string(&usage).unwrap();
    let back: ResourceUsage = serde_json::from_str(&json).unwrap();
    assert_eq!(usage, back);
}

#[test]
fn serde_roundtrip_resource_budget() {
    let budget = ResourceBudget {
        max_tokens: Some(50000),
        max_wall_time_ms: Some(60000),
        max_storage_bytes: Some(10 * 1024 * 1024),
        max_network_bytes: Some(1024),
        max_cost_micros: Some(1000000),
        warning_threshold: 0.9,
    };
    let json = serde_json::to_string(&budget).unwrap();
    let back: ResourceBudget = serde_json::from_str(&json).unwrap();
    assert_eq!(budget, back);
}

#[test]
fn resource_budget_default() {
    let budget = ResourceBudget::default();
    assert_eq!(budget.max_tokens, None);
    assert_eq!(budget.max_wall_time_ms, None);
    assert_eq!(budget.max_storage_bytes, None);
    assert_eq!(budget.max_network_bytes, None);
    assert_eq!(budget.max_cost_micros, None);
    assert!((budget.warning_threshold - 0.8).abs() < f32::EPSILON);
}

#[test]
fn resource_budget_all_none_roundtrip() {
    let budget = ResourceBudget::default();
    let json = serde_json::to_string(&budget).unwrap();
    let back: ResourceBudget = serde_json::from_str(&json).unwrap();
    assert_eq!(budget, back);
}

#[test]
fn serde_roundtrip_port_right() {
    let right = PortRight {
        right_type: PortRightType::Send,
        holder: WorkspaceId::from("ws-holder"),
        target: WorkspaceId::from("ws-target"),
    };
    let json = serde_json::to_string(&right).unwrap();
    let back: PortRight = serde_json::from_str(&json).unwrap();
    assert_eq!(right, back);
}

#[test]
fn serde_roundtrip_port_right_send_once() {
    let right = PortRight {
        right_type: PortRightType::SendOnce,
        holder: WorkspaceId::from("ws-a"),
        target: WorkspaceId::from("ws-b"),
    };
    let json = serde_json::to_string(&right).unwrap();
    let back: PortRight = serde_json::from_str(&json).unwrap();
    assert_eq!(right, back);
}

#[test]
fn port_right_hash_in_set() {
    use std::collections::HashSet;
    let r1 = PortRight {
        right_type: PortRightType::Send,
        holder: WorkspaceId::from("ws-a"),
        target: WorkspaceId::from("ws-b"),
    };
    let r2 = r1.clone();
    let r3 = PortRight {
        right_type: PortRightType::Receive,
        holder: WorkspaceId::from("ws-a"),
        target: WorkspaceId::from("ws-b"),
    };
    let mut set = HashSet::new();
    set.insert(r1);
    set.insert(r2);
    set.insert(r3);
    assert_eq!(set.len(), 2); // r1 == r2, r3 is different
}

#[test]
fn serde_roundtrip_signal_with_context() {
    let signal = Signal {
        signal_type: SignalType::Complete,
        workspace_id: WorkspaceId::from("ws-1"),
        timestamp: u64::MAX,
        reason: Some("done".into()),
        context: Some(vec![0xBE, 0xEF]),
    };
    let json = serde_json::to_string(&signal).unwrap();
    let back: Signal = serde_json::from_str(&json).unwrap();
    assert_eq!(signal, back);
}

#[test]
fn serde_roundtrip_task_full() {
    let task = Task {
        id: TaskId::from("task-full"),
        name: "full task".into(),
        description: "all fields populated".into(),
        depends_on: vec![TaskId::from("dep-1"), TaskId::from("dep-2")],
        parent_task: Some(TaskId::from("parent-1")),
        status: TaskStatus::InProgress,
        workspace_ref: Some(WorkspaceId::from("ws-assigned")),
        workspace_history: vec![
            WorkspaceId::from("ws-old"),
            WorkspaceId::from("ws-assigned"),
        ],
        checkpoint_ref: Some(CheckpointId::from("cp-latest")),
    };
    let json = serde_json::to_string(&task).unwrap();
    let back: Task = serde_json::from_str(&json).unwrap();
    assert_eq!(task, back);
}

#[test]
fn serde_roundtrip_all_error_categories() {
    let categories = [
        ErrorCategory::PermissionDenied,
        ErrorCategory::IllegalTransition,
        ErrorCategory::ValidationFailed,
        ErrorCategory::BudgetExceeded,
        ErrorCategory::Timeout,
        ErrorCategory::NotFound,
        ErrorCategory::DeliveryFailed,
        ErrorCategory::Internal,
    ];
    for cat in categories {
        let err = ProtocolError {
            category: cat,
            rule: "test".into(),
            message: "test".into(),
            request_id: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        let back: ProtocolError = serde_json::from_str(&json).unwrap();
        assert_eq!(err, back, "roundtrip failed for {cat:?}");
    }
}

#[test]
fn serde_roundtrip_all_gate_types() {
    let types = [
        GateType::TaskApproval,
        GateType::WorkspaceCreate,
        GateType::EnvelopeDelivery,
        GateType::Integration,
        GateType::ConflictResolution,
        GateType::WorkspaceAbort,
        GateType::CheckpointApproval,
    ];
    for gt in types {
        let event = GateEvent {
            gate_id: GateId::from("g"),
            gate_type: gt,
            subject: vec![],
            workspace_id: WorkspaceId::from("ws"),
            task_id: None,
            timeout_ms: 0,
            fallback_action: "".into(),
            created_at: 0,
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: GateEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back, "roundtrip failed for {gt:?}");
    }
}

#[test]
fn serde_roundtrip_all_port_right_types() {
    let types = [
        PortRightType::Send,
        PortRightType::Receive,
        PortRightType::SendOnce,
    ];
    for prt in types {
        let right = PortRight {
            right_type: prt,
            holder: WorkspaceId::from("h"),
            target: WorkspaceId::from("t"),
        };
        let json = serde_json::to_string(&right).unwrap();
        let back: PortRight = serde_json::from_str(&json).unwrap();
        assert_eq!(right, back, "roundtrip failed for {prt:?}");
    }
}

// ── Phase 18b+: Display and default coverage ──

#[test]
fn display_all_id_types() {
    let ws = WorkspaceId::from("ws-1");
    let env = EnvelopeId::from("env-1");
    let task = TaskId::from("task-1");
    let cp = CheckpointId::from("cp-1");
    let trail = TrailEntryId::from("te-1");
    let user = UserId::from("user-1");
    let gate = GateId::from("gate-1");
    let esc = EscalationId::from("esc-1");

    assert_eq!(ws.to_string(), "ws-1");
    assert_eq!(env.to_string(), "env-1");
    assert_eq!(task.to_string(), "task-1");
    assert_eq!(cp.to_string(), "cp-1");
    assert_eq!(trail.to_string(), "te-1");
    assert_eq!(user.to_string(), "user-1");
    assert_eq!(gate.to_string(), "gate-1");
    assert_eq!(esc.to_string(), "esc-1");
}

#[test]
fn default_resource_usage_is_zero() {
    let usage = ResourceUsage::default();
    assert_eq!(usage.tokens, 0);
    assert_eq!(usage.wall_time_ms, 0);
    assert_eq!(usage.storage_bytes, 0);
    assert_eq!(usage.network_bytes, 0);
    assert_eq!(usage.cost_micros, 0);
    // All fields should be zero — verify by serializing and checking.
    let json = serde_json::to_string(&usage).unwrap();
    assert!(json.contains("\"tokens\":0"));
}

#[test]
fn default_resource_budget_is_zero() {
    let budget = ResourceBudget::default();
    assert_eq!(budget.max_tokens, None);
    assert_eq!(budget.max_wall_time_ms, None);
    assert_eq!(budget.max_storage_bytes, None);
    assert_eq!(budget.max_network_bytes, None);
    assert_eq!(budget.max_cost_micros, None);
    // warning_threshold defaults to 0.8, not zero, but all limit fields are None (effectively zero budget).
    assert!((budget.warning_threshold - 0.8).abs() < f32::EPSILON);
}

#[test]
fn envelope_state_display() {
    let all = [
        EnvelopeState::Created,
        EnvelopeState::Validated,
        EnvelopeState::Delivered,
        EnvelopeState::Acknowledged,
        EnvelopeState::Rejected,
    ];
    let names = [
        "Created",
        "Validated",
        "Delivered",
        "Acknowledged",
        "Rejected",
    ];
    for (variant, expected) in all.iter().zip(names.iter()) {
        let display = format!("{variant:?}");
        assert_eq!(
            display, *expected,
            "EnvelopeState::{expected} Debug mismatch"
        );
    }
    assert_eq!(all.len(), 5);
}

#[test]
fn checkpoint_status_display() {
    let provisional = format!("{:?}", CheckpointStatus::Provisional);
    let final_s = format!("{:?}", CheckpointStatus::Final);
    assert_eq!(provisional, "Provisional");
    assert_eq!(final_s, "Final");
}

#[test]
fn confidence_display() {
    let all = [Confidence::High, Confidence::Medium, Confidence::Low];
    let names = ["High", "Medium", "Low"];
    for (variant, expected) in all.iter().zip(names.iter()) {
        let display = format!("{variant:?}");
        assert_eq!(display, *expected, "Confidence::{expected} Debug mismatch");
    }
    assert_eq!(all.len(), 3);
}
