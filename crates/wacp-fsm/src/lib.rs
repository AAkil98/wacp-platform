//! WACP — State machine engine.
//!
//! Generic `StateMachine` trait with three protocol instantiations:
//! workspace lifecycle (9 states), envelope lifecycle (5 states), task lifecycle (8 statuses).

mod envelope;
mod task;
mod workspace;

pub use envelope::{EnvelopeFsm, EnvelopeTrigger};
pub use task::{TaskFsm, TaskTrigger};
pub use workspace::{WorkspaceFsm, WorkspaceTrigger};

/// A deterministic state machine. Given (state, trigger), returns the new state or an error.
pub trait StateMachine {
    type State: Copy + Eq + std::fmt::Debug;
    type Trigger: std::fmt::Debug;

    fn transition(
        state: Self::State,
        trigger: &Self::Trigger,
    ) -> Result<Self::State, TransitionError>;
}

/// Errors produced by illegal or terminal-state transitions.
#[derive(Debug, Clone, thiserror::Error)]
pub enum TransitionError {
    #[error("illegal transition: {state} + {trigger}")]
    IllegalTransition { state: String, trigger: String },

    #[error("terminal state: {state} rejects all triggers")]
    TerminalState { state: String },
}

#[cfg(test)]
mod tests;
