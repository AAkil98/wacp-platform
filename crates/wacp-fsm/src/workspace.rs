use wacp_types::WorkspaceState;

use crate::{StateMachine, TransitionError};

/// Triggers for workspace lifecycle transitions (PROTOCOL.md §6.3).
#[derive(Debug, Clone, Copy)]
pub enum WorkspaceTrigger {
    // Agent signals
    AgentReady,
    AgentStarted,
    AgentBlocked,
    AgentComplete,
    AgentFailed,
    // Coordinator actions
    ReceiveFirstEnvelope,
    CoordinatorAbort,
    CoordinatorSuspend,
    CoordinatorResume,
    CoordinatorMigrate,
    MigrationSucceeded,
    MigrationSucceededBlocked,
    MigrationFailed,
    // Integration
    IntegrationSucceeded,
    IntegrationFailed,
    ConflictDetected,
    ConflictResolved,
    ConflictUnresolvable,
    // External
    TimeoutExceeded,
    BudgetExceeded,
    CreationError,
}

/// Workspace lifecycle FSM — 9 states.
pub struct WorkspaceFsm;

impl StateMachine for WorkspaceFsm {
    type State = WorkspaceState;
    type Trigger = WorkspaceTrigger;

    fn transition(
        state: Self::State,
        trigger: &Self::Trigger,
    ) -> Result<Self::State, TransitionError> {
        use WorkspaceState::*;
        use WorkspaceTrigger::*;

        // Terminal states reject all triggers.
        if state.is_terminal() {
            return Err(TransitionError::TerminalState {
                state: format!("{state:?}"),
            });
        }

        match (state, trigger) {
            // Idle
            (Idle, ReceiveFirstEnvelope) => Ok(Active),
            (Idle, CreationError | TimeoutExceeded | CoordinatorAbort | BudgetExceeded) => {
                Ok(Failed)
            }

            // Active
            (Active, AgentBlocked) => Ok(Blocked),
            (Active, CoordinatorMigrate) => Ok(Migrating),
            (Active, CoordinatorSuspend) => Ok(Suspended),
            (Active, AgentComplete) => Ok(Integrating),
            (Active, AgentFailed | CoordinatorAbort | TimeoutExceeded | BudgetExceeded) => {
                Ok(Failed)
            }

            // Blocked
            (Blocked, AgentStarted) => Ok(Active),
            (Blocked, CoordinatorMigrate) => Ok(Migrating),
            (Blocked, CoordinatorSuspend) => Ok(Suspended),
            (Blocked, CoordinatorAbort | TimeoutExceeded | BudgetExceeded) => Ok(Failed),

            // Migrating
            (Migrating, MigrationSucceeded) => Ok(Active),
            (Migrating, MigrationSucceededBlocked) => Ok(Blocked),
            (Migrating, MigrationFailed | CoordinatorAbort) => Ok(Failed),

            // Suspended
            (Suspended, CoordinatorResume) => Ok(Active),
            (Suspended, CoordinatorAbort) => Ok(Failed),

            // Integrating
            (Integrating, IntegrationSucceeded) => Ok(Closed),
            (Integrating, ConflictDetected) => Ok(Conflicted),
            (Integrating, IntegrationFailed) => Ok(Failed),

            // Conflicted
            (Conflicted, ConflictResolved) => Ok(Closed),
            (Conflicted, ConflictUnresolvable) => Ok(Failed),

            // Everything else is illegal.
            _ => Err(TransitionError::IllegalTransition {
                state: format!("{state:?}"),
                trigger: format!("{trigger:?}"),
            }),
        }
    }
}
