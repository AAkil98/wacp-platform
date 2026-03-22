use std::collections::{HashMap, HashSet};

use wacp_types::BaseRole;

use crate::definition::TaxonomyDefinition;
use crate::error::TaxonomyError;
use crate::resolved::{EnvelopePermissionRow, ResolvedRole, Taxonomy};

const BASE_ROLES: &[&str] = &["coordinator", "worker", "observer"];
const BASE_ENVELOPE_TYPES: &[&str] = &["directive", "feedback", "query"];
const BASE_CHECKPOINT_TYPES: &[&str] = &["artifact", "observation"];

/// Coordinator-level capabilities that derived roles cannot add.
const COORDINATOR_CAPABILITIES: &[&str] = &[
    "create_workspace",
    "perform_integration",
    "manage_budgets",
    "abort_workspace",
    "assign_roles",
];

/// Default capabilities per base role.
fn base_capabilities(role: BaseRole) -> HashSet<String> {
    match role {
        BaseRole::Worker => [
            "send: query → coordinator",
            "create: artifact",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        BaseRole::Observer => ["create: observation"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        BaseRole::Coordinator => HashSet::new(),
    }
}

/// Default checkpoint types per base role.
fn base_checkpoint_types(role: BaseRole) -> HashSet<String> {
    match role {
        BaseRole::Worker => ["artifact"].iter().map(|s| s.to_string()).collect(),
        BaseRole::Observer => ["observation"].iter().map(|s| s.to_string()).collect(),
        BaseRole::Coordinator => HashSet::new(),
    }
}

/// Base permission matrix rows (PROTOCOL.md §5.5).
fn base_permission_rows() -> Vec<EnvelopePermissionRow> {
    vec![
        EnvelopePermissionRow {
            sender_role: "coordinator".into(),
            envelope_type: "directive".into(),
            receiver_role: "worker".into(),
        },
        EnvelopePermissionRow {
            sender_role: "coordinator".into(),
            envelope_type: "feedback".into(),
            receiver_role: "worker".into(),
        },
        EnvelopePermissionRow {
            sender_role: "worker".into(),
            envelope_type: "query".into(),
            receiver_role: "coordinator".into(),
        },
    ]
}

/// Load and validate a taxonomy from a parsed definition.
pub fn load(
    def: TaxonomyDefinition,
    protocol_version: &str,
) -> Result<Taxonomy, TaxonomyError> {
    // 1. Protocol version check.
    if def.protocol_version != protocol_version {
        return Err(TaxonomyError::ProtocolVersionMismatch {
            expected: protocol_version.to_string(),
            found: def.protocol_version.clone(),
        });
    }

    // Collect derived role names for cross-reference checks.
    let derived_role_names: HashSet<&str> = def.roles.iter().map(|r| r.name.as_str()).collect();
    let custom_checkpoint_names: HashSet<&str> =
        def.checkpoint_types.iter().map(|c| c.name.as_str()).collect();

    // Helper: is this a known role (base or derived)?
    let is_known_role =
        |name: &str| BASE_ROLES.contains(&name) || derived_role_names.contains(name);

    // 2. Role name uniqueness.
    {
        let mut seen = HashSet::new();
        for role in &def.roles {
            if !seen.insert(&role.name) {
                return Err(TaxonomyError::DuplicateRoleName(role.name.clone()));
            }
        }
    }

    // 3. Role name vs base names.
    for role in &def.roles {
        if BASE_ROLES.contains(&role.name.as_str()) {
            return Err(TaxonomyError::BaseNameCollision(role.name.clone()));
        }
    }

    // 4. Envelope type name uniqueness.
    {
        let mut seen = HashSet::new();
        for et in &def.envelope_types {
            if !seen.insert(&et.name) {
                return Err(TaxonomyError::DuplicateEnvelopeType(et.name.clone()));
            }
        }
    }

    // 5. Envelope type vs base names.
    for et in &def.envelope_types {
        if BASE_ENVELOPE_TYPES.contains(&et.name.as_str()) {
            return Err(TaxonomyError::BaseNameCollision(et.name.clone()));
        }
    }

    // 6. Checkpoint type name uniqueness.
    {
        let mut seen = HashSet::new();
        for ct in &def.checkpoint_types {
            if !seen.insert(&ct.name) {
                return Err(TaxonomyError::DuplicateCheckpointType(ct.name.clone()));
            }
        }
    }

    // 7. Checkpoint type vs base names.
    for ct in &def.checkpoint_types {
        if BASE_CHECKPOINT_TYPES.contains(&ct.name.as_str()) {
            return Err(TaxonomyError::BaseNameCollision(ct.name.clone()));
        }
    }

    // 8. Inheritance validity.
    for role in &def.roles {
        match role.extends.as_str() {
            "worker" | "observer" => {}
            other => {
                return Err(TaxonomyError::InvalidInheritance {
                    role: role.name.clone(),
                    extends: other.to_string(),
                });
            }
        }
    }

    // 9. No privilege escalation.
    for role in &def.roles {
        for cap in &role.add {
            for coord_cap in COORDINATOR_CAPABILITIES {
                if cap.contains(coord_cap) {
                    return Err(TaxonomyError::PrivilegeEscalation {
                        role: role.name.clone(),
                        capability: cap.clone(),
                    });
                }
            }
        }
    }

    // 10. Cross-registry consistency (checkpoint type overrides).
    for role in &def.roles {
        if let Some(ref ct_overrides) = role.checkpoint_types {
            for ct_name in ct_overrides {
                if !BASE_CHECKPOINT_TYPES.contains(&ct_name.as_str())
                    && !custom_checkpoint_names.contains(ct_name.as_str())
                {
                    return Err(TaxonomyError::CrossRegistryMissing {
                        role: role.name.clone(),
                        reference: ct_name.clone(),
                    });
                }
            }
        }
    }

    // 11. Envelope type role references exist.
    for et in &def.envelope_types {
        if et.permissions.is_empty() {
            return Err(TaxonomyError::EmptyPermissions(et.name.clone()));
        }
        for perm in &et.permissions {
            if !is_known_role(&perm.sender_role) {
                return Err(TaxonomyError::EnvelopeRoleNotFound {
                    envelope_type: et.name.clone(),
                    role: perm.sender_role.clone(),
                });
            }
            if !is_known_role(&perm.receiver_role) {
                return Err(TaxonomyError::EnvelopeRoleNotFound {
                    envelope_type: et.name.clone(),
                    role: perm.receiver_role.clone(),
                });
            }
        }
    }

    // 12. Checkpoint type role references exist + non-empty.
    for ct in &def.checkpoint_types {
        if ct.permitted_roles.is_empty() {
            return Err(TaxonomyError::EmptyRoleList(ct.name.clone()));
        }
        for role_name in &ct.permitted_roles {
            if !is_known_role(role_name) {
                return Err(TaxonomyError::CheckpointRoleNotFound {
                    checkpoint_type: ct.name.clone(),
                    role: role_name.clone(),
                });
            }
        }
    }

    // --- Validation passed. Build the taxonomy. ---

    // Resolve derived roles.
    let mut resolved_roles = HashMap::new();
    for role_def in &def.roles {
        let base = match role_def.extends.as_str() {
            "worker" => BaseRole::Worker,
            "observer" => BaseRole::Observer,
            _ => unreachable!(), // validated above
        };

        let mut capabilities = base_capabilities(base);
        for cap in &role_def.remove {
            capabilities.remove(cap);
        }
        for cap in &role_def.add {
            capabilities.insert(cap.clone());
        }

        let checkpoint_types = if let Some(ref overrides) = role_def.checkpoint_types {
            overrides.iter().cloned().collect()
        } else {
            base_checkpoint_types(base)
        };

        resolved_roles.insert(
            role_def.name.clone(),
            ResolvedRole {
                name: role_def.name.clone(),
                base,
                capabilities,
                checkpoint_types,
            },
        );
    }

    // Build envelope types set.
    let mut envelope_types: HashSet<String> =
        BASE_ENVELOPE_TYPES.iter().map(|s| s.to_string()).collect();
    for et in &def.envelope_types {
        envelope_types.insert(et.name.clone());
    }

    // Build permission matrix rows.
    let mut envelope_permissions = base_permission_rows();
    for et in &def.envelope_types {
        for perm in &et.permissions {
            envelope_permissions.push(EnvelopePermissionRow {
                sender_role: perm.sender_role.clone(),
                envelope_type: et.name.clone(),
                receiver_role: perm.receiver_role.clone(),
            });
        }
    }

    // Build checkpoint type table.
    let mut checkpoint_types: HashMap<String, HashSet<String>> = HashMap::new();
    checkpoint_types.insert(
        "artifact".into(),
        ["worker"].iter().map(|s| s.to_string()).collect(),
    );
    checkpoint_types.insert(
        "observation".into(),
        ["observer"].iter().map(|s| s.to_string()).collect(),
    );
    for ct in &def.checkpoint_types {
        checkpoint_types.insert(
            ct.name.clone(),
            ct.permitted_roles.iter().cloned().collect(),
        );
    }

    // Add derived role checkpoint type overrides to the table.
    for role in resolved_roles.values() {
        for ct_name in &role.checkpoint_types {
            checkpoint_types
                .entry(ct_name.clone())
                .or_default()
                .insert(role.name.clone());
        }
    }

    Ok(Taxonomy {
        id: def.id,
        version: def.version,
        protocol_version: def.protocol_version,
        resolved_roles,
        envelope_types,
        envelope_permissions,
        checkpoint_types,
    })
}
