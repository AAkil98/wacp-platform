//! WACP — Permission engine.
//!
//! Three lookup tables built from a Taxonomy: permission matrix, checkpoint type table,
//! and port rights table. Default-deny evaluation for all protocol actions.

use std::collections::{HashMap, HashSet};

use wacp_taxonomy::Taxonomy;
use wacp_types::{EnvelopeOrigin, PortRight, PortRightType, SignalType, WorkspaceId};

/// The permission engine. Built from a Taxonomy at initialization.
pub struct PermissionEngine {
    /// (sender_role, envelope_type, receiver_role) → allowed
    permission_matrix: HashSet<(String, String, String)>,
    /// checkpoint_type → set of permitted role names
    checkpoint_table: HashMap<String, HashSet<String>>,
    /// Signal emit sets per base role name (includes derived roles mapped to their base).
    signal_emit_sets: HashMap<String, HashSet<SignalType>>,
    /// All known role names (for derived → base mapping).
    role_bases: HashMap<String, String>,
    /// workspace_id → list of active port rights
    port_rights: HashMap<WorkspaceId, Vec<PortRight>>,
}

/// An action to be evaluated.
#[derive(Debug)]
pub enum Action<'a> {
    SendEnvelope {
        sender_role: &'a str,
        envelope_type: &'a str,
        receiver_role: &'a str,
        sender_workspace: &'a WorkspaceId,
        receiver_workspace: &'a WorkspaceId,
        origin: EnvelopeOrigin,
    },
    CreateCheckpoint {
        role: &'a str,
        checkpoint_type: &'a str,
    },
    EmitSignal {
        role: &'a str,
        signal_type: SignalType,
    },
}

/// Why a permission was denied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PermissionDenied {
    #[error("not in permission matrix ({rule})")]
    NotInPermissionMatrix { rule: String },

    #[error("no port right ({rule})")]
    NoPortRight { rule: String },

    #[error("checkpoint type not permitted ({rule})")]
    CheckpointTypeNotPermitted { rule: String },

    #[error("signal type not permitted ({rule})")]
    SignalTypeNotPermitted { rule: String },
}

/// Base signal emit sets (PROTOCOL.md §5.2).
fn coordinator_emit_set() -> HashSet<SignalType> {
    [
        SignalType::Ready,
        SignalType::Started,
        SignalType::Failed,
        SignalType::Integrate,
        SignalType::Acknowledged,
    ]
    .into_iter()
    .collect()
}

fn worker_emit_set() -> HashSet<SignalType> {
    [
        SignalType::Ready,
        SignalType::Started,
        SignalType::Blocked,
        SignalType::Checkpoint,
        SignalType::Complete,
        SignalType::Failed,
        SignalType::Escalation,
    ]
    .into_iter()
    .collect()
}

fn observer_emit_set() -> HashSet<SignalType> {
    [
        SignalType::Ready,
        SignalType::Started,
        SignalType::Complete,
        SignalType::Failed,
        SignalType::Escalation,
    ]
    .into_iter()
    .collect()
}

impl PermissionEngine {
    /// Build the permission engine from a validated taxonomy.
    pub fn new(taxonomy: &Taxonomy) -> Self {
        // Build permission matrix.
        let mut permission_matrix = HashSet::new();
        for row in &taxonomy.envelope_permissions {
            permission_matrix.insert((
                row.sender_role.clone(),
                row.envelope_type.clone(),
                row.receiver_role.clone(),
            ));
        }

        // Build checkpoint table.
        let checkpoint_table = taxonomy.checkpoint_types.clone();

        // Build signal emit sets and role→base mapping.
        let mut signal_emit_sets: HashMap<String, HashSet<SignalType>> = HashMap::new();
        signal_emit_sets.insert("coordinator".into(), coordinator_emit_set());
        signal_emit_sets.insert("worker".into(), worker_emit_set());
        signal_emit_sets.insert("observer".into(), observer_emit_set());

        let mut role_bases: HashMap<String, String> = HashMap::new();
        role_bases.insert("coordinator".into(), "coordinator".into());
        role_bases.insert("worker".into(), "worker".into());
        role_bases.insert("observer".into(), "observer".into());

        for (name, resolved) in &taxonomy.resolved_roles {
            let base_name = match resolved.base {
                wacp_types::BaseRole::Coordinator => "coordinator",
                wacp_types::BaseRole::Worker => "worker",
                wacp_types::BaseRole::Observer => "observer",
            };
            role_bases.insert(name.clone(), base_name.into());
            // Derived roles inherit their base role's emit set.
            signal_emit_sets.insert(
                name.clone(),
                signal_emit_sets.get(base_name).cloned().unwrap_or_default(),
            );
        }

        PermissionEngine {
            permission_matrix,
            checkpoint_table,
            signal_emit_sets,
            role_bases,
            port_rights: HashMap::new(),
        }
    }

