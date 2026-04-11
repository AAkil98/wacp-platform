# WACP Implementation: Deployment

```yaml
id: wacp-impl-deployment
type: implementation-spec
status: draft
created: 2026-03-21
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §11 (security model)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-protocol-interface
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, deployment, configuration, tls, logging, docker, production]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Configuration File Format](#2-configuration-file-format)
3. [CLI Interface](#3-cli-interface)
4. [TLS Configuration](#4-tls-configuration)
5. [Authenticator Providers](#5-authenticator-providers)
6. [Structured Logging](#6-structured-logging)
7. [Metrics Endpoint](#7-metrics-endpoint)
8. [Health Checks](#8-health-checks)
9. [Docker Image](#9-docker-image)
10. [Systemd Unit](#10-systemd-unit)
11. [Environment Variable Overrides](#11-environment-variable-overrides)
12. [Startup and Shutdown Sequence](#12-startup-and-shutdown-sequence)
13. [References](#13-references)

## 1. Purpose

This spec defines how the WACP runtime is configured, packaged, and operated in production. It answers "how do I run this" — not "what happens inside" (that's the runtime spec's job) or "what crosses the wire" (that's the protocol-interface spec's job).

The runtime binary (`wacp-runtime`) is the trust root (runtime spec, §1). In development, it starts with defaults — plaintext gRPC, no authentication, console logging. In production, it needs TLS, authenticated connections, structured logging, health checks, and a configuration file that controls all of this without recompilation. This spec defines the production surface.

**Scope.** YAML configuration file format and schema. CLI interface for the `wacp-runtime` binary. TLS certificate setup for agent and highway gRPC endpoints. Pluggable authenticator configuration. Structured logging via the `tracing` crate. Prometheus-compatible metrics endpoint. gRPC and HTTP health checks. Docker image build. Systemd unit file. Environment variable overrides for twelve-factor deployments.

**Not in scope.** Runtime internals (runtime spec). Protocol semantics (PROTOCOL.md). Wire format (protocol-interface spec). Highway UI deployment (highway-ui spec — the UI is static files served independently). Kubernetes manifests, Helm charts, or orchestrator-specific configuration — these are deployment-environment concerns that build on the primitives defined here.

**Design constraint.** Configuration is declarative, not programmatic. The config file is the single source of truth for a running instance. Every tunable has a sensible default — the runtime must start with zero configuration for development. Environment variables override config file values for container deployments. No configuration reload at runtime — changing configuration requires a restart. This is deliberate: the runtime is a trust root, and hot-reloading configuration introduces a class of partial-state bugs that are hard to reason about.

---

## 2. Configuration File Format

The configuration file is the single source of truth for a running `wacp-runtime` instance. It is YAML, loaded once at startup, and immutable for the lifetime of the process. No hot reload — changing configuration requires a restart (§1, design constraint).

**File location.** The runtime looks for its configuration in the following order, taking the first file it finds:

1. The path given by the `--config` CLI flag (§3).
2. The path in the `WACP_CONFIG` environment variable.
3. `./wacp-runtime.yaml` in the current working directory.
4. If none is found, the runtime starts with all defaults — zero-config development mode.

**Format rules.** The file is parsed with `serde_yaml`. Unknown keys at any level are rejected — a typo in a key name is a startup error, not a silent default. This prevents misconfiguration from hiding behind ignored fields. The parser produces a typed `RuntimeConfig` struct; every field has a default expressed in the struct definition, so a partial file is valid — omitted sections inherit defaults.

### 2.1 Schema

The configuration file has nine top-level sections. Each maps to a subsystem of the runtime. The sections are independent — specifying one does not require specifying others.

```yaml
server:       # listen addresses for gRPC endpoints
tls:          # TLS certificate configuration
auth:         # authenticator provider selection
taxonomy:     # taxonomy file path
storage:      # data directory and storage tunables
resources:    # default workspace resource limits
delivery:     # envelope delivery retry policy
logging:      # structured logging configuration
observability: # metrics and health check endpoints
```

### 2.2 `server`

Controls the gRPC listen addresses for the two external boundaries (runtime spec, §2).

```yaml
server:
  agent_listen: "[::1]:9090"      # agent-facing gRPC endpoint
  highway_listen: "[::1]:9091"    # highway-facing gRPC endpoint
```

Two addresses because the two boundaries have different trust profiles, authentication flows, and may need different network exposure. A production deployment typically exposes the highway to an internal network and the agent endpoint to a more restricted set of hosts — separate listen addresses enable this without a reverse proxy.

**Why `[::1]` (localhost IPv6) as default.** Development mode binds to localhost only. Production deployments override to `0.0.0.0:<port>` or a specific interface address. Binding to a public interface by default would be a security mistake — the runtime starts without TLS or authentication in zero-config mode.

### 2.3 `tls`

TLS configuration for both gRPC endpoints. When enabled, both agent and highway endpoints use the same certificate. Separate per-endpoint certificates are not supported in the initial implementation — the runtime presents one identity.

```yaml
tls:
  enabled: false
  cert_file: ""          # path to PEM-encoded certificate chain
  key_file: ""           # path to PEM-encoded private key
  client_ca_file: ""     # path to PEM-encoded CA for client certificate verification (mTLS)
  min_version: "1.2"     # minimum TLS version: "1.2" or "1.3"
```

When `enabled: true`, both endpoints serve over TLS. When `enabled: false`, both serve plaintext gRPC (development only — the spec strongly recommends TLS in production, protocol-interface spec §7). The `client_ca_file` field enables mutual TLS — when set, the runtime requires connecting clients to present a certificate signed by this CA. This is independent of the application-level authentication (§5 of this spec) — mTLS authenticates the transport, the `Bind`/`Authenticate` RPCs authenticate the protocol identity.

### 2.4 `auth`

Selects the authenticator provider (protocol-interface spec, §7). The runtime ships with two implementations. Exactly one is active per run.

```yaml
auth:
  provider: "psk"                # "psk" or "external"
  external:
    url: ""                      # HTTP endpoint for external authentication
    timeout_ms: 5000             # timeout for external auth requests
  rate_limit:
    max_failures: 10             # max failed auth attempts per source
    window_seconds: 60           # sliding window for rate limiting
```

**`psk` provider.** Pre-shared key. The runtime generates a unique token per workspace at creation time and delivers it to the agent through the launch mechanism. No additional configuration — the token store is runtime-internal. Suitable for single-machine deployments where the runtime launches its own agents.

**`external` provider.** Delegates authentication to an HTTP service. On each `Bind` or `Authenticate` request, the runtime POSTs `{ "token": "<token>", "workspace_id": "<id>" }` (for agents) or `{ "token": "<token>" }` (for humans) to `auth.external.url`. A 200 response with `{ "identity": "<id>" }` is success. Any other response is rejection. The external service is outside the trust boundary — the runtime validates the response shape but trusts the identity it returns.

**Rate limiting.** Applies to both providers. After `max_failures` failed authentication attempts from the same source IP within `window_seconds`, subsequent attempts from that IP are rejected without consulting the authenticator. Rate limiting is recorded in the trail (`authentication_rate_limited` entries). Setting `max_failures: 0` disables rate limiting.

### 2.5 `taxonomy`

Path to the taxonomy definition file.

```yaml
taxonomy:
  file: ""               # path to YAML taxonomy file; empty = base types only
```

An empty string (or omitted field) means the runtime operates with base protocol types only — no derived roles, no custom envelope types, no custom checkpoint types. The taxonomy loader (runtime spec, §11) performs all validation at startup; an invalid taxonomy aborts the run.

### 2.6 `storage`

Controls the data directory layout and tuning parameters for all three storage domains (storage spec, §2).

```yaml
storage:
  data_dir: "./data"                         # root directory for all persistent data

  trail:
    segment_size_bytes: 67108864             # 64 MB — segment rotation threshold
    index_batch_size: 100                    # entries per SQLite index batch
    index_batch_timeout_ms: 50               # max delay before flushing index batch

  snapshots:
    workspace_checkpoint_interval: 5         # persist workspace state every N checkpoints
    workspace_time_interval_seconds: 60      # or every N seconds, whichever comes first
    system_entry_interval: 10000             # system snapshot every N trail entries
    system_time_interval_minutes: 30         # or every N minutes, whichever comes first
    system_retention_count: 3                # number of old system snapshots to keep

  tiered:
    hot_segments: 10                         # sealed segments kept uncompressed in hot tier
    warm_retention_days: 90                  # max age before warm → cold transition
    cold_retention: "indefinite"             # days or "indefinite"; "indefinite" = never delete
    cold_destination: ""                     # empty = no cold tier; otherwise a path or URI
    compaction_interval_minutes: 60          # background compaction cycle period
```

**`data_dir` layout.** The runtime creates subdirectories under `data_dir` at first startup:

```
<data_dir>/
├── trail/
│   ├── segment-*.trail          # append-only trail segments
│   ├── trail.meta               # segment metadata
│   └── trail-index.db           # SQLite index (derived, rebuildable)
├── checkpoints/
│   ├── <xx>/                    # two-character hash prefix directories
│   │   └── <hash>.blob          # content-addressable payload files
│   └── checkpoints.meta
└── snapshots/
    ├── ws-*.snapshot             # per-workspace snapshots
    ├── system-*.snapshot         # system snapshots
    └── system-latest.snapshot    # symlink to most recent
```

No checkpoint store configuration is exposed because the content-addressable store has no meaningful tunables — its behavior is fully determined by the storage spec (§5). Directory sharding, deduplication, and integrity verification are implementation-fixed.

**`cold_destination`.** When non-empty, enables the cold tier. The value is a path (local directory, NFS mount) or a URI scheme recognized by a storage plugin. The initial implementation supports local paths only. S3 or object store support is a future extension point — the tier transition logic is the same regardless of destination.

**`cold_retention`.** Either the string `"indefinite"` or a positive integer representing days. When set to a number, segments older than that many days in the cold tier are deleted (with trail recording per storage spec §9).

### 2.7 `resources`

Default resource limits applied to every workspace at creation time. The coordinator may override these per workspace or per task — these are the fallback values.

```yaml
resources:
  default_timeout_ms: 0                    # workspace timeout; 0 = no timeout
  default_budget:
    max_tokens: 0                          # 0 = unlimited for each dimension
    max_wall_time_ms: 0
    max_storage_bytes: 0
    max_network_bytes: 0
    max_cost_micros: 0
  warning_threshold: 0.8                   # fraction of budget that triggers warning
  liveness_interval_ms: 0                  # 0 = liveness monitoring disabled
```

**Zero means unlimited.** For every budget dimension, `0` disables that limit — the resource meter tracks consumption but never triggers a failure. This matches the development-mode philosophy: no artificial limits until production configures them.

**`warning_threshold`.** A float in the range `(0.0, 1.0]`. When a workspace's consumption in any budget dimension exceeds this fraction of its limit, the runtime emits a `resource_warning` trail entry and sends a feedback envelope to the agent (runtime spec, §12). Setting `1.0` disables warnings — the workspace goes straight from normal to hard failure.

**`liveness_interval_ms`.** When non-zero, the coordinator monitors each active workspace for activity. If no trail entry is recorded for a workspace within this interval, a `liveness_warning` trail entry is produced. Liveness is advisory — the coordinator decides the response (runtime spec, §12). Setting `0` disables monitoring entirely.

### 2.8 `delivery`

Envelope delivery retry policy (runtime spec, §9).

```yaml
delivery:
  max_retries: 3                           # delivery attempts before rejection
  retry_backoff_ms: 100                    # linear backoff base between attempts
```

If delivery fails, the runtime retries up to `max_retries` times with linearly increasing delays (`retry_backoff_ms`, `2 * retry_backoff_ms`, `3 * retry_backoff_ms`). Each attempt is recorded in the trail. After exhausting retries, the envelope transitions to `Rejected` with `reason: delivery_failed`.

### 2.9 `logging`

Structured logging via the `tracing` crate. Logging is orthogonal to the trail — the trail is the protocol's audit record; logs are the operator's operational record.

```yaml
logging:
  level: "info"                            # "trace", "debug", "info", "warn", "error"
  format: "json"                           # "json" or "pretty"
  output: "stderr"                         # "stderr" or "file"
  file: ""                                 # path when output is "file"
```

**`json` format.** One JSON object per log line. Fields: `timestamp` (ISO 8601), `level`, `target` (Rust module path), `message`, `span` (active tracing spans), and event-specific structured fields. This is the production default — machine-parseable, compatible with log aggregators (Elasticsearch, Loki, Datadog).

**`pretty` format.** Human-readable, colored output for terminal use. Development convenience — not intended for production.

**`output: "file"`.** When set to `"file"`, logs are written to the path in `logging.file`. The runtime opens the file in append mode. Log rotation is the operator's responsibility (logrotate, systemd journal, etc.) — the runtime does not rotate its own log files.

### 2.10 `observability`

Metrics endpoint and health checks. These are HTTP endpoints, separate from the gRPC services.

```yaml
observability:
  metrics:
    enabled: false
    listen: "[::1]:9092"
    path: "/metrics"
  health:
    enabled: true
    listen: "[::1]:9093"
    path: "/healthz"
