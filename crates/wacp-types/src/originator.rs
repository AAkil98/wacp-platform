use serde::{Deserialize, Serialize};

use crate::id::UserId;

/// Causal origin of a workspace (PROTOCOL.md §4.8).
///
/// Every workspace carries an originator — either `System` for
/// coordinator-initiated workspaces, or `User(user_id)` for
/// human-injected workspaces. Immutable once set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Originator {
    System,
    User(UserId),
}
