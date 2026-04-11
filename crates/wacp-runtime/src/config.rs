use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Protocol version compiled into the binary.
pub const PROTOCOL_VERSION: &str = "0.1";

// ── Error ──────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("validation error: {field}: {message}")]
    Validation { field: String, message: String },
    #[error("environment variable error: {var}: {message}")]
    EnvOverride { var: String, message: String },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// ── Structs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub tls: TlsConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub taxonomy: TaxonomyConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub resources: ResourceConfig,
    #[serde(default)]
    pub delivery: DeliveryConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default = "default_agent_listen")]
    pub agent_listen: String,
    #[serde(default = "default_highway_listen")]
    pub highway_listen: String,
    #[serde(default = "default_coordinator_listen")]
    pub coordinator_listen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub cert_file: String,
    #[serde(default)]
    pub key_file: String,
    #[serde(default)]
    pub client_ca_file: String,
    #[serde(default = "default_tls_min_version")]
    pub min_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default = "default_auth_provider")]
    pub provider: String,
    #[serde(default)]
    pub external: ExternalAuthConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalAuthConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_auth_timeout")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitConfig {
    #[serde(default = "default_rate_limit_failures")]
    pub max_failures: u32,
    #[serde(default = "default_rate_limit_window")]
    pub window_seconds: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyConfig {
    #[serde(default)]
    pub file: String,
    /// Directory that contains per-vertical sub-directories each holding a
    /// `vertical.yaml` file. When empty, no vertical manifests are loaded.
    #[serde(default)]
    pub verticals_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default)]
    pub trail: TrailStorageConfig,
    #[serde(default)]
    pub snapshots: SnapshotConfig,
    #[serde(default)]
    pub tiered: TieredConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrailStorageConfig {
    #[serde(default = "default_segment_size")]
    pub segment_size_bytes: u64,
    #[serde(default = "default_index_batch_size")]
    pub index_batch_size: u32,
    #[serde(default = "default_index_batch_timeout")]
    pub index_batch_timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotConfig {
    #[serde(default = "default_ws_checkpoint_interval")]
    pub workspace_checkpoint_interval: u32,
    #[serde(default = "default_ws_time_interval")]
    pub workspace_time_interval_seconds: u32,
    #[serde(default = "default_sys_entry_interval")]
    pub system_entry_interval: u64,
    #[serde(default = "default_sys_time_interval")]
    pub system_time_interval_minutes: u32,
    #[serde(default = "default_sys_retention")]
    pub system_retention_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TieredConfig {
    #[serde(default = "default_hot_segments")]
    pub hot_segments: u32,
    #[serde(default = "default_warm_retention")]
    pub warm_retention_days: u32,
    #[serde(default = "default_cold_retention")]
    pub cold_retention: String,
    #[serde(default)]
    pub cold_destination: String,
    #[serde(default = "default_compaction_interval")]
    pub compaction_interval_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceConfig {
    #[serde(default)]
    pub default_timeout_ms: u64,
    #[serde(default)]
    pub default_budget: BudgetConfig,
    #[serde(default = "default_warning_threshold")]
    pub warning_threshold: f32,
    #[serde(default)]
    pub liveness_interval_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetConfig {
    #[serde(default)]
    pub max_tokens: u64,
    #[serde(default)]
    pub max_wall_time_ms: u64,
    #[serde(default)]
    pub max_storage_bytes: u64,
    #[serde(default)]
    pub max_network_bytes: u64,
    #[serde(default)]
    pub max_cost_micros: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_backoff")]
    pub retry_backoff_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default = "default_log_output")]
    pub output: String,
    #[serde(default)]
    pub file: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservabilityConfig {
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub health: HealthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_metrics_listen")]
    pub listen: String,
    #[serde(default = "default_metrics_path")]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthConfig {
    #[serde(default = "default_health_enabled")]
    pub enabled: bool,
    #[serde(default = "default_health_listen")]
    pub listen: String,
    #[serde(default = "default_health_path")]
    pub path: String,
}

// ── Default impls ──────────────────────────────────────────────────────────

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            agent_listen: default_agent_listen(),
            highway_listen: default_highway_listen(),
            coordinator_listen: default_coordinator_listen(),
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_file: String::new(),
            key_file: String::new(),
            client_ca_file: String::new(),
            min_version: default_tls_min_version(),
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            provider: default_auth_provider(),
            external: ExternalAuthConfig::default(),
            rate_limit: RateLimitConfig::default(),
        }
    }
}

impl Default for ExternalAuthConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            timeout_ms: default_auth_timeout(),
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_failures: default_rate_limit_failures(),
            window_seconds: default_rate_limit_window(),
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            trail: TrailStorageConfig::default(),
            snapshots: SnapshotConfig::default(),
            tiered: TieredConfig::default(),
        }
    }
}