```

**Metrics.** When enabled, serves Prometheus-compatible metrics (§7 of this spec). Disabled by default in development.

**Health.** When enabled, serves HTTP health checks (§8 of this spec). Enabled by default — health checks are useful even in development.

### 2.11 Complete Field Reference

Every configurable field, its type, default value, and constraints.

| Field | Type | Default | Constraints |
|-------|------|---------|-------------|
| `server.agent_listen` | string | `"[::1]:9090"` | Valid socket address |
| `server.highway_listen` | string | `"[::1]:9091"` | Valid socket address |
| `tls.enabled` | bool | `false` | — |
| `tls.cert_file` | string | `""` | Must be a readable file when `tls.enabled` |
| `tls.key_file` | string | `""` | Must be a readable file when `tls.enabled` |
| `tls.client_ca_file` | string | `""` | Must be a readable file if non-empty |
| `tls.min_version` | string | `"1.2"` | `"1.2"` or `"1.3"` |
| `auth.provider` | string | `"psk"` | `"psk"` or `"external"` |
| `auth.external.url` | string | `""` | Must be a valid URL when `auth.provider` is `"external"` |
| `auth.external.timeout_ms` | u64 | `5000` | > 0 |
| `auth.rate_limit.max_failures` | u32 | `10` | 0 disables rate limiting |
| `auth.rate_limit.window_seconds` | u32 | `60` | > 0 when `max_failures` > 0 |
| `taxonomy.file` | string | `""` | Must be a readable file if non-empty |
| `storage.data_dir` | string | `"./data"` | Must be a writable directory (created if absent) |
| `storage.trail.segment_size_bytes` | u64 | `67108864` | > 0 |
| `storage.trail.index_batch_size` | u32 | `100` | > 0 |
| `storage.trail.index_batch_timeout_ms` | u32 | `50` | > 0 |
| `storage.snapshots.workspace_checkpoint_interval` | u32 | `5` | > 0 |
| `storage.snapshots.workspace_time_interval_seconds` | u32 | `60` | > 0 |
| `storage.snapshots.system_entry_interval` | u64 | `10000` | > 0 |
| `storage.snapshots.system_time_interval_minutes` | u32 | `30` | > 0 |
| `storage.snapshots.system_retention_count` | u32 | `3` | >= 1 |
| `storage.tiered.hot_segments` | u32 | `10` | >= 1 |
| `storage.tiered.warm_retention_days` | u32 | `90` | > 0 |
| `storage.tiered.cold_retention` | string | `"indefinite"` | `"indefinite"` or positive integer |
| `storage.tiered.cold_destination` | string | `""` | Valid path or URI if non-empty |
| `storage.tiered.compaction_interval_minutes` | u32 | `60` | > 0 |
| `resources.default_timeout_ms` | u64 | `0` | 0 = no timeout |
| `resources.default_budget.max_tokens` | u64 | `0` | 0 = unlimited |
| `resources.default_budget.max_wall_time_ms` | u64 | `0` | 0 = unlimited |
| `resources.default_budget.max_storage_bytes` | u64 | `0` | 0 = unlimited |
| `resources.default_budget.max_network_bytes` | u64 | `0` | 0 = unlimited |
| `resources.default_budget.max_cost_micros` | u64 | `0` | 0 = unlimited |
| `resources.warning_threshold` | f32 | `0.8` | (0.0, 1.0] |
| `resources.liveness_interval_ms` | u64 | `0` | 0 = disabled |
| `delivery.max_retries` | u32 | `3` | >= 0 |
| `delivery.retry_backoff_ms` | u64 | `100` | > 0 when `max_retries` > 0 |
| `logging.level` | string | `"info"` | `"trace"`, `"debug"`, `"info"`, `"warn"`, `"error"` |
| `logging.format` | string | `"json"` | `"json"` or `"pretty"` |
| `logging.output` | string | `"stderr"` | `"stderr"` or `"file"` |
| `logging.file` | string | `""` | Must be a writable path when `output` is `"file"` |
| `observability.metrics.enabled` | bool | `false` | — |
| `observability.metrics.listen` | string | `"[::1]:9092"` | Valid socket address |
| `observability.metrics.path` | string | `"/metrics"` | Must start with `/` |
| `observability.health.enabled` | bool | `true` | — |
| `observability.health.listen` | string | `"[::1]:9093"` | Valid socket address |
| `observability.health.path` | string | `"/healthz"` | Must start with `/` |

### 2.12 Validation

The configuration parser runs validation after deserialization, before the runtime initializes any subsystem. Validation is all-or-nothing — any failure aborts the run with a diagnostic error message identifying the failing field and the violated constraint.

**Validation checks, in order:**

1. **TLS completeness.** If `tls.enabled` is true, both `tls.cert_file` and `tls.key_file` must be non-empty and point to readable files. If `tls.client_ca_file` is non-empty, it must also be readable. The runtime attempts to parse the PEM contents — malformed certificates are rejected at startup, not at connection time.
2. **Auth provider completeness.** If `auth.provider` is `"external"`, `auth.external.url` must be a non-empty, syntactically valid URL. The runtime does not probe the URL at startup — it may not be reachable yet in container deployments where services start concurrently.
3. **Taxonomy readability.** If `taxonomy.file` is non-empty, the file must exist and be readable. Taxonomy content validation is performed by the taxonomy loader (runtime spec, §11), not the config parser.
4. **Data directory.** `storage.data_dir` must be a writable directory. If it does not exist, the runtime creates it (and its subdirectories). If it exists but is not writable, startup fails.
5. **Address uniqueness.** The four listen addresses (`server.agent_listen`, `server.highway_listen`, `observability.metrics.listen`, `observability.health.listen`) must be pairwise distinct when the corresponding subsystem is enabled. Binding two services to the same address is a startup error.
6. **Numeric constraints.** All numeric fields are checked against the constraints in §2.11. Out-of-range values are rejected with the field path and the constraint (e.g., `"storage.snapshots.system_retention_count must be >= 1, got 0"`).
7. **Enum fields.** `tls.min_version`, `auth.provider`, `logging.level`, `logging.format`, `logging.output` must be one of their allowed values. Unrecognized strings are rejected.
8. **Logging file path.** If `logging.output` is `"file"`, `logging.file` must be non-empty. The runtime opens the file in append mode at startup — if the path is not writable, startup fails.
9. **Cold retention parse.** `storage.tiered.cold_retention` must be either the literal string `"indefinite"` or a string that parses as a positive integer (representing days).

**Fail-fast rationale.** The runtime is the trust root (§1). A misconfigured trust root is worse than a trust root that refuses to start. Every configuration error is detectable at startup — there are no deferred validation checks that could fail minutes or hours into a run. The operator sees the error immediately, fixes it, and restarts.

### 2.13 Rust Struct

The configuration file deserializes into the following struct hierarchy. `#[serde(default)]` on every field ensures partial files work — omitted fields take the value from the `Default` impl.

```rust
#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default = "default_agent_listen")]
    pub agent_listen: String,        // "[::1]:9090"
    #[serde(default = "default_highway_listen")]
    pub highway_listen: String,      // "[::1]:9091"
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    #[serde(default)]
    pub enabled: bool,               // false
    #[serde(default)]
    pub cert_file: String,
    #[serde(default)]
    pub key_file: String,
    #[serde(default)]
    pub client_ca_file: String,
    #[serde(default = "default_tls_min_version")]
    pub min_version: String,         // "1.2"
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default = "default_auth_provider")]
    pub provider: String,            // "psk"
    #[serde(default)]
    pub external: ExternalAuthConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalAuthConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_auth_timeout")]
    pub timeout_ms: u64,             // 5000
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitConfig {
    #[serde(default = "default_rate_limit_failures")]
    pub max_failures: u32,           // 10
    #[serde(default = "default_rate_limit_window")]
    pub window_seconds: u32,         // 60
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyConfig {
    #[serde(default)]
    pub file: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: String,            // "./data"
    #[serde(default)]
    pub trail: TrailStorageConfig,
    #[serde(default)]
    pub snapshots: SnapshotConfig,
    #[serde(default)]
    pub tiered: TieredConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrailStorageConfig {
    #[serde(default = "default_segment_size")]
    pub segment_size_bytes: u64,     // 67_108_864 (64 MB)
    #[serde(default = "default_index_batch_size")]
    pub index_batch_size: u32,       // 100
    #[serde(default = "default_index_batch_timeout")]
    pub index_batch_timeout_ms: u32, // 50
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceConfig {
    #[serde(default)]
    pub default_timeout_ms: u64,               // 0
    #[serde(default)]
    pub default_budget: BudgetConfig,
    #[serde(default = "default_warning_threshold")]
    pub warning_threshold: f32,                // 0.8
    #[serde(default)]
    pub liveness_interval_ms: u64,             // 0
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,                      // 3
    #[serde(default = "default_retry_backoff")]
    pub retry_backoff_ms: u64,                 // 100
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservabilityConfig {
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub health: HealthConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    #[serde(default)]
    pub enabled: bool,                         // false
    #[serde(default = "default_metrics_listen")]
    pub listen: String,                        // "[::1]:9092"
    #[serde(default = "default_metrics_path")]
    pub path: String,                          // "/metrics"
}

#[derive(Debug, Clone, Deserialize)]
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

**`deny_unknown_fields` on every struct.** This is the enforcement mechanism for the "no typo goes unnoticed" rule. If an operator writes `segement_size_bytes` instead of `segment_size_bytes`, serde rejects the file at parse time with an error that names the unrecognized field and the struct that rejected it. Without this, the typo'd field is silently ignored and the default applies — a misconfiguration that is difficult to diagnose.

### 2.14 Production Example

A minimal production configuration. Fields not shown inherit defaults.

```yaml
server:
  agent_listen: "0.0.0.0:9090"
  highway_listen: "0.0.0.0:9091"

tls:
  enabled: true
  cert_file: "/etc/wacp/tls/server.crt"
  key_file: "/etc/wacp/tls/server.key"
  client_ca_file: "/etc/wacp/tls/ca.crt"

auth:
  provider: "external"
  external:
    url: "http://auth-service:8080/validate"
    timeout_ms: 3000

taxonomy:
  file: "/etc/wacp/taxonomy.yaml"

storage:
  data_dir: "/var/lib/wacp"

resources:
  default_timeout_ms: 600000            # 10 minutes
  default_budget:
    max_tokens: 1000000
    max_cost_micros: 5000000            # $5.00
  warning_threshold: 0.8
  liveness_interval_ms: 30000           # 30 seconds

logging:
  level: "info"
  format: "json"
  output: "stderr"

observability:
  metrics:
    enabled: true
    listen: "0.0.0.0:9092"
  health:
    enabled: true
    listen: "0.0.0.0:9093"
```

---

## 3. CLI Interface

The `wacp-runtime` binary is the single entry point for operating the WACP runtime. It uses `clap` for argument parsing with derive-based subcommands. The binary has one primary mode (serve) and two utility subcommands (validate, defaults).

### 3.1 Subcommands

```
wacp-runtime [OPTIONS] <COMMAND>

Commands:
  serve       Start the runtime (default if no command given)
  validate    Parse and validate configuration, then exit
  defaults    Print the full default configuration to stdout

Options:
  -c, --config <PATH>    Path to configuration file
  -V, --version          Print version information
  -h, --help             Print help
```

**`serve`** is the default subcommand — running `wacp-runtime` with no arguments is equivalent to `wacp-runtime serve`. This means the binary does useful work with zero arguments: it searches for a config file (§2, file location rules), falls back to defaults, and starts the runtime.

**`validate`** loads the configuration through the full pipeline — file discovery, YAML parsing, deserialization, validation (§2.12), taxonomy loading (if configured) — then exits with code 0 on success or code 1 with a diagnostic error on failure. It does not open storage, bind ports, or start any runtime subsystem. This is a pre-deploy sanity check: `wacp-runtime validate --config /etc/wacp/runtime.yaml` in a CI pipeline catches configuration errors before deployment.

**`defaults`** prints the full `RuntimeConfig` with all default values as YAML to stdout and exits. This is a bootstrapping tool — an operator runs `wacp-runtime defaults > wacp-runtime.yaml` and edits the result. The output is valid YAML that the runtime can consume directly.

### 3.2 Global Options

**`--config <PATH>`.** Overrides the config file search (§2, file location rules). When provided, the runtime loads exactly this file — it does not search the environment variable or the current directory. If the file does not exist or is not readable, the runtime exits with code 1 and a diagnostic error.

**`--version`.** Prints version information in the format:

```
wacp-runtime <version>
protocol: wacp-v0.1
built: <build timestamp>
commit: <git commit hash>
```

The protocol version is compiled into the binary — it is the version the taxonomy loader checks against (runtime spec, §11). The build timestamp and commit hash are set at compile time via `build.rs` environment variables. This output is essential for incident response: "which build is running?" has one unambiguous answer.

### 3.3 `serve` Subcommand

```
wacp-runtime serve [OPTIONS]

Options:
  -c, --config <PATH>    Path to configuration file
```

`serve` accepts only the global `--config` option. All other runtime behavior is controlled by the configuration file and environment variables (§11). There are no CLI flags for individual config fields — the config file is the single source of truth (§1, design constraint). CLI flags for individual fields would create a third layer of precedence (file < env < flag) that makes it harder to reason about the running configuration.

**Startup output.** On successful startup, the runtime logs:

1. The resolved config file path (or "no config file, using defaults").
2. The effective listen addresses for all enabled endpoints.
3. Whether TLS is enabled.
4. The taxonomy file path and version (or "no taxonomy, base types only").
5. The data directory path.

These are logged at `info` level in the configured format (§2.9). They are the minimum an operator needs to verify the runtime started with the expected configuration.

### 3.4 Signal Handling

The runtime handles two Unix signals for lifecycle management.

**`SIGTERM` — graceful shutdown.** The runtime begins a coordinated drain:

1. Stop accepting new connections on both transport endpoints.
2. Signal all active workspace actors to begin graceful termination — each workspace receives a `GracefulTermination` command on its high-priority channel (runtime spec, §14). Workspaces in `Active` state are given a grace period (configurable via `resources.default_timeout_ms` or the workspace's individual timeout, whichever is shorter, with a floor of 5 seconds) to reach a checkpoint before being aborted.
3. Wait for all workspace actors to reach terminal states (`Closed` or `Failed`).
4. Take a final system snapshot (storage spec, §7).
5. Close the trail store, flushing any pending index writes.
6. Exit with code 0.

**`SIGINT` — immediate shutdown.** Same sequence as `SIGTERM`, but the grace period for workspace actors is zero — all active workspaces are immediately aborted (transitioned to `Failed` with `reason: runtime_shutdown`). Trail entries for the aborts are written if possible. This is the "operator pressed Ctrl-C" path — faster than graceful, but no workspace gets a chance to checkpoint.

**`SIGTERM` during graceful shutdown.** A second `SIGTERM` (or `SIGINT`) during an in-progress graceful shutdown escalates to immediate shutdown. This handles the case where a workspace is unresponsive and the graceful drain hangs.

**Implementation.** Signal handling uses `tokio::signal::unix::signal()`. The coordinator actor's select loop includes signal channels alongside its message channels:

```rust
loop {
    tokio::select! {
        biased;
        _ = sigterm_rx.recv() => self.begin_graceful_shutdown().await,
        _ = sigint_rx.recv() => self.begin_immediate_shutdown().await,
        Some(cmd) = internal_rx.recv() => self.handle_internal(cmd).await,
        // ... other channels
    }
}
```

The `biased` select ensures signals take priority over internal messages, matching the abort precedence invariant (runtime spec, §14, invariant 2).

### 3.5 Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Clean shutdown (graceful or immediate completed successfully) |
| 1 | Configuration error (parse failure, validation failure, taxonomy error) |
| 2 | Storage initialization error (data directory not writable, trail corruption detected) |
| 3 | Port binding error (listen address already in use) |
| 4 | TLS initialization error (certificate parse failure, key mismatch) |
| 101 | Internal error (coordinator actor panicked — should not happen under normal operation) |

Exit codes 1–4 are startup errors — the runtime failed to initialize and never began serving. Code 101 is a runtime error — the runtime was serving but encountered an unrecoverable failure. Code 0 is the only success code.

**Diagnostic output.** For exit codes 1–4, the runtime prints a structured error to stderr before exiting. The error includes the failing component, the specific error, and (where applicable) the config field or file path involved. This is printed directly to stderr regardless of the configured logging format — the logging subsystem may not be initialized when the error occurs.

### 3.6 Rust Implementation

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wacp-runtime", version, about = "WACP Runtime")]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the runtime (default)
    Serve,
    /// Parse and validate configuration, then exit
    Validate,
    /// Print the full default configuration to stdout
    Defaults,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => cmd_serve(cli.config),
        Command::Validate => cmd_validate(cli.config),
        Command::Defaults => cmd_defaults(),
    }
}
```

