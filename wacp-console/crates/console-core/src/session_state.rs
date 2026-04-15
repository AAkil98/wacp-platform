//! Session state machine — all transitions from `wcon-data-model` §4.3.
//!
//! States: configuring → validating → launching → active → completed/failed/cancelled

use crate::error::ConsoleError;

/// Valid session states.
pub const CONFIGURING: &str = "configuring";
pub const VALIDATING: &str = "validating";
pub const LAUNCHING: &str = "launching";
pub const ACTIVE: &str = "active";
pub const COMPLETED: &str = "completed";
pub const FAILED: &str = "failed";
pub const CANCELLED: &str = "cancelled";

/// Check if a state is terminal (no transitions out).
pub fn is_terminal(state: &str) -> bool {
    matches!(state, COMPLETED | FAILED | CANCELLED)
}

/// Validate a state transition. Returns Ok(()) if valid, Err if not.
pub fn validate_transition(from: &str, to: &str) -> Result<(), ConsoleError> {
    let valid = match (from, to) {
        (CONFIGURING, VALIDATING) => true,
        (CONFIGURING, CANCELLED) => true,
        (VALIDATING, CONFIGURING) => true, // validation failed, back to configuring
        (VALIDATING, LAUNCHING) => true,
        (VALIDATING, CANCELLED) => true,
        (LAUNCHING, ACTIVE) => true,
        (LAUNCHING, FAILED) => true,
        (LAUNCHING, CANCELLED) => true,
        (ACTIVE, COMPLETED) => true,
        (ACTIVE, FAILED) => true,
        (ACTIVE, CANCELLED) => true,
        _ => false,
    };

    if valid {
        Ok(())
    } else {
        Err(ConsoleError::Conflict(format!(
            "Invalid state transition: {from} → {to}"
        )))
    }
}

/// Determine what cleanup is needed when cancelling from a given state.
pub enum CancelAction {
    /// No runtime cleanup needed (configuring/validating).
    NoOp,
    /// Best-effort abort of partial launch.
    BestEffortAbort,
    /// Full abort via CoordinatorService.AbortWorkspace.
    AbortWorkspace,
}

pub fn cancel_action_for_state(state: &str) -> Option<CancelAction> {
    match state {
        CONFIGURING | VALIDATING => Some(CancelAction::NoOp),
        LAUNCHING => Some(CancelAction::BestEffortAbort),
        ACTIVE => Some(CancelAction::AbortWorkspace),
        _ => None, // terminal states can't be cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_transitions() {
        assert!(validate_transition(CONFIGURING, VALIDATING).is_ok());
        assert!(validate_transition(VALIDATING, LAUNCHING).is_ok());
        assert!(validate_transition(LAUNCHING, ACTIVE).is_ok());
        assert!(validate_transition(ACTIVE, COMPLETED).is_ok());
        assert!(validate_transition(ACTIVE, FAILED).is_ok());
        assert!(validate_transition(ACTIVE, CANCELLED).is_ok());
        assert!(validate_transition(CONFIGURING, CANCELLED).is_ok());
        assert!(validate_transition(VALIDATING, CONFIGURING).is_ok());
    }

    #[test]
    fn invalid_transitions() {
        assert!(validate_transition(COMPLETED, ACTIVE).is_err());
        assert!(validate_transition(FAILED, CONFIGURING).is_err());
        assert!(validate_transition(CANCELLED, ACTIVE).is_err());
        assert!(validate_transition(ACTIVE, CONFIGURING).is_err());
        assert!(validate_transition(LAUNCHING, CONFIGURING).is_err());
    }

    #[test]
    fn terminal_states() {
        assert!(is_terminal(COMPLETED));
        assert!(is_terminal(FAILED));
        assert!(is_terminal(CANCELLED));
        assert!(!is_terminal(CONFIGURING));
        assert!(!is_terminal(ACTIVE));
    }

    #[test]
    fn cancel_actions() {
        assert!(matches!(
            cancel_action_for_state(CONFIGURING),
            Some(CancelAction::NoOp)
        ));
        assert!(matches!(
            cancel_action_for_state(VALIDATING),
            Some(CancelAction::NoOp)
        ));
        assert!(matches!(
            cancel_action_for_state(LAUNCHING),
            Some(CancelAction::BestEffortAbort)
        ));
        assert!(matches!(
            cancel_action_for_state(ACTIVE),
            Some(CancelAction::AbortWorkspace)
        ));
        assert!(cancel_action_for_state(COMPLETED).is_none());
    }
}
