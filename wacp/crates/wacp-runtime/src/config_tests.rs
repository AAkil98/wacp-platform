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
    assert_eq!(
        config.resources.warning_threshold,
        defaults.resources.warning_threshold
    );
    assert_eq!(config.delivery.max_retries, defaults.delivery.max_retries);
    assert_eq!(config.logging.level, defaults.logging.level);
    assert_eq!(config.logging.format, defaults.logging.format);
    assert_eq!(
        config.observability.health.enabled,
        defaults.observability.health.enabled
    );
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
    listen: "0.0.0.0:9095"
    path: "/metrics"
  health:
    enabled: true
    listen: "0.0.0.0:9094"
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

fn override_one(config: &mut RuntimeConfig, key: &str, value: &str) -> Result<(), ConfigError> {
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
    assert_eq!(
        parsed.storage.trail.segment_size_bytes,
        d.storage.trail.segment_size_bytes
    );
    assert_eq!(
        parsed.resources.warning_threshold,
        d.resources.warning_threshold
    );
    assert_eq!(
        parsed.observability.health.path,
        d.observability.health.path
    );
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
    let err = override_one(
        &mut config,
        "WACP_STORAGE__TRAIL__SEGMENT_SIZE_BYTES",
        "abc",
    );
    assert!(err.is_err());
}

#[test]
fn env_override_invalid_f32() {
    let mut config = RuntimeConfig::default();
    let err = override_one(
        &mut config,
        "WACP_RESOURCES__WARNING_THRESHOLD",
        "not_a_number",
    );
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
        "server:",
        "tls:",
        "auth:",
        "taxonomy:",
        "storage:",
        "resources:",
        "delivery:",
        "logging:",
        "observability:",
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
    assert_eq!(
        parsed.server.coordinator_listen,
        d.server.coordinator_listen
    );
    assert_eq!(parsed.server.rest_listen, d.server.rest_listen);
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
    assert_eq!(
        parsed.auth.rate_limit.max_failures,
        d.auth.rate_limit.max_failures
    );
    assert_eq!(
        parsed.auth.rate_limit.window_seconds,
        d.auth.rate_limit.window_seconds
    );
    // Storage
    assert_eq!(parsed.storage.data_dir, d.storage.data_dir);
    assert_eq!(
        parsed.storage.trail.segment_size_bytes,
        d.storage.trail.segment_size_bytes
    );
    assert_eq!(
        parsed.storage.snapshots.system_retention_count,
        d.storage.snapshots.system_retention_count
    );
    assert_eq!(
        parsed.storage.tiered.hot_segments,
        d.storage.tiered.hot_segments
    );
    assert_eq!(
        parsed.storage.tiered.cold_retention,
        d.storage.tiered.cold_retention
    );
    // Resources
    assert_eq!(
        parsed.resources.default_timeout_ms,
        d.resources.default_timeout_ms
    );
    assert!(
        (parsed.resources.warning_threshold - d.resources.warning_threshold).abs() < f32::EPSILON
    );
    // Delivery
    assert_eq!(parsed.delivery.max_retries, d.delivery.max_retries);
    assert_eq!(
        parsed.delivery.retry_backoff_ms,
        d.delivery.retry_backoff_ms
    );
    // Logging
    assert_eq!(parsed.logging.level, d.logging.level);
    assert_eq!(parsed.logging.format, d.logging.format);
    assert_eq!(parsed.logging.output, d.logging.output);
    // Observability
    assert_eq!(
        parsed.observability.metrics.enabled,
        d.observability.metrics.enabled
    );
    assert_eq!(
        parsed.observability.health.enabled,
        d.observability.health.enabled
    );
    assert_eq!(
        parsed.observability.health.path,
        d.observability.health.path
    );
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
    std::fs::write(&path, "server:\n  agent_listen: \"127.0.0.1:8080\"\n").unwrap();
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
            coordinator_listen: "127.0.0.1:9092".into(),
            rest_listen: "127.0.0.1:9093".into(),
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
                listen: "127.0.0.1:9095".into(),
                path: "/metrics".into(),
            },
            health: HealthConfig {
                enabled: true,
                listen: "127.0.0.1:9094".into(),
                path: "/healthz".into(),
            },
        },
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_ok(), "validation failed: {:?}", result.err());
}