**`cmd_serve`.** Resolves the config file (§2, file location rules), parses and validates (§2.12), initializes logging (§6), runs the startup sequence (§12), and enters the coordinator actor's main loop. Returns the appropriate exit code on shutdown or initialization failure.

**`cmd_validate`.** Resolves the config file, parses, validates, and if `taxonomy.file` is set, runs the taxonomy loader's full validation (runtime spec, §11). Prints "configuration valid" on success. On failure, prints the diagnostic error. Does not touch storage, network, or any runtime subsystem.

**`cmd_defaults`.** Constructs a `RuntimeConfig::default()`, serializes it to YAML via `serde_yaml::to_string`, prints to stdout. No file access, no validation — purely a formatting operation.

---

## 4. TLS Configuration

TLS secures the two gRPC boundaries — agent-facing and highway-facing. Without TLS, authentication tokens travel in cleartext, voiding the security model's message integrity guarantees (§11.4 of the protocol). TLS is a deployment concern, not a protocol requirement — the runtime operates correctly without it — but production deployments must enable it.

This section defines how TLS is configured, how it integrates with the gRPC transport, and how it relates to application-level authentication.

### 4.1 Two Layers of Authentication

TLS and the `Bind`/`Authenticate` RPCs serve different purposes. Conflating them is a common deployment mistake.

| Layer | What it authenticates | When it happens | What it protects |
|-------|----------------------|-----------------|-----------------|
| TLS | The transport — "am I talking to the real server?" (and with mTLS: "is this client allowed to connect at all?") | Connection establishment, before any gRPC message | Confidentiality and integrity of all bytes on the wire |
| Application auth | The protocol identity — "which workspace does this agent control?" or "which human is this?" | First RPC after connection (`Bind` or `Authenticate`) | Protocol-level access control, trail attribution |

TLS without application auth: the connection is encrypted, but anyone with network access can bind to any workspace. mTLS limits which clients can connect, but does not associate them with protocol identities.

Application auth without TLS: the `Bind` token travels in cleartext. An attacker on the network can steal the token and impersonate the agent. The protocol's trust model is broken.

Production requires both layers.

### 4.2 Server-Side TLS

The minimum production TLS setup. The runtime presents a certificate; clients verify it.

**Certificate requirements:**

- **Format.** PEM-encoded. The cert file (`tls.cert_file`) contains the server certificate followed by any intermediate CA certificates, in chain order (leaf first). The key file (`tls.key_file`) contains the private key for the leaf certificate.
- **Key types.** ECDSA (P-256, P-384) or RSA (2048-bit minimum). ECDSA P-256 is preferred — faster handshakes, smaller certificates, adequate security.
- **Subject Alternative Name.** The certificate must include a SAN matching the hostname clients use to connect. Clients that verify the server certificate (all well-configured gRPC clients do) will reject a certificate whose SAN does not match the connection target.
- **Validity.** The runtime does not check certificate expiry at startup — the TLS library (`rustls`) handles this at handshake time. An expired certificate causes connection failures, not a startup error. Operators are responsible for rotation before expiry.

**Integration with tonic.** The runtime uses `tonic` for gRPC, which uses `rustls` as its TLS backend (not OpenSSL). The TLS configuration is applied identically to both the agent and highway gRPC servers — they share a single `rustls::ServerConfig`. Both endpoints use the same certificate because the runtime presents one identity.

```rust
fn build_tls_config(tls: &TlsConfig) -> Result<ServerTlsConfig, TlsError> {
    let cert_pem = std::fs::read(&tls.cert_file)?;
    let key_pem = std::fs::read(&tls.key_file)?;

    let identity = Identity::from_pem(cert_pem, key_pem);

    let mut config = ServerTlsConfig::new().identity(identity);

    if !tls.client_ca_file.is_empty() {
        let ca_pem = std::fs::read(&tls.client_ca_file)?;
        let ca = Certificate::from_pem(ca_pem);
        config = config.client_ca_root(ca);
    }

    Ok(config)
}
```

The `ServerTlsConfig` is passed to `tonic::transport::Server::tls_config()`. Tonic applies it to every incoming connection on both endpoints.

**Minimum TLS version.** Configured via `tls.min_version` (default: `"1.2"`). TLS 1.0 and 1.1 are never accepted — `rustls` does not support them. The choice is between 1.2 (broader client compatibility) and 1.3 (stronger security, fewer round-trips). The runtime maps the config value to `rustls` protocol versions:

| Config value | Accepted versions |
|-------------|-------------------|
| `"1.2"` | TLS 1.2, TLS 1.3 |
| `"1.3"` | TLS 1.3 only |

### 4.3 Mutual TLS (mTLS)

When `tls.client_ca_file` is set, the runtime requires connecting clients to present a certificate signed by the specified CA. This is mutual TLS — both sides authenticate the transport.

**When to use mTLS.** mTLS is appropriate when the set of machines allowed to connect is known and each has a unique certificate. This is common in internal infrastructure where a private CA issues certs to all services. mTLS provides a second layer of defense: even if an attacker obtains a valid `Bind` token, they cannot connect without a client certificate signed by the trusted CA.

**When mTLS is unnecessary.** Single-machine deployments where agents and the runtime run on the same host. The transport never leaves the machine — network interception is not a concern. Development environments. Deployments behind a TLS-terminating reverse proxy that handles client certificate verification upstream.

**Client certificate identity.** The runtime does not extract identity information from the client certificate. The certificate authenticates the transport ("this client is allowed to connect"), not the protocol identity ("this client controls workspace X"). Protocol identity comes from the `Bind`/`Authenticate` RPC. This separation is deliberate — coupling certificate identity to workspace identity would require issuing a new certificate per workspace, which is operationally expensive and unnecessary.

**CA file format.** The CA file contains one or more PEM-encoded root CA certificates. Intermediate CAs are not placed here — they belong in the client's certificate chain. The runtime loads the CA file at startup and builds a `rustls` `RootCertStore`. Any client certificate that chains to one of these roots is accepted at the transport level.

### 4.4 Plaintext Mode

When `tls.enabled` is false (the default), both gRPC endpoints serve plaintext HTTP/2. This is development mode — no encryption, no certificate management, no handshake overhead. The runtime logs a warning at `warn` level on startup when TLS is disabled:

```
TLS disabled — gRPC endpoints serving plaintext. Do not use in production.
```

This warning is unconditional (cannot be suppressed by log level) because running without TLS in production is a security violation that should be visible in any log configuration.

### 4.5 Certificate Rotation

Certificates are loaded once at startup. There is no certificate watcher, no file-change listener, no graceful reload of TLS state. Rotating certificates requires a restart.

**Why no hot reload.** The runtime is a trust root (§1). Hot-reloading TLS configuration introduces a window where the runtime may serve with partially loaded state — a new cert with the old key, or vice versa. The failure modes are subtle (connections fail intermittently during reload, race conditions between listener threads) and difficult to test exhaustively. A restart is atomic — the new configuration takes effect all at once.

**Rotation procedure:**

1. Place the new certificate and key files at the configured paths (or at new paths).
2. If using new paths, update the config file.
3. Send `SIGTERM` to the running runtime.
4. After graceful shutdown completes, start the runtime.

The graceful shutdown (§3.4) drains active workspaces before stopping. The restart loads the new certificate. The gap between shutdown and restart is the rotation window — during this window, no connections are accepted. For zero-downtime rotation, deploy behind a load balancer that drains connections from the old instance while the new instance starts.

**Certificate monitoring.** The runtime does not monitor certificate expiry. External monitoring (a cron job, a Prometheus alert on `tls_certificate_expiry_seconds`, or a cert-manager) is the operator's responsibility. The metrics endpoint (§7) exposes the certificate's `not_after` timestamp as a gauge for this purpose.

### 4.6 Cipher Suites

The runtime does not expose cipher suite configuration. `rustls` selects a safe default set:

- TLS 1.3: `TLS_AES_256_GCM_SHA384`, `TLS_AES_128_GCM_SHA256`, `TLS_CHACHA20_POLY1305_SHA256`
- TLS 1.2: `TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384`, `TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256`, `TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384`, `TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256`, `TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256`, `TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256`

All suites use AEAD ciphers with forward secrecy. CBC-mode ciphers, static RSA key exchange, and export-grade cryptography are never offered. Exposing cipher suite configuration invites misconfiguration without a meaningful benefit — the `rustls` defaults are secure, well-maintained, and updated with the library.

---

## 5. Authenticator Providers

The runtime requires authenticated identity before any protocol action (§11.3 of the protocol). Authentication is pluggable — the `Authenticator` trait (protocol-interface spec, §7) defines the contract, and the deployment configuration selects which provider handles it. This section defines the two built-in providers and the integration mechanics.

### 5.1 The Authenticator Trait

The trait is defined in the `wacp-transport` crate (protocol-interface spec, §7). Both providers implement it. The runtime holds one `Arc<dyn Authenticator>`, selected at startup based on `auth.provider` (§2.4).

```rust
pub trait Authenticator: Send + Sync {
    fn authenticate_agent(
        &self,
        token: &str,
        workspace_id: &WorkspaceId,
    ) -> Result<AgentIdentity, AuthError>;

    fn authenticate_human(
        &self,
        token: &str,
    ) -> Result<UserId, AuthError>;
}

pub struct AgentIdentity {
    pub workspace_id: WorkspaceId,
    pub role: RoleName,
}

pub enum AuthError {
    InvalidToken,
    WorkspaceMismatch,     // token valid but not for this workspace
    ProviderUnavailable,   // external provider unreachable
    RateLimited,           // source IP exceeded failure threshold
}
```

**Call sites.** `authenticate_agent` is called by the transport actor when processing a `Bind` RPC. `authenticate_human` is called when processing an `Authenticate` RPC on the highway service. Both calls happen before any protocol state is touched — a failed authentication produces a trail entry and a gRPC `UNAUTHENTICATED` status, nothing else.

### 5.2 Pre-Shared Key Provider

Selected by `auth.provider: "psk"`. The default for single-machine deployments where the runtime launches its own agents.

**How it works.** The PSK provider maintains an in-memory table mapping tokens to agent identities. The runtime generates a unique token for each workspace at creation time and registers it in this table. The token is delivered to the agent through the launch mechanism — passed as an environment variable, a file, or a command-line argument, depending on how agents are spawned.

**Token generation.** Each token is a 256-bit random value encoded as a 43-character base64url string (no padding). Generated using `ring::rand::SystemRandom` — cryptographically secure, not guessable. The token is unique per workspace and valid for the lifetime of that workspace.

```rust
pub struct PskAuthenticator {
    agent_tokens: RwLock<HashMap<String, AgentIdentity>>,
    human_tokens: RwLock<HashMap<String, UserId>>,
}

impl PskAuthenticator {
    pub fn register_agent(
        &self,
        workspace_id: WorkspaceId,
        role: RoleName,
    ) -> String {
        let token = generate_random_token();
        let identity = AgentIdentity { workspace_id: workspace_id.clone(), role };
        self.agent_tokens.write().insert(token.clone(), identity);
        token
    }

    pub fn register_human(&self, user_id: UserId) -> String {
        let token = generate_random_token();
        self.human_tokens.write().insert(token.clone(), user_id);
        token
    }

    pub fn revoke_agent(&self, workspace_id: &WorkspaceId) {
        self.agent_tokens.write().retain(|_, id| &id.workspace_id != workspace_id);
    }
}
```

**Agent authentication flow:**

1. Coordinator creates a workspace, calls `psk.register_agent(workspace_id, role)`. Receives a token.
2. Coordinator launches the agent process, passing the token (e.g., `WACP_AUTH_TOKEN=<token>`).
3. Agent connects to the agent gRPC endpoint, calls `Bind` with the token and workspace id.
4. Transport actor calls `psk.authenticate_agent(token, workspace_id)`. The provider looks up the token, verifies the workspace id matches, returns the `AgentIdentity`.
5. On success, the connection is bound to the workspace. On failure, `UNAUTHENTICATED`.

**Human authentication.** For the PSK provider, human tokens are pre-registered — the operator provisions them before the runtime starts (via a startup hook or an init script that calls `register_human`). This is adequate for development and single-operator deployments. Multi-user production deployments should use the external provider.

**Workspace termination.** When a workspace reaches a terminal state (`Closed` or `Failed`), the coordinator calls `psk.revoke_agent(workspace_id)`. The token is removed from the table. Subsequent authentication attempts with that token fail with `InvalidToken`. This prevents a terminated workspace's token from being reused.

