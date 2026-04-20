//! `WACP_*` environment-variable override application for `RuntimeConfig`.
//!
//! Extracted from `config.rs` per `bucket-b-refactor-plan.md` §B.5e follow-up. The
//! top-level `apply_env_overrides` is re-exported from `config.rs` so existing
//! call-sites (`config::apply_env_overrides`) and the test module's `use super::*;`
//! keep resolving without modification. The helpers below are `pub(crate)` — only
//! reachable within the binary.

use crate::config::{ConfigError, RuntimeConfig};

/// Apply WACP_ environment variable overrides to the config.
pub fn apply_env_overrides(config: &mut RuntimeConfig) -> Result<(), ConfigError> {
    apply_overrides_from(config, std::env::vars())
}

pub(crate) fn apply_overrides_from(
    config: &mut RuntimeConfig,
    vars: impl Iterator<Item = (String, String)>,
) -> Result<(), ConfigError> {
    for (key, value) in vars {
        if !key.starts_with("WACP_") || key == "WACP_CONFIG" {
            continue;
        }
        let path = key
            .strip_prefix("WACP_")
            .unwrap()
            .to_lowercase()
            .replace("__", ".");
        apply_single_override(config, &key, &path, &value)?;
    }
    Ok(())
}

pub(crate) fn apply_single_override(
    config: &mut RuntimeConfig,
    var: &str,
    path: &str,
    value: &str,
) -> Result<(), ConfigError> {
    match path {
        // server
        "server.agent_listen" => config.server.agent_listen = value.into(),
        "server.highway_listen" => config.server.highway_listen = value.into(),
        "server.coordinator_listen" => config.server.coordinator_listen = value.into(),
        "server.rest_listen" => config.server.rest_listen = value.into(),
        // tls
        "tls.enabled" => config.tls.enabled = parse_bool_env(var, value)?,
        "tls.cert_file" => config.tls.cert_file = value.into(),
        "tls.key_file" => config.tls.key_file = value.into(),
        "tls.client_ca_file" => config.tls.client_ca_file = value.into(),
        "tls.min_version" => config.tls.min_version = value.into(),
        // auth
        "auth.provider" => config.auth.provider = value.into(),
        "auth.external.url" => config.auth.external.url = value.into(),
        "auth.external.timeout_ms" => {
            config.auth.external.timeout_ms = parse_u64_env(var, value)?;
        }
        "auth.rate_limit.max_failures" => {
            config.auth.rate_limit.max_failures = parse_u32_env(var, value)?;
        }
        "auth.rate_limit.window_seconds" => {
            config.auth.rate_limit.window_seconds = parse_u32_env(var, value)?;
        }
        // taxonomy
        "taxonomy.file" => config.taxonomy.file = value.into(),
        // storage
        "storage.data_dir" => config.storage.data_dir = value.into(),
        "storage.trail.segment_size_bytes" => {
            config.storage.trail.segment_size_bytes = parse_u64_env(var, value)?;
        }
        "storage.trail.index_batch_size" => {
            config.storage.trail.index_batch_size = parse_u32_env(var, value)?;
        }
        "storage.trail.index_batch_timeout_ms" => {
            config.storage.trail.index_batch_timeout_ms = parse_u32_env(var, value)?;
        }
        "storage.snapshots.workspace_checkpoint_interval" => {
            config.storage.snapshots.workspace_checkpoint_interval = parse_u32_env(var, value)?;
        }
        "storage.snapshots.workspace_time_interval_seconds" => {
            config.storage.snapshots.workspace_time_interval_seconds = parse_u32_env(var, value)?;
        }
        "storage.snapshots.system_entry_interval" => {
            config.storage.snapshots.system_entry_interval = parse_u64_env(var, value)?;
        }
        "storage.snapshots.system_time_interval_minutes" => {
            config.storage.snapshots.system_time_interval_minutes = parse_u32_env(var, value)?;
        }
        "storage.snapshots.system_retention_count" => {
            config.storage.snapshots.system_retention_count = parse_u32_env(var, value)?;
        }
        "storage.tiered.hot_segments" => {
            config.storage.tiered.hot_segments = parse_u32_env(var, value)?;
        }
        "storage.tiered.warm_retention_days" => {
            config.storage.tiered.warm_retention_days = parse_u32_env(var, value)?;
        }
        "storage.tiered.cold_retention" => config.storage.tiered.cold_retention = value.into(),
        "storage.tiered.cold_destination" => {
            config.storage.tiered.cold_destination = value.into();
        }
        "storage.tiered.compaction_interval_minutes" => {
            config.storage.tiered.compaction_interval_minutes = parse_u32_env(var, value)?;
        }
        // resources
        "resources.default_timeout_ms" => {
            config.resources.default_timeout_ms = parse_u64_env(var, value)?;
        }
        "resources.default_budget.max_tokens" => {
            config.resources.default_budget.max_tokens = parse_u64_env(var, value)?;
        }
        "resources.default_budget.max_wall_time_ms" => {
            config.resources.default_budget.max_wall_time_ms = parse_u64_env(var, value)?;
        }
        "resources.default_budget.max_storage_bytes" => {
            config.resources.default_budget.max_storage_bytes = parse_u64_env(var, value)?;
        }
        "resources.default_budget.max_network_bytes" => {
            config.resources.default_budget.max_network_bytes = parse_u64_env(var, value)?;
        }
        "resources.default_budget.max_cost_micros" => {
            config.resources.default_budget.max_cost_micros = parse_u64_env(var, value)?;
        }
        "resources.warning_threshold" => {
            config.resources.warning_threshold = parse_f32_env(var, value)?;
        }
        "resources.liveness_interval_ms" => {
            config.resources.liveness_interval_ms = parse_u64_env(var, value)?;
        }
        // delivery
        "delivery.max_retries" => {
            config.delivery.max_retries = parse_u32_env(var, value)?;
        }
        "delivery.retry_backoff_ms" => {
            config.delivery.retry_backoff_ms = parse_u64_env(var, value)?;
        }
        // logging
        "logging.level" => config.logging.level = value.into(),
        "logging.format" => config.logging.format = value.into(),
        "logging.output" => config.logging.output = value.into(),
        "logging.file" => config.logging.file = value.into(),
        // observability
        "observability.metrics.enabled" => {
            config.observability.metrics.enabled = parse_bool_env(var, value)?;
        }
        "observability.metrics.listen" => {
            config.observability.metrics.listen = value.into();
        }
        "observability.metrics.path" => config.observability.metrics.path = value.into(),
        "observability.health.enabled" => {
            config.observability.health.enabled = parse_bool_env(var, value)?;
        }
        "observability.health.listen" => {
            config.observability.health.listen = value.into();
        }
        "observability.health.path" => config.observability.health.path = value.into(),
        // Unknown WACP_ variable — ignore silently
        _ => {}
    }
    Ok(())
}

fn parse_bool_env(var: &str, value: &str) -> Result<bool, ConfigError> {
    match value {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(ConfigError::EnvOverride {
            var: var.into(),
            message: format!("expected bool (true/false/1/0), got {value:?}"),
        }),
    }
}

fn parse_u32_env(var: &str, value: &str) -> Result<u32, ConfigError> {
    value.parse().map_err(|_| ConfigError::EnvOverride {
        var: var.into(),
        message: format!("expected u32, got {value:?}"),
    })
}

fn parse_u64_env(var: &str, value: &str) -> Result<u64, ConfigError> {
    value.parse().map_err(|_| ConfigError::EnvOverride {
        var: var.into(),
        message: format!("expected u64, got {value:?}"),
    })
}

fn parse_f32_env(var: &str, value: &str) -> Result<f32, ConfigError> {
    value.parse().map_err(|_| ConfigError::EnvOverride {
        var: var.into(),
        message: format!("expected f32, got {value:?}"),
    })
}
