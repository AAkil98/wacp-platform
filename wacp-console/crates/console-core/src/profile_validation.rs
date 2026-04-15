//! Profile validation engine — 14 error codes + 2 warning types.
//!
//! Spec: `wcon-profiles` §3, `wcon-data-model` §10.1

use console_db::queries::profiles;
use console_db::DbPool;
use serde::Serialize;

use crate::taxonomy::TaxonomyIndex;

/// A validation violation (blocks save).
#[derive(Debug, Clone, Serialize)]
pub struct ValidationViolation {
    pub code: &'static str,
    pub message: String,
    pub field: Option<String>,
}

/// A validation warning (does not block save).
#[derive(Debug, Clone, Serialize)]
pub struct ValidationWarning {
    pub code: &'static str,
    pub message: String,
}

/// Result of validating a profile.
#[derive(Debug, Clone, Serialize)]
pub struct ValidationResult {
    pub violations: Vec<ValidationViolation>,
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Input fields for validation (common between create, update, import).
pub struct ProfileInput<'a> {
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub tags: Option<&'a str>,
    pub role_ref: &'a str,
    pub llm_provider: &'a str,
    pub llm_model: &'a str,
    pub llm_temperature: Option<f64>,
    pub llm_max_tokens: Option<i64>,
    pub autonomy: &'a str,
    pub tool_allowlist: Option<&'a str>,
    pub tool_denylist: Option<&'a str>,
    pub budget_max_cost_micros: Option<i64>,
    pub budget_max_tokens: Option<i64>,
    pub budget_max_wall_time_ms: Option<i64>,
    pub budget_warning_threshold: Option<f64>,
    pub visibility: &'a str,
    pub owner_user_id: &'a str,
    /// Exclude this profile ID from duplicate name check (for updates).
    pub exclude_id: Option<&'a str>,
}

