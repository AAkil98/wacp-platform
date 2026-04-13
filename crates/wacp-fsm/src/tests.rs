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
        ws(
            WorkspaceState::Migrating,
            WorkspaceTrigger::MigrationSucceeded
        )
        .unwrap(),
        WorkspaceState::Active
    );
}

#[test]
fn ws_migrating_succeeded_blocked() {
    assert_eq!(
        ws(
            WorkspaceState::Migrating,
            WorkspaceTrigger::MigrationSucceededBlocked
        )
        .unwrap(),
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
        ws(
            WorkspaceState::Migrating,
            WorkspaceTrigger::CoordinatorAbort
        )
        .unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_suspended_resume() {
    assert_eq!(
        ws(
            WorkspaceState::Suspended,
            WorkspaceTrigger::CoordinatorResume
        )
        .unwrap(),
        WorkspaceState::Active
    );
}

#[test]
fn ws_suspended_abort() {
    assert_eq!(
        ws(
            WorkspaceState::Suspended,
            WorkspaceTrigger::CoordinatorAbort
        )
        .unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_integrating_to_closed() {
    assert_eq!(
        ws(
            WorkspaceState::Integrating,
            WorkspaceTrigger::IntegrationSucceeded
        )
        .unwrap(),
        WorkspaceState::Closed
    );
}

#[test]
fn ws_integrating_to_conflicted() {
    assert_eq!(
        ws(
            WorkspaceState::Integrating,
            WorkspaceTrigger::ConflictDetected
        )
        .unwrap(),
        WorkspaceState::Conflicted
    );
}

#[test]
fn ws_integrating_to_failed() {
    assert_eq!(
        ws(
            WorkspaceState::Integrating,
            WorkspaceTrigger::IntegrationFailed
        )
        .unwrap(),
        WorkspaceState::Failed
    );
}

#[test]
fn ws_conflicted_to_closed() {
    assert_eq!(
        ws(
            WorkspaceState::Conflicted,
            WorkspaceTrigger::ConflictResolved
        )
        .unwrap(),
        WorkspaceState::Closed
    );
}

#[test]
fn ws_conflicted_to_failed() {
    assert_eq!(
        ws(
            WorkspaceState::Conflicted,
            WorkspaceTrigger::ConflictUnresolvable
        )
        .unwrap(),
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

fn env(state: EnvelopeState, trigger: EnvelopeTrigger) -> Result<EnvelopeState, TransitionError> {
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

// ── Phase 18a.2: Exhaustive transition tables ──

/// Expected result for a (state, trigger) pair.
enum Expect<S> {
    Ok(S),
    Terminal,
    Illegal,
}

// ── Envelope: 5 states × 5 triggers = 25 pairs ──

#[test]
fn envelope_exhaustive_transition_table() {
    use EnvelopeState::*;
    use EnvelopeTrigger::*;
    use Expect::*;

    let table: Vec<(EnvelopeState, EnvelopeTrigger, Expect<EnvelopeState>)> = vec![
        // Created
        (Created, Submit, Illegal),
        (Created, ValidationPassed, Ok(Validated)),
        (Created, ValidationFailed, Ok(Rejected)),
        (Created, Deliver, Illegal),
        (Created, Acknowledge, Illegal),
        // Validated
        (Validated, Submit, Illegal),
        (Validated, ValidationPassed, Illegal),
        (Validated, ValidationFailed, Illegal),
        (Validated, Deliver, Ok(Delivered)),
        (Validated, Acknowledge, Illegal),
        // Delivered
        (Delivered, Submit, Illegal),
        (Delivered, ValidationPassed, Illegal),
        (Delivered, ValidationFailed, Illegal),
        (Delivered, Deliver, Illegal),
        (Delivered, Acknowledge, Ok(Acknowledged)),
        // Acknowledged (terminal)
        (Acknowledged, Submit, Terminal),
        (Acknowledged, ValidationPassed, Terminal),
        (Acknowledged, ValidationFailed, Terminal),
        (Acknowledged, Deliver, Terminal),
        (Acknowledged, Acknowledge, Terminal),
        // Rejected (terminal)
        (Rejected, Submit, Terminal),
        (Rejected, ValidationPassed, Terminal),
        (Rejected, ValidationFailed, Terminal),
        (Rejected, Deliver, Terminal),
        (Rejected, Acknowledge, Terminal),
    ];

    assert_eq!(table.len(), 25, "5 states × 5 triggers");
    for (state, trigger, expected) in &table {
        let result = env(*state, *trigger);
        match expected {
            Ok(target) => assert_eq!(
                result.as_ref().unwrap(),
                target,
                "{state:?} + {trigger:?} should → {target:?}"
            ),
            Terminal => assert!(
                matches!(&result, Err(TransitionError::TerminalState { .. })),
                "{state:?} + {trigger:?} should be TerminalState, got {result:?}"
            ),
            Illegal => assert!(
                matches!(&result, Err(TransitionError::IllegalTransition { .. })),
                "{state:?} + {trigger:?} should be IllegalTransition, got {result:?}"
            ),
        }
    }
}

// ── Task: 8 states × 7 triggers = 56 pairs ──

#[test]
fn task_exhaustive_transition_table() {
    use Expect::*;
    use TaskStatus::*;
    use TaskTrigger::*;

    let table: Vec<(TaskStatus, TaskTrigger, Expect<TaskStatus>)> = vec![
        // Draft
        (Draft, Approve, Ok(Pending)),
        (Draft, Assign, Illegal),
        (Draft, Start, Illegal),
        (Draft, Complete, Illegal),
        (Draft, Fail, Illegal),
        (Draft, Integrate, Illegal),
        (Draft, Cancel, Ok(Cancelled)),
        // Pending
        (Pending, Approve, Illegal),
        (Pending, Assign, Ok(Assigned)),
        (Pending, Start, Illegal),
        (Pending, Complete, Illegal),
        (Pending, Fail, Illegal),
        (Pending, Integrate, Illegal),
        (Pending, Cancel, Ok(Cancelled)),
        // Assigned
        (Assigned, Approve, Illegal),
        (Assigned, Assign, Illegal),
        (Assigned, Start, Ok(InProgress)),
        (Assigned, Complete, Illegal),
        (Assigned, Fail, Illegal),
        (Assigned, Integrate, Illegal),
        (Assigned, Cancel, Ok(Cancelled)),
        // InProgress
        (InProgress, Approve, Illegal),
        (InProgress, Assign, Illegal),
        (InProgress, Start, Illegal),
        (InProgress, Complete, Ok(Completed)),
        (InProgress, Fail, Ok(Failed)),
        (InProgress, Integrate, Illegal),
        (InProgress, Cancel, Ok(Cancelled)),
        // Completed
        (Completed, Approve, Illegal),
        (Completed, Assign, Illegal),
        (Completed, Start, Illegal),
        (Completed, Complete, Illegal),
        (Completed, Fail, Ok(Failed)),
        (Completed, Integrate, Ok(Integrated)),
        (Completed, Cancel, Illegal),
        // Failed (retriable)
        (Failed, Approve, Illegal),
        (Failed, Assign, Ok(Assigned)),
        (Failed, Start, Illegal),
        (Failed, Complete, Illegal),
        (Failed, Fail, Illegal),
        (Failed, Integrate, Illegal),
        (Failed, Cancel, Illegal),
        // Integrated (terminal)
        (Integrated, Approve, Terminal),
        (Integrated, Assign, Terminal),
        (Integrated, Start, Terminal),
        (Integrated, Complete, Terminal),
        (Integrated, Fail, Terminal),
        (Integrated, Integrate, Terminal),
        (Integrated, Cancel, Terminal),
        // Cancelled (terminal)
        (Cancelled, Approve, Terminal),
        (Cancelled, Assign, Terminal),
        (Cancelled, Start, Terminal),
        (Cancelled, Complete, Terminal),
        (Cancelled, Fail, Terminal),
        (Cancelled, Integrate, Terminal),
        (Cancelled, Cancel, Terminal),
    ];

    assert_eq!(table.len(), 56, "8 states × 7 triggers");
    for (state, trigger, expected) in &table {
        let result = task(*state, *trigger);
        match expected {
            Ok(target) => assert_eq!(
                result.as_ref().unwrap(),
                target,
                "{state:?} + {trigger:?} should → {target:?}"
            ),
            Terminal => assert!(
                matches!(&result, Err(TransitionError::TerminalState { .. })),
                "{state:?} + {trigger:?} should be TerminalState, got {result:?}"
            ),
            Illegal => assert!(
                matches!(&result, Err(TransitionError::IllegalTransition { .. })),
                "{state:?} + {trigger:?} should be IllegalTransition, got {result:?}"
            ),
        }
    }
}

// ── Workspace: 9 states × 21 triggers = 189 pairs ──

#[test]
fn workspace_exhaustive_transition_table() {
    use Expect::*;
    use WorkspaceState::*;
    use WorkspaceTrigger::*;

    // All 21 triggers, in declaration order.
    let all_triggers = [
        AgentReady,
        AgentStarted,
        AgentBlocked,
        AgentComplete,
        AgentFailed,
        ReceiveFirstEnvelope,
        CoordinatorAbort,
        CoordinatorSuspend,
        CoordinatorResume,
        CoordinatorMigrate,
        MigrationSucceeded,
        MigrationSucceededBlocked,
        MigrationFailed,
        IntegrationSucceeded,
        IntegrationFailed,
        ConflictDetected,
        ConflictResolved,
        ConflictUnresolvable,
        TimeoutExceeded,
        BudgetExceeded,
        CreationError,
    ];

    // 7 non-terminal states × 21 triggers.
    // Each row: (state, expected results for each trigger in order).
    let rows: Vec<(WorkspaceState, Vec<Expect<WorkspaceState>>)> = vec![
        (
            Idle,
            vec![
                Illegal,    // AgentReady
                Illegal,    // AgentStarted
                Illegal,    // AgentBlocked
                Illegal,    // AgentComplete
                Illegal,    // AgentFailed
                Ok(Active), // ReceiveFirstEnvelope
                Ok(Failed), // CoordinatorAbort
                Illegal,    // CoordinatorSuspend
                Illegal,    // CoordinatorResume
                Illegal,    // CoordinatorMigrate
                Illegal,    // MigrationSucceeded
                Illegal,    // MigrationSucceededBlocked
                Illegal,    // MigrationFailed
                Illegal,    // IntegrationSucceeded
                Illegal,    // IntegrationFailed
                Illegal,    // ConflictDetected
                Illegal,    // ConflictResolved
                Illegal,    // ConflictUnresolvable
                Ok(Failed), // TimeoutExceeded
                Ok(Failed), // BudgetExceeded
                Ok(Failed), // CreationError
            ],
        ),
        (
            Active,
            vec![
                Illegal,         // AgentReady
                Illegal,         // AgentStarted
                Ok(Blocked),     // AgentBlocked
                Ok(Integrating), // AgentComplete
                Ok(Failed),      // AgentFailed
                Illegal,         // ReceiveFirstEnvelope
                Ok(Failed),      // CoordinatorAbort
                Ok(Suspended),   // CoordinatorSuspend
                Illegal,         // CoordinatorResume
                Ok(Migrating),   // CoordinatorMigrate
                Illegal,         // MigrationSucceeded
                Illegal,         // MigrationSucceededBlocked
                Illegal,         // MigrationFailed
                Illegal,         // IntegrationSucceeded
                Illegal,         // IntegrationFailed
                Illegal,         // ConflictDetected
                Illegal,         // ConflictResolved
                Illegal,         // ConflictUnresolvable
                Ok(Failed),      // TimeoutExceeded
                Ok(Failed),      // BudgetExceeded
                Illegal,         // CreationError
            ],
        ),
        (
            Blocked,
            vec![
                Illegal,       // AgentReady
                Ok(Active),    // AgentStarted
                Illegal,       // AgentBlocked
                Illegal,       // AgentComplete
                Illegal,       // AgentFailed
                Illegal,       // ReceiveFirstEnvelope
                Ok(Failed),    // CoordinatorAbort
                Ok(Suspended), // CoordinatorSuspend
                Illegal,       // CoordinatorResume
                Ok(Migrating), // CoordinatorMigrate
                Illegal,       // MigrationSucceeded
                Illegal,       // MigrationSucceededBlocked
                Illegal,       // MigrationFailed
                Illegal,       // IntegrationSucceeded
                Illegal,       // IntegrationFailed
                Illegal,       // ConflictDetected
                Illegal,       // ConflictResolved
                Illegal,       // ConflictUnresolvable
                Ok(Failed),    // TimeoutExceeded
                Ok(Failed),    // BudgetExceeded
                Illegal,       // CreationError
            ],
        ),
        (
            Suspended,
            vec![
                Illegal,    // AgentReady
                Illegal,    // AgentStarted
                Illegal,    // AgentBlocked
                Illegal,    // AgentComplete
                Illegal,    // AgentFailed
                Illegal,    // ReceiveFirstEnvelope
                Ok(Failed), // CoordinatorAbort
                Illegal,    // CoordinatorSuspend
                Ok(Active), // CoordinatorResume
                Illegal,    // CoordinatorMigrate
                Illegal,    // MigrationSucceeded
                Illegal,    // MigrationSucceededBlocked
                Illegal,    // MigrationFailed
                Illegal,    // IntegrationSucceeded
                Illegal,    // IntegrationFailed
                Illegal,    // ConflictDetected
                Illegal,    // ConflictResolved
                Illegal,    // ConflictUnresolvable
                Illegal,    // TimeoutExceeded
                Illegal,    // BudgetExceeded
                Illegal,    // CreationError
            ],
        ),
        (
            Migrating,
            vec![
                Illegal,     // AgentReady
                Illegal,     // AgentStarted
                Illegal,     // AgentBlocked
                Illegal,     // AgentComplete
                Illegal,     // AgentFailed
                Illegal,     // ReceiveFirstEnvelope
                Ok(Failed),  // CoordinatorAbort
                Illegal,     // CoordinatorSuspend
                Illegal,     // CoordinatorResume
                Illegal,     // CoordinatorMigrate
                Ok(Active),  // MigrationSucceeded
                Ok(Blocked), // MigrationSucceededBlocked
                Ok(Failed),  // MigrationFailed
                Illegal,     // IntegrationSucceeded
                Illegal,     // IntegrationFailed
                Illegal,     // ConflictDetected
                Illegal,     // ConflictResolved
                Illegal,     // ConflictUnresolvable
                Illegal,     // TimeoutExceeded
                Illegal,     // BudgetExceeded
                Illegal,     // CreationError
            ],
        ),
        (
            Integrating,
            vec![
                Illegal,        // AgentReady
                Illegal,        // AgentStarted
                Illegal,        // AgentBlocked
                Illegal,        // AgentComplete
                Illegal,        // AgentFailed
                Illegal,        // ReceiveFirstEnvelope
                Illegal,        // CoordinatorAbort
                Illegal,        // CoordinatorSuspend
                Illegal,        // CoordinatorResume
                Illegal,        // CoordinatorMigrate
                Illegal,        // MigrationSucceeded
                Illegal,        // MigrationSucceededBlocked
                Illegal,        // MigrationFailed
                Ok(Closed),     // IntegrationSucceeded
                Ok(Failed),     // IntegrationFailed
                Ok(Conflicted), // ConflictDetected
                Illegal,        // ConflictResolved
                Illegal,        // ConflictUnresolvable
                Illegal,        // TimeoutExceeded
                Illegal,        // BudgetExceeded
                Illegal,        // CreationError
            ],
        ),
        (
            Conflicted,
            vec![
                Illegal,    // AgentReady
                Illegal,    // AgentStarted
                Illegal,    // AgentBlocked
                Illegal,    // AgentComplete
                Illegal,    // AgentFailed
                Illegal,    // ReceiveFirstEnvelope
                Illegal,    // CoordinatorAbort
                Illegal,    // CoordinatorSuspend
                Illegal,    // CoordinatorResume
                Illegal,    // CoordinatorMigrate
                Illegal,    // MigrationSucceeded
                Illegal,    // MigrationSucceededBlocked
                Illegal,    // MigrationFailed
                Illegal,    // IntegrationSucceeded
                Illegal,    // IntegrationFailed
                Illegal,    // ConflictDetected
                Ok(Closed), // ConflictResolved
                Ok(Failed), // ConflictUnresolvable
                Illegal,    // TimeoutExceeded
                Illegal,    // BudgetExceeded
                Illegal,    // CreationError
            ],
        ),
    ];

    // Non-terminal: 7 states × 21 triggers = 147.
    let mut count = 0;
    for (state, expected_row) in &rows {
        assert_eq!(
            expected_row.len(),
            all_triggers.len(),
            "row for {state:?} has wrong length"
        );
        for (trigger, expected) in all_triggers.iter().zip(expected_row.iter()) {
            let result = ws(*state, *trigger);
            match expected {
                Ok(target) => assert_eq!(
                    result.as_ref().unwrap(),
                    target,
                    "{state:?} + {trigger:?} should → {target:?}"
                ),
                Terminal => unreachable!("terminal not expected for non-terminal states"),
                Illegal => assert!(
                    matches!(&result, Err(TransitionError::IllegalTransition { .. })),
                    "{state:?} + {trigger:?} should be IllegalTransition, got {result:?}"
                ),
            }
            count += 1;
        }
    }
    assert_eq!(count, 147, "7 non-terminal states × 21 triggers");

    // Terminal: 2 states × 21 triggers = 42.
    for terminal in [Closed, Failed] {
        for trigger in &all_triggers {
            assert!(
                matches!(
                    ws(terminal, *trigger),
                    Err(TransitionError::TerminalState { .. })
                ),
                "{terminal:?} + {trigger:?} should be TerminalState"
            );
        }
    }
}

#[test]
fn workspace_valid_transition_count() {
    use WorkspaceState::*;
    use WorkspaceTrigger::*;

    let all_triggers = [
        AgentReady,
        AgentStarted,
        AgentBlocked,
        AgentComplete,
        AgentFailed,
        ReceiveFirstEnvelope,
        CoordinatorAbort,
        CoordinatorSuspend,
        CoordinatorResume,
        CoordinatorMigrate,
        MigrationSucceeded,
        MigrationSucceededBlocked,
        MigrationFailed,
        IntegrationSucceeded,
        IntegrationFailed,
        ConflictDetected,
        ConflictResolved,
        ConflictUnresolvable,
        TimeoutExceeded,
        BudgetExceeded,
        CreationError,
    ];
    let non_terminal = [
        Idle,
        Active,
        Blocked,
        Suspended,
        Migrating,
        Integrating,
        Conflicted,
    ];

    let valid_count: usize = non_terminal
        .iter()
        .flat_map(|s| all_triggers.iter().map(move |t| ws(*s, *t)))
        .filter(|r| r.is_ok())
        .count();

    assert_eq!(
        valid_count, 30,
        "exactly 30 valid transitions in workspace FSM"
    );
}

#[test]
fn task_valid_transition_count() {
    use TaskStatus::*;
    use TaskTrigger::*;

    let all_triggers = [Approve, Assign, Start, Complete, Fail, Integrate, Cancel];
    let non_terminal = [Draft, Pending, Assigned, InProgress, Completed, Failed];

    let valid_count: usize = non_terminal
        .iter()
        .flat_map(|s| all_triggers.iter().map(move |t| task(*s, *t)))
        .filter(|r| r.is_ok())
        .count();

    assert_eq!(valid_count, 12, "exactly 12 valid transitions in task FSM");
}

#[test]
fn envelope_valid_transition_count() {
    use EnvelopeState::*;
    use EnvelopeTrigger::*;

    let all_triggers = [
        Submit,
        ValidationPassed,
        ValidationFailed,
        Deliver,
        Acknowledge,
    ];
    let non_terminal = [Created, Validated, Delivered];

    let valid_count: usize = non_terminal
        .iter()
        .flat_map(|s| all_triggers.iter().map(move |t| env(*s, *t)))
        .filter(|r| r.is_ok())
        .count();

    assert_eq!(
        valid_count, 4,
        "exactly 4 valid transitions in envelope FSM"
    );
}

#[test]
fn terminal_states_produce_no_valid_successors() {
    use WorkspaceState::*;
    use WorkspaceTrigger::*;

    let all_triggers = [
        AgentReady,
        AgentStarted,
        AgentBlocked,
        AgentComplete,
        AgentFailed,
        ReceiveFirstEnvelope,
        CoordinatorAbort,
        CoordinatorSuspend,
        CoordinatorResume,
        CoordinatorMigrate,
        MigrationSucceeded,
        MigrationSucceededBlocked,
        MigrationFailed,
        IntegrationSucceeded,
        IntegrationFailed,
        ConflictDetected,
        ConflictResolved,
        ConflictUnresolvable,
        TimeoutExceeded,
        BudgetExceeded,
        CreationError,
    ];

    for terminal in [Closed, Failed] {
        for trigger in &all_triggers {
            let result = ws(terminal, *trigger);
            assert!(result.is_err(), "{terminal:?} should reject {trigger:?}");
        }
    }

    for terminal in [TaskStatus::Integrated, TaskStatus::Cancelled] {
        for trigger in [
            TaskTrigger::Approve,
            TaskTrigger::Assign,
            TaskTrigger::Start,
            TaskTrigger::Complete,
            TaskTrigger::Fail,
            TaskTrigger::Integrate,
            TaskTrigger::Cancel,
        ] {
            assert!(task(terminal, trigger).is_err());
        }
    }

    for terminal in [EnvelopeState::Acknowledged, EnvelopeState::Rejected] {
        for trigger in [
            EnvelopeTrigger::Submit,
            EnvelopeTrigger::ValidationPassed,
            EnvelopeTrigger::ValidationFailed,
            EnvelopeTrigger::Deliver,
            EnvelopeTrigger::Acknowledge,
        ] {
            assert!(env(terminal, trigger).is_err());
        }
    }
}

// ── Phase 18b+: Additional FSM coverage ──

#[test]
fn transition_error_display_format() {
    let err = TransitionError::IllegalTransition {
        state: "Active".into(),
        trigger: "AgentReady".into(),
    };
    let display = format!("{err}");
    assert!(
        display.contains("Active"),
        "display should contain the state name"
    );
    assert!(
        display.contains("AgentReady"),
        "display should contain the trigger name"
    );
    assert!(
        display.contains("illegal transition"),
        "display should contain 'illegal transition'"
    );

    let err2 = TransitionError::TerminalState {
        state: "Closed".into(),
    };
    let display2 = format!("{err2}");
    assert!(display2.contains("Closed"));
    assert!(display2.contains("terminal state"));
}

#[test]
fn workspace_all_triggers_from_idle() {
    use WorkspaceState::*;
    use WorkspaceTrigger::*;

    // Valid transitions from Idle.
    assert_eq!(ws(Idle, ReceiveFirstEnvelope).unwrap(), Active);
    assert_eq!(ws(Idle, CreationError).unwrap(), Failed);
    assert_eq!(ws(Idle, TimeoutExceeded).unwrap(), Failed);
    assert_eq!(ws(Idle, CoordinatorAbort).unwrap(), Failed);
    assert_eq!(ws(Idle, BudgetExceeded).unwrap(), Failed);

    // Invalid transitions from Idle — all should be IllegalTransition.
    let illegal = [
        AgentReady,
        AgentStarted,
        AgentBlocked,
        AgentComplete,
        AgentFailed,
        CoordinatorSuspend,
        CoordinatorResume,
        CoordinatorMigrate,
        MigrationSucceeded,
        MigrationSucceededBlocked,
        MigrationFailed,
        IntegrationSucceeded,
        IntegrationFailed,
        ConflictDetected,
        ConflictResolved,
        ConflictUnresolvable,
    ];
    for t in illegal {
        assert!(
            matches!(ws(Idle, t), Err(TransitionError::IllegalTransition { .. })),
            "Idle + {t:?} should be IllegalTransition"
        );
    }
}

#[test]
fn task_retry_from_failed() {
    // Failed → Assigned via Assign (the "retry" path).
    assert_eq!(
        task(TaskStatus::Failed, TaskTrigger::Assign).unwrap(),
        TaskStatus::Assigned
    );
    // All other triggers from Failed should be illegal.
    let illegal = [
        TaskTrigger::Approve,
        TaskTrigger::Start,
        TaskTrigger::Complete,
        TaskTrigger::Fail,
        TaskTrigger::Integrate,
        TaskTrigger::Cancel,
    ];
    for t in illegal {
        assert!(
            matches!(
                task(TaskStatus::Failed, t),
                Err(TransitionError::IllegalTransition { .. })
            ),
            "Failed + {t:?} should be IllegalTransition"
        );
    }
}

#[test]
fn envelope_all_triggers_from_created() {
    use EnvelopeState::*;
    use EnvelopeTrigger::*;

    // Valid transitions from Created.
    assert_eq!(env(Created, ValidationPassed).unwrap(), Validated);
    assert_eq!(env(Created, ValidationFailed).unwrap(), Rejected);

    // Invalid transitions from Created.
    let illegal = [Submit, Deliver, Acknowledge];
    for t in illegal {
        assert!(
            matches!(
                env(Created, t),
                Err(TransitionError::IllegalTransition { .. })
            ),
            "Created + {t:?} should be IllegalTransition"
        );
    }
}

#[test]
fn terminal_state_count() {
    use WorkspaceState::*;

    let all_states = [
        Idle,
        Active,
        Blocked,
        Suspended,
        Migrating,
        Integrating,
        Conflicted,
        Closed,
        Failed,
    ];
    let terminal_count = all_states.iter().filter(|s| s.is_terminal()).count();
    assert_eq!(
        terminal_count, 2,
        "exactly 2 terminal workspace states: Closed and Failed"
    );

    // Verify which ones are terminal.
    assert!(Closed.is_terminal());
    assert!(Failed.is_terminal());
    for s in [
        Idle,
        Active,
        Blocked,
        Suspended,
        Migrating,
        Integrating,
        Conflicted,
    ] {
        assert!(!s.is_terminal(), "{s:?} should not be terminal");
    }
}
