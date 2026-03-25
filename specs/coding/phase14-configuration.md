# Task 14.1: Configuration

## Scope

Replace the minimal 6-field `RuntimeConfig` with the full 9-section, 47-field configuration hierarchy defined in `impl/deployment.md §2`. The configuration file is YAML, loaded once at startup, immutable for the process lifetime. Unknown keys at any level are rejected. Every field has a default — an empty file is valid. Environment variables with `WACP_` prefix override config file values.

This task produces the `RuntimeConfig` struct hierarchy, YAML deserialization, 9 validation checks, environment variable overrides, and file discovery logic. It does NOT implement the subsystems those fields configure (TLS, logging, auth, metrics, health) — those are tasks 14.2–14.6.

The existing `RuntimeConfig` (6 fields: `data_dir`, `taxonomy_path`, `protocol_version`, `max_segment_size`, `agent_port`, `highway_port`) is replaced entirely. All call sites in `init.rs`, `main.rs`, and `tests.rs` are updated to use the new struct.

## Dependencies

- `serde` (workspace — already present)
- `serde_yaml` (workspace — already present, not yet in wacp-runtime's Cargo.toml)
- `Serialize` derive needed for `defaults` subcommand (task 14.2 will consume it)

## Types

### `RuntimeConfig`

Top-level config. 9 sections, each defaultable.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
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
```

### `ServerConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default = "default_agent_listen")]
    pub agent_listen: String,     // "[::1]:9090"
    #[serde(default = "default_highway_listen")]
    pub highway_listen: String,   // "[::1]:9091"
}
```

### `TlsConfig`

```rust
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
    pub min_version: String,      // "1.2"
}
```

### `AuthConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default = "default_auth_provider")]
    pub provider: String,         // "psk"
    #[serde(default)]
    pub external: ExternalAuthConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}
