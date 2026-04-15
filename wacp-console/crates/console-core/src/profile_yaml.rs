//! Profile YAML export and import.
//!
//! Spec: `wcon-data-model` §8, `wcon-profiles` §7.3, ADR-007

use console_db::queries::profiles::ProfileRow;
use serde::{Deserialize, Serialize};

use crate::error::ConsoleError;

/// YAML export format — `format_version: 1`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileYaml {
    pub format_version: u32,
    pub profile: ProfileData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileData {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    pub role: String,
    pub llm: LlmConfig,
    #[serde(default = "default_autonomy")]
    pub autonomy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetConfig>,
}

fn default_autonomy() -> String {
    "assisted".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BudgetConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cost_micros: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_wall_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning_threshold: Option<f64>,
}

/// Export a profile row to YAML string.
pub fn export_yaml(row: &ProfileRow) -> Result<String, ConsoleError> {
    let tags: Option<Vec<String>> = row
        .tags
        .as_ref()
        .and_then(|t| serde_json::from_str(t).ok())
        .filter(|v: &Vec<String>| !v.is_empty());

    let allow: Option<Vec<String>> = row
        .tool_allowlist
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok());
    let deny: Option<Vec<String>> = row
        .tool_denylist
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok());

    let tools = if allow.is_some() || deny.is_some() {
        Some(ToolConfig { allow, deny })
    } else {
        None
    };

    let has_budget = row.budget_max_cost_micros.is_some()
        || row.budget_max_tokens.is_some()
        || row.budget_max_wall_time_ms.is_some()
        || row.budget_warning_threshold.is_some();

    let budget = if has_budget {
        Some(BudgetConfig {
            max_cost_micros: row.budget_max_cost_micros,
            max_tokens: row.budget_max_tokens,
            max_wall_time_ms: row.budget_max_wall_time_ms,
            warning_threshold: row.budget_warning_threshold,
        })
    } else {
        None
    };

    let yaml = ProfileYaml {
        format_version: 1,
        profile: ProfileData {
            name: row.name.clone(),
            description: row.description.clone(),
            tags,
            role: row.role_ref.clone(),
            llm: LlmConfig {
                provider: row.llm_provider.clone(),
                model: row.llm_model.clone(),
                temperature: row.llm_temperature,
                max_tokens: row.llm_max_tokens,
            },
            autonomy: row.autonomy.clone(),
            tools,
            budget,
        },
    };

    serde_yaml_ng::to_string(&yaml)
        .map_err(|e| ConsoleError::Internal(format!("YAML serialization failed: {e}")))
}

/// Parse a YAML string into profile fields for import.
/// Returns the parsed data; caller is responsible for generating UUID,
/// setting owner, and running validation.
pub fn parse_import_yaml(yaml: &str) -> Result<ProfileData, ConsoleError> {
    let parsed: ProfileYaml = serde_yaml_ng::from_str(yaml).map_err(|e| {
        ConsoleError::validation("INVALID_YAML", format!("Failed to parse YAML: {e}"))
    })?;

    if parsed.format_version != 1 {
        return Err(ConsoleError::validation(
            "UNSUPPORTED_FORMAT_VERSION",
            format!("Expected format_version 1, got {}", parsed.format_version),
        ));
    }

    Ok(parsed.profile)
}

