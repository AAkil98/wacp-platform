//! Integration: gate lifecycle — wacp-coordinator (3 tests)

use wacp_coordinator::{GateController, GateFallback};
use wacp_types::*;

#[test]
fn gate_created_human_approves() {
    let mut gc = GateController::new(30_000, GateFallback::Cancel);

    let event = gc.open_gate(
        TaskId::from("task-1"),
        "Test Task".into(),
        "description",
        None,
        None,
    );
    let gate_id = event.gate_id.clone();

    assert!(gc.is_pending(&gate_id));
    assert_eq!(gc.pending_count(), 1);

    let resolution = gc.resolve(&gate_id, GateDecision::Approve);
    assert!(resolution.is_some());
    assert!(!gc.is_pending(&gate_id));
    assert_eq!(gc.pending_count(), 0);
}

#[test]
fn gate_timeout_auto_approve() {
    let mut gc = GateController::new(5_000, GateFallback::AutoApprove);

    let event = gc.open_gate(TaskId::from("task-1"), "Task".into(), "desc", None, None);

    let resolution = gc.timeout(&event.gate_id);
    assert!(resolution.is_some());
    assert!(!gc.is_pending(&event.gate_id));
}

#[test]
fn gate_timeout_cancel() {
    let mut gc = GateController::new(5_000, GateFallback::Cancel);

    let event = gc.open_gate(TaskId::from("task-2"), "Task 2".into(), "desc", None, None);

    let resolution = gc.timeout(&event.gate_id);
    assert!(resolution.is_some());
    assert!(!gc.is_pending(&event.gate_id));
}