```

### `ExternalAuthConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalAuthConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_auth_timeout")]
    pub timeout_ms: u64,          // 5000
}
```

### `RateLimitConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitConfig {
    #[serde(default = "default_rate_limit_failures")]
    pub max_failures: u32,        // 10
    #[serde(default = "default_rate_limit_window")]
    pub window_seconds: u32,      // 60
}
```

### `TaxonomyConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyConfig {
    #[serde(default)]
    pub file: String,
}
```

### `StorageConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: String,         // "./data"
    #[serde(default)]
    pub trail: TrailStorageConfig,
    #[serde(default)]
    pub snapshots: SnapshotConfig,
    #[serde(default)]
    pub tiered: TieredConfig,
}
```

### `TrailStorageConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrailStorageConfig {
    #[serde(default = "default_segment_size")]
    pub segment_size_bytes: u64,      // 67_108_864
    #[serde(default = "default_index_batch_size")]
    pub index_batch_size: u32,        // 100
    #[serde(default = "default_index_batch_timeout")]
    pub index_batch_timeout_ms: u32,  // 50
}
```

### `SnapshotConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotConfig {
    #[serde(default = "default_ws_checkpoint_interval")]
    pub workspace_checkpoint_interval: u32,    // 5
    #[serde(default = "default_ws_time_interval")]
    pub workspace_time_interval_seconds: u32,  // 60
    #[serde(default = "default_sys_entry_interval")]
    pub system_entry_interval: u64,            // 10_000
    #[serde(default = "default_sys_time_interval")]
    pub system_time_interval_minutes: u32,     // 30
    #[serde(default = "default_sys_retention")]
    pub system_retention_count: u32,           // 3
}
```

### `TieredConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TieredConfig {
    #[serde(default = "default_hot_segments")]
    pub hot_segments: u32,                     // 10
    #[serde(default = "default_warm_retention")]
    pub warm_retention_days: u32,              // 90
    #[serde(default = "default_cold_retention")]
    pub cold_retention: String,                // "indefinite"
    #[serde(default)]
    pub cold_destination: String,
    #[serde(default = "default_compaction_interval")]
    pub compaction_interval_minutes: u32,      // 60
}
```

### `ResourceConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceConfig {
    #[serde(default)]
    pub default_timeout_ms: u64,
    #[serde(default)]
    pub default_budget: BudgetConfig,
    #[serde(default = "default_warning_threshold")]
    pub warning_threshold: f32,                // 0.8
    #[serde(default)]
    pub liveness_interval_ms: u64,
}
```

### `BudgetConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
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
```

### `DeliveryConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,                      // 3
    #[serde(default = "default_retry_backoff")]
    pub retry_backoff_ms: u64,                 // 100
}
```

### `LoggingConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,                         // "info"
    #[serde(default = "default_log_format")]
    pub format: String,                        // "json"
    #[serde(default = "default_log_output")]
    pub output: String,                        // "stderr"
    #[serde(default)]
    pub file: String,
}
```

### `ObservabilityConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservabilityConfig {
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub health: HealthConfig,
}
```

### `MetricsConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_metrics_listen")]
    pub listen: String,                        // "[::1]:9092"
    #[serde(default = "default_metrics_path")]
    pub path: String,                          // "/metrics"
}
```

### `HealthConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthConfig {
    #[serde(default = "default_health_enabled")]
    pub enabled: bool,                         // true
    #[serde(default = "default_health_listen")]
    pub listen: String,                        // "[::1]:9093"
    #[serde(default = "default_health_path")]
    pub path: String,                          // "/healthz"
}
```

### `ConfigError`

```rust
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
```

## Functions

### Loading

```rust
/// Load configuration: --config path → WACP_CONFIG env → ./wacp-runtime.yaml → defaults.
/// After loading, applies env var overrides, then validates.
pub fn load(config_path: Option<&Path>) -> Result<(RuntimeConfig, Option<PathBuf>), ConfigError>
```

Returns the config and the resolved file path (None if defaults were used).

```rust
/// Parse YAML string into RuntimeConfig.
pub fn parse(yaml: &str) -> Result<RuntimeConfig, ConfigError>
```

### Validation

```rust
impl RuntimeConfig {
    /// Run all 9 validation checks in order. Returns first failure.
    pub fn validate(&self) -> Result<(), ConfigError>
}
```

The 9 checks, in order:

1. **TLS completeness** — if `tls.enabled`, `cert_file` and `key_file` must be non-empty. If `client_ca_file` is non-empty, it must be non-empty (structural check only — file existence is checked at TLS init, not config validation, since the files may not exist yet in container environments where secrets are mounted late).
2. **Auth provider completeness** — if `auth.provider == "external"`, `auth.external.url` must be non-empty and parseable as a URL.
3. **Taxonomy readability** — if `taxonomy.file` is non-empty, verify the path is non-empty (file existence checked at taxonomy load time).
4. **Data directory** — `storage.data_dir` must be non-empty.
5. **Address uniqueness** — the 4 listen addresses (server.agent_listen, server.highway_listen, observability.metrics.listen, observability.health.listen) must be pairwise distinct when the corresponding subsystem is enabled. Metrics and health addresses only checked when their `enabled` flag is true.
6. **Numeric constraints** — all `> 0` constraints from §2.11 (segment_size_bytes, index_batch_size, index_batch_timeout_ms, all snapshot intervals, system_retention_count >= 1, hot_segments >= 1, warm_retention_days, compaction_interval_minutes, warning_threshold in (0.0, 1.0], retry_backoff_ms > 0 when max_retries > 0, rate_limit.window_seconds > 0 when max_failures > 0).
7. **Enum fields** — `tls.min_version` in {"1.2", "1.3"}, `auth.provider` in {"psk", "external"}, `logging.level` in {"trace", "debug", "info", "warn", "error"}, `logging.format` in {"json", "pretty"}, `logging.output` in {"stderr", "file"}.
8. **Logging file path** — if `logging.output == "file"`, `logging.file` must be non-empty.
9. **Cold retention parse** — `storage.tiered.cold_retention` must be "indefinite" or a positive integer string.

### Environment Variable Overrides

```rust
/// Apply WACP_ environment variable overrides to the config.
/// Unknown WACP_ variables are ignored (not errors).
pub fn apply_env_overrides(config: &mut RuntimeConfig) -> Result<(), ConfigError>
```

Naming convention: `WACP_` prefix, uppercase, `__` for nesting (e.g., `WACP_SERVER__AGENT_LISTEN`). `WACP_CONFIG` is skipped (it selects the file, not a field). Types parsed: String as-is, u32/u64 via `str::parse`, f32 via `str::parse`, bool via "true"/"1" → true, "false"/"0" → false.

Implementation: match on the lowercased, `__`→`.` converted path to set the corresponding field. A `match` statement over known paths — not reflection.

## Internal Design

- All default functions are module-level `fn default_*() -> T` functions referenced by `#[serde(default = "...")]`.
- `validate()` returns `Err(ConfigError::Validation { field, message })` on the first failing check. The `field` string uses the dotted config path (e.g., `"tls.cert_file"`).
- `apply_env_overrides` iterates `std::env::vars()`, filters `WACP_` prefix, converts the key to a dotted path, and dispatches to the correct field via a match. Unrecognized paths are silently ignored.
- `load()` calls `parse()` then `apply_env_overrides()` then `validate()` in that order.
- The existing `init.rs` call sites are updated: `config.data_dir` → `PathBuf::from(&config.storage.data_dir)`, `config.max_segment_size` → `config.storage.trail.segment_size_bytes`, `config.agent_port`/`highway_port` → parse from `config.server.agent_listen`/`highway_listen`, `config.taxonomy_path` → `config.taxonomy.file` (empty string = None).

