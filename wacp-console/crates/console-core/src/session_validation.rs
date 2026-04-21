//! Session validation engine — 12 checks run before launch.
//!
//! Spec: `wcon-sessions` §3.1

use console_db::DbPool;
use console_db::queries::{profiles, session_assignments};
use serde::Serialize;

use crate::taxonomy::TaxonomyIndex;

/// A session validation violation.
#[derive(Debug, Clone, Serialize)]
pub struct SessionViolation {
    pub code: &'static str,
    pub message: String,
    pub slot: Option<i64>,
}

/// Result of session validation.
#[derive(Debug, Clone, Serialize)]
pub struct SessionValidationResult {
    pub violations: Vec<SessionViolation>,
}

impl SessionValidationResult {
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Input for session validation.
pub struct SessionValidationInput<'a> {
    pub session_id: &'a str,
    pub vertical: &'a str,
    pub workflow: &'a str,
    pub context: Option<&'a str>,
    pub budget_max_cost_micros: Option<i64>,
    pub budget_max_tokens: Option<i64>,
    pub budget_max_wall_time_ms: Option<i64>,
}

/// Validate a session's configuration before launch.
pub async fn validate_session(
    input: &SessionValidationInput<'_>,
    index: &TaxonomyIndex,
    pool: &DbPool,
) -> SessionValidationResult {
    let mut violations = Vec::new();

    // 1. UNKNOWN_VERTICAL
    let vertical = index.get_vertical(input.vertical);
    if vertical.is_none() {
        violations.push(SessionViolation {
            code: "UNKNOWN_VERTICAL",
            message: format!("Vertical '{}' not found in taxonomy", input.vertical),
            slot: None,
        });
        // Can't continue checking without vertical
        return SessionValidationResult { violations };
    }
    let vertical = vertical.expect("checked above");

    // Stub verticals can't be used
    if vertical.load_error.is_some() {
        violations.push(SessionViolation {
            code: "UNKNOWN_VERTICAL",
            message: format!(
                "Vertical '{}' failed to load: {}",
                input.vertical,
                vertical.load_error.as_deref().unwrap_or("unknown error")
            ),
            slot: None,
        });
        return SessionValidationResult { violations };
    }

    // 2. UNKNOWN_WORKFLOW
    if !vertical.workflows.iter().any(|w| w.id == input.workflow) {
        violations.push(SessionViolation {
            code: "UNKNOWN_WORKFLOW",
            message: format!(
                "Workflow '{}' not found in vertical '{}'",
                input.workflow, input.vertical
            ),
            slot: None,
        });
    }

    // Fetch assignments
    let assignments = session_assignments::list_by_session(pool, input.session_id)
        .await
        .unwrap_or_default();

    // 3. MISSING_ASSIGNMENT — check all vertical roles have an assignment
    for role in &vertical.roles {
        if !assignments.iter().any(|a| a.role_ref == *role) {
            violations.push(SessionViolation {
                code: "MISSING_ASSIGNMENT",
                message: format!("No assignment for required role '{role}'"),
                slot: None,
            });
        }
    }

    // Per-assignment validation
    for assignment in &assignments {
        let slot = Some(assignment.slot_position);

        let pid = &assignment.profile_id;
        let profile = profiles::get_current(pool, pid).await.ok().flatten();
        let versioned_profile = profiles::get_version(pool, pid, assignment.profile_version)
            .await
            .ok()
            .flatten();

        // 4a. DELETED_PROFILE_IN_ASSIGNMENT
        if let Some(ref p) = profile {
            if p.deleted_at.is_some() {
                violations.push(SessionViolation {
                    code: "DELETED_PROFILE_IN_ASSIGNMENT",
                    message: format!(
                        "Profile '{}' (slot {}) has been deleted",
                        p.name, assignment.slot_position
                    ),
                    slot,
                });
            }
        } else {
            violations.push(SessionViolation {
                code: "UNKNOWN_PROFILE",
                message: format!("Profile '{pid}' not found"),
                slot,
            });
            continue;
        }

        // 5. UNKNOWN_VERSION
        if versioned_profile.is_none() {
            violations.push(SessionViolation {
                code: "UNKNOWN_VERSION",
                message: format!(
                    "Profile '{pid}' version {} not found",
                    assignment.profile_version
                ),
                slot,
            });
            continue;
        }

        // 6. ROLE_MISMATCH
        if let Some(ref p) = versioned_profile
            && p.role_ref != assignment.role_ref
        {
            violations.push(SessionViolation {
                code: "ROLE_MISMATCH",
                message: format!(
                    "Profile '{}' has role '{}' but slot requires '{}'",
                    p.name, p.role_ref, assignment.role_ref
                ),
                slot,
            });
        }

        // 7. INVALID_PROFILE — full profile validation
        // (Deferred to the profile validation engine — called during launch)
    }

    // 8+9. Context validation
    if !vertical.context_schema.is_empty() {
        let context: serde_json::Map<String, serde_json::Value> = input
            .context
            .and_then(|c| serde_json::from_str(c).ok())
            .unwrap_or_default();

        for (field_name, field_def) in &vertical.context_schema {
            let value = context.get(field_name);

            // 8. MISSING_CONTEXT
            if field_def.required && value.is_none() {
                violations.push(SessionViolation {
                    code: "MISSING_CONTEXT",
                    message: format!("Required context field '{field_name}' is missing"),
                    slot: None,
                });
                continue;
            }

            // 9. INVALID_CONTEXT — type validation
            if let Some(val) = value {
                use console_runtime::FieldType;
                let valid = match field_def.field_type {
                    FieldType::String => val.is_string(),
                    FieldType::Number => val.is_number(),
                    FieldType::Boolean => val.is_boolean(),
                    FieldType::Enum => {
                        if let Some(ref enums) = field_def.enum_values {
                            val.as_str().is_some_and(|s| enums.contains(&s.to_string()))
                        } else {
                            val.is_string()
                        }
                    }
                };
                if !valid {
                    violations.push(SessionViolation {
                        code: "INVALID_CONTEXT",
                        message: format!(
                            "Context field '{field_name}' expected type {:?}, got {}",
                            field_def.field_type, val
                        ),
                        slot: None,
                    });
                }
            }
        }
    }

    // 10. INVALID_BUDGET
    for (name, val) in [
        ("budget_max_cost_micros", input.budget_max_cost_micros),
        ("budget_max_tokens", input.budget_max_tokens),
        ("budget_max_wall_time_ms", input.budget_max_wall_time_ms),
    ] {
        if let Some(v) = val
            && v < 0
        {
            violations.push(SessionViolation {
                code: "INVALID_BUDGET",
                message: format!("Session {name} must be non-negative"),
                slot: None,
            });
        }
    }

    // 11. RUNTIME_UNREACHABLE — checked at launch time, not here
    // (the caller checks this separately since it requires async gRPC)

    SessionValidationResult { violations }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy_builder;
    use console_db::create_test_pool;
    use console_runtime::{ProfileSummary, ToolSummary, VerticalManifest, WorkflowSummary};
    use std::collections::HashMap;

    fn test_manifest() -> VerticalManifest {
        VerticalManifest {
            id: "swe".into(),
            name: "SWE".into(),
            defining_constraint: "".into(),
            context_schema: HashMap::new(),
            tool_policies: HashMap::new(),
            checkpoint_types: HashMap::new(),
            quality_criteria: vec![],
            task_types: vec![],
            workflows: vec![WorkflowSummary {
                id: "debug".into(),
                name: "Debug".into(),
                description: "Debug workflow".into(),
                stage_count: 2,
                gated_stage_count: 1,
            }],
            profiles: vec![ProfileSummary {
                role_id: "implementer".into(),
                autonomy: "autonomous".into(),
            }],
            tools: vec![ToolSummary {
                name: "code_edit".into(),
                description: "".into(),
            }],
        }
    }

    #[tokio::test]
    async fn unknown_vertical() {
        let pool = create_test_pool().await.unwrap();
        let index = taxonomy_builder::build_index(None, &[], &[]).index;
        let input = SessionValidationInput {
            session_id: "s1",
            vertical: "nonexistent",
            workflow: "w1",
            context: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
        };
        let result = validate_session(&input, &index, &pool).await;
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.code == "UNKNOWN_VERTICAL")
        );
    }

    #[tokio::test]
    async fn unknown_workflow() {
        let pool = create_test_pool().await.unwrap();
        let index = taxonomy_builder::build_index(None, &[test_manifest()], &[]).index;
        let input = SessionValidationInput {
            session_id: "s1",
            vertical: "swe",
            workflow: "nonexistent",
            context: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
        };
        let result = validate_session(&input, &index, &pool).await;
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.code == "UNKNOWN_WORKFLOW")
        );
    }

    #[tokio::test]
    async fn missing_assignment() {
        let pool = create_test_pool().await.unwrap();
        let index = taxonomy_builder::build_index(None, &[test_manifest()], &[]).index;
        let input = SessionValidationInput {
            session_id: "s1",
            vertical: "swe",
            workflow: "debug",
            context: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
        };
        let result = validate_session(&input, &index, &pool).await;
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.code == "MISSING_ASSIGNMENT")
        );
    }

    #[tokio::test]
    async fn missing_context_required_field() {
        let pool = create_test_pool().await.unwrap();
        let mut manifest = test_manifest();
        manifest.context_schema.insert(
            "environment".into(),
            console_runtime::ContextField {
                field_type: console_runtime::FieldType::String,
                required: true,
                description: "Target environment".into(),
                enum_values: None,
                default: None,
            },
        );
        let index = taxonomy_builder::build_index(None, &[manifest], &[]).index;
        let input = SessionValidationInput {
            session_id: "s1",
            vertical: "swe",
            workflow: "debug",
            context: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
        };
        let result = validate_session(&input, &index, &pool).await;
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.code == "MISSING_CONTEXT")
        );
    }
}