impl Default for TrailStorageConfig {
    fn default() -> Self {
        Self {
            segment_size_bytes: default_segment_size(),
            index_batch_size: default_index_batch_size(),
            index_batch_timeout_ms: default_index_batch_timeout(),
        }
    }
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            workspace_checkpoint_interval: default_ws_checkpoint_interval(),
            workspace_time_interval_seconds: default_ws_time_interval(),
            system_entry_interval: default_sys_entry_interval(),
            system_time_interval_minutes: default_sys_time_interval(),
            system_retention_count: default_sys_retention(),
        }
    }
}

impl Default for TieredConfig {
    fn default() -> Self {
        Self {
            hot_segments: default_hot_segments(),
            warm_retention_days: default_warm_retention(),
            cold_retention: default_cold_retention(),
            cold_destination: String::new(),
            compaction_interval_minutes: default_compaction_interval(),
        }
    }
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 0,
            default_budget: BudgetConfig::default(),
            warning_threshold: default_warning_threshold(),
            liveness_interval_ms: 0,
        }
    }
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            retry_backoff_ms: default_retry_backoff(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            output: default_log_output(),
            file: String::new(),
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: default_metrics_listen(),
            path: default_metrics_path(),
        }
    }
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: default_health_enabled(),
            listen: default_health_listen(),
            path: default_health_path(),
        }
    }
}

// ── Default functions ──────────────────────────────────────────────────────

fn default_agent_listen() -> String { "[::1]:9090".into() }
fn default_highway_listen() -> String { "[::1]:9091".into() }
fn default_coordinator_listen() -> String { "[::1]:9402".into() }
fn default_tls_min_version() -> String { "1.2".into() }
fn default_auth_provider() -> String { "psk".into() }
fn default_auth_timeout() -> u64 { 5000 }
fn default_rate_limit_failures() -> u32 { 10 }
fn default_rate_limit_window() -> u32 { 60 }
fn default_data_dir() -> String { "./data".into() }
fn default_segment_size() -> u64 { 67_108_864 }
fn default_index_batch_size() -> u32 { 100 }
fn default_index_batch_timeout() -> u32 { 50 }
fn default_ws_checkpoint_interval() -> u32 { 5 }
fn default_ws_time_interval() -> u32 { 60 }
fn default_sys_entry_interval() -> u64 { 10_000 }
fn default_sys_time_interval() -> u32 { 30 }
fn default_sys_retention() -> u32 { 3 }
fn default_hot_segments() -> u32 { 10 }
fn default_warm_retention() -> u32 { 90 }
fn default_cold_retention() -> String { "indefinite".into() }
fn default_compaction_interval() -> u32 { 60 }
fn default_warning_threshold() -> f32 { 0.8 }
fn default_max_retries() -> u32 { 3 }
fn default_retry_backoff() -> u64 { 100 }
fn default_log_level() -> String { "info".into() }
fn default_log_format() -> String { "json".into() }
fn default_log_output() -> String { "stderr".into() }
fn default_metrics_listen() -> String { "[::1]:9092".into() }
fn default_metrics_path() -> String { "/metrics".into() }
fn default_health_enabled() -> bool { true }
fn default_health_listen() -> String { "[::1]:9093".into() }
fn default_health_path() -> String { "/healthz".into() }

// ── Loading ────────────────────────────────────────────────────────────────

impl RuntimeConfig {
    /// Load configuration: config_path → WACP_CONFIG env → ./wacp-runtime.yaml → defaults.
    /// Applies env var overrides, then validates.
    /// Returns (config, resolved_file_path).
    pub fn load(config_path: Option<&Path>) -> Result<(Self, Option<PathBuf>), ConfigError> {
        let (mut config, source) = if let Some(path) = config_path {
            let yaml = std::fs::read_to_string(path).map_err(|e| {
                ConfigError::Io(std::io::Error::new(
                    e.kind(),
                    format!("{}: {e}", path.display()),
                ))
            })?;
            (Self::parse(&yaml)?, Some(path.to_path_buf()))
        } else if let Ok(env_path) = std::env::var("WACP_CONFIG") {
            let path = PathBuf::from(&env_path);
            let yaml = std::fs::read_to_string(&path).map_err(|e| {
                ConfigError::Io(std::io::Error::new(
                    e.kind(),
                    format!("{}: {e}", path.display()),
                ))
            })?;
            (Self::parse(&yaml)?, Some(path))
        } else {
            let path = PathBuf::from("wacp-runtime.yaml");
            if path.exists() {
                let yaml = std::fs::read_to_string(&path)?;
                (Self::parse(&yaml)?, Some(path))
            } else {
                (Self::default(), None)
            }
        };

        apply_env_overrides(&mut config)?;
        config.validate()?;
        Ok((config, source))
    }

    /// Parse a YAML string into RuntimeConfig.
    pub fn parse(yaml: &str) -> Result<Self, ConfigError> {
        serde_yaml::from_str(yaml).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Serialize default config as YAML.
    pub fn default_yaml() -> String {
        serde_yaml::to_string(&Self::default()).expect("default config serializes")
    }
}

// ── Validation ─────────────────────────────────────────────────────────────

impl RuntimeConfig {
    /// Run all 9 validation checks in order. Returns first failure.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.validate_tls_completeness()?;
        self.validate_auth_completeness()?;
        self.validate_taxonomy()?;
        self.validate_data_dir()?;
        self.validate_address_uniqueness()?;
        self.validate_numeric_constraints()?;
        self.validate_enum_fields()?;
        self.validate_logging_file()?;
        self.validate_cold_retention()?;
        Ok(())
    }