## Tests

| Test | Verifies |
|------|----------|
| `parse_empty_yields_defaults` | `parse("")` succeeds; all fields match `RuntimeConfig::default()` |
| `parse_partial_server` | Setting only `server.agent_listen` works; other fields default |
| `parse_full_config` | A complete YAML with all 47 fields parses without error |
| `reject_unknown_top_level_key` | YAML with `unknown_key: x` at root → `ConfigError::Parse` |
| `reject_unknown_nested_key` | YAML with `server.unknown: x` → `ConfigError::Parse` |
| `validate_tls_completeness` | `tls.enabled: true` with empty `cert_file` → validation error on `tls.cert_file` |
| `validate_tls_completeness_ok` | `tls.enabled: true` with non-empty cert+key → passes |
| `validate_auth_external_requires_url` | `auth.provider: "external"` with empty url → validation error |
| `validate_address_uniqueness` | Same address for agent and highway → validation error |
| `validate_address_uniqueness_disabled` | Same address for metrics (disabled) and health → passes (metrics not checked) |
| `validate_numeric_segment_size_zero` | `storage.trail.segment_size_bytes: 0` → validation error |
| `validate_numeric_retention_count_zero` | `storage.snapshots.system_retention_count: 0` → validation error |
| `validate_warning_threshold_bounds` | `resources.warning_threshold: 0.0` → error; `1.1` → error; `0.5` → ok |
| `validate_enum_tls_min_version` | `tls.min_version: "1.0"` → validation error |
| `validate_enum_log_level` | `logging.level: "verbose"` → validation error |
| `validate_log_file_required` | `logging.output: "file"` with empty `file` → validation error |
| `validate_cold_retention_indefinite` | `"indefinite"` → passes |
| `validate_cold_retention_integer` | `"30"` → passes; `"abc"` → error; `"0"` → error; `"-1"` → error |
| `env_override_string` | Set `WACP_SERVER__AGENT_LISTEN`, verify it overrides the config file value |
| `env_override_u64` | Set `WACP_STORAGE__TRAIL__SEGMENT_SIZE_BYTES=1024`, verify parsed as u64 |
| `env_override_f32` | Set `WACP_RESOURCES__WARNING_THRESHOLD=0.9`, verify parsed as f32 |
| `env_override_bool` | Set `WACP_TLS__ENABLED=true`, verify parsed as bool; `"1"` also works |
| `env_override_precedence` | Config file sets agent_listen to X, env sets to Y → Y wins |
| `env_override_unknown_ignored` | `WACP_UNKNOWN__FIELD=x` does not cause an error |
| `load_file_discovery` | With `WACP_CONFIG` set to a file path, that file is loaded |
| `load_defaults_no_file` | With no file and no env vars, `load(None)` returns defaults |
| `default_roundtrip` | `RuntimeConfig::default()` serializes to YAML and parses back identically |

## Acceptance Criteria

- `RuntimeConfig` has 9 sections, 47 fields, matching `impl/deployment.md §2.11` exactly.
- `#[serde(deny_unknown_fields)]` on every struct — typos in config keys are startup errors.
- Every field has a default matching §2.11.
- All 9 validation checks implemented in order, returning the first failure with field path.
- Env var overrides work for all 47 fields.
- `load()` follows the 4-step file discovery: --config arg → `WACP_CONFIG` → `./wacp-runtime.yaml` → defaults.
- All existing tests in `wacp-runtime` still pass after updating call sites.
- `cargo test -p wacp-runtime` passes with all new tests.
- `cargo clippy -p wacp-runtime` clean.