**Persistence.** The PSK token table is not persisted to disk. On restart, all tokens are invalidated. Agents must re-register. This is acceptable because the coordinator re-creates workspaces during recovery (runtime spec, §13) and generates fresh tokens. The PSK provider is a runtime-internal component, not a durable store.

### 5.3 External Provider

Selected by `auth.provider: "external"`. For deployments where identity is managed by an existing system — OAuth, OIDC, API keys, or a custom auth service.

**HTTP contract.** The runtime delegates authentication to an external HTTP service. On each `Bind` or `Authenticate` RPC, the runtime sends a POST request to `auth.external.url`.

**Agent authentication request:**

```json
POST <auth.external.url>
Content-Type: application/json

{
    "type": "agent",
    "token": "<token from Bind RPC>",
    "workspace_id": "<workspace_id from Bind RPC>"
}
```

**Human authentication request:**

```json
POST <auth.external.url>
Content-Type: application/json

{
    "type": "human",
    "token": "<token from Authenticate RPC>"
}
```

**Success response (HTTP 200):**

```json
{
    "identity": "<workspace_id or user_id>",
    "role": "<role_name>"              // required for agents, ignored for humans
}
```

**Failure response (HTTP 401 or 403):**

```json
{
    "error": "<machine-readable reason>",
    "message": "<human-readable explanation>"
}
```

**Any other HTTP status** (including timeouts, 5xx, connection errors) is treated as `AuthError::ProviderUnavailable`. The runtime does not retry — a single failed attempt produces an `UNAUTHENTICATED` response to the client. Retrying authentication on transient errors is dangerous: the client is unauthenticated, and holding the connection open while retrying consumes resources. The client should retry the `Bind` or `Authenticate` call itself.

**Timeout.** Configured via `auth.external.timeout_ms` (default: 5000ms). The runtime uses `reqwest` with this timeout as the total request duration (connection + response). A timeout is treated the same as a connection error — `ProviderUnavailable`.

**Security of the callback.** The external URL should use HTTPS in production — the authentication token is sent in the request body. The runtime does not enforce HTTPS for the auth callback (the external service may be on the same host or behind a service mesh that handles TLS), but an `http://` URL triggers a `warn`-level log at startup:

```
External auth URL uses plaintext HTTP — auth tokens will be sent unencrypted to <url>.
```

**No caching.** The external provider does not cache authentication results. Every `Bind` and `Authenticate` RPC produces a fresh HTTP call to the external service. Caching would introduce a window where a revoked token still works — the TTL would determine how long a revoked user can act. The runtime prioritizes correctness over latency. If the external service is slow, the auth latency is the operator's problem to solve at the external service level.

**Implementation:**

```rust
pub struct ExternalAuthenticator {
    client: reqwest::Client,
    url: String,
}

impl Authenticator for ExternalAuthenticator {
    fn authenticate_agent(
        &self,
        token: &str,
        workspace_id: &WorkspaceId,
    ) -> Result<AgentIdentity, AuthError> {
        let resp = self.client
            .post(&self.url)
            .json(&serde_json::json!({
                "type": "agent",
                "token": token,
                "workspace_id": workspace_id.as_str(),
            }))
            .send()
            .map_err(|_| AuthError::ProviderUnavailable)?;

        match resp.status() {
            StatusCode::OK => {
                let body: ExternalAuthResponse = resp.json()
                    .map_err(|_| AuthError::ProviderUnavailable)?;
                Ok(AgentIdentity {
                    workspace_id: workspace_id.clone(),
                    role: RoleName::from(body.role),
                })
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Err(AuthError::InvalidToken)
            }
            _ => Err(AuthError::ProviderUnavailable),
        }
    }

    fn authenticate_human(
        &self,
        token: &str,
    ) -> Result<UserId, AuthError> {
        let resp = self.client
            .post(&self.url)
            .json(&serde_json::json!({
                "type": "human",
                "token": token,
            }))
            .send()
            .map_err(|_| AuthError::ProviderUnavailable)?;

        match resp.status() {
            StatusCode::OK => {
                let body: ExternalAuthResponse = resp.json()
                    .map_err(|_| AuthError::ProviderUnavailable)?;
                Ok(UserId::from(body.identity))
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Err(AuthError::InvalidToken)
            }
            _ => Err(AuthError::ProviderUnavailable),
        }
    }
}
```

The `reqwest::Client` is constructed once at startup with the configured timeout and connection pooling enabled. It is reused for all authentication calls.

### 5.4 Rate Limiting

Rate limiting applies to both providers. It is implemented in the transport actor, upstream of the authenticator — the rate limit check happens before the token reaches the provider.

**Mechanism.** A sliding-window counter per source IP address. Each failed authentication increments the counter for the source IP. When the counter for an IP exceeds `auth.rate_limit.max_failures` within the most recent `auth.rate_limit.window_seconds`, subsequent attempts from that IP are immediately rejected with `AuthError::RateLimited` — the authenticator is not consulted.

**Implementation.** The rate limiter holds a `HashMap<IpAddr, VecDeque<Instant>>`. Each entry is a list of failure timestamps. On each failed auth, the current timestamp is appended. On each check, timestamps older than `window_seconds` are drained from the front. If the remaining count exceeds `max_failures`, the IP is rate-limited.

```rust
pub struct AuthRateLimiter {
    failures: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    max_failures: u32,
    window: Duration,
}

impl AuthRateLimiter {
    pub fn check(&self, ip: &IpAddr) -> Result<(), AuthError> {
        let mut map = self.failures.lock();
        if let Some(timestamps) = map.get_mut(ip) {
            let cutoff = Instant::now() - self.window;
            while timestamps.front().is_some_and(|t| *t < cutoff) {
                timestamps.pop_front();
            }
            if timestamps.len() >= self.max_failures as usize {
                return Err(AuthError::RateLimited);
            }
        }
        Ok(())
    }

    pub fn record_failure(&self, ip: &IpAddr) {
        let mut map = self.failures.lock();
        map.entry(*ip).or_default().push_back(Instant::now());
    }
}
```

**Memory bounds.** The failure map grows with the number of distinct IPs that fail authentication. In pathological cases (distributed brute-force from many IPs), this could consume unbounded memory. The limiter caps the map at 10,000 entries — when full, the oldest entry (by most recent failure) is evicted. This bounds memory at roughly 10,000 × (16 bytes IP + deque overhead) — a few hundred kilobytes. The cap is not configurable — it is a safety valve, not a tunable.

**Trail recording.** Rate-limited rejections produce `authentication_rate_limited` trail entries with the source IP and the current failure count. The token is not recorded — rate limiting fires before the token is examined, so the token may or may not be valid.

**Disabling.** Setting `auth.rate_limit.max_failures: 0` disables rate limiting entirely. The rate limiter is not instantiated. This is appropriate only for development or deployments behind an external rate limiter (API gateway, reverse proxy).

### 5.5 Authentication Trail Events

Every authentication attempt — success or failure — is recorded in the trail. These entries exist outside the workspace scope (workspace_id is empty on the trail entry) because authentication happens before workspace binding.

| Event type | When | Fields |
|-----------|------|--------|
| `authentication_success` | Agent `Bind` or human `Authenticate` succeeds | `identity`, `provider` (`psk` or `external`), `source_ip`, `workspace_id` (for agents) |
| `authentication_failed` | Token rejected by provider | `provider`, `source_ip`, `workspace_id` (for agents, if provided), `reason` (`invalid_token`, `workspace_mismatch`, `provider_unavailable`) |
| `authentication_rate_limited` | Source IP exceeded failure threshold | `source_ip`, `failure_count`, `window_seconds` |

**Token redaction.** Authentication tokens are never recorded in the trail. A leaked token in the trail would be a security vulnerability — anyone with trail read access could impersonate the token holder. The trail records the outcome and metadata, not the credential.

---

## 6. Structured Logging

Logging is the operator's view into the runtime — distinct from the trail, which is the protocol's audit record. The trail captures every protocol event with formal structure and hash-chain integrity. Logs capture operational detail: startup progress, configuration decisions, connection lifecycle, performance observations, and infrastructure errors that may not reach the trail.

The runtime uses the `tracing` crate for structured, span-aware logging. Configuration is defined in §2.9.

### 6.1 Logging vs Trail

The distinction is load-bearing. Blurring it leads to either a noisy trail or insufficient operational visibility.

| | Trail | Logs |
|---|---|---|
| **Audience** | Protocol participants, auditors, recovery engine | Operators, monitoring systems, on-call engineers |
| **Content** | Protocol events (state transitions, envelope delivery, permission decisions) | Operational events (startup, shutdown, connection management, performance, errors) |
| **Durability** | Fsync-durable, hash-chained, tamper-evident | Best-effort, may be lost on crash |
| **Format** | Binary trail entries with typed fields | Structured text (JSON or pretty-printed) |
| **Queryable** | Via trail index (SQL), by workspace/time/event type | Via log aggregator (grep, Loki, Elasticsearch) |
| **Lifecycle** | Retained per tiered storage policy (§2.6) | Retained per operator's log rotation policy |

**Rule: the trail never duplicates into logs, and logs never substitute for trail entries.** A permission denial is a trail entry (`permission_denied`). The log may note "permission check completed in 2.3ms" but does not repeat the permission decision. A connection drop is a log event. If the connection drop causes a workspace state change, that state change is a trail entry — the log does not record the state change, only the connection event that caused it.

### 6.2 Tracing Integration

The runtime initializes a global `tracing` subscriber at startup, before any other subsystem. This is the first action in `cmd_serve` (§3.6) — if logging initialization fails, the runtime exits with code 1 and a plaintext error to stderr.

**Subscriber stack.** The subscriber is built from `tracing_subscriber::fmt`:

```rust
fn init_logging(config: &LoggingConfig) -> Result<(), LoggingError> {
    let env_filter = EnvFilter::try_new(&config.level)
        .map_err(LoggingError::InvalidLevel)?;

    let builder = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false);

    match (config.format.as_str(), config.output.as_str()) {
        ("json", "stderr") => {
            builder.json().with_writer(std::io::stderr).init();
        }
        ("json", "file") => {
            let file = open_log_file(&config.file)?;
            builder.json().with_writer(Mutex::new(file)).init();
        }
        ("pretty", "stderr") => {
            builder.pretty().with_writer(std::io::stderr).init();
        }
        ("pretty", "file") => {
            let file = open_log_file(&config.file)?;
            builder.pretty().with_ansi(false).with_writer(Mutex::new(file)).init();
        }
        _ => unreachable!(), // validated in §2.12
    }

    Ok(())
}
```

**`with_target(true)`.** Includes the Rust module path (`wacp_coordinator::orchestration`, `wacp_transport::grpc`) in every log line. This is essential for filtering — an operator debugging transport issues can filter to `wacp_transport::*` without wading through coordinator logic.

**No file/line numbers.** These change with every code edit, making log-based alerts brittle. The target (module path) is stable across minor code changes.

**`RUST_LOG` override.** The `EnvFilter` respects the `RUST_LOG` environment variable if set, overriding `logging.level` from the config file. This enables per-module log levels without changing the config:

```bash
RUST_LOG=wacp_transport=debug,wacp_trail=trace wacp-runtime serve
```

This is consistent with §11 (environment variable overrides) — env vars take precedence over config file values.

### 6.3 JSON Format

The production format. One JSON object per log line, newline-delimited. Every field is a flat key — no nested objects, because flat keys are easier to index and query in log aggregators.

```json
{"timestamp":"2026-03-21T14:32:01.847Z","level":"INFO","target":"wacp_runtime","message":"runtime started","agent_listen":"0.0.0.0:9090","highway_listen":"0.0.0.0:9091","tls":true,"taxonomy":"production.yaml"}
```

**Standard fields (present on every line):**