/// Convert parsed import data into a ProfileRow (without id/version/owner — caller sets those).
pub fn import_data_to_row(data: &ProfileData) -> ProfileRow {
    ProfileRow {
        id: String::new(),
        version: 1,
        name: data.name.clone(),
        description: data.description.clone(),
        tags: data
            .tags
            .as_ref()
            .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".into())),
        role_ref: data.role.clone(),
        llm_provider: data.llm.provider.clone(),
        llm_model: data.llm.model.clone(),
        llm_temperature: data.llm.temperature,
        llm_max_tokens: data.llm.max_tokens,
        autonomy: data.autonomy.clone(),
        tool_allowlist: data
            .tools
            .as_ref()
            .and_then(|t| t.allow.as_ref())
            .map(|a| serde_json::to_string(a).unwrap_or_else(|_| "[]".into())),
        tool_denylist: data
            .tools
            .as_ref()
            .and_then(|t| t.deny.as_ref())
            .map(|d| serde_json::to_string(d).unwrap_or_else(|_| "[]".into())),
        budget_max_cost_micros: data.budget.as_ref().and_then(|b| b.max_cost_micros),
        budget_max_tokens: data.budget.as_ref().and_then(|b| b.max_tokens),
        budget_max_wall_time_ms: data.budget.as_ref().and_then(|b| b.max_wall_time_ms),
        budget_warning_threshold: data.budget.as_ref().and_then(|b| b.warning_threshold),
        owner_user_id: String::new(),
        visibility: "private".into(),
        is_current: true,
        created_at: String::new(),
        deleted_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row() -> ProfileRow {
        ProfileRow {
            id: "p1".into(),
            version: 3,
            name: "Fast Implementer".into(),
            description: Some("High-autonomy".into()),
            tags: Some(r#"["swe","fast"]"#.into()),
            role_ref: "swe:implementer".into(),
            llm_provider: "anthropic".into(),
            llm_model: "claude-sonnet-4-20250514".into(),
            llm_temperature: Some(0.3),
            llm_max_tokens: Some(8192),
            autonomy: "autonomous".into(),
            tool_allowlist: Some(r#"["code_edit","test_run"]"#.into()),
            tool_denylist: None,
            budget_max_cost_micros: Some(500000),
            budget_max_tokens: Some(100000),
            budget_max_wall_time_ms: None,
            budget_warning_threshold: Some(0.8),
            owner_user_id: "user1".into(),
            visibility: "shared".into(),
            is_current: true,
            created_at: "2026-01-01T00:00:00Z".into(),
            deleted_at: None,
        }
    }

    #[test]
    fn export_excludes_internal_fields() {
        let yaml = export_yaml(&sample_row()).unwrap();
        // Internal fields must not appear (format_version is allowed)
        assert!(!yaml.contains("\nid:"));
        assert!(!yaml.contains("is_current"));
        assert!(!yaml.contains("owner_user_id"));
        assert!(!yaml.contains("visibility"));
        assert!(!yaml.contains("created_at"));
        assert!(yaml.contains("format_version: 1"));
        assert!(yaml.contains("Fast Implementer"));
    }

    #[test]
    fn export_omits_null_fields() {
        let mut row = sample_row();
        row.description = None;
        row.budget_max_wall_time_ms = None;
        let yaml = export_yaml(&row).unwrap();
        // description should not appear
        assert!(!yaml.contains("description:"));
    }

    #[test]
    fn round_trip_fidelity() {
        let row = sample_row();
        let yaml = export_yaml(&row).unwrap();
        let parsed = parse_import_yaml(&yaml).unwrap();
        let imported = import_data_to_row(&parsed);

        assert_eq!(imported.name, row.name);
        assert_eq!(imported.role_ref, row.role_ref);
        assert_eq!(imported.llm_provider, row.llm_provider);
        assert_eq!(imported.llm_model, row.llm_model);
        assert_eq!(imported.llm_temperature, row.llm_temperature);
        assert_eq!(imported.llm_max_tokens, row.llm_max_tokens);
        assert_eq!(imported.autonomy, row.autonomy);
        assert_eq!(imported.tool_allowlist, row.tool_allowlist);
        assert_eq!(imported.budget_max_cost_micros, row.budget_max_cost_micros);
        assert_eq!(
            imported.budget_warning_threshold,
            row.budget_warning_threshold
        );
    }

    #[test]
    fn import_rejects_wrong_format_version() {
        let yaml = "format_version: 99\nprofile:\n  name: test\n  role: x\n  llm:\n    provider: a\n    model: b\n  autonomy: assisted\n";
        let result = parse_import_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn import_rejects_invalid_yaml() {
        let result = parse_import_yaml("not: valid: yaml: {{{}}}");
        assert!(result.is_err());
    }
}
