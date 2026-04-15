//! Settings service — known-key registry with defaults and type validation.
//!
//! Spec: `wcon-data-model` §5.1–5.2, `wcon-api` §10

use console_db::DbPool;
use console_db::queries::settings as db;
use serde::{Deserialize, Serialize};

use crate::error::ConsoleError;

/// A setting value with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: serde_json::Value,
    pub source: SettingSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SettingSource {
    Default,
    Database,
}

/// Describes a known setting key with its expected type and default.
struct KnownKey {
    key: &'static str,
    value_type: ValueType,
    default: serde_json::Value,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum ValueType {
    String,
    Integer,
    Boolean,
}

/// Registry of all known settings keys.
fn known_keys() -> Vec<KnownKey> {
    vec![
        KnownKey {
            key: "runtime.agent_address",
            value_type: ValueType::String,
            default: serde_json::json!(""),
        },
        KnownKey {
            key: "runtime.highway_address",
            value_type: ValueType::String,
            default: serde_json::json!(""),
        },
        KnownKey {
            key: "runtime.coordinator_address",
            value_type: ValueType::String,
            default: serde_json::json!(""),
        },
        KnownKey {
            key: "runtime.rest_address",
            value_type: ValueType::String,
            default: serde_json::json!(""),
        },
        KnownKey {
            key: "taxonomy.path",
            value_type: ValueType::String,
            default: serde_json::json!(""),
        },
        KnownKey {
            key: "export.directory",
            value_type: ValueType::String,
            default: serde_json::json!(""),
        },
        KnownKey {
            key: "ui.theme",
            value_type: ValueType::String,
            default: serde_json::json!("light"),
        },
        KnownKey {
            key: "ui.trail_buffer_size",
            value_type: ValueType::Integer,
            default: serde_json::json!(500),
        },
        KnownKey {
            key: "auth.session_ttl_hours",
            value_type: ValueType::Integer,
            default: serde_json::json!(24),
        },
    ]
}

fn find_known_key(key: &str) -> Option<KnownKey> {
    known_keys().into_iter().find(|k| k.key == key)
}

fn default_for_key(key: &str) -> Option<serde_json::Value> {
    find_known_key(key).map(|k| k.default)
}

/// Get all settings: database values merged with defaults for known keys.
pub async fn get_all(pool: &DbPool) -> Result<Vec<Setting>, ConsoleError> {
    let rows = db::get_all_settings(pool)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))?;

    let mut result: Vec<Setting> = Vec::new();
    let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Database values first
    for row in rows {
        seen_keys.insert(row.key.clone());
        let value =
            serde_json::from_str(&row.value).unwrap_or(serde_json::Value::String(row.value));
        result.push(Setting {
            key: row.key,
            value,
            source: SettingSource::Database,
        });
    }

    // Fill in defaults for known keys not in database
    for kk in known_keys() {
        if !seen_keys.contains(kk.key) {
            result.push(Setting {
                key: kk.key.to_string(),
                value: kk.default.clone(),
                source: SettingSource::Default,
            });
        }
    }

    result.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(result)
}

/// Get a single setting by key.
pub async fn get(pool: &DbPool, key: &str) -> Result<Setting, ConsoleError> {
    let row = db::get_setting(pool, key)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))?;

    if let Some(row) = row {
        let value =
            serde_json::from_str(&row.value).unwrap_or(serde_json::Value::String(row.value));
        Ok(Setting {
            key: row.key,
            value,
            source: SettingSource::Database,
        })
    } else if let Some(default) = default_for_key(key) {
        Ok(Setting {
            key: key.to_string(),
            value: default,
            source: SettingSource::Default,
        })
    } else {
        Err(ConsoleError::NotFound {
            entity: "setting",
            id: key.to_string(),
        })
    }
}

/// Set a setting value. Validates type for known keys; accepts any JSON for unknown keys.
pub async fn set(pool: &DbPool, key: &str, value: &serde_json::Value) -> Result<(), ConsoleError> {
    if let Some(kk) = find_known_key(key) {
        validate_type(&kk, value)?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let value_str = serde_json::to_string(value)
        .map_err(|e| ConsoleError::Internal(format!("JSON serialization: {e}")))?;

    db::upsert_setting(pool, key, &value_str, &now)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))
}

/// Delete a setting (reset to default). Returns error if key doesn't exist in DB.
pub async fn delete(pool: &DbPool, key: &str) -> Result<(), ConsoleError> {
    let deleted = db::delete_setting(pool, key)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))?;

    if !deleted {
        return Err(ConsoleError::NotFound {
            entity: "setting",
            id: key.to_string(),
        });
    }
    Ok(())
}

fn validate_type(kk: &KnownKey, value: &serde_json::Value) -> Result<(), ConsoleError> {
    let valid = match kk.value_type {
        ValueType::String => value.is_string(),
        ValueType::Integer => value.is_i64() || value.is_u64(),
        ValueType::Boolean => value.is_boolean(),
    };
    if !valid {
        let expected = match kk.value_type {
            ValueType::String => "string",
            ValueType::Integer => "integer",
            ValueType::Boolean => "boolean",
        };
        return Err(ConsoleError::validation(
            "INVALID_SETTING_TYPE",
            format!("Setting '{}' expects {expected}, got {}", kk.key, value),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use console_db::create_test_pool;

    #[tokio::test]
    async fn get_all_returns_defaults() {
        let pool = create_test_pool().await.unwrap();
        let all = get_all(&pool).await.unwrap();

        // Should have all known keys with default source
        let theme = all.iter().find(|s| s.key == "ui.theme").unwrap();
        assert_eq!(theme.source, SettingSource::Default);
    }

    #[tokio::test]
    async fn set_and_get() {
        let pool = create_test_pool().await.unwrap();
        set(&pool, "ui.theme", &serde_json::json!("dark"))
            .await
            .unwrap();

        let s = get(&pool, "ui.theme").await.unwrap();
        assert_eq!(s.value, serde_json::json!("dark"));
        assert_eq!(s.source, SettingSource::Database);
    }

    #[tokio::test]
    async fn type_validation_rejects_wrong_type() {
        let pool = create_test_pool().await.unwrap();
        let err = set(
            &pool,
            "auth.session_ttl_hours",
            &serde_json::json!("not an int"),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ConsoleError::Validation { .. }));
    }

    #[tokio::test]
    async fn unknown_key_accepted() {
        let pool = create_test_pool().await.unwrap();
        set(&pool, "custom.key", &serde_json::json!({"any": "value"}))
            .await
            .unwrap();

        let s = get(&pool, "custom.key").await.unwrap();
        assert_eq!(s.value["any"], "value");
    }

    #[tokio::test]
    async fn delete_resets_to_default() {
        let pool = create_test_pool().await.unwrap();
        set(&pool, "ui.theme", &serde_json::json!("dark"))
            .await
            .unwrap();
        delete(&pool, "ui.theme").await.unwrap();

        let s = get(&pool, "ui.theme").await.unwrap();
        assert_eq!(s.source, SettingSource::Default);
    }
}