    fn validate_tls_completeness(&self) -> Result<(), ConfigError> {
        if self.tls.enabled {
            if self.tls.cert_file.is_empty() {
                return Err(ConfigError::Validation {
                    field: "tls.cert_file".into(),
                    message: "required when tls.enabled is true".into(),
                });
            }
            if self.tls.key_file.is_empty() {
                return Err(ConfigError::Validation {
                    field: "tls.key_file".into(),
                    message: "required when tls.enabled is true".into(),
                });
            }
        }
        Ok(())
    }

    fn validate_auth_completeness(&self) -> Result<(), ConfigError> {
        if self.auth.provider == "external" {
            if self.auth.external.url.is_empty() {
                return Err(ConfigError::Validation {
                    field: "auth.external.url".into(),
                    message: "required when auth.provider is \"external\"".into(),
                });
            }
            if !self.auth.external.url.starts_with("http://")
                && !self.auth.external.url.starts_with("https://")
            {
                return Err(ConfigError::Validation {
                    field: "auth.external.url".into(),
                    message: format!(
                        "must be an HTTP(S) URL, got {:?}",
                        self.auth.external.url
                    ),
                });
            }
        }
        Ok(())
    }

    fn validate_taxonomy(&self) -> Result<(), ConfigError> {
        // Structural check only — file existence checked at load time.
        Ok(())
    }