| Field | Type | Source |
|-------|------|--------|
| `timestamp` | string (ISO 8601, UTC) | `tracing_subscriber` clock |
| `level` | string (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`) | `tracing` event level |
| `target` | string | Rust module path |
| `message` | string | The log message |

**Structured fields.** Each `tracing` event may carry additional key-value pairs. These appear as top-level keys in the JSON output. The runtime uses structured fields consistently:

| Context | Structured fields |
|---------|-------------------|
| Connection events | `source_ip`, `workspace_id`, `provider` |
| gRPC events | `method`, `status_code`, `duration_ms` |
| Storage events | `segment_id`, `bytes_written`, `fsync_duration_ms` |
| Resource events | `workspace_id`, `dimension`, `current`, `limit` |
| Lifecycle events | `workspace_id`, `from_state`, `to_state` |

### 6.4 Spans

`tracing` spans provide context propagation — a request's journey through the runtime can be traced by following its span hierarchy. The runtime creates spans at three granularities:

**Request span.** Created by the transport actor for each incoming gRPC call. Carries `method` (RPC name), `workspace_id` (after binding), and `request_id` (from `client_request_id` if provided). All log events produced while processing this request inherit these fields.

```rust
#[tracing::instrument(
    skip(self, request),
    fields(method = "SendEnvelope", workspace_id, request_id)
)]
async fn handle_send_envelope(&self, request: SendEnvelopeRequest) -> Result<...> {
    tracing::Span::current()
        .record("workspace_id", &self.workspace_id.as_str())
        .record("request_id", &request.client_request_id.as_str());
    // ...
}
```

**Workspace span.** Created by each workspace actor at initialization. Carries `workspace_id` and `role`. All log events from workspace message processing inherit these fields. Nested inside the request span when processing a request.

**Coordinator span.** Created by the coordinator actor. Carries no workspace-specific fields — it handles all workspaces. Individual orchestration decisions create child spans with the relevant workspace id.

### 6.5 Log Levels

Each level has a defined purpose. The runtime follows these consistently — not as guidelines, but as rules.

| Level | Purpose | Examples |
|-------|---------|---------|
| `ERROR` | Infrastructure failures that affect correctness or availability. Every `ERROR` is actionable — something is broken. | Trail write failure, coordinator actor panic, TLS certificate parse failure |
| `WARN` | Conditions that are not failures but require operator attention. | TLS disabled, plaintext auth URL, certificate expiring within 7 days, authentication rate limiting active |
| `INFO` | Lifecycle events and operational milestones. The "what happened" level. | Runtime started/stopped, workspace created/terminated, connection opened/closed, snapshot taken, tier transition |
| `DEBUG` | Internal decisions and data flow. Useful for diagnosing behavior. | Permission check detail, envelope routing, FSM transition detail, delivery retry |
| `TRACE` | High-volume instrumentation. Performance-sensitive — may affect throughput. | Every trail write with timing, every channel send/recv, HLC tick, index batch flush |

**The `INFO` test.** If an operator runs the runtime at `INFO` level and something goes wrong, do the logs contain enough context to identify the problem? If not, the relevant event should be `INFO`, not `DEBUG`. In practice, this means `INFO` produces roughly 1–10 log lines per protocol operation — workspace lifecycle events, connection events, snapshot events. Not individual envelope deliveries (that is `DEBUG`).

### 6.6 Startup Log Sequence

On successful startup, the runtime logs the following events at `INFO` level, in order. This sequence is the operator's confirmation that the runtime initialized correctly.

```
INFO runtime starting                     version=0.1.0 protocol=wacp-v0.1 commit=abc1234
INFO configuration loaded                 source="/etc/wacp/runtime.yaml"
INFO logging initialized                  level=info format=json output=stderr
INFO taxonomy loaded                      file="production.yaml" roles=5 envelope_types=3 checkpoint_types=2
INFO storage initialized                  data_dir="/var/lib/wacp" trail_segments=12 checkpoints=847
INFO recovery completed                   workspaces_recovered=3 trail_entries_replayed=1204 duration_ms=340
INFO agent endpoint listening             address=0.0.0.0:9090 tls=true
INFO highway endpoint listening           address=0.0.0.0:9091 tls=true
INFO metrics endpoint listening           address=0.0.0.0:9092 path=/metrics
INFO health endpoint listening            address=0.0.0.0:9093 path=/healthz
INFO runtime ready                        uptime_ms=412
```

The `runtime ready` line is the signal that the runtime is fully operational — all subsystems initialized, recovery complete, all endpoints accepting connections. Orchestration tools (systemd, Docker health checks, Kubernetes readiness probes) can key on this log line or on the health endpoint (§8).

---

## 7. Metrics Endpoint

The metrics endpoint serves Prometheus-compatible metrics over HTTP. It exposes the runtime's operational state as time-series data — connection counts, request latencies, trail throughput, resource consumption, storage utilization. These are the numbers an operator monitors in steady state and investigates during incidents.

Metrics are orthogonal to both the trail and logs. The trail records what happened (protocol events). Logs record why it happened (operational context). Metrics record how much and how fast (quantitative state over time).

Configuration is defined in §2.10. When `observability.metrics.enabled` is true, the runtime starts a lightweight HTTP server on `observability.metrics.listen` serving the configured path (default: `/metrics`).

### 7.1 Implementation

The runtime uses the `prometheus` crate for metric registration and exposition, and `hyper` for the HTTP server. Metrics are registered at initialization — no dynamic metric creation at runtime.

```rust
fn start_metrics_server(config: &MetricsConfig) -> Result<(), MetricsError> {
    let addr: SocketAddr = config.listen.parse()?;
    let path = config.path.clone();

    tokio::spawn(async move {
        let make_svc = hyper::service::make_service_fn(move |_| {
            let path = path.clone();
            async move {
                Ok::<_, hyper::Error>(hyper::service::service_fn(move |req| {
                    let path = path.clone();
                    async move { handle_metrics(req, &path) }
                }))
            }
        });
        hyper::Server::bind(&addr).serve(make_svc).await
    });

    Ok(())
}

fn handle_metrics(
    req: hyper::Request<hyper::Body>,
    path: &str,
) -> Result<hyper::Response<hyper::Body>, hyper::Error> {
    if req.uri().path() != path {
        return Ok(hyper::Response::builder()
            .status(404)
            .body(hyper::Body::empty())
            .unwrap());
    }

    let encoder = prometheus::TextEncoder::new();
    let metrics = prometheus::gather();
    let body = encoder.encode_to_string(&metrics).unwrap();

    Ok(hyper::Response::builder()
        .header("Content-Type", encoder.format_type())
        .body(hyper::Body::from(body))
        .unwrap())
}
```

The metrics server is a standalone `tokio` task — it does not share the gRPC transport or its TLS configuration. It serves plaintext HTTP. This is deliberate: metrics scrapers (Prometheus, Datadog agent, Grafana Agent) typically do not support mTLS, and the metrics endpoint exposes operational data, not protocol data. If network isolation is required, bind the metrics endpoint to a management interface or localhost.

### 7.2 Metric Categories

Metrics are organized into five categories, each with a consistent naming prefix. All metric names follow the Prometheus naming convention: `wacp_<category>_<metric>_<unit>`.

#### gRPC Metrics

Request-level instrumentation for both agent and highway services.

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `wacp_grpc_requests_total` | counter | `service` (`agent`, `highway`), `method`, `status` | Total RPC count by method and gRPC status code |
| `wacp_grpc_request_duration_seconds` | histogram | `service`, `method` | RPC latency distribution |
| `wacp_grpc_active_connections` | gauge | `service` | Currently open gRPC connections |
| `wacp_grpc_stream_messages_total` | counter | `service`, `method`, `direction` (`sent`, `received`) | Messages on streaming RPCs |

**Histogram buckets for `request_duration_seconds`.** `[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0]` — covering the range from sub-millisecond local operations to multi-second external auth calls. The buckets are fixed at compile time.

#### Workspace Metrics

Workspace lifecycle and activity.

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `wacp_workspaces_active` | gauge | — | Currently active workspace actors |
| `wacp_workspaces_total` | counter | `terminal_state` (`closed`, `failed`) | Workspaces that have reached terminal states |
| `wacp_workspace_state_transitions_total` | counter | `from`, `to` | State machine transitions |
| `wacp_workspace_lifetime_seconds` | histogram | `terminal_state` | Duration from creation to terminal state |

#### Trail Metrics

Trail write path performance and storage utilization.

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `wacp_trail_writes_total` | counter | — | Total trail entries written |
| `wacp_trail_write_duration_seconds` | histogram | — | Time per trail write (including fsync) |
| `wacp_trail_fsync_duration_seconds` | histogram | — | Fsync latency alone (the durability bottleneck) |
| `wacp_trail_bytes_total` | counter | — | Total bytes written to trail segments |
| `wacp_trail_segments_total` | gauge | `tier` (`hot`, `warm`, `cold`) | Segment count per storage tier |
| `wacp_trail_index_lag_entries` | gauge | — | Entries written but not yet indexed |

**`trail_fsync_duration_seconds`** is the single most important performance metric. Every protocol operation blocks on this fsync. If this histogram shifts right, the runtime's throughput ceiling drops. The operator should alert on P99 exceeding 10ms on SSD or 50ms on HDD.

#### Envelope Metrics

Delivery pipeline throughput.

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `wacp_envelopes_total` | counter | `origin` (`agent`, `human`), `state` (`created`, `delivered`, `rejected`) | Envelope lifecycle counts |
| `wacp_envelope_delivery_duration_seconds` | histogram | — | Time from creation to delivery |
| `wacp_envelope_retries_total` | counter | — | Total delivery retry attempts |
| `wacp_envelope_payload_bytes` | histogram | `origin` | Envelope payload size distribution |

#### Resource Metrics

Per-workspace resource consumption, aggregated.

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `wacp_resource_warnings_total` | counter | `dimension` | Budget warning events by dimension |
| `wacp_resource_exhaustions_total` | counter | `dimension` | Budget exhaustion events (workspace failures) |
| `wacp_resource_tokens_total` | counter | — | Cumulative tokens consumed across all workspaces |
| `wacp_resource_cost_micros_total` | counter | — | Cumulative cost across all workspaces |

#### Authentication Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `wacp_auth_attempts_total` | counter | `provider`, `type` (`agent`, `human`), `result` (`success`, `failed`, `rate_limited`) | Authentication attempts |
| `wacp_auth_external_duration_seconds` | histogram | — | External auth provider call latency |

#### Infrastructure Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `wacp_tls_certificate_expiry_seconds` | gauge | — | Seconds until server certificate expires (§4.5) |
| `wacp_checkpoint_store_bytes` | gauge | — | Total bytes in checkpoint store |
| `wacp_snapshot_last_success_timestamp` | gauge | — | Unix timestamp of last successful system snapshot |
| `wacp_recovery_duration_seconds` | gauge | — | Duration of most recent recovery (set once at startup) |
| `wacp_uptime_seconds` | gauge | — | Seconds since runtime started |

### 7.3 Metric Hygiene

**No per-workspace label cardinality.** Metrics do not carry `workspace_id` as a label. A long-running system may create thousands of workspaces — each would produce a unique time series, causing cardinality explosion in Prometheus. Per-workspace data is available in the trail (queryable by workspace id). Metrics provide aggregate views.

**No token or identity values in labels.** Labels contain categories (`agent`/`human`, `closed`/`failed`), not identifiers. This prevents sensitive data from appearing in the metrics endpoint.

**Metric registration is static.** All metrics are registered at initialization via `prometheus::register_*`. No metric is created dynamically during operation. This makes the metric set predictable — a Grafana dashboard built against one version of the runtime works against all instances of that version.

---

## 8. Health Checks

Health checks answer "is this runtime instance able to serve?" — a binary yes/no that orchestrators (systemd, Docker, Kubernetes) use to decide whether to route traffic, restart the process, or hold off during startup. The runtime exposes both an HTTP health endpoint and a gRPC health service.

Configuration is defined in §2.10. The HTTP endpoint is enabled by default (`observability.health.enabled: true`).

### 8.1 Three Health States

The runtime distinguishes three internal states. These are not protocol states — they are operational states of the process itself.

| State | Meaning | HTTP response | gRPC response |
|-------|---------|---------------|---------------|
| **Starting** | Initialization in progress — config loaded, recovery running, endpoints not yet ready | 503 Service Unavailable | `NOT_SERVING` |
| **Ready** | All subsystems initialized, recovery complete, all endpoints accepting connections | 200 OK | `SERVING` |
| **Draining** | Graceful shutdown in progress — no new connections accepted, active workspaces draining | 503 Service Unavailable | `NOT_SERVING` |

The state transitions are: `Starting → Ready → Draining`. No backward transitions. The runtime enters `Ready` exactly once and leaves it at most once.

### 8.2 Health Evaluation

The `Ready` state is not just "the process is alive." The health check verifies that the runtime's critical subsystems are functional. The check runs on every health request — it is not cached.

**Checks performed:**

1. **Trail store writable.** The trail writer must be operational — not in degraded mode (runtime spec, §6). If trail writes are failing persistently, the runtime cannot guarantee protocol correctness. This check reads a flag set by the trail writer; it does not perform a test write.

2. **Coordinator actor alive.** The coordinator's message channel must be open. If the coordinator has panicked (exit code 101 territory), the runtime cannot orchestrate workspaces. The check sends a lightweight ping on the coordinator's channel and expects a pong within 100ms.

3. **Transport actors alive.** Both transport actors (agent-facing, highway-facing) must have open channels. If a transport actor has crashed, the corresponding boundary is unreachable.

If all three pass, the response is `Ready`. If any fails, the response is `Starting` (during initialization) or the runtime should be restarted (post-initialization failure — a transport or coordinator crash is unrecoverable without restart).

```rust
pub struct HealthChecker {
    state: AtomicU8,  // 0 = Starting, 1 = Ready, 2 = Draining
    trail_healthy: Arc<AtomicBool>,
    coordinator_tx: mpsc::Sender<CoordinatorMsg>,
    agent_transport_tx: mpsc::Sender<TransportMsg>,
    highway_transport_tx: mpsc::Sender<TransportMsg>,
}

impl HealthChecker {
    pub async fn check(&self) -> HealthStatus {
        let state = self.state.load(Ordering::Relaxed);
        if state != 1 {
            return HealthStatus::NotReady;
        }

        if !self.trail_healthy.load(Ordering::Relaxed) {
            return HealthStatus::NotReady;
        }

        // Ping coordinator — if the channel is closed or the pong
        // doesn't arrive within 100ms, the coordinator is dead.
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.coordinator_tx.send(CoordinatorMsg::HealthPing(reply_tx)).await.is_err() {
            return HealthStatus::NotReady;
        }
        if tokio::time::timeout(Duration::from_millis(100), reply_rx).await.is_err() {
            return HealthStatus::NotReady;
        }

        HealthStatus::Ready
    }
}
```

### 8.3 HTTP Health Endpoint

A minimal HTTP server, same architecture as the metrics server (§7.1) — standalone `hyper` task, plaintext, no TLS.

**Response format.** The endpoint returns a JSON body with the health state and subsystem details. The HTTP status code carries the primary signal (200 vs 503) — the body is supplementary.

**Healthy response (200):**

```json
{
    "status": "ready",
    "checks": {
        "trail": "ok",
        "coordinator": "ok",
        "transport_agent": "ok",
        "transport_highway": "ok"
    },
    "uptime_seconds": 3847
}
```

**Unhealthy response (503):**

```json
{
    "status": "starting",
    "checks": {
        "trail": "ok",
        "coordinator": "pending",
        "transport_agent": "pending",
        "transport_highway": "pending"
    },
    "uptime_seconds": 2
}
```

**No authentication on the health endpoint.** The health endpoint serves operational status, not protocol data. Requiring authentication would prevent orchestrators from checking health without credentials. The response contains no sensitive information — subsystem names and ok/pending/failed states.

### 8.4 gRPC Health Service

The runtime implements the standard gRPC health checking protocol (`grpc.health.v1.Health`) on both the agent and highway gRPC endpoints. This enables gRPC-native health checks from load balancers and service meshes that speak gRPC rather than HTTP.

```protobuf
// Standard gRPC health check (grpc.health.v1)
service Health {
    rpc Check(HealthCheckRequest) returns (HealthCheckResponse);
    rpc Watch(HealthCheckRequest) returns (stream HealthCheckResponse);
}

message HealthCheckRequest {
    string service = 1;  // empty = overall, "agent" or "highway" = specific
}

