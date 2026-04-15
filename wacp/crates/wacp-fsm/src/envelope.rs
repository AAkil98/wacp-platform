use wacp_types::EnvelopeState;

use crate::{StateMachine, TransitionError};

/// Triggers for envelope lifecycle transitions (PROTOCOL.md §4.2).
#[derive(Debug, Clone, Copy)]
pub enum EnvelopeTrigger {
    Submit,
    ValidationPassed,
    ValidationFailed,
    Deliver,
    Acknowledge,
}

/// Envelope lifecycle FSM — 5 states.
pub struct EnvelopeFsm;

impl StateMachine for EnvelopeFsm {
    type State = EnvelopeState;
    type Trigger = EnvelopeTrigger;

    fn transition(
        state: Self::State,
        trigger: &Self::Trigger,
    ) -> Result<Self::State, TransitionError> {
        use EnvelopeState::*;
        use EnvelopeTrigger::*;

        // Terminal states.
        if matches!(state, Acknowledged | Rejected) {
            return Err(TransitionError::TerminalState {
                state: format!("{state:?}"),
            });
        }

        match (state, trigger) {
            (Created, ValidationPassed) => Ok(Validated),
            (Created, ValidationFailed) => Ok(Rejected),
            (Validated, Deliver) => Ok(Delivered),
            (Delivered, Acknowledge) => Ok(Acknowledged),

            _ => Err(TransitionError::IllegalTransition {
                state: format!("{state:?}"),
                trigger: format!("{trigger:?}"),
            }),
        }
    }
}