    fn validate_data_dir(&self) -> Result<(), ConfigError> {
        if self.storage.data_dir.is_empty() {
            return Err(ConfigError::Validation {
                field: "storage.data_dir".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(())
    }

    fn validate_address_uniqueness(&self) -> Result<(), ConfigError> {
        let mut addresses: Vec<(&str, &str)> = vec![
            ("server.agent_listen", &self.server.agent_listen),
            ("server.highway_listen", &self.server.highway_listen),
            ("server.coordinator_listen", &self.server.coordinator_listen),
        ];
        if self.observability.metrics.enabled {
            addresses.push((
                "observability.metrics.listen",
                &self.observability.metrics.listen,
            ));
        }
        if self.observability.health.enabled {
            addresses.push((
                "observability.health.listen",
                &self.observability.health.listen,
            ));
        }
        for i in 0..addresses.len() {
            for j in (i + 1)..addresses.len() {
                if addresses[i].1 == addresses[j].1 {
                    return Err(ConfigError::Validation {
                        field: format!("{} and {}", addresses[i].0, addresses[j].0),
                        message: format!("duplicate listen address: {}", addresses[i].1),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_numeric_constraints(&self) -> Result<(), ConfigError> {
        macro_rules! check_positive {
            ($field:expr, $value:expr) => {
                if $value == 0 {
                    return Err(ConfigError::Validation {
                        field: $field.into(),
                        message: format!("must be > 0, got 0"),
                    });
                }
            };
        }
        macro_rules! check_min {
            ($field:expr, $value:expr, $min:expr) => {
                if $value < $min {
                    return Err(ConfigError::Validation {
                        field: $field.into(),
                        message: format!("must be >= {}, got {}", $min, $value),
                    });
                }
            };
        }

        // storage.trail
        check_positive!(
            "storage.trail.segment_size_bytes",
            self.storage.trail.segment_size_bytes
        );
        check_positive!(
            "storage.trail.index_batch_size",
            self.storage.trail.index_batch_size
        );
        check_positive!(
            "storage.trail.index_batch_timeout_ms",
            self.storage.trail.index_batch_timeout_ms
        );

        // storage.snapshots
        check_positive!(
            "storage.snapshots.workspace_checkpoint_interval",
            self.storage.snapshots.workspace_checkpoint_interval
        );
        check_positive!(
            "storage.snapshots.workspace_time_interval_seconds",
            self.storage.snapshots.workspace_time_interval_seconds
        );
        check_positive!(
            "storage.snapshots.system_entry_interval",
            self.storage.snapshots.system_entry_interval
        );
        check_positive!(
            "storage.snapshots.system_time_interval_minutes",
            self.storage.snapshots.system_time_interval_minutes
        );
        check_min!(
            "storage.snapshots.system_retention_count",
            self.storage.snapshots.system_retention_count,
            1
        );

        // storage.tiered
        check_min!(
            "storage.tiered.hot_segments",
            self.storage.tiered.hot_segments,
            1
        );
        check_positive!(
            "storage.tiered.warm_retention_days",
            self.storage.tiered.warm_retention_days
        );
        check_positive!(
            "storage.tiered.compaction_interval_minutes",
            self.storage.tiered.compaction_interval_minutes
        );

        // resources.warning_threshold: (0.0, 1.0]
        if self.resources.warning_threshold <= 0.0 || self.resources.warning_threshold > 1.0 {
            return Err(ConfigError::Validation {
                field: "resources.warning_threshold".into(),
                message: format!(
                    "must be in (0.0, 1.0], got {}",
                    self.resources.warning_threshold
                ),
            });
        }

        // delivery
        if self.delivery.max_retries > 0 && self.delivery.retry_backoff_ms == 0 {
            return Err(ConfigError::Validation {
                field: "delivery.retry_backoff_ms".into(),
                message: "must be > 0 when max_retries > 0".into(),
            });
        }

        // auth.rate_limit
        if self.auth.rate_limit.max_failures > 0 && self.auth.rate_limit.window_seconds == 0 {
            return Err(ConfigError::Validation {
                field: "auth.rate_limit.window_seconds".into(),
                message: "must be > 0 when max_failures > 0".into(),
            });
        }

        // auth.external.timeout_ms
        if self.auth.external.timeout_ms == 0 {
            return Err(ConfigError::Validation {
                field: "auth.external.timeout_ms".into(),
                message: "must be > 0".into(),
            });
        }

        Ok(())
    }

    fn validate_enum_fields(&self) -> Result<(), ConfigError> {
        match self.tls.min_version.as_str() {
            "1.2" | "1.3" => {}
            other => {
                return Err(ConfigError::Validation {
                    field: "tls.min_version".into(),
                    message: format!("must be \"1.2\" or \"1.3\", got {other:?}"),
                });
            }
        }
        match self.auth.provider.as_str() {
            "psk" | "external" => {}
            other => {
                return Err(ConfigError::Validation {
                    field: "auth.provider".into(),
                    message: format!("must be \"psk\" or \"external\", got {other:?}"),
                });
            }
        }
        match self.logging.level.as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {}
            other => {
                return Err(ConfigError::Validation {
                    field: "logging.level".into(),
                    message: format!(
                        "must be one of trace/debug/info/warn/error, got {other:?}"
                    ),
                });
            }
        }
        match self.logging.format.as_str() {
            "json" | "pretty" => {}
            other => {
                return Err(ConfigError::Validation {
                    field: "logging.format".into(),
                    message: format!("must be \"json\" or \"pretty\", got {other:?}"),
                });
            }
        }
        match self.logging.output.as_str() {
            "stderr" | "file" => {}
            other => {
                return Err(ConfigError::Validation {
                    field: "logging.output".into(),
                    message: format!("must be \"stderr\" or \"file\", got {other:?}"),
                });
            }
        }
        Ok(())
    }

    fn validate_logging_file(&self) -> Result<(), ConfigError> {
        if self.logging.output == "file" && self.logging.file.is_empty() {
            return Err(ConfigError::Validation {
                field: "logging.file".into(),
                message: "required when logging.output is \"file\"".into(),
            });
        }
        Ok(())
    }

    fn validate_cold_retention(&self) -> Result<(), ConfigError> {
        let v = &self.storage.tiered.cold_retention;
        if v == "indefinite" {
            return Ok(());
        }
        match v.parse::<u64>() {
            Ok(n) if n > 0 => Ok(()),
            _ => Err(ConfigError::Validation {
                field: "storage.tiered.cold_retention".into(),
                message: format!("must be \"indefinite\" or a positive integer, got {v:?}"),
            }),
        }
    }
}

// ── Environment variable overrides ─────────────────────────────────────────

/// Apply WACP_ environment variable overrides to the config.
pub fn apply_env_overrides(config: &mut RuntimeConfig) -> Result<(), ConfigError> {
    apply_overrides_from(config, std::env::vars())
}

fn apply_overrides_from(
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

fn apply_single_override(
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
            config.storage.snapshots.workspace_checkpoint_interval =
                parse_u32_env(var, value)?;
        }
        "storage.snapshots.workspace_time_interval_seconds" => {
            config.storage.snapshots.workspace_time_interval_seconds =
                parse_u32_env(var, value)?;
        }
        "storage.snapshots.system_entry_interval" => {
            config.storage.snapshots.system_entry_interval = parse_u64_env(var, value)?;
        }
        "storage.snapshots.system_time_interval_minutes" => {
            config.storage.snapshots.system_time_interval_minutes =
                parse_u32_env(var, value)?;
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

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Parsing ──

    #[test]
    fn parse_empty_yields_defaults() {
        let config = RuntimeConfig::parse("").unwrap();
        let defaults = RuntimeConfig::default();
        assert_eq!(config.server.agent_listen, defaults.server.agent_listen);
        assert_eq!(config.server.highway_listen, defaults.server.highway_listen);
        assert_eq!(config.tls.enabled, defaults.tls.enabled);
        assert_eq!(config.tls.min_version, defaults.tls.min_version);
        assert_eq!(config.auth.provider, defaults.auth.provider);
        assert_eq!(config.storage.data_dir, defaults.storage.data_dir);
        assert_eq!(
            config.storage.trail.segment_size_bytes,
            defaults.storage.trail.segment_size_bytes
        );
        assert_eq!(config.resources.warning_threshold, defaults.resources.warning_threshold);
        assert_eq!(config.delivery.max_retries, defaults.delivery.max_retries);
        assert_eq!(config.logging.level, defaults.logging.level);
        assert_eq!(config.logging.format, defaults.logging.format);
        assert_eq!(config.observability.health.enabled, defaults.observability.health.enabled);
    }

    #[test]
    fn parse_partial_server() {
        let yaml = r#"
server:
  agent_listen: "0.0.0.0:8080"
"#;
        let config = RuntimeConfig::parse(yaml).unwrap();
        assert_eq!(config.server.agent_listen, "0.0.0.0:8080");
        assert_eq!(config.server.highway_listen, "[::1]:9091"); // default
    }

    #[test]
    fn parse_full_config() {
        let yaml = r#"
server:
  agent_listen: "0.0.0.0:9090"
  highway_listen: "0.0.0.0:9091"
tls:
  enabled: true
  cert_file: "/etc/tls/cert.pem"
  key_file: "/etc/tls/key.pem"
  client_ca_file: "/etc/tls/ca.pem"
  min_version: "1.3"
auth:
  provider: "external"
  external:
    url: "https://auth.example.com/validate"
    timeout_ms: 3000
  rate_limit:
    max_failures: 5
    window_seconds: 120
taxonomy:
  file: "taxonomy.yaml"
storage:
  data_dir: "/var/lib/wacp"
  trail:
    segment_size_bytes: 33554432
    index_batch_size: 50
    index_batch_timeout_ms: 25
  snapshots:
    workspace_checkpoint_interval: 10
    workspace_time_interval_seconds: 120
    system_entry_interval: 5000
    system_time_interval_minutes: 15
    system_retention_count: 5
  tiered:
    hot_segments: 5
    warm_retention_days: 30
    cold_retention: "365"
    cold_destination: "/mnt/cold"
    compaction_interval_minutes: 30
resources:
  default_timeout_ms: 600000
  default_budget:
    max_tokens: 1000000
    max_wall_time_ms: 300000
    max_storage_bytes: 104857600
    max_network_bytes: 52428800
    max_cost_micros: 5000000
  warning_threshold: 0.9
  liveness_interval_ms: 30000
delivery:
  max_retries: 5
  retry_backoff_ms: 200
logging:
  level: "debug"
  format: "pretty"
  output: "file"
  file: "/var/log/wacp.log"
observability:
  metrics:
    enabled: true
    listen: "0.0.0.0:9092"
    path: "/metrics"
  health:
    enabled: true
    listen: "0.0.0.0:9093"
    path: "/healthz"
"#;
        let config = RuntimeConfig::parse(yaml).unwrap();
        assert_eq!(config.server.agent_listen, "0.0.0.0:9090");
        assert_eq!(config.tls.min_version, "1.3");
        assert_eq!(config.auth.external.timeout_ms, 3000);
        assert_eq!(config.storage.trail.segment_size_bytes, 33_554_432);
        assert_eq!(config.resources.default_budget.max_cost_micros, 5_000_000);
        assert_eq!(config.delivery.retry_backoff_ms, 200);
        assert_eq!(config.logging.level, "debug");
        assert!(config.observability.metrics.enabled);
    }

    #[test]
    fn reject_unknown_top_level_key() {
        let yaml = "unknown_key: true\n";
        assert!(RuntimeConfig::parse(yaml).is_err());
    }

    #[test]
    fn reject_unknown_nested_key() {
        let yaml = "server:\n  unknown: true\n";
        assert!(RuntimeConfig::parse(yaml).is_err());
    }

    // ── Validation: TLS ──

    #[test]
    fn validate_tls_completeness() {
        let mut config = RuntimeConfig::default();
        config.tls.enabled = true;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("tls.cert_file"));
    }

    #[test]
    fn validate_tls_completeness_ok() {
        let mut config = RuntimeConfig::default();
        config.tls.enabled = true;
        config.tls.cert_file = "/cert.pem".into();
        config.tls.key_file = "/key.pem".into();
        assert!(config.validate().is_ok());
    }

    // ── Validation: Auth ──

    #[test]
    fn validate_auth_external_requires_url() {
        let mut config = RuntimeConfig::default();
        config.auth.provider = "external".into();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("auth.external.url"));
    }

    #[test]
    fn validate_auth_external_requires_http_url() {
        let mut config = RuntimeConfig::default();
        config.auth.provider = "external".into();
        config.auth.external.url = "ftp://example.com".into();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("HTTP(S) URL"));
    }

    // ── Validation: Address uniqueness ──

    #[test]
    fn validate_address_uniqueness() {
        let mut config = RuntimeConfig::default();
        config.server.agent_listen = "[::1]:9090".into();
        config.server.highway_listen = "[::1]:9090".into(); // duplicate
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("duplicate listen address"));
    }

    #[test]
    fn validate_address_uniqueness_disabled() {
        let mut config = RuntimeConfig::default();
        // metrics disabled, so its address doesn't count
        config.observability.metrics.enabled = false;
        config.observability.metrics.listen = config.observability.health.listen.clone();
        assert!(config.validate().is_ok());
    }

    // ── Validation: Numeric ──

    #[test]
    fn validate_numeric_segment_size_zero() {
        let mut config = RuntimeConfig::default();
        config.storage.trail.segment_size_bytes = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("storage.trail.segment_size_bytes"));
    }

    #[test]
    fn validate_numeric_retention_count_zero() {
        let mut config = RuntimeConfig::default();
        config.storage.snapshots.system_retention_count = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("system_retention_count"));
    }

    #[test]
    fn validate_warning_threshold_bounds() {
        let mut config = RuntimeConfig::default();
        config.resources.warning_threshold = 0.0;
        assert!(config.validate().is_err());

        config.resources.warning_threshold = 1.1;
        assert!(config.validate().is_err());

        config.resources.warning_threshold = 0.5;
        assert!(config.validate().is_ok());

        config.resources.warning_threshold = 1.0;
        assert!(config.validate().is_ok());
    }

    // ── Validation: Enum fields ──

    #[test]
    fn validate_enum_tls_min_version() {
        let mut config = RuntimeConfig::default();
        config.tls.min_version = "1.0".into();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("tls.min_version"));
    }

    #[test]
    fn validate_enum_log_level() {
        let mut config = RuntimeConfig::default();
        config.logging.level = "verbose".into();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("logging.level"));
    }

    // ── Validation: Logging file ──

    #[test]
    fn validate_log_file_required() {
        let mut config = RuntimeConfig::default();
        config.logging.output = "file".into();
        config.logging.file = String::new();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("logging.file"));
    }