message HealthCheckResponse {
    enum ServingStatus {
        UNKNOWN = 0;
        SERVING = 1;
        NOT_SERVING = 2;
    }
    ServingStatus status = 1;
}
```

**Service names.** Three service names are registered:

| Service name | What it checks |
|-------------|----------------|
| `""` (empty) | Overall runtime health — same as the HTTP endpoint |
| `"wacp.v1.AgentService"` | Agent transport specifically |
| `"wacp.v1.HighwayService"` | Highway transport specifically |

The `Watch` RPC streams health status changes — the client receives a message whenever the serving status changes. This is used by gRPC-aware load balancers that subscribe to health updates rather than polling.

**Implementation.** `tonic-health` provides the standard gRPC health service implementation. The runtime updates the health status via `tonic_health::server::HealthReporter`:

```rust
// At startup, after all subsystems are initialized:
health_reporter.set_serving::<AgentServiceServer<AgentHandler>>().await;
health_reporter.set_serving::<HighwayServiceServer<HighwayHandler>>().await;

// At shutdown:
health_reporter.set_not_serving::<AgentServiceServer<AgentHandler>>().await;
health_reporter.set_not_serving::<HighwayServiceServer<HighwayHandler>>().await;
```

### 8.5 Liveness vs Readiness

The runtime exposes a single health endpoint, not separate liveness and readiness probes. The distinction matters for Kubernetes but is unnecessary at the runtime level:

**Liveness** ("should this process be killed and restarted?") — if the coordinator has panicked or the trail writer is permanently failed, yes. The health check returns 503 in both cases. The orchestrator should restart.

**Readiness** ("should traffic be routed to this instance?") — during `Starting` (recovery in progress) and `Draining` (graceful shutdown), the health check returns 503. The orchestrator should not route new connections.

Both signals collapse into the same 200/503 response. A Kubernetes deployment that needs separate probes can use the same endpoint for both — the semantics are compatible. A `livenessProbe` with a longer `failureThreshold` and a `readinessProbe` with `failureThreshold: 1` gives the correct behavior: the process is killed only after sustained failure, but traffic is stopped immediately on the first 503.

---

## 9. Docker Image

The Docker image packages the `wacp-runtime` binary with its minimal dependencies into a container that can run anywhere Docker runs. The image is designed around three principles: small size (fast pull, small attack surface), no root (least privilege), and externalized state (configuration and data mounted from outside).

### 9.1 Multi-Stage Build

The image uses a two-stage Dockerfile. The build stage compiles the Rust binary with all build dependencies. The runtime stage copies only the binary into a minimal base image.

```dockerfile
# --- Build stage ---
FROM rust:1.85-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY proto/ proto/

# Build release binary with LTO for smaller size
RUN cargo build --release --bin wacp-runtime \
    && strip target/release/wacp-runtime

# --- Runtime stage ---
FROM debian:bookworm-slim

# Install only the runtime dependencies:
# - ca-certificates: for TLS verification (external auth provider, cold storage)
# - libssl3: rustls is pure Rust, but reqwest may use system SSL for external calls
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd --gid 1000 wacp \
    && useradd --uid 1000 --gid 1000 --no-create-home --shell /sbin/nologin wacp

# Create data directory with correct ownership
RUN mkdir -p /var/lib/wacp && chown wacp:wacp /var/lib/wacp

# Copy binary from build stage
COPY --from=builder /build/target/release/wacp-runtime /usr/local/bin/wacp-runtime

# Default configuration: data in /var/lib/wacp, listen on all interfaces
# Operators override by mounting a config file or setting env vars (§11)
ENV WACP_STORAGE__DATA_DIR=/var/lib/wacp
ENV WACP_SERVER__AGENT_LISTEN=0.0.0.0:9090
ENV WACP_SERVER__HIGHWAY_LISTEN=0.0.0.0:9091
ENV WACP_OBSERVABILITY__METRICS__LISTEN=0.0.0.0:9092
ENV WACP_OBSERVABILITY__HEALTH__LISTEN=0.0.0.0:9093

USER wacp

EXPOSE 9090 9091 9092 9093

VOLUME /var/lib/wacp

ENTRYPOINT ["wacp-runtime"]
CMD ["serve"]
```

**Why `debian:bookworm-slim` over Alpine.** Alpine uses musl libc. Rust compiles against musl without issue, but musl's DNS resolver behaves differently from glibc's in some container networking configurations (notably, `search` directives in `/etc/resolv.conf` are handled differently). Since the external auth provider (§5.3) requires DNS resolution, using glibc avoids a class of subtle networking bugs. The size difference is ~30MB — acceptable given the binary itself is ~20–40MB.

**Why not `scratch` or `distroless`.** The runtime needs CA certificates for TLS verification of external auth providers and cold storage destinations. `scratch` has no filesystem at all. `distroless` would work but makes debugging harder — no shell for `docker exec` troubleshooting. `bookworm-slim` is a pragmatic middle ground: small, debuggable, with a real package manager for installing CA certs.

### 9.2 Image Size

Target: under 100MB compressed. The image contains:

| Component | Approximate size |
|-----------|-----------------|
| `debian:bookworm-slim` base | ~30MB |
| `ca-certificates` | ~1MB |
| `wacp-runtime` binary (stripped, LTO) | ~20–40MB |
| **Total** | **~50–70MB** |

The `strip` and LTO (`lto = true` in the release profile) reduce the binary size significantly. The release profile in `Cargo.toml`:

```toml
[profile.release]
lto = true
codegen-units = 1
opt-level = "z"    # optimize for size
strip = true
```

`opt-level = "z"` over `"3"` — the runtime is not CPU-bound (it is I/O-bound on fsync). Optimizing for size over speed reduces the binary by ~20% with negligible performance impact.

### 9.3 Non-Root Execution

The container runs as user `wacp` (UID 1000, GID 1000). The binary is owned by root and not writable by the runtime user. The only writable location is `/var/lib/wacp` (the data directory).

**Why non-root matters.** A compromised runtime running as root gives the attacker full control of the container and, depending on the container runtime configuration, potentially the host. Running as a non-root user limits the blast radius — the attacker can read and write the data directory but cannot modify the binary, install packages, or escalate privileges within the container.

**Filesystem permissions:**

| Path | Owner | Permissions | Purpose |
|------|-------|-------------|---------|
| `/usr/local/bin/wacp-runtime` | root | 755 | Binary (read + execute, not writable) |
| `/var/lib/wacp/` | wacp | 755 | Data directory (trail, checkpoints, snapshots) |
| `/etc/wacp/` | root | 755 (dir), 644 (files) | Config and TLS certs (mounted, read-only) |

### 9.4 Volume Mounts

The container expects three categories of external data, all mounted at runtime:

**Data volume (`/var/lib/wacp`).** The persistent data directory — trail segments, checkpoint store, snapshots. This must be a persistent volume. Losing this volume means losing trail history and requiring full recovery from scratch (or data loss if no external backup exists). Declared as a `VOLUME` in the Dockerfile — Docker creates an anonymous volume if none is mounted, which survives container restarts but not container removal.

**Configuration (`/etc/wacp/runtime.yaml`).** Optional. Mounted as a read-only bind mount or ConfigMap. If not mounted, the runtime uses environment variable overrides (§11) or defaults.

```bash
docker run -v ./runtime.yaml:/etc/wacp/runtime.yaml:ro \
           -e WACP_CONFIG=/etc/wacp/runtime.yaml \
           wacp-runtime
```

**TLS certificates (`/etc/wacp/tls/`).** When TLS is enabled. Mounted read-only. The config file (or env vars) references these paths:

```bash
docker run -v ./tls:/etc/wacp/tls:ro \
           -v ./runtime.yaml:/etc/wacp/runtime.yaml:ro \
           -v wacp-data:/var/lib/wacp \
           -e WACP_CONFIG=/etc/wacp/runtime.yaml \
           wacp-runtime
```

### 9.5 Container Health Check

The Dockerfile includes a Docker-native health check using the HTTP health endpoint (§8):

```dockerfile
HEALTHCHECK --interval=10s --timeout=3s --start-period=30s --retries=3 \
    CMD ["/usr/local/bin/wacp-runtime-healthcheck"]
