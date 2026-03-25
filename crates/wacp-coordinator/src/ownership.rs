use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wacp_types::{Originator, UserId, WorkspaceId};

/// Derived index for escalation routing (topology.md §5.1).
///
/// Maps workspace id → owner for O(1) lookup when an escalation
/// signal needs to be routed to the owning human. Maintained on
/// workspace creation and ownership transfer.
#[derive(Serialize, Deserialize)]
pub struct EscalationRouter {
    routing: HashMap<String, UserId>,
}

impl EscalationRouter {
    pub fn new() -> Self {
        Self {
            routing: HashMap::new(),
        }
    }

    /// Register a workspace's owner. Called on workspace creation.
    pub fn register(&mut self, id: &WorkspaceId, owner: &UserId) {
        self.routing.insert(id.to_string(), owner.clone());
    }

    /// Update after ownership transfer.
    pub fn update(&mut self, id: &WorkspaceId, new_owner: &UserId) {
        self.routing.insert(id.to_string(), new_owner.clone());
    }

    /// Look up the owner for escalation routing.
    pub fn route(&self, id: &WorkspaceId) -> Option<&UserId> {
        self.routing.get(id.as_ref())
    }

    /// Remove an entry.
    pub fn remove(&mut self, id: &WorkspaceId) {
        self.routing.remove(id.as_ref());
    }
}

impl Default for EscalationRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve workspace owner at creation (topology.md §5.2).
///
/// Returns `explicit` if provided, otherwise inherits `parent_owner`.
pub fn resolve_owner(parent_owner: &UserId, explicit: Option<&UserId>) -> UserId {
    explicit.cloned().unwrap_or_else(|| parent_owner.clone())
}

/// Resolve workspace originator at creation (topology.md §6.1).
///
/// - Delegation (not injection): inherits parent's originator.
/// - Human injection: `Originator::User(injector)`.
///
/// Panics if `is_injection` is true but `injector` is `None`.
pub fn resolve_originator(
    parent_originator: &Originator,
    is_injection: bool,
    injector: Option<&UserId>,
) -> Originator {
    if is_injection {
        Originator::User(
            injector
                .expect("injection must have an injector")
                .clone(),
        )
    } else {
        parent_originator.clone()
    }
}