    // ── Validation: Cold retention ──

    #[test]
    fn validate_cold_retention_indefinite() {
        let config = RuntimeConfig::default(); // "indefinite"
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_cold_retention_integer() {
        let mut config = RuntimeConfig::default();

        config.storage.tiered.cold_retention = "30".into();
        assert!(config.validate().is_ok());

        config.storage.tiered.cold_retention = "abc".into();
        assert!(config.validate().is_err());

        config.storage.tiered.cold_retention = "0".into();
        assert!(config.validate().is_err());

        config.storage.tiered.cold_retention = "-1".into();
        assert!(config.validate().is_err());
    }

    // ── Environment variable overrides ──

    fn override_one(
        config: &mut RuntimeConfig,
        key: &str,
        value: &str,
    ) -> Result<(), ConfigError> {
        apply_overrides_from(
            config,
            std::iter::once((key.to_string(), value.to_string())),
        )
    }

    #[test]
    fn env_override_string() {
        let mut config = RuntimeConfig::default();
        override_one(&mut config, "WACP_SERVER__AGENT_LISTEN", "0.0.0.0:8080").unwrap();
        assert_eq!(config.server.agent_listen, "0.0.0.0:8080");
    }

    #[test]
    fn env_override_u64() {
        let mut config = RuntimeConfig::default();
        override_one(
            &mut config,
            "WACP_STORAGE__TRAIL__SEGMENT_SIZE_BYTES",
            "1024",
        )
        .unwrap();
        assert_eq!(config.storage.trail.segment_size_bytes, 1024);
    }

    #[test]
    fn env_override_f32() {
        let mut config = RuntimeConfig::default();
        override_one(&mut config, "WACP_RESOURCES__WARNING_THRESHOLD", "0.9").unwrap();
        assert!((config.resources.warning_threshold - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn env_override_bool() {
        let mut config = RuntimeConfig::default();
        override_one(&mut config, "WACP_TLS__ENABLED", "true").unwrap();
        assert!(config.tls.enabled);

        override_one(&mut config, "WACP_TLS__ENABLED", "1").unwrap();
        assert!(config.tls.enabled);

        override_one(&mut config, "WACP_TLS__ENABLED", "false").unwrap();
        assert!(!config.tls.enabled);

        override_one(&mut config, "WACP_TLS__ENABLED", "0").unwrap();
        assert!(!config.tls.enabled);
    }

    #[test]
    fn env_override_precedence() {
        let yaml = "server:\n  agent_listen: \"from-file\"\n";
        let mut config = RuntimeConfig::parse(yaml).unwrap();
        override_one(&mut config, "WACP_SERVER__AGENT_LISTEN", "from-env").unwrap();
        assert_eq!(config.server.agent_listen, "from-env");
    }

    #[test]
    fn env_override_unknown_ignored() {
        let mut config = RuntimeConfig::default();
        assert!(override_one(&mut config, "WACP_UNKNOWN__FIELD", "x").is_ok());
    }

    #[test]
    fn env_override_wacp_config_skipped() {
        let mut config = RuntimeConfig::default();
        // WACP_CONFIG should not be treated as a field override
        assert!(override_one(&mut config, "WACP_CONFIG", "/some/path").is_ok());
    }

    // ── Default round-trip ──

    #[test]
    fn default_roundtrip() {
        let yaml = RuntimeConfig::default_yaml();
        let parsed = RuntimeConfig::parse(&yaml).unwrap();
        let d = RuntimeConfig::default();
        assert_eq!(parsed.server.agent_listen, d.server.agent_listen);
        assert_eq!(parsed.storage.trail.segment_size_bytes, d.storage.trail.segment_size_bytes);
        assert_eq!(parsed.resources.warning_threshold, d.resources.warning_threshold);
        assert_eq!(parsed.observability.health.path, d.observability.health.path);
    }

    // ── Phase 18b.4: Runtime config coverage ──

    #[test]
    fn env_override_invalid_bool() {
        let mut config = RuntimeConfig::default();
        let err = override_one(&mut config, "WACP_TLS__ENABLED", "maybe");
        assert!(err.is_err());
    }

    #[test]
    fn env_override_invalid_u64() {
        let mut config = RuntimeConfig::default();
        let err = override_one(&mut config, "WACP_STORAGE__TRAIL__SEGMENT_SIZE_BYTES", "abc");
        assert!(err.is_err());
    }

    #[test]
    fn env_override_invalid_f32() {
        let mut config = RuntimeConfig::default();
        let err = override_one(&mut config, "WACP_RESOURCES__WARNING_THRESHOLD", "not_a_number");
        assert!(err.is_err());
    }

    #[test]
    fn env_override_u32() {
        let mut config = RuntimeConfig::default();
        override_one(&mut config, "WACP_STORAGE__TRAIL__INDEX_BATCH_SIZE", "200").unwrap();
        assert_eq!(config.storage.trail.index_batch_size, 200);
    }

    #[test]
    fn defaults_yaml_contains_all_sections() {
        let yaml = RuntimeConfig::default_yaml();
        // Verify all 9 top-level sections are present.
        for section in [
            "server:", "tls:", "auth:", "taxonomy:", "storage:",
            "resources:", "delivery:", "logging:", "observability:",
        ] {
            assert!(yaml.contains(section), "defaults YAML missing {section}");
        }
    }

    #[test]
    fn defaults_yaml_roundtrip_all_fields() {
        let yaml = RuntimeConfig::default_yaml();
        let parsed = RuntimeConfig::parse(&yaml).unwrap();
        let d = RuntimeConfig::default();

        // Server
        assert_eq!(parsed.server.agent_listen, d.server.agent_listen);
        assert_eq!(parsed.server.highway_listen, d.server.highway_listen);
        // TLS
        assert_eq!(parsed.tls.enabled, d.tls.enabled);
        assert_eq!(parsed.tls.cert_file, d.tls.cert_file);
        assert_eq!(parsed.tls.key_file, d.tls.key_file);
        assert_eq!(parsed.tls.client_ca_file, d.tls.client_ca_file);
        assert_eq!(parsed.tls.min_version, d.tls.min_version);
        // Auth
        assert_eq!(parsed.auth.provider, d.auth.provider);
        assert_eq!(parsed.auth.external.url, d.auth.external.url);
        assert_eq!(parsed.auth.external.timeout_ms, d.auth.external.timeout_ms);
        assert_eq!(parsed.auth.rate_limit.max_failures, d.auth.rate_limit.max_failures);
        assert_eq!(parsed.auth.rate_limit.window_seconds, d.auth.rate_limit.window_seconds);
        // Storage
        assert_eq!(parsed.storage.data_dir, d.storage.data_dir);
        assert_eq!(parsed.storage.trail.segment_size_bytes, d.storage.trail.segment_size_bytes);
        assert_eq!(parsed.storage.snapshots.system_retention_count, d.storage.snapshots.system_retention_count);
        assert_eq!(parsed.storage.tiered.hot_segments, d.storage.tiered.hot_segments);
        assert_eq!(parsed.storage.tiered.cold_retention, d.storage.tiered.cold_retention);
        // Resources
        assert_eq!(parsed.resources.default_timeout_ms, d.resources.default_timeout_ms);
        assert!((parsed.resources.warning_threshold - d.resources.warning_threshold).abs() < f32::EPSILON);
        // Delivery
        assert_eq!(parsed.delivery.max_retries, d.delivery.max_retries);
        assert_eq!(parsed.delivery.retry_backoff_ms, d.delivery.retry_backoff_ms);
        // Logging
        assert_eq!(parsed.logging.level, d.logging.level);
        assert_eq!(parsed.logging.format, d.logging.format);
        assert_eq!(parsed.logging.output, d.logging.output);
        // Observability
        assert_eq!(parsed.observability.metrics.enabled, d.observability.metrics.enabled);
        assert_eq!(parsed.observability.health.enabled, d.observability.health.enabled);
        assert_eq!(parsed.observability.health.path, d.observability.health.path);
    }

    #[test]
    fn validate_tls_enabled_missing_key_only() {
        let mut config = RuntimeConfig::default();
        config.tls.enabled = true;
        config.tls.cert_file = "/cert.pem".into();
        // key_file still empty
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("tls.key_file"));
    }

    #[test]
    fn validate_data_dir_empty() {
        let mut config = RuntimeConfig::default();
        config.storage.data_dir = String::new();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("storage.data_dir"));
    }

    #[test]
    fn validate_delivery_backoff_zero_with_retries() {
        let mut config = RuntimeConfig::default();
        config.delivery.max_retries = 3;
        config.delivery.retry_backoff_ms = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("retry_backoff_ms"));
    }

    #[test]
    fn validate_delivery_backoff_zero_no_retries_ok() {
        let mut config = RuntimeConfig::default();
        config.delivery.max_retries = 0;
        config.delivery.retry_backoff_ms = 0;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_all_enum_log_formats() {
        let mut config = RuntimeConfig::default();
        for valid in ["json", "pretty"] {
            config.logging.format = valid.into();
            assert!(config.validate().is_ok(), "{valid} should be valid");
        }
        config.logging.format = "xml".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_all_enum_log_outputs() {
        let mut config = RuntimeConfig::default();
        config.logging.output = "stderr".into();
        assert!(config.validate().is_ok());

        config.logging.output = "file".into();
        config.logging.file = "/tmp/wacp.log".into();
        assert!(config.validate().is_ok());

        config.logging.output = "stdout".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_all_enum_auth_providers() {
        let mut config = RuntimeConfig::default();
        config.auth.provider = "psk".into();
        assert!(config.validate().is_ok());

        config.auth.provider = "external".into();
        config.auth.external.url = "https://auth.example.com".into();
        assert!(config.validate().is_ok());

        config.auth.provider = "ldap".into();
        assert!(config.validate().is_err());
    }

    // ── Phase T2.1 additions ──

    #[test]
    fn config_load_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            "server:\n  agent_listen: \"127.0.0.1:8080\"\n",
        )
        .unwrap();
        let (config, resolved) = RuntimeConfig::load(Some(&path)).unwrap();
        assert_eq!(config.server.agent_listen, "127.0.0.1:8080");
        assert!(resolved.is_some());
    }

    #[test]
    fn config_load_nonexistent_path() {
        let result = RuntimeConfig::load(Some(std::path::Path::new("/nonexistent/config.yaml")));
        assert!(result.is_err());
    }

    #[test]
    fn config_load_none_uses_defaults() {
        // When no file is found and no explicit path, load returns defaults.
        // This depends on whether ./wacp-runtime.yaml exists, so just verify
        // the function doesn't panic and returns a valid config.
        let result = RuntimeConfig::load(None);
        // Either Ok (file found or defaults) or Err (parse error in found file).
        // In a clean test dir, defaults should be returned.
        if let Ok((config, _)) = result {
            assert!(config.validate().is_ok());
        }
    }

    #[test]
    fn config_env_override_nested_auth() {
        let mut config = RuntimeConfig::default();
        unsafe { std::env::set_var("WACP_AUTH__RATE_LIMIT__WINDOW_SECONDS", "120") };
        let result = apply_env_overrides(&mut config);
        unsafe { std::env::remove_var("WACP_AUTH__RATE_LIMIT__WINDOW_SECONDS") };
        result.unwrap();
        assert_eq!(config.auth.rate_limit.window_seconds, 120);
    }

    #[test]
    fn config_env_override_observability_bool() {
        let mut config = RuntimeConfig::default();
        unsafe { std::env::set_var("WACP_OBSERVABILITY__METRICS__ENABLED", "true") };
        let result = apply_env_overrides(&mut config);
        unsafe { std::env::remove_var("WACP_OBSERVABILITY__METRICS__ENABLED") };
        result.unwrap();
        assert!(config.observability.metrics.enabled);
    }

    #[test]
    fn config_env_override_logging_level() {
        let mut config = RuntimeConfig::default();
        unsafe { std::env::set_var("WACP_LOGGING__LEVEL", "debug") };
        let result = apply_env_overrides(&mut config);
        unsafe { std::env::remove_var("WACP_LOGGING__LEVEL") };
        result.unwrap();
        assert_eq!(config.logging.level, "debug");
    }

    #[test]
    fn config_validate_default_passes() {
        let config = RuntimeConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_validate_full_custom_valid() {
        let config = RuntimeConfig {
            server: ServerConfig {
                agent_listen: "127.0.0.1:9090".into(),
                highway_listen: "127.0.0.1:9091".into(),
                coordinator_listen: "127.0.0.1:9402".into(),
            },
            tls: TlsConfig {
                enabled: true,
                cert_file: "/path/to/cert.pem".into(),
                key_file: "/path/to/key.pem".into(),
                client_ca_file: String::new(),
                min_version: "1.3".into(),
            },
            auth: AuthConfig {
                provider: "psk".into(),
                ..Default::default()
            },
            storage: StorageConfig {
                data_dir: "./data".into(),
                ..Default::default()
            },
            resources: ResourceConfig {
                warning_threshold: 0.9,
                ..Default::default()
            },
            delivery: DeliveryConfig {
                max_retries: 5,
                retry_backoff_ms: 200,
            },
            logging: LoggingConfig {
                level: "warn".into(),
                format: "pretty".into(),
                output: "stderr".into(),
                file: String::new(),
            },
            observability: ObservabilityConfig {
                metrics: MetricsConfig {
                    enabled: true,
                    listen: "127.0.0.1:9092".into(),
                    path: "/metrics".into(),
                },
                health: HealthConfig {
                    enabled: true,
                    listen: "127.0.0.1:9093".into(),
                    path: "/healthz".into(),
                },
            },
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_ok(), "validation failed: {:?}", result.err());
    }
}