```

The healthcheck binary is a minimal static program that makes an HTTP GET to `localhost:9093/healthz` and exits 0 on 200 or 1 on anything else. It is compiled alongside the main binary and included in the image (~500KB). Using a separate binary avoids installing `curl` or `wget` in the image.

```rust
// wacp-runtime-healthcheck — minimal health probe, no dependencies beyond std
fn main() -> ExitCode {
    let addr = std::env::var("WACP_HEALTH_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:9093".to_string());
    let path = std::env::var("WACP_HEALTH_PATH")
        .unwrap_or_else(|_| "/healthz".to_string());

    match std::net::TcpStream::connect(&addr) {
        Ok(mut stream) => {
            use std::io::{Read, Write};
            let request = format!("GET {path} HTTP/1.0\r\nHost: localhost\r\n\r\n");
            if stream.write_all(request.as_bytes()).is_err() {
                return ExitCode::from(1);
            }
            let mut response = String::new();
            if stream.read_to_string(&mut response).is_err() {
                return ExitCode::from(1);
            }
            if response.contains("200") {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(_) => ExitCode::from(1),
    }
}
```

**`start-period=30s`.** The runtime may take up to 30 seconds to complete recovery on a large trail. During this period, health check failures do not count against the retry threshold. After 30 seconds, three consecutive failures mark the container as unhealthy.

### 9.6 Docker Compose Example

A self-contained development setup with the runtime and an external auth service stub:

```yaml
services:
  wacp-runtime:
    image: wacp-runtime:latest
    ports:
      - "9090:9090"    # agent gRPC
      - "9091:9091"    # highway gRPC
      - "9092:9092"    # metrics
      - "9093:9093"    # health
    volumes:
      - wacp-data:/var/lib/wacp
      - ./runtime.yaml:/etc/wacp/runtime.yaml:ro
      - ./tls:/etc/wacp/tls:ro
    environment:
      WACP_CONFIG: /etc/wacp/runtime.yaml
    healthcheck:
      test: ["/usr/local/bin/wacp-runtime-healthcheck"]
      interval: 10s
      timeout: 3s
      start_period: 30s
      retries: 3

volumes:
  wacp-data:
```

---

## 10. Systemd Unit

For bare-metal and VM deployments where Docker is not used, the runtime runs as a systemd service. The unit file defines process lifecycle, resource limits, security hardening, and integration with systemd's service management.

### 10.1 Unit File

```ini
[Unit]
Description=WACP Runtime
Documentation=https://github.com/wacp/wacp
After=network-online.target
Wants=network-online.target

[Service]
Type=exec
User=wacp
Group=wacp

ExecStart=/usr/local/bin/wacp-runtime serve --config /etc/wacp/runtime.yaml
ExecReload=/bin/kill -HUP $MAINPID

Restart=on-failure
RestartSec=5
StartLimitIntervalSec=300
StartLimitBurst=5

# --- Resource limits ---
LimitNOFILE=65536
LimitNPROC=4096

# --- Security hardening ---
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
RestrictSUIDSGID=yes
RestrictNamespaces=yes
RestrictRealtime=yes
MemoryDenyWriteExecute=yes
LockPersonality=yes
SystemCallArchitectures=native
SystemCallFilter=@system-service
SystemCallFilter=~@privileged @resources

ReadWritePaths=/var/lib/wacp
ReadOnlyPaths=/etc/wacp

# --- Logging ---
StandardOutput=journal
StandardError=journal
SyslogIdentifier=wacp-runtime

# --- Shutdown ---
TimeoutStopSec=60
KillMode=mixed
KillSignal=SIGTERM

[Install]
WantedBy=multi-user.target
```

### 10.2 Key Directives Explained

**`Type=exec`.** Systemd considers the service started once `execve()` succeeds — the binary is running. This is simpler than `notify` (which requires the binary to send `READY=1` via the sd_notify protocol). The runtime's readiness is checked by the health endpoint (§8), not by systemd's notion of "started." An operator who wants systemd-native readiness can switch to `Type=notify` and add sd_notify integration to the startup sequence — this is a future enhancement, not an initial requirement.

**`ExecReload=/bin/kill -HUP $MAINPID`.** The runtime does not support hot reload (§1, design constraint). Sending `SIGHUP` is a no-op — the runtime ignores it. The `ExecReload` directive exists so that `systemctl reload wacp-runtime` does not return an error, but it does nothing. This prevents operator confusion ("reload failed" vs "reload is not supported"). The runtime logs a message at `INFO` level when it receives `SIGHUP`:

```
SIGHUP received — configuration reload is not supported, ignoring. Restart the service to apply configuration changes.
```

**`Restart=on-failure` with backoff.** If the runtime exits with a non-zero exit code (§3.5), systemd restarts it after 5 seconds. The `StartLimitBurst=5` and `StartLimitIntervalSec=300` settings cap restarts at 5 within a 5-minute window — after that, systemd stops attempting and the unit enters a failed state. This prevents restart loops when the failure is persistent (corrupted trail, misconfigured TLS, port conflict).

**`LimitNOFILE=65536`.** The runtime holds file descriptors for: trail segment (1), trail index (1), checkpoint store files (variable), snapshot files (variable), gRPC connections (one per agent + highway client), and the metrics/health HTTP servers. The default systemd limit (1024) is too low for production workloads with many concurrent agents. 65536 provides headroom without requiring kernel tuning.

**`TimeoutStopSec=60`.** On `systemctl stop`, systemd sends `SIGTERM` (the runtime's graceful shutdown trigger, §3.4) and waits 60 seconds. If the runtime has not exited by then, systemd sends `SIGKILL`. The 60-second window accommodates graceful workspace drain. Operators with longer-running workspaces should increase this value.

**`KillMode=mixed`.** Systemd sends `SIGTERM` to the main process only (not to child processes). After `TimeoutStopSec`, it sends `SIGKILL` to the entire cgroup. `mixed` gives the runtime a chance to handle shutdown gracefully while ensuring cleanup of any orphaned child processes.

### 10.3 Security Hardening

The unit file applies systemd's security sandbox directives. Each reduces the attack surface if the runtime process is compromised.

| Directive | Effect |
|-----------|--------|
| `NoNewPrivileges` | Prevents the process from gaining new privileges via setuid, setgid, or capabilities |
| `ProtectSystem=strict` | Mounts the entire filesystem read-only except paths listed in `ReadWritePaths` |
| `ProtectHome` | Makes `/home`, `/root`, `/run/user` inaccessible |
| `PrivateTmp` | Gives the service its own `/tmp` — other services cannot read its temporary files |
| `PrivateDevices` | Removes access to physical devices (`/dev/sda`, etc.) — the runtime has no need for device access |
| `ProtectKernelTunables` | Makes `/proc/sys`, `/sys` read-only — prevents kernel parameter modification |
| `ProtectKernelModules` | Prevents loading kernel modules |
| `ProtectControlGroups` | Makes `/sys/fs/cgroup` read-only |
| `RestrictSUIDSGID` | Prevents creating setuid/setgid files |
| `RestrictNamespaces` | Prevents creating new namespaces (mount, network, PID, etc.) |
| `RestrictRealtime` | Prevents setting real-time scheduling policy |
| `MemoryDenyWriteExecute` | Prevents creating memory mappings that are both writable and executable (W^X) |
| `LockPersonality` | Locks the execution domain to the current personality |
| `SystemCallArchitectures=native` | Only allows syscalls for the native architecture (no 32-bit compat) |
| `SystemCallFilter=@system-service` | Allows only syscalls used by typical system services |
| `SystemCallFilter=~@privileged @resources` | Denies privileged syscalls and resource-control syscalls |

**`ReadWritePaths=/var/lib/wacp`.** Combined with `ProtectSystem=strict`, this makes the data directory the only writable location on the filesystem. The binary, config file, and TLS certificates are all read-only.

**`ReadOnlyPaths=/etc/wacp`.** Explicitly marks the configuration directory as read-only — the runtime can read its config and certificates but cannot modify them.

### 10.4 System User Setup

The unit runs as a dedicated `wacp` user. This user is created during package installation or provisioning:

```bash
# Create system user (no home directory, no login shell)
useradd --system --no-create-home --shell /sbin/nologin --user-group wacp

# Create and own the data directory
mkdir -p /var/lib/wacp
chown wacp:wacp /var/lib/wacp
chmod 750 /var/lib/wacp

# Config directory (owned by root, readable by wacp)
mkdir -p /etc/wacp/tls
chmod 755 /etc/wacp
chmod 750 /etc/wacp/tls    # TLS keys are sensitive
```

**TLS key permissions.** The private key file (`/etc/wacp/tls/server.key`) should be readable only by the `wacp` user and root: `chmod 640`, `chown root:wacp`. The unit's `ProtectSystem=strict` plus `ReadOnlyPaths=/etc/wacp` ensures the runtime cannot modify these files, only read them.

### 10.5 Journal Integration

`StandardOutput=journal` and `StandardError=journal` route all runtime output to the systemd journal. Combined with the runtime's JSON logging (§6.3), this produces structured log entries queryable with `journalctl`:

```bash
# Follow runtime logs
journalctl -u wacp-runtime -f

# Filter by priority (maps to log level)
journalctl -u wacp-runtime -p err

# Export JSON for log aggregation
journalctl -u wacp-runtime -o json --since "1 hour ago"
```

The `SyslogIdentifier=wacp-runtime` tag ensures journal entries are identifiable even when multiple services write to the journal.

**Log rotation.** The systemd journal handles rotation automatically based on systemd's `journald.conf` settings (`SystemMaxUse`, `MaxRetentionSec`). The runtime does not need its own log rotation when using journal output. When `logging.output: "file"` is configured instead, the runtime writes to its own log file and journal integration is bypassed — log rotation becomes the operator's responsibility (§6.2).

### 10.6 Service Management

Standard `systemctl` operations:

```bash
systemctl enable wacp-runtime      # start on boot
systemctl start wacp-runtime       # start now
systemctl stop wacp-runtime        # graceful shutdown (SIGTERM → 60s → SIGKILL)
systemctl restart wacp-runtime     # stop + start (for config changes, cert rotation)
systemctl status wacp-runtime      # process state, recent journal lines
```

**No `reload`.** `systemctl reload wacp-runtime` sends `SIGHUP`, which the runtime ignores (§10.2). Configuration changes require `systemctl restart`. This is consistent with the no-hot-reload design constraint (§1).

---

## 11. Environment Variable Overrides

Environment variables override config file values for twelve-factor deployments. Container orchestrators (Docker, Kubernetes) pass configuration as environment variables — this section defines the mapping from config file fields to variable names, the precedence rules, and the parsing behavior.

### 11.1 Precedence

Configuration values are resolved in this order, highest priority first:

1. **Environment variable** — always wins if set.
2. **Config file value** — used if the env var is not set.
3. **Default** — used if neither env var nor config file provides a value.

There is no fourth layer. CLI flags do not override individual config fields (§3.3) — the `--config` flag selects which config file to load, not which values to override. This two-layer model (file + env) is simple to reason about: "what is the effective value of X?" has exactly one answer, determined by checking the env var first, then the file.

### 11.2 Naming Convention

Environment variable names are derived mechanically from config file paths. The rule:

1. Start with the prefix `WACP_`.
2. Convert each YAML path segment to uppercase.
3. Join segments with double underscore `__`.

| Config path | Environment variable |
|-------------|---------------------|
| `server.agent_listen` | `WACP_SERVER__AGENT_LISTEN` |
| `server.highway_listen` | `WACP_SERVER__HIGHWAY_LISTEN` |
| `tls.enabled` | `WACP_TLS__ENABLED` |
| `tls.cert_file` | `WACP_TLS__CERT_FILE` |
| `tls.key_file` | `WACP_TLS__KEY_FILE` |
| `tls.client_ca_file` | `WACP_TLS__CLIENT_CA_FILE` |
| `tls.min_version` | `WACP_TLS__MIN_VERSION` |
| `auth.provider` | `WACP_AUTH__PROVIDER` |
| `auth.external.url` | `WACP_AUTH__EXTERNAL__URL` |
| `auth.external.timeout_ms` | `WACP_AUTH__EXTERNAL__TIMEOUT_MS` |
| `auth.rate_limit.max_failures` | `WACP_AUTH__RATE_LIMIT__MAX_FAILURES` |
| `auth.rate_limit.window_seconds` | `WACP_AUTH__RATE_LIMIT__WINDOW_SECONDS` |
| `taxonomy.file` | `WACP_TAXONOMY__FILE` |
| `storage.data_dir` | `WACP_STORAGE__DATA_DIR` |
| `storage.trail.segment_size_bytes` | `WACP_STORAGE__TRAIL__SEGMENT_SIZE_BYTES` |
| `storage.trail.index_batch_size` | `WACP_STORAGE__TRAIL__INDEX_BATCH_SIZE` |
| `storage.trail.index_batch_timeout_ms` | `WACP_STORAGE__TRAIL__INDEX_BATCH_TIMEOUT_MS` |
| `storage.snapshots.workspace_checkpoint_interval` | `WACP_STORAGE__SNAPSHOTS__WORKSPACE_CHECKPOINT_INTERVAL` |
| `storage.snapshots.workspace_time_interval_seconds` | `WACP_STORAGE__SNAPSHOTS__WORKSPACE_TIME_INTERVAL_SECONDS` |
| `storage.snapshots.system_entry_interval` | `WACP_STORAGE__SNAPSHOTS__SYSTEM_ENTRY_INTERVAL` |
| `storage.snapshots.system_time_interval_minutes` | `WACP_STORAGE__SNAPSHOTS__SYSTEM_TIME_INTERVAL_MINUTES` |
| `storage.snapshots.system_retention_count` | `WACP_STORAGE__SNAPSHOTS__SYSTEM_RETENTION_COUNT` |
| `storage.tiered.hot_segments` | `WACP_STORAGE__TIERED__HOT_SEGMENTS` |
| `storage.tiered.warm_retention_days` | `WACP_STORAGE__TIERED__WARM_RETENTION_DAYS` |
| `storage.tiered.cold_retention` | `WACP_STORAGE__TIERED__COLD_RETENTION` |
| `storage.tiered.cold_destination` | `WACP_STORAGE__TIERED__COLD_DESTINATION` |
| `storage.tiered.compaction_interval_minutes` | `WACP_STORAGE__TIERED__COMPACTION_INTERVAL_MINUTES` |
| `resources.default_timeout_ms` | `WACP_RESOURCES__DEFAULT_TIMEOUT_MS` |
| `resources.default_budget.max_tokens` | `WACP_RESOURCES__DEFAULT_BUDGET__MAX_TOKENS` |
| `resources.default_budget.max_wall_time_ms` | `WACP_RESOURCES__DEFAULT_BUDGET__MAX_WALL_TIME_MS` |
| `resources.default_budget.max_storage_bytes` | `WACP_RESOURCES__DEFAULT_BUDGET__MAX_STORAGE_BYTES` |
| `resources.default_budget.max_network_bytes` | `WACP_RESOURCES__DEFAULT_BUDGET__MAX_NETWORK_BYTES` |
| `resources.default_budget.max_cost_micros` | `WACP_RESOURCES__DEFAULT_BUDGET__MAX_COST_MICROS` |
| `resources.warning_threshold` | `WACP_RESOURCES__WARNING_THRESHOLD` |
| `resources.liveness_interval_ms` | `WACP_RESOURCES__LIVENESS_INTERVAL_MS` |
| `delivery.max_retries` | `WACP_DELIVERY__MAX_RETRIES` |
| `delivery.retry_backoff_ms` | `WACP_DELIVERY__RETRY_BACKOFF_MS` |
| `logging.level` | `WACP_LOGGING__LEVEL` |
| `logging.format` | `WACP_LOGGING__FORMAT` |
| `logging.output` | `WACP_LOGGING__OUTPUT` |
| `logging.file` | `WACP_LOGGING__FILE` |
| `observability.metrics.enabled` | `WACP_OBSERVABILITY__METRICS__ENABLED` |
| `observability.metrics.listen` | `WACP_OBSERVABILITY__METRICS__LISTEN` |
| `observability.metrics.path` | `WACP_OBSERVABILITY__METRICS__PATH` |
| `observability.health.enabled` | `WACP_OBSERVABILITY__HEALTH__ENABLED` |
| `observability.health.listen` | `WACP_OBSERVABILITY__HEALTH__LISTEN` |
| `observability.health.path` | `WACP_OBSERVABILITY__HEALTH__PATH` |

**Why double underscore.** Single underscore is ambiguous — `WACP_DATA_DIR` could mean `data.dir` or `data_dir`. Double underscore (`__`) is the nesting separator; single underscore is part of the field name. This convention is used by `config-rs`, Django, and other frameworks that support env-to-struct mapping.

**`WACP_CONFIG`.** One additional variable outside the naming convention: `WACP_CONFIG` specifies the config file path (§2, file location rules). It is not an override of a config field — it selects the file itself.

**`RUST_LOG`.** The `RUST_LOG` variable overrides `logging.level` through `tracing`'s `EnvFilter` (§6.2). It is not part of the `WACP_` namespace because it follows the Rust ecosystem convention. `RUST_LOG` takes precedence over `WACP_LOGGING__LEVEL` — if both are set, `RUST_LOG` wins because `EnvFilter` reads it directly.

### 11.3 Type Parsing

Environment variables are strings. The runtime parses them into the expected type for each field.

| Target type | Parsing rule | Examples |
|------------|-------------|---------|
| `String` | Used as-is | `WACP_SERVER__AGENT_LISTEN=0.0.0.0:9090` |
| `u32`, `u64` | `str::parse::<T>()` — decimal integer, no leading zeros | `WACP_DELIVERY__MAX_RETRIES=5` |
| `f32` | `str::parse::<f32>()` — decimal with optional fractional part | `WACP_RESOURCES__WARNING_THRESHOLD=0.9` |
| `bool` | `"true"` or `"1"` → true; `"false"` or `"0"` → false; anything else → error | `WACP_TLS__ENABLED=true` |

A parse failure is a startup error — the runtime exits with code 1 and a diagnostic identifying the variable name, the raw value, and the expected type. Parse failures are reported before config file validation (§2.12) — the env var layer resolves first.

### 11.4 Implementation

The override is applied after config file loading and before validation. The sequence:

1. Load and parse the config file into `RuntimeConfig` (or construct `RuntimeConfig::default()` if no file).
2. Scan the process environment for variables starting with `WACP_`.
3. For each matching variable, map the name to a config field path using the naming convention (§11.2).
4. Parse the value to the field's type (§11.3).
5. Overwrite the field in the `RuntimeConfig` struct.
6. Run validation (§2.12) on the final struct.

```rust
fn apply_env_overrides(config: &mut RuntimeConfig) -> Result<(), ConfigError> {
    for (key, value) in std::env::vars() {
        if !key.starts_with("WACP_") || key == "WACP_CONFIG" {
            continue;
        }

        let path = key
            .strip_prefix("WACP_")
            .unwrap()
            .to_lowercase()
            .replace("__", ".");

        apply_override(config, &path, &value)?;
    }
    Ok(())
}
```

The `apply_override` function matches the dotted path against the `RuntimeConfig` struct fields. Unrecognized paths (env var names that don't map to any config field) produce a `warn`-level log but do not abort startup. This is a deliberate difference from the config file's `deny_unknown_fields` behavior (§2.13) — environment variables are shared across all processes in a container, and other `WACP_`-prefixed variables may exist for other tools. Rejecting unknown env vars would make the runtime fragile in mixed environments.

### 11.5 Effective Configuration Logging

After env var overrides are applied and validation passes, the runtime logs the effective configuration at `DEBUG` level. This is the merged result of defaults + config file + env vars — the actual values the runtime will use.

```
DEBUG effective configuration  server.agent_listen="0.0.0.0:9090" server.highway_listen="0.0.0.0:9091" tls.enabled=true auth.provider="external" storage.data_dir="/var/lib/wacp" ...
```

Fields containing secrets (`tls.key_file` path is logged, but not the key contents; `auth.external.url` is logged) are included because the paths and URLs are operational information, not secrets themselves. The PSK token table is never logged.

For any field where an env var override was applied, the log includes an annotation:

```
DEBUG config override applied  field="server.agent_listen" source="WACP_SERVER__AGENT_LISTEN" value="0.0.0.0:9090"
```

These annotations are logged individually at `DEBUG` level before the effective configuration summary. They allow an operator to trace which values came from the environment versus the config file — essential for debugging "why is the runtime using this value?" in container deployments where env vars may be injected by the orchestrator, the Dockerfile, or the compose file.

---

## 12. Startup and Shutdown Sequence

This section defines the exact order of operations for bringing the runtime up and taking it down. The order is not arbitrary — each step depends on the previous step's success, and the shutdown sequence is the reverse of startup. Getting the order wrong produces subtle bugs: a trail write during recovery before the clock is initialized, or a connection accepted before recovery completes.

### 12.1 Startup Sequence

The startup sequence runs in `cmd_serve` (§3.6). Every step is fallible. A failure at any step aborts the run — the runtime does not start partially. The exit code corresponds to the failing subsystem (§3.5).

```
Step  What                           Depends on     Failure exit code
────  ─────────────────────────────  ─────────────  ─────────────────
 1    Resolve and load config file   —              1
 2    Apply env var overrides        Step 1         1
 3    Validate configuration         Step 2         1
 4    Initialize logging             Step 3         1
 5    Build TLS configuration        Step 3         4
 6    Initialize authenticator       Step 3         1
 7    Open storage backends          Step 3         2
 8    Run trail integrity check      Step 7         2
 9    Initialize clock from trail    Step 8         2
10    Load and validate taxonomy     Step 3         1
11    Build permission engine        Step 10        1
12    Run recovery                   Steps 7–11     2
13    Spawn coordinator actor        Step 12        101
14    Spawn transport actors         Steps 5, 6     3
15    Start health endpoint          Step 3         3
16    Start metrics endpoint         Step 3         3
17    Set health state to Ready      Steps 13–16    —
18    Install signal handlers        Step 17        —
19    Enter coordinator main loop    Step 18        —
```

**Step 1–3: Configuration.** Load the config file (§2, file location), apply env overrides (§11), validate (§2.12). If no config file is found and no env vars are set, the runtime proceeds with `RuntimeConfig::default()`. Failure here is a configuration error (exit 1).

**Step 4: Logging.** Initialize the `tracing` subscriber (§6.2). This is the earliest possible moment — all subsequent steps can produce log output. Logging initialization must succeed before anything else happens, because diagnostic output from later failures depends on it. If logging fails (invalid level string, log file not writable), the error is printed directly to stderr.

**Step 5: TLS.** Parse certificates and build the `rustls::ServerConfig` (§4.2). This happens early — before storage — because a TLS failure is fast to detect and requires no cleanup. If TLS is disabled, this step is a no-op.

**Step 6: Authenticator.** Construct the `Arc<dyn Authenticator>` — either `PskAuthenticator` or `ExternalAuthenticator` based on `auth.provider` (§5). For the external provider, the `reqwest::Client` is constructed here but the external URL is not probed (§2.12, validation check 2).

**Step 7: Storage.** Open the three storage backends (storage spec, §2): trail store, checkpoint store, snapshot store. Create the data directory and subdirectories if they don't exist. Open the active trail segment. Open the trail index SQLite database. If the data directory is not writable or existing data is unreadable, this is a storage error (exit 2).

**Step 8: Trail integrity.** Walk the hash chain from first entry to last (runtime spec, §13, step 1). Verify every link. Truncate any partial entry at the end of the active segment (storage spec, §3, crash safety). A broken hash chain (not a truncated tail — an actual mismatch in the middle) halts startup. Trail corruption requires human intervention.

**Step 9: Clock.** Initialize the HLC from the last trail entry's timestamp (runtime spec, §7). If the trail is empty (first run), initialize from the system clock. The clock must be initialized before recovery, because recovery may need to generate timestamps for in-flight operation completion.

**Step 10: Taxonomy.** Load and validate the taxonomy file if configured (runtime spec, §11). Build resolved roles. If the taxonomy is invalid, this is a configuration error (exit 1). If no taxonomy is configured, the runtime operates with base types only.

**Step 11: Permission engine.** Build the permission matrix, checkpoint type table, and role table from base types plus taxonomy (runtime spec, §5). These are immutable for the lifetime of the run.

**Step 12: Recovery.** Replay trail entries to reconstruct state (runtime spec, §13). Load system snapshot if available. Reconstruct workspace states, task graph, port rights, resource meters. Redeliver in-flight envelopes. Reconstruct timers. On a first run with an empty trail, this step is a no-op. Recovery is the longest step — its duration depends on trail size and snapshot availability. The `wacp_recovery_duration_seconds` metric (§7.2) records how long it took.

**Step 13: Coordinator.** Spawn the coordinator actor as a `tokio` task. Pass it the recovered state: workspace tree, task graph, permission engine, taxonomy, trail writer handle, clock. The coordinator immediately begins processing — it is ready to accept workspace-related messages from transport actors.

**Step 14: Transport.** Spawn the agent and highway transport actors. Bind to the configured listen addresses (§2.2). If a port is already in use, this is a port binding error (exit 3). Once bound, the endpoints begin accepting connections. New connections before this point are impossible — the TCP listener does not exist yet.

**Step 15–16: Observability.** Start the HTTP health (§8) and metrics (§7) servers on their configured addresses. These are independent `tokio` tasks.

**Step 17: Ready.** Set the health state to `Ready` (§8.1). Log the startup summary (§6.6). The runtime is now fully operational.

**Step 18: Signals.** Install `SIGTERM` and `SIGINT` handlers (§3.4). Register `SIGHUP` as ignored with a log message (§10.2). Signal handlers must be installed after the coordinator is running — a signal before the coordinator exists has nothing to drain.

**Step 19: Main loop.** Control enters the coordinator actor's select loop (runtime spec, §14). The coordinator processes messages until a shutdown signal arrives.

### 12.2 Shutdown Sequence

Shutdown is triggered by `SIGTERM` (graceful) or `SIGINT` (immediate). The sequence is the reverse of startup — outer layers shut down first, inner layers last. The trail store is the last thing closed because every preceding step may need to write trail entries.

#### Graceful Shutdown (SIGTERM)

```
Step  What                                Duration
────  ──────────────────────────────────  ──────────────────────
 1    Set health state to Draining        Immediate
 2    Stop accepting new connections      Immediate
 3    Send GracefulTermination to all     Immediate
      active workspaces
 4    Wait for workspaces to drain        Up to timeout per workspace
 5    Abort remaining workspaces          Immediate (if any timed out)
 6    Take final system snapshot          Seconds
 7    Stop transport actors               Immediate (connections closed)
 8    Stop metrics and health servers     Immediate
 9    Flush trail index                   Milliseconds
10    Close trail store                   Immediate (final fsync)
11    Exit with code 0                    —
```

**Step 1: Draining.** The health endpoint immediately returns 503. Load balancers and orchestrators stop routing new traffic.

**Step 2: No new connections.** The transport actors stop calling `accept()` on their TCP listeners. Existing connections remain open.

**Step 3: Graceful termination.** Each active workspace actor receives a `GracefulTermination` command on its high-priority channel (runtime spec, §14). The command carries a grace period — the shorter of the workspace's remaining timeout and the configured default, with a 5-second floor (§3.4). Workspaces in `Active` state attempt to reach a checkpoint before the grace period expires.

**Step 4: Wait.** The coordinator waits for all workspace actors to reach terminal states. Each workspace either completes normally (`Closed`), creates a final checkpoint and transitions to `Failed` with `reason: runtime_shutdown`, or simply transitions to `Failed` when the grace period expires.

**Step 5: Abort stragglers.** Any workspace that has not reached a terminal state after its grace period is aborted — transitioned to `Failed` with `reason: runtime_shutdown`. The abort trail entry is written.

**Step 6: Snapshot.** The coordinator takes a final system snapshot (storage spec, §7). This accelerates recovery on the next startup.

**Step 7–8: Stop servers.** Transport actors close all remaining connections. The metrics and health HTTP servers shut down. After this step, no network I/O is possible.

**Step 9: Flush index.** The trail index writer flushes any pending batch to SQLite. This is best-effort — the index is rebuildable from trail segments if this step fails.

**Step 10: Close trail store.** Final fsync on the active trail segment. Close all file handles. After this step, no trail writes are possible.

**Step 11: Exit.** The process exits with code 0. Systemd sees a clean exit. Docker sees a healthy stop.

#### Immediate Shutdown (SIGINT)

Same sequence, but step 3 uses a zero grace period — all workspaces are immediately aborted. Steps 4 and 5 collapse into a single immediate abort. The shutdown completes in seconds rather than the potentially minutes-long drain of a graceful shutdown.

#### Double Signal Escalation

If a second `SIGTERM` or `SIGINT` arrives during an in-progress shutdown (graceful or immediate), the runtime skips to step 7 — stop servers, flush index, close trail. Workspace aborts that haven't completed are abandoned. Trail entries for abandoned aborts may be missing — the next startup's recovery will detect the inconsistency (workspaces in non-terminal states with no subsequent events) and handle it.

### 12.3 Startup Timing

The startup sequence has two performance-critical steps: trail integrity check (step 8) and recovery (step 12). Both are O(n) in trail size.

**First run.** No trail, no recovery. Startup completes in milliseconds — parse config, open empty storage, bind ports.

**Clean restart.** Trail exists, system snapshot exists. Integrity check walks the full chain (unavoidable — the chain must be verified end-to-end). Recovery loads the snapshot and replays only entries after the snapshot point. Typical startup: seconds to low tens of seconds, depending on trail size and entries since the last snapshot.

**Crash recovery.** Same as clean restart, plus in-flight operation recovery (redeliver uncommitted envelopes, restart interrupted integrations). The additional cost is proportional to the number of in-flight operations at crash time — usually small.

**Optimization: integrity check with checkpoint.** The trail integrity check (step 8) can be accelerated by recording the last verified chain hash in `trail.meta`. On startup, the check begins from the last verified position rather than the beginning. The check is still O(n) in the worst case (if `trail.meta` is missing or corrupt), but O(delta) in the common case (only new entries since the last run need verification). The full chain can be verified periodically as a background task during normal operation.

---

## 13. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §11 (security model) | §1 | Protocol security model — basis for TLS and authentication requirements |
| §11.3 (identity and authentication) | §5 | Authenticated identity required before any protocol action |
| §11.4 (message integrity) | §4 | TLS protects message integrity — cleartext voids this guarantee |

### Runtime Spec (`impl/runtime.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §1 (purpose) | §1, §2.12, §4.5, §10.2 | Runtime is the trust root — no hot reload, fail-fast on misconfiguration |
| §2 (runtime boundaries) | §2.2 | Two external boundaries — agent-facing and highway-facing |
| §3 (process model) | §12.1 | Coordinator, workspace, and transport actors |
| §5 (permission engine) | §12.1 | Permission matrix, checkpoint type table — immutable after init |
| §6 (trail write-ahead path) | §8.2 | Trail writer degraded mode flag |
| §7 (clock implementation) | §12.1 | HLC initialization from last trail entry timestamp |
| §9 (envelope delivery) | §2.8 | Delivery retry policy — 3 retries, linear backoff |
| §11 (taxonomy loader) | §2.5, §2.12, §3.6, §12.1 | Taxonomy file path, loading, validation |
| §12 (resource enforcement) | §2.7 | Timeouts, budgets, liveness interval, warning threshold |
| §13 (recovery engine) | §5.2, §12.1 | Recovery replays trail, reconstructs workspace state and tokens |
| §14 (concurrency model) | §3.4, §12.2 | Biased select, abort precedence, coordinator main loop |

### Protocol Interface Spec (`impl/protocol-interface.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §7 (authentication at the boundary) | §2.3, §4.1, §5 | Authenticator trait, PSK and external providers, TLS recommendation |

### Storage Spec (`impl/storage.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §2 (storage domains) | §2.6, §12.1 | Three storage domains — trail, checkpoints, snapshots |
| §3 (trail backend) | §12.1 | Segment rotation, crash safety, partial entry truncation |
| §5 (checkpoint store) | §2.6 | Content-addressable store — no configuration needed |
| §7 (snapshots) | §3.4, §12.2 | System snapshots — taken at shutdown, accelerate recovery |
| §9 (retention and compaction) | §2.6 | Retention policy — hot, warm, cold tiers and compaction |

### Internal Cross-References

| Section | Referenced by | Topic |
|---------|--------------|-------|
| §1 (purpose) | §2, §3.3, §4.5, §10.2 | Design constraint: no hot reload, config file is single source of truth |
| §2 (configuration) | §3, §5.1, §6.2, §7.1, §8.1, §9.1, §11, §12.1 | Config schema, defaults, validation, file location rules |
| §3 (CLI) | §12.1 | Exit codes, `cmd_serve` function, signal handling |
| §4 (TLS) | §5, §9.4 | TLS prerequisite for transport, certificate monitoring metric |
| §5 (authenticator) | §9.1, §12.1 | Provider selection at startup |
| §6 (logging) | §10.5, §11.2, §12.1 | Tracing subscriber initialization, `RUST_LOG` override |
| §7 (metrics) | §4.5, §8.3, §12.1 | `tls_certificate_expiry_seconds` gauge, metrics server architecture |
| §8 (health) | §6.6, §9.5, §10.6, §12.1 | Health endpoint as readiness signal, Docker and systemd integration |
| §11 (env vars) | §9.1, §9.4, §12.1 | Container env var defaults, override precedence in startup |

### Rust Crate Dependencies

| Crate | Used in | Purpose |
|-------|---------|---------|
| `clap` | §3 | CLI argument parsing with derive macros |
| `serde`, `serde_yaml` | §2 | Configuration file deserialization |
| `tracing`, `tracing-subscriber` | §6 | Structured logging with spans and JSON output |
| `tonic`, `tonic-health` | §4, §8.4 | gRPC server, TLS integration, standard health service |
| `rustls` | §4 | TLS backend — pure Rust, no OpenSSL dependency |
| `reqwest` | §5.3 | HTTP client for external auth provider |
| `prometheus` | §7 | Metric registration and Prometheus text exposition |
| `hyper` | §7, §8 | Lightweight HTTP server for metrics and health endpoints |
| `tokio` | §3.4, §8.2, §12 | Async runtime, signal handling, task spawning |
| `ring` | §5.2 | Cryptographically secure random token generation |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/PROTOCOL.md) | Taxonomy: [TAXONOMY.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/TAXONOMY.md)*
