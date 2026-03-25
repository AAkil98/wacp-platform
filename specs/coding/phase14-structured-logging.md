# Task 14.3: Structured Logging

## Scope

Replace all `eprintln!` in `wacp-runtime` with `tracing` subscriber-based structured logging. Add `init_logging(config: &LoggingConfig)` that sets up JSON or pretty format, stderr or file output, and log level filtering via `EnvFilter`. Emit an unconditional TLS-disabled warning when `tls.enabled` is false.

This task produces `logging.rs` with the subscriber setup, replaces `eprintln!` calls in `init.rs` and `main.rs` with `tracing` macros, and adds `tracing`/`tracing-subscriber` to dependencies.

## Dependencies

- `tracing` (workspace — already present, not yet in wacp-runtime)
- `tracing-subscriber` (new workspace dep, features: `json`, `env-filter`, `fmt`)

## Functions

### `init_logging`

```rust
pub fn init_logging(config: &LoggingConfig) -> Result<(), LoggingError>
```

Sets up a global `tracing` subscriber. Must be called exactly once, before any other subsystem. The 2×2 matrix: (json|pretty) × (stderr|file).

- `json` format: one JSON object per line, flat keys, timestamp/level/target/message.
- `pretty` format: human-readable colored output (ansi disabled for file output).
- `EnvFilter` from config level, respects `RUST_LOG` env var override.
- File output opens in append mode via `std::fs::OpenOptions`.

### `LoggingError`

```rust
pub enum LoggingError {
    InvalidLevel(String),
    FileOpen { path: String, source: std::io::Error },
}
```

## Tests

| Test | Verifies |
|------|----------|
| `json_format_parseable` | JSON format output can be parsed as `serde_json::Value` |
| `pretty_format_not_json` | Pretty format output is not valid JSON |
| `file_output_writes` | File output mode creates and writes to the specified file |
| `level_filtering` | Events below configured level are not emitted |

## Acceptance Criteria

- All `eprintln!` in init.rs and main.rs replaced with `tracing::info!`/`tracing::warn!`/`tracing::error!`.
- `init_logging` called as first action in `cmd_serve`, before any other initialization.
- JSON and pretty formats produce correct output.
- File output works in append mode.
- `RUST_LOG` overrides config level.
- TLS-disabled warning emitted unconditionally at warn level when `tls.enabled` is false.
- `cargo clippy -p wacp-runtime` clean.
