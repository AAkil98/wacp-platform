use wacp_types::TaskStatus;

use crate::{StateMachine, TransitionError};

/// Triggers for task lifecycle transitions (PROTOCOL.md §4.6, §8.2).
#[derive(Debug, Clone, Copy)]
pub enum TaskTrigger {
    Approve,
    Assign,
    Start,
    Complete,
    Fail,
    Integrate,
    Cancel,
}

/// Task lifecycle FSM — 8 statuses.
pub struct TaskFsm;

impl StateMachine for TaskFsm {
    type State = TaskStatus;
    type Trigger = TaskTrigger;

    fn transition(
        state: Self::State,
        trigger: &Self::Trigger,
    ) -> Result<Self::State, TransitionError> {
        use TaskStatus::*;
        use TaskTrigger::*;

        // Terminal states.
        if matches!(state, Integrated | Cancelled) {
            return Err(TransitionError::TerminalState {
                state: format!("{state:?}"),
            });
        }

        match (state, trigger) {
            // Draft
            (Draft, Approve) => Ok(Pending),
            (Draft, Cancel) => Ok(Cancelled),

            // Pending
            (Pending, Assign) => Ok(Assigned),
            (Pending, Cancel) => Ok(Cancelled),

            // Assigned
            (Assigned, Start) => Ok(InProgress),
            (Assigned, Cancel) => Ok(Cancelled),

            // InProgress
            (InProgress, Complete) => Ok(Completed),
            (InProgress, Fail) => Ok(Failed),
            (InProgress, Cancel) => Ok(Cancelled),

            // Completed
            (Completed, Integrate) => Ok(Integrated),
            (Completed, Fail) => Ok(Failed),

            // Failed — retriable
            (Failed, Assign) => Ok(Assigned),

            _ => Err(TransitionError::IllegalTransition {
                state: format!("{state:?}"),
                trigger: format!("{trigger:?}"),
            }),
        }
    }
}
