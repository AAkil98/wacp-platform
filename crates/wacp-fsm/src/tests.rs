use wacp_types::{EnvelopeState, TaskStatus, WorkspaceState};

use crate::*;

// ── Workspace FSM ──

fn ws(state: WorkspaceState, trigger: WorkspaceTrigger) -> Result<WorkspaceState, TransitionError> {
    WorkspaceFsm::transition(state, &trigger)
}

#[test]
fn ws_idle_to_active() {
    assert_eq!(
        ws(WorkspaceState::Idle, WorkspaceTrigger::ReceiveFirstEnvelope).unwrap(),
        WorkspaceState::Active
    );
}

#[test]
fn ws_idle_to_failed() {
    assert_eq!(
        ws(WorkspaceState::Idle, WorkspaceTrigger::CreationError).unwrap(),
        WorkspaceState::Failed
    );
    assert_eq!(
        ws(WorkspaceState::Idle, WorkspaceTrigger::TimeoutExceeded).unwrap(),
        WorkspaceState::Failed
    );
    assert_eq!(
        ws(WorkspaceState::Idle, WorkspaceTrigger::CoordinatorAbort).unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_active_to_blocked() {
    assert_eq!(
        ws(WorkspaceState::Active, WorkspaceTrigger::AgentBlocked).unwrap(),
        WorkspaceState::Blocked
    );
}

#[test]
fn ws_active_to_integrating() {
    assert_eq!(
        ws(WorkspaceState::Active, WorkspaceTrigger::AgentComplete).unwrap(),
        WorkspaceState::Integrating
    );
}

#[test]
fn ws_active_to_failed_abort() {
    assert_eq!(
        ws(WorkspaceState::Active, WorkspaceTrigger::CoordinatorAbort).unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_active_to_failed_timeout() {
    assert_eq!(
        ws(WorkspaceState::Active, WorkspaceTrigger::TimeoutExceeded).unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_active_to_failed_budget() {
    assert_eq!(
        ws(WorkspaceState::Active, WorkspaceTrigger::BudgetExceeded).unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_active_to_migrating() {
    assert_eq!(
        ws(WorkspaceState::Active, WorkspaceTrigger::CoordinatorMigrate).unwrap(),
        WorkspaceState::Migrating
    );
}

#[test]
fn ws_active_to_suspended() {
    assert_eq!(
        ws(WorkspaceState::Active, WorkspaceTrigger::CoordinatorSuspend).unwrap(),
        WorkspaceState::Suspended
    );
}

#[test]
fn ws_blocked_to_active() {
    assert_eq!(
        ws(WorkspaceState::Blocked, WorkspaceTrigger::AgentStarted).unwrap(),
        WorkspaceState::Active
    );
}

#[test]
fn ws_blocked_to_failed() {
    assert_eq!(
        ws(WorkspaceState::Blocked, WorkspaceTrigger::CoordinatorAbort).unwrap(),
        WorkspaceState::Failed
    );
    assert_eq!(
        ws(WorkspaceState::Blocked, WorkspaceTrigger::TimeoutExceeded).unwrap(),
        WorkspaceState::Failed
    );
    assert_eq!(
        ws(WorkspaceState::Blocked, WorkspaceTrigger::BudgetExceeded).unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_migrating_succeeded() {
    assert_eq!(
        ws(WorkspaceState::Migrating, WorkspaceTrigger::MigrationSucceeded).unwrap(),
        WorkspaceState::Active
    );
}

#[test]
fn ws_migrating_succeeded_blocked() {
    assert_eq!(
        ws(WorkspaceState::Migrating, WorkspaceTrigger::MigrationSucceededBlocked).unwrap(),
        WorkspaceState::Blocked
    );
}

#[test]
fn ws_migrating_failed() {
    assert_eq!(
        ws(WorkspaceState::Migrating, WorkspaceTrigger::MigrationFailed).unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_migrating_abort() {
    assert_eq!(
        ws(WorkspaceState::Migrating, WorkspaceTrigger::CoordinatorAbort).unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_suspended_resume() {
    assert_eq!(
        ws(WorkspaceState::Suspended, WorkspaceTrigger::CoordinatorResume).unwrap(),
        WorkspaceState::Active
    );
}

#[test]
fn ws_suspended_abort() {
    assert_eq!(
        ws(WorkspaceState::Suspended, WorkspaceTrigger::CoordinatorAbort).unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_integrating_to_closed() {
    assert_eq!(
        ws(WorkspaceState::Integrating, WorkspaceTrigger::IntegrationSucceeded).unwrap(),
        WorkspaceState::Closed
    );
}

#[test]
fn ws_integrating_to_conflicted() {
    assert_eq!(
        ws(WorkspaceState::Integrating, WorkspaceTrigger::ConflictDetected).unwrap(),
        WorkspaceState::Conflicted
    );
}

#[test]
fn ws_integrating_to_failed() {
    assert_eq!(
        ws(WorkspaceState::Integrating, WorkspaceTrigger::IntegrationFailed).unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_conflicted_to_closed() {
    assert_eq!(
        ws(WorkspaceState::Conflicted, WorkspaceTrigger::ConflictResolved).unwrap(),
        WorkspaceState::Closed
    );
}

#[test]
fn ws_conflicted_to_failed() {
    assert_eq!(
        ws(WorkspaceState::Conflicted, WorkspaceTrigger::ConflictUnresolvable).unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_closed_rejects_all() {
    let triggers = [
        WorkspaceTrigger::AgentReady,
        WorkspaceTrigger::AgentStarted,
        WorkspaceTrigger::AgentComplete,
        WorkspaceTrigger::CoordinatorAbort,
        WorkspaceTrigger::ReceiveFirstEnvelope,
    ];
    for t in triggers {
        assert!(
            matches!(
                ws(WorkspaceState::Closed, t),
                Err(TransitionError::TerminalState { .. })
            ),
            "Closed + {t:?} should be TerminalState"
        );
    }
}

#[test]
fn ws_failed_rejects_all() {
    let triggers = [
        WorkspaceTrigger::AgentStarted,
        WorkspaceTrigger::CoordinatorResume,
        WorkspaceTrigger::MigrationSucceeded,
    ];
    for t in triggers {
        assert!(
            matches!(
                ws(WorkspaceState::Failed, t),
                Err(TransitionError::TerminalState { .. })
            ),
            "Failed + {t:?} should be TerminalState"
        );
    }
}

#[test]
fn ws_illegal_transitions() {
    // Active + AgentReady is not a valid transition
    assert!(matches!(
        ws(WorkspaceState::Active, WorkspaceTrigger::AgentReady),
        Err(TransitionError::IllegalTransition { .. })
    ));
    // Idle + AgentComplete is not valid
    assert!(matches!(
        ws(WorkspaceState::Idle, WorkspaceTrigger::AgentComplete),
        Err(TransitionError::IllegalTransition { .. })
    ));
    // Suspended + AgentBlocked is not valid
    assert!(matches!(
        ws(WorkspaceState::Suspended, WorkspaceTrigger::AgentBlocked),
        Err(TransitionError::IllegalTransition { .. })
    ));
    // Integrating + AgentStarted is not valid
    assert!(matches!(
        ws(WorkspaceState::Integrating, WorkspaceTrigger::AgentStarted),
        Err(TransitionError::IllegalTransition { .. })
    ));
}

// ── Envelope FSM ──

fn env(
    state: EnvelopeState,
    trigger: EnvelopeTrigger,
) -> Result<EnvelopeState, TransitionError> {
    EnvelopeFsm::transition(state, &trigger)
}

#[test]
fn env_created_to_validated() {
    assert_eq!(
        env(EnvelopeState::Created, EnvelopeTrigger::ValidationPassed).unwrap(),
        EnvelopeState::Validated
    );
}

#[test]
fn env_created_to_rejected() {
    assert_eq!(
        env(EnvelopeState::Created, EnvelopeTrigger::ValidationFailed).unwrap(),
        EnvelopeState::Rejected
    );
}

#[test]
fn env_validated_to_delivered() {
    assert_eq!(
        env(EnvelopeState::Validated, EnvelopeTrigger::Deliver).unwrap(),
        EnvelopeState::Delivered
    );
}

#[test]
fn env_delivered_to_acknowledged() {
    assert_eq!(
        env(EnvelopeState::Delivered, EnvelopeTrigger::Acknowledge).unwrap(),
        EnvelopeState::Acknowledged
    );
}

#[test]
fn env_acknowledged_rejects_all() {
    let triggers = [
        EnvelopeTrigger::Submit,
        EnvelopeTrigger::ValidationPassed,
        EnvelopeTrigger::Deliver,
        EnvelopeTrigger::Acknowledge,
    ];
    for t in triggers {
        assert!(matches!(
            env(EnvelopeState::Acknowledged, t),
            Err(TransitionError::TerminalState { .. })
        ));
    }
}

#[test]
fn env_rejected_rejects_all() {
    let triggers = [
        EnvelopeTrigger::Submit,
        EnvelopeTrigger::ValidationPassed,
        EnvelopeTrigger::Deliver,
    ];
    for t in triggers {
        assert!(matches!(
            env(EnvelopeState::Rejected, t),
            Err(TransitionError::TerminalState { .. })
        ));
    }
}

#[test]
fn env_illegal_transitions() {
    // Created + Deliver (skips Validated)
    assert!(matches!(
        env(EnvelopeState::Created, EnvelopeTrigger::Deliver),
        Err(TransitionError::IllegalTransition { .. })
    ));
    // Validated + Acknowledge (skips Delivered)
    assert!(matches!(
        env(EnvelopeState::Validated, EnvelopeTrigger::Acknowledge),
        Err(TransitionError::IllegalTransition { .. })
    ));
    // Created + Acknowledge
    assert!(matches!(
        env(EnvelopeState::Created, EnvelopeTrigger::Acknowledge),
        Err(TransitionError::IllegalTransition { .. })
    ));
}

// ── Task FSM ──

fn task(state: TaskStatus, trigger: TaskTrigger) -> Result<TaskStatus, TransitionError> {
    TaskFsm::transition(state, &trigger)
}

#[test]
fn task_draft_to_pending() {
    assert_eq!(
        task(TaskStatus::Draft, TaskTrigger::Approve).unwrap(),
        TaskStatus::Pending
    );
}

#[test]
fn task_pending_to_assigned() {
    assert_eq!(
        task(TaskStatus::Pending, TaskTrigger::Assign).unwrap(),
        TaskStatus::Assigned
    );
}

#[test]
fn task_assigned_to_in_progress() {
    assert_eq!(
        task(TaskStatus::Assigned, TaskTrigger::Start).unwrap(),
        TaskStatus::InProgress
    );
}

#[test]
fn task_in_progress_to_completed() {
    assert_eq!(
        task(TaskStatus::InProgress, TaskTrigger::Complete).unwrap(),
        TaskStatus::Completed
    );
}

#[test]
fn task_in_progress_to_failed() {
    assert_eq!(
        task(TaskStatus::InProgress, TaskTrigger::Fail).unwrap(),
        TaskStatus::Failed
    );
}

#[test]
fn task_completed_to_integrated() {
    assert_eq!(
        task(TaskStatus::Completed, TaskTrigger::Integrate).unwrap(),
        TaskStatus::Integrated
    );
}

#[test]
fn task_failed_to_assigned() {
    assert_eq!(
        task(TaskStatus::Failed, TaskTrigger::Assign).unwrap(),
        TaskStatus::Assigned
    );
}

#[test]
fn task_cancel_from_draft() {
    assert_eq!(
        task(TaskStatus::Draft, TaskTrigger::Cancel).unwrap(),
        TaskStatus::Cancelled
    );
}

#[test]
fn task_cancel_from_in_progress() {
    assert_eq!(
        task(TaskStatus::InProgress, TaskTrigger::Cancel).unwrap(),
        TaskStatus::Cancelled
    );
}

#[test]
fn task_integrated_rejects_all() {
    let triggers = [
        TaskTrigger::Approve,
        TaskTrigger::Assign,
        TaskTrigger::Start,
        TaskTrigger::Complete,
        TaskTrigger::Fail,
        TaskTrigger::Cancel,
    ];
    for t in triggers {
        assert!(matches!(
            task(TaskStatus::Integrated, t),
            Err(TransitionError::TerminalState { .. })
        ));
    }
}

#[test]
fn task_cancelled_rejects_all() {
    let triggers = [
        TaskTrigger::Approve,
        TaskTrigger::Assign,
        TaskTrigger::Start,
        TaskTrigger::Complete,
    ];
    for t in triggers {
        assert!(matches!(
            task(TaskStatus::Cancelled, t),
            Err(TransitionError::TerminalState { .. })
        ));
    }
}
