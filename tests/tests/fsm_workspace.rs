//! Integration: wacp-fsm + wacp-workspace (4 tests)

use wacp_fsm::{StateMachine, TransitionError, WorkspaceFsm, WorkspaceTrigger};
use wacp_types::WorkspaceState;

#[test]
fn workspace_fsm_idle_to_active() {
    let result = WorkspaceFsm::transition(
        WorkspaceState::Idle,
        &WorkspaceTrigger::ReceiveFirstEnvelope,
    );
    assert_eq!(result.unwrap(), WorkspaceState::Active);
}

#[test]
fn workspace_fsm_full_lifecycle() {
    // Idle → Active
    let s = WorkspaceFsm::transition(
        WorkspaceState::Idle,
        &WorkspaceTrigger::ReceiveFirstEnvelope,
    )
    .unwrap();
    assert_eq!(s, WorkspaceState::Active);

    // Active → Integrating
    let s = WorkspaceFsm::transition(s, &WorkspaceTrigger::AgentComplete).unwrap();
    assert_eq!(s, WorkspaceState::Integrating);

    // Integrating → Closed
    let s = WorkspaceFsm::transition(s, &WorkspaceTrigger::IntegrationSucceeded).unwrap();
    assert_eq!(s, WorkspaceState::Closed);

    assert!(s.is_terminal());
}

#[test]
fn workspace_fsm_illegal_transition_rejected() {
    let result = WorkspaceFsm::transition(WorkspaceState::Idle, &WorkspaceTrigger::AgentComplete);
    assert!(matches!(
        result,
        Err(TransitionError::IllegalTransition { .. })
    ));
}

#[test]
fn workspace_fsm_terminal_rejects_all() {
    let s = WorkspaceFsm::transition(WorkspaceState::Idle, &WorkspaceTrigger::CoordinatorAbort)
        .unwrap();
    assert_eq!(s, WorkspaceState::Failed);
    assert!(s.is_terminal());

    assert!(WorkspaceFsm::transition(s, &WorkspaceTrigger::ReceiveFirstEnvelope).is_err());
    assert!(WorkspaceFsm::transition(s, &WorkspaceTrigger::AgentComplete).is_err());
    assert!(WorkspaceFsm::transition(s, &WorkspaceTrigger::CoordinatorSuspend).is_err());
}