/// Validate a profile against field rules, taxonomy index, and name uniqueness.
pub async fn validate_profile(
    input: &ProfileInput<'_>,
    index: &TaxonomyIndex,
    pool: &DbPool,
) -> ValidationResult {
    let mut violations = Vec::new();
    let mut warnings = Vec::new();

    // --- Field validation ---

    // INVALID_NAME
    if input.name.is_empty() || input.name.len() > 200 {
        violations.push(ValidationViolation {
            code: "INVALID_NAME",
            message: "Name must be 1–200 characters".into(),
            field: Some("name".into()),
        });
    }

    // INVALID_PROVIDER
    if input.llm_provider.is_empty() {
        violations.push(ValidationViolation {
            code: "INVALID_PROVIDER",
            message: "LLM provider is required".into(),
            field: Some("llm_provider".into()),
        });
    }

    // INVALID_MODEL
    if input.llm_model.is_empty() {
        violations.push(ValidationViolation {
            code: "INVALID_MODEL",
            message: "LLM model is required".into(),
            field: Some("llm_model".into()),
        });
    }

    // INVALID_TEMPERATURE
    if let Some(t) = input.llm_temperature
        && !(0.0..=2.0).contains(&t)
    {
        violations.push(ValidationViolation {
            code: "INVALID_TEMPERATURE",
            message: format!("Temperature must be 0.0–2.0, got {t}"),
            field: Some("llm_temperature".into()),
        });
    }

    // INVALID_MAX_TOKENS
    if let Some(mt) = input.llm_max_tokens
        && mt <= 0
    {
        violations.push(ValidationViolation {
            code: "INVALID_MAX_TOKENS",
            message: "Max tokens must be a positive integer".into(),
            field: Some("llm_max_tokens".into()),
        });
    }

    // INVALID_AUTONOMY
    if !matches!(input.autonomy, "autonomous" | "assisted" | "supervised") {
        violations.push(ValidationViolation {
            code: "INVALID_AUTONOMY",
            message: format!("Autonomy must be one of: autonomous, assisted, supervised; got '{}'", input.autonomy),
            field: Some("autonomy".into()),
        });
    }

    // INVALID_THRESHOLD
    if let Some(th) = input.budget_warning_threshold
        && !(0.0..=1.0).contains(&th)
    {
        violations.push(ValidationViolation {
            code: "INVALID_THRESHOLD",
            message: format!("Budget warning threshold must be 0.0–1.0, got {th}"),
            field: Some("budget_warning_threshold".into()),
        });
    }

    // INVALID_BUDGET
    for (field, val) in [
        ("budget_max_cost_micros", input.budget_max_cost_micros),
        ("budget_max_tokens", input.budget_max_tokens),
        ("budget_max_wall_time_ms", input.budget_max_wall_time_ms),
    ] {
        if let Some(v) = val
            && v < 0
        {
            violations.push(ValidationViolation {
                code: "INVALID_BUDGET",
                message: format!("{field} must be non-negative"),
                field: Some(field.into()),
            });
        }
    }

    // INVALID_TAGS
    if let Some(tags_json) = input.tags {
        match serde_json::from_str::<Vec<String>>(tags_json) {
            Ok(tags) => {
                for tag in &tags {
                    if tag.len() > 50 {
                        violations.push(ValidationViolation {
                            code: "INVALID_TAGS",
                            message: format!("Tag '{}' exceeds 50 characters", &tag[..50]),
                            field: Some("tags".into()),
                        });
                        break;
                    }
                }
            }
            Err(_) => {
                violations.push(ValidationViolation {
                    code: "INVALID_TAGS",
                    message: "Tags must be a JSON array of strings".into(),
                    field: Some("tags".into()),
                });
            }
        }
    }

    // INVALID_VISIBILITY
    if !matches!(input.visibility, "private" | "shared") {
        violations.push(ValidationViolation {
            code: "INVALID_VISIBILITY",
            message: format!("Visibility must be 'private' or 'shared', got '{}'", input.visibility),
            field: Some("visibility".into()),
        });
    }

    // --- Taxonomy validation ---

    // UNKNOWN_ROLE
    let role = index.get_role(input.role_ref);
    if role.is_none() {
        violations.push(ValidationViolation {
            code: "UNKNOWN_ROLE",
            message: format!("Role '{}' not found in taxonomy", input.role_ref),
            field: Some("role_ref".into()),
        });
    }

    // Tool list parsing + validation
    let allowlist: Vec<String> = input
        .tool_allowlist
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let denylist: Vec<String> = input
        .tool_denylist
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let role_vertical = role.and_then(|r| r.vertical.as_deref());

    for tool_name in allowlist.iter().chain(denylist.iter()) {
        match index.get_tool(tool_name) {
            None => {
                violations.push(ValidationViolation {
                    code: "UNKNOWN_TOOL",
                    message: format!("Tool '{tool_name}' not found in taxonomy"),
                    field: Some("tool_allowlist".into()),
                });
            }
            Some(tool) => {
                // TOOL_NOT_IN_ROLE_VERTICAL
                if let Some(rv) = role_vertical
                    && tool.vertical.as_deref() != Some(rv)
                {
                    violations.push(ValidationViolation {
                        code: "TOOL_NOT_IN_ROLE_VERTICAL",
                        message: format!(
                            "Tool '{tool_name}' belongs to vertical '{}' but role '{}' is in '{rv}'",
                            tool.vertical.as_deref().unwrap_or("none"),
                            input.role_ref
                        ),
                        field: Some("tool_allowlist".into()),
                    });
                }
            }
        }
    }

    // EMPTY_TOOL_SET — only for vertical (non-base) roles
    if let Some(r) = role
        && r.vertical.is_some()
    {
            let effective_tools: Vec<&str> = if allowlist.is_empty() {
                // All role tools minus denylist
                r.tools
                    .iter()
                    .filter(|t| !denylist.iter().any(|d| d == *t))
                    .map(|s| s.as_str())
                    .collect()
            } else {
                // Allowlist minus denylist
                allowlist
                    .iter()
                    .filter(|t| !denylist.iter().any(|d| d == *t))
                    .map(|s| s.as_str())
                    .collect()
            };

            if effective_tools.is_empty() {
                violations.push(ValidationViolation {
                    code: "EMPTY_TOOL_SET",
                    message: "Effective tool set is empty after applying allow/deny lists".into(),
                    field: Some("tool_allowlist".into()),
                });
            }

            // --- Warnings ---

            // TOOL_HAS_RUNTIME_POLICY
            for tool_name in &effective_tools {
                if let Some(tool) = index.get_tool(tool_name)
                    && tool.policy.is_some()
                {
                    warnings.push(ValidationWarning {
                        code: "TOOL_HAS_RUNTIME_POLICY",
                        message: format!("Tool '{tool_name}' has a runtime-enforced policy"),
                    });
                }
            }

            // Autonomous worker with write-capable tools
            if input.autonomy == "autonomous" && r.base_role == "worker" {
                let high_impact_suffixes = [
                    "_execute", "_write", "_deploy", "_delete",
                    "_rotate", "_launch", "_submit",
                ];
                for tool_name in &effective_tools {
                    if high_impact_suffixes.iter().any(|s| tool_name.ends_with(s)) {
                        warnings.push(ValidationWarning {
                            code: "AUTONOMOUS_WORKER_HIGH_IMPACT",
                            message: format!(
                                "Autonomous worker with high-impact tool '{tool_name}'"
                            ),
                        });
                        break; // One warning is enough
                    }
                }
            }
        }


    // --- Database validation ---

    // DUPLICATE_NAME
    if let Ok(true) = profiles::name_exists_for_user(
        pool,
        input.name,
        input.owner_user_id,
        input.exclude_id,
    )
    .await
    {
        violations.push(ValidationViolation {
            code: "DUPLICATE_NAME",
            message: format!("You already have a profile named '{}'", input.name),
            field: Some("name".into()),
        });
    }

    ValidationResult {
        violations,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy_builder;
    use console_db::create_test_pool;
    use console_runtime::{ProfileSummary, ToolPolicy, ToolPolicyKind, ToolSummary, VerticalManifest};
    use std::collections::HashMap;

    fn test_manifest() -> VerticalManifest {
        VerticalManifest {
            id: "swe".into(),
            name: "SWE".into(),
            defining_constraint: "".into(),
            context_schema: HashMap::new(),
            tool_policies: {
                let mut m = HashMap::new();
                m.insert("code_execute".into(), ToolPolicy {
                    kind: ToolPolicyKind::RequiresGate,
                    description: "Needs gate".into(),
                    checkpoint_type: None,
                    matching_field: None,
                    expires_after_ms: None,
                    gate_condition: Some("approved".into()),
                    budget_field: None,
                    blocked_classifications: None,
                    override_flag: None,
                });
                m
            },
            checkpoint_types: HashMap::new(),
            quality_criteria: vec![],
            task_types: vec![],
            workflows: vec![],
            profiles: vec![
                ProfileSummary { role_id: "implementer".into(), autonomy: "autonomous".into() },
            ],
            tools: vec![
                ToolSummary { name: "code_edit".into(), description: "Edit code".into() },
                ToolSummary { name: "code_execute".into(), description: "Execute code".into() },
                ToolSummary { name: "test_run".into(), description: "Run tests".into() },
            ],
        }
    }

    fn build_test_index() -> TaxonomyIndex {
        taxonomy_builder::build_index(None, &[test_manifest()], &[]).index
    }

    fn valid_input() -> ProfileInput<'static> {
        ProfileInput {
            name: "Test Profile",
            description: None,
            tags: None,
            role_ref: "swe:implementer",
            llm_provider: "anthropic",
            llm_model: "claude-sonnet-4-20250514",
            llm_temperature: Some(0.7),
            llm_max_tokens: Some(4096),
            autonomy: "assisted",
            tool_allowlist: None,
            tool_denylist: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
            budget_warning_threshold: Some(0.8),
            visibility: "private",
            owner_user_id: "user1",
            exclude_id: None,
        }
    }

    #[tokio::test]
    async fn valid_profile_passes() {
        let pool = create_test_pool().await.unwrap();
        let index = build_test_index();
        let input = valid_input();
        let result = validate_profile(&input, &index, &pool).await;
        assert!(result.is_valid(), "violations: {:?}", result.violations);
    }

    #[tokio::test]
    async fn invalid_name_empty() {
        let pool = create_test_pool().await.unwrap();
        let index = build_test_index();
        let mut input = valid_input();
        input.name = "";
        let result = validate_profile(&input, &index, &pool).await;
        assert!(result.violations.iter().any(|v| v.code == "INVALID_NAME"));
    }

    #[tokio::test]
    async fn unknown_role() {
        let pool = create_test_pool().await.unwrap();
        let index = build_test_index();
        let mut input = valid_input();
        input.role_ref = "nonexistent:role";
        let result = validate_profile(&input, &index, &pool).await;
        assert!(result.violations.iter().any(|v| v.code == "UNKNOWN_ROLE"));
    }

    #[tokio::test]
    async fn unknown_tool() {
        let pool = create_test_pool().await.unwrap();
        let index = build_test_index();
        let mut input = valid_input();
        input.tool_allowlist = Some(r#"["nonexistent_tool"]"#);
        let result = validate_profile(&input, &index, &pool).await;
        assert!(result.violations.iter().any(|v| v.code == "UNKNOWN_TOOL"));
    }

    #[tokio::test]
    async fn tool_not_in_role_vertical() {
        let pool = create_test_pool().await.unwrap();
        // Build index with two verticals to have cross-vertical tools
        let mut m2 = test_manifest();
        m2.id = "finance".into();
        m2.name = "Finance".into();
        m2.tools = vec![ToolSummary { name: "trade_execute".into(), description: "".into() }];
        m2.profiles = vec![ProfileSummary { role_id: "analyst".into(), autonomy: "gated".into() }];
        let index = taxonomy_builder::build_index(None, &[test_manifest(), m2], &[]).index;

        let mut input = valid_input();
        input.tool_allowlist = Some(r#"["trade_execute"]"#);
        let result = validate_profile(&input, &index, &pool).await;
        assert!(result.violations.iter().any(|v| v.code == "TOOL_NOT_IN_ROLE_VERTICAL"));
    }

    #[tokio::test]
    async fn empty_tool_set() {
        let pool = create_test_pool().await.unwrap();
        let index = build_test_index();
        let mut input = valid_input();
        // Deny all tools
        input.tool_denylist = Some(r#"["code_edit","code_execute","test_run"]"#);
        let result = validate_profile(&input, &index, &pool).await;
        assert!(result.violations.iter().any(|v| v.code == "EMPTY_TOOL_SET"));
    }

    #[tokio::test]
    async fn invalid_temperature() {
        let pool = create_test_pool().await.unwrap();
        let index = build_test_index();
        let mut input = valid_input();
        input.llm_temperature = Some(3.0);
        let result = validate_profile(&input, &index, &pool).await;
        assert!(result.violations.iter().any(|v| v.code == "INVALID_TEMPERATURE"));
    }

    #[tokio::test]
    async fn invalid_autonomy() {
        let pool = create_test_pool().await.unwrap();
        let index = build_test_index();
        let mut input = valid_input();
        input.autonomy = "manual";
        let result = validate_profile(&input, &index, &pool).await;
        assert!(result.violations.iter().any(|v| v.code == "INVALID_AUTONOMY"));
    }

    #[tokio::test]
    async fn tool_has_runtime_policy_warning() {
        let pool = create_test_pool().await.unwrap();
        let index = build_test_index();
        let mut input = valid_input();
        input.tool_allowlist = Some(r#"["code_execute"]"#);
        let result = validate_profile(&input, &index, &pool).await;
        assert!(result.warnings.iter().any(|w| w.code == "TOOL_HAS_RUNTIME_POLICY"));
    }

    #[tokio::test]
    async fn autonomous_worker_high_impact_warning() {
        let pool = create_test_pool().await.unwrap();
        let index = build_test_index();
        let mut input = valid_input();
        input.autonomy = "autonomous";
        input.tool_allowlist = Some(r#"["code_execute"]"#);
        let result = validate_profile(&input, &index, &pool).await;
        assert!(result.warnings.iter().any(|w| w.code == "AUTONOMOUS_WORKER_HIGH_IMPACT"));
    }

    #[tokio::test]
    async fn duplicate_name_detected() {
        let pool = create_test_pool().await.unwrap();
        // Insert a user first
        console_db::queries::users::insert_user(
            &pool, "user1", "testuser", "Test User", "$hash$", "operator", false,
            &chrono::Utc::now().to_rfc3339(),
        ).await.unwrap();

        // Insert a profile with the same name
        let row = console_db::queries::profiles::ProfileRow {
            id: "p1".into(),
            version: 1,
            name: "Test Profile".into(),
            description: None,
            tags: None,
            role_ref: "swe:implementer".into(),
            llm_provider: "anthropic".into(),
            llm_model: "claude-sonnet-4-20250514".into(),
            llm_temperature: None,
            llm_max_tokens: None,
            autonomy: "assisted".into(),
            tool_allowlist: None,
            tool_denylist: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
            budget_warning_threshold: None,
            owner_user_id: "user1".into(),
            visibility: "private".into(),
            is_current: true,
            created_at: chrono::Utc::now().to_rfc3339(),
            deleted_at: None,
        };
        console_db::queries::profiles::insert_profile(&pool, &row).await.unwrap();

        let index = build_test_index();
        let input = valid_input();
        let result = validate_profile(&input, &index, &pool).await;
        assert!(result.violations.iter().any(|v| v.code == "DUPLICATE_NAME"));
    }
}
