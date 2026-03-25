# Task 14.2: CLI Interface

## Scope

Replace the bare `main()` with a `clap` derive CLI: `serve` (default), `validate`, `defaults` subcommands. Add `--config` and `--version` global options. Define exit codes (0–4, 101). Add signal handling (SIGTERM graceful, SIGINT immediate, double-signal escalation). Add `build.rs` for compile-time git commit hash and build timestamp.

This task rewrites `main.rs` to use `clap::Parser` and restructures the entry point into three command handlers. It does NOT implement logging (14.3), TLS (14.4), or auth (14.5) — those are wired in by later tasks.

## Dependencies

- `clap` (new workspace dep, features: `derive`)
- `chrono` (new workspace dep, for build timestamp — or use `std::time` formatted manually)

## Types

### `Cli`

```rust
#[derive(Parser)]
#[command(name = "wacp-runtime", version, about = "WACP Runtime")]
struct Cli {
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}
```

### `Command`

```rust
#[derive(Subcommand)]
enum Command {
    /// Start the runtime (default)
    Serve,
    /// Parse and validate configuration, then exit
    Validate,
    /// Print the full default configuration to stdout
    Defaults,
}
```

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Clean shutdown |
| 1 | Configuration error |
| 2 | Storage initialization error |
| 3 | Port binding error |
| 4 | TLS initialization error |
| 101 | Internal error (coordinator panic) |

## Functions

### `main`

```rust
fn main() -> ExitCode
```

Parse CLI args via `Cli::parse()`. Dispatch to `cmd_serve`, `cmd_validate`, or `cmd_defaults` based on `command.unwrap_or(Command::Serve)`.

### `cmd_serve`

```rust
fn cmd_serve(config_path: Option<&Path>) -> ExitCode
```

1. Load and validate config via `RuntimeConfig::load(config_path)`.
2. Log startup info to stderr.
3. Initialize runtime via `Runtime::init(config)`.
4. Install signal handlers (SIGTERM, SIGINT).
5. Enter event loop.
6. Return ExitCode::SUCCESS on clean shutdown.

Errors map to exit codes: config error → 1, storage → 2, transport → 3.

### `cmd_validate`

```rust
fn cmd_validate(config_path: Option<&Path>) -> ExitCode
```

Load and validate config. If taxonomy is configured, load and validate it. Print "configuration valid" on success. On failure, print error and return ExitCode(1).

### `cmd_defaults`

```rust
fn cmd_defaults() -> ExitCode
```

Print `RuntimeConfig::default_yaml()` to stdout. Return ExitCode::SUCCESS.

### Signal handling

In `cmd_serve`, the event loop integrates SIGTERM and SIGINT:

- SIGTERM → `begin_graceful_shutdown()` (drain workspaces with grace period)
- SIGINT → `begin_immediate_shutdown()` (abort all workspaces immediately)
- Second signal during shutdown → escalate to immediate

Uses `tokio::signal::unix::signal(SignalKind::terminate())` for SIGTERM and `tokio::signal::ctrl_c()` for SIGINT.

### `build.rs`

Emits `WACP_BUILD_TIMESTAMP` and `WACP_GIT_COMMIT` as compile-time env vars. `--version` output format:

```
wacp-runtime <version>
protocol: wacp-v0.1
built: <timestamp>
commit: <hash>
```

## Tests

| Test | Verifies |
|------|----------|
| `defaults_prints_valid_yaml` | `cmd_defaults` output parses back as `RuntimeConfig` |
| `validate_accepts_valid_config` | `cmd_validate` with valid config file returns exit code 0 |
| `validate_rejects_invalid_config` | `cmd_validate` with invalid config returns exit code 1 |
| `serve_exits_on_bad_config` | `cmd_serve` with unparseable config returns exit code 1 |
| `default_command_is_serve` | `Command` from `None` resolves to `Serve` |
| `version_includes_protocol` | Version string contains "wacp-v0.1" |
| `build_info_available` | Build timestamp and commit env vars are set at compile time |

## Acceptance Criteria

- `wacp-runtime` with no args starts in serve mode.
- `wacp-runtime defaults` prints valid YAML to stdout.
- `wacp-runtime validate --config <file>` exits 0 on valid, 1 on invalid.
- `wacp-runtime --version` shows version, protocol, build timestamp, commit.
- SIGTERM triggers graceful shutdown; SIGINT triggers immediate shutdown.
- Second signal during shutdown escalates to immediate.
- Exit codes match the table above.
- `cargo clippy -p wacp-runtime` clean.