    /// Evaluate an action. Returns Ok(()) if permitted, Err(PermissionDenied) if denied.
    pub fn evaluate(&self, action: &Action) -> Result<(), PermissionDenied> {
        match action {
            Action::SendEnvelope { .. } => self.check_envelope_send(action),
            Action::CreateCheckpoint { .. } => self.check_checkpoint_create(action),
            Action::EmitSignal { .. } => self.check_signal_emit(action),
        }
    }

    /// Check envelope send permission: matrix + port rights.
    fn check_envelope_send(&self, action: &Action) -> Result<(), PermissionDenied> {
        let Action::SendEnvelope {
            sender_role,
            envelope_type,
            receiver_role,
            sender_workspace,
            receiver_workspace,
            origin,
        } = action
        else {
            unreachable!()
        };

        // Highway override: human-origin envelopes bypass matrix and port rights (§8.1).
        if *origin == EnvelopeOrigin::Human {
            return Ok(());
        }

        // Permission matrix check.
        // Check with exact roles first, then try base role mappings.
        if !self.matrix_allows(sender_role, envelope_type, receiver_role) {
            return Err(PermissionDenied::NotInPermissionMatrix {
                rule: "§5.5".into(),
            });
        }

        // Port rights check.
        if !self.has_send_right(sender_workspace, receiver_workspace) {
            return Err(PermissionDenied::NoPortRight {
                rule: "§5.6".into(),
            });
        }

        Ok(())
    }

    /// Check the permission matrix, considering derived→base role mappings.
    fn matrix_allows(&self, sender: &str, envelope_type: &str, receiver: &str) -> bool {
        // Direct match.
        if self.permission_matrix.contains(&(
            sender.to_string(),
            envelope_type.to_string(),
            receiver.to_string(),
        )) {
            return true;
        }

        // Try sender's base role.
        let sender_base = self.role_bases.get(sender).map(|s| s.as_str());
        let receiver_base = self.role_bases.get(receiver).map(|s| s.as_str());

        if let Some(sb) = sender_base {
            if self.permission_matrix.contains(&(
                sb.to_string(),
                envelope_type.to_string(),
                receiver.to_string(),
            )) {
                return true;
            }
            if let Some(rb) = receiver_base
                && self.permission_matrix.contains(&(
                    sb.to_string(),
                    envelope_type.to_string(),
                    rb.to_string(),
                ))
            {
                return true;
            }
        }

        if let Some(rb) = receiver_base
            && self.permission_matrix.contains(&(
                sender.to_string(),
                envelope_type.to_string(),
                rb.to_string(),
            ))
        {
            return true;
        }

        false
    }

    /// Check checkpoint creation permission.
    fn check_checkpoint_create(&self, action: &Action) -> Result<(), PermissionDenied> {
        let Action::CreateCheckpoint {
            role,
            checkpoint_type,
        } = action
        else {
            unreachable!()
        };

        if let Some(permitted) = self.checkpoint_table.get(*checkpoint_type) {
            // Check exact role or base role.
            if permitted.contains(*role) {
                return Ok(());
            }
            if let Some(base) = self.role_bases.get(*role)
                && permitted.contains(base)
            {
                return Ok(());
            }
        }

        Err(PermissionDenied::CheckpointTypeNotPermitted {
            rule: "§7.2".into(),
        })
    }

    /// Check signal emission permission.
    fn check_signal_emit(&self, action: &Action) -> Result<(), PermissionDenied> {
        let Action::EmitSignal { role, signal_type } = action else {
            unreachable!()
        };

        if let Some(emit_set) = self.signal_emit_sets.get(*role)
            && emit_set.contains(signal_type)
        {
            return Ok(());
        }

        Err(PermissionDenied::SignalTypeNotPermitted {
            rule: "§5.2".into(),
        })
    }

    // --- Port rights management ---

    /// Grant a port right.
    pub fn grant_port_right(&mut self, right: PortRight) {
        self.port_rights
            .entry(right.holder.clone())
            .or_default()
            .push(right);
    }

    /// Revoke a specific port right.
    pub fn revoke_port_right(
        &mut self,
        holder: &WorkspaceId,
        target: &WorkspaceId,
        right_type: PortRightType,
    ) {
        if let Some(rights) = self.port_rights.get_mut(holder) {
            rights.retain(|r| !(r.target == *target && r.right_type == right_type));
        }
    }

    /// Consume a SendOnce right. Returns true if consumed, false if none exists.
    pub fn consume_send_once(&mut self, holder: &WorkspaceId, target: &WorkspaceId) -> bool {
        if let Some(rights) = self.port_rights.get_mut(holder)
            && let Some(pos) = rights
                .iter()
                .position(|r| r.target == *target && r.right_type == PortRightType::SendOnce)
        {
            rights.remove(pos);
            return true;
        }
        false
    }

    /// Check if the holder has a Send or SendOnce right to the target.
    pub fn has_send_right(&self, holder: &WorkspaceId, target: &WorkspaceId) -> bool {
        self.port_rights.get(holder).is_some_and(|rights| {
            rights.iter().any(|r| {
                r.target == *target
                    && matches!(r.right_type, PortRightType::Send | PortRightType::SendOnce)
            })
        })
    }
}

#[cfg(test)]
mod tests;
