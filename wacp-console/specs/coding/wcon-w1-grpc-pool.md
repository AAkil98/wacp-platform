---
id: wcon-w1-grpc-pool
type: coding
status: final
created: 2026-04-15T04:20:00
revised: 2026-04-15T04:20:00
authors: [AAkil98, Claude Opus 4.6]
tags: [wiring, w1, grpc, pool, appstate, startup]
depends_on: [wcon-wiring-phases, wcon-wiring-strategy, wcon-architecture]
---

# W1 — gRPC Pool → AppState

## Table of Contents

- 1. Scope
- 2. Dependencies
- 3. Types & Signatures
- 4. Internal Design
- 5. Test Cases
- 6. Acceptance Criteria

---

## 1. Scope

Wire the already-implemented `console_runtime::GrpcPool` into `AppState` so Axum handlers can obtain `AgentServiceClient`, `HighwayServiceClient`, and `CoordinatorServiceClient` instances. Instantiate the pool at process startup, propagate connect errors according to the retry policy, and replace the TCP-probe-based health check with pool-status reads.

**Out of scope.** Actually *using* the clients in a handler — that starts at W2. W1 is the plumbing only.

**Files touched.**
- `wacp-console/crates/console-api/src/lib.rs` — `AppState` struct.
- `wacp-console/crates/console/src/main.rs` — startup sequence.
- `wacp-console/crates/console-api/src/routes/health.rs` — health endpoint body.
- `wacp-console/crates/console-runtime/src/grpc_pool.rs` — may need a `status()` accessor if not already present; no behavioral change.

## 2. Dependencies

- **`wcon-w0` (merge)** — `monorepo-v0` tag. DONE.
- **`wcon-architecture` §4.1** — connection model defines three gRPC channels, TLS disabled in dev.
- **Existing code:** `console-runtime/src/grpc_pool.rs` defines `GrpcPool`, `PoolConfig`, `ChannelStatus`, per-service reconnect. Review it end-to-end before editing — no new impl should duplicate what's there.

## 3. Types & Signatures

### 3.1 AppState additions (`console-api/src/lib.rs`)

```rust
use std::sync::Arc;
use console_runtime::GrpcPool;

pub struct AppState {
    pub db: Arc<ConsoleDb>,
    pub taxonomy: Arc<TaxonomyIndex>,
    pub settings: Arc<SettingsStore>,
    pub grpc_pool: Arc<GrpcPool>,   // NEW
    // …existing fields…
}
```

The field is `Arc<GrpcPool>` so `AppState` remains `Clone + Send + Sync`. `GrpcPool` internally owns its channels and reconnect task; cloning the Arc is cheap.

### 3.2 Pool startup signature (`console/src/main.rs`)

```rust
async fn connect_pool(cfg: &PoolConfig) -> Result<Arc<GrpcPool>, PoolConnectError> {
    let pool = GrpcPool::new(cfg.clone());
    pool.connect().await?;   // waits for first successful dial on each channel, bounded by cfg.startup_timeout
    Ok(Arc::new(pool))
}
```

`PoolConnectError` distinguishes:
- `Transient { attempts: u32, last_error: tonic::Status }` — caller retries according to policy.
- `Terminal { reason: TerminalReason }` — caller aborts startup (invalid config, resolvable hostname to a service that reports `Unimplemented`, TLS config error).

### 3.3 Health endpoint (`console-api/src/routes/health.rs`)

```rust
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let statuses = state.grpc_pool.status_snapshot();   // Vec<(ServiceName, ChannelStatus)>
    let checks: BTreeMap<&'static str, &'static str> = statuses.iter().map(|(svc, st)| {
        (svc.health_check_key(), st.as_health_string())
    }).collect();
    // plus database + rest probes (unchanged)
    Json(HealthResponse { checks, status: overall(&checks), version: env!("CARGO_PKG_VERSION").into() })
}
```

`ChannelStatus::as_health_string()` maps:
- `Ready` → `"ok"`
- `Connecting` | `TransientFailure` → `"degraded"`
- `Failed` | `Shutdown` → `"error"`

## 4. Internal Design

### 4.1 Startup sequence

```
fn main() -> …
  load config
  init logging
  connect db + migrate
  build taxonomy index (REST probe of wacp-runtime)
  → NEW: connect_pool(...) with retry-until-configured-deadline
  build AppState { db, taxonomy, settings, grpc_pool, … }
  spawn Axum server
```

The pool connect happens *after* the taxonomy load because taxonomy uses REST (not gRPC) and gives us an early signal that the runtime is reachable. If taxonomy load passes but pool connect fails, we know the runtime is up but gRPC ports are misconfigured — a specific, reportable condition.

### 4.2 Retry policy

Pool connect retries with exponential backoff (200 ms → cap 5 s) for `startup_timeout = 30 s` total. On timeout:
- If all three channels are `Connecting` or `TransientFailure`: return `Transient`. `main` logs an error and **exits with code 2** (retry at orchestrator level — docker will restart the container).
- If any channel is `Failed` (terminal): return `Terminal`. `main` logs the reason and **exits with code 1** (misconfiguration — do not retry).

### 4.3 Shutdown

On `SIGTERM` / `SIGINT`: Axum's graceful shutdown fires first (drains in-flight requests), then `pool.shutdown().await` drains the reconnect task and closes channels. Log format: `grpc pool drained agent=Shutdown highway=Shutdown coordinator=Shutdown in=<ms>`.

### 4.4 Health endpoint semantics

Before W1: health hits `TcpStream::connect` on each runtime port. Probe is one-shot and doesn't reflect real channel state under reconnect.

After W1: health reads `pool.status_snapshot()` which is always fresh (updated by the reconnect task). REST probe stays for the `rest` check because REST is not pooled.

## 5. Test Cases

### 5.1 Unit (in-crate, no I/O)

- **T1.1** `ChannelStatus::as_health_string()` maps each variant correctly — 5 cases.
- **T1.2** `overall()` returns `"healthy"` only if all checks are `"ok"`; `"degraded"` if any `"degraded"` and none `"error"`; `"unhealthy"` if any `"error"`.

### 5.2 Mock runtime

- **T1.3** Pool connects to `mock_grpc` server with all three services stubbed → 3 channels `Ready` within 500 ms.
- **T1.4** Pool connect with one service intentionally not bound → 2 channels `Ready`, 1 `TransientFailure`; startup returns `Transient` after timeout; `main` would exit code 2 (assert via `connect_pool()` direct call).
- **T1.5** Health endpoint with all `Ready` → `{"status":"healthy","checks":{…all "ok"}}`.
- **T1.6** Health endpoint with one `TransientFailure` → `{"status":"degraded",…}`.
- **T1.7** Pool shutdown drains cleanly — spawned 1k in-flight reconnect loops, `shutdown().await` completes in ≤ 1 s.

### 5.3 Real runtime (deferred to W7 sweep, but advisory here)

W1 alone does not ship real-runtime tests; W7 will replay these with the real binary. Advisory: when W7 runs, expect `/api/health` within 5 s of runtime start to report all `"ok"`.

## 6. Acceptance Criteria

- [ ] `cargo check -p console-api` and `cargo check -p wacp-console` green.
- [ ] `cargo test -p console-api --lib` passes, including 6 new health-related tests.
- [ ] `cargo test -p console-runtime --lib grpc_pool::` still green (no regressions).
- [ ] Manual smoke: `wacp-runtime serve` in one shell, `wacp-console serve` in another → `curl -s http://[::1]:8080/api/health | jq .status` = `"healthy"`. Kill runtime → `"degraded"` (db stays ok while gRPC channels flip to `"error"`); db failure would be `"unhealthy"`. Restart runtime → `"healthy"` after next reconnect pass.
- [ ] `git grep 'TODO.*pool\|grpc_pool.*TODO' wacp-console/` returns zero.

### Deviations landed with the W1 commit

- **Status freshness.** The current `GrpcPool` does not run a background reconnect loop — statuses update only when the pool is explicitly told to reconnect (`reconnect_*()`) or when a call returns `None` and the caller triggers a reconnect. That means after a runtime crash the pool's status stays `Connected` until either a handler hits `None` on `.agent()` / etc., or W3's monitor reconnects from its stream driver. Acceptable for W1 (W2+ call sites + W3 monitor drive the refresh); a dedicated `PoolRefresh` task would add scope. If this becomes a usability issue we add a small tick-based refresh — tracked in `impl/wiring-phases.md` §7.
- **Overall status mapping.** Health returns `"degraded"` when db is `ok` but any runtime check fails (preserves the pre-W1 semantics from `health.rs`). Only db failure escalates to `"unhealthy"`.

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-wiring-phases | Wiring Phases | parent (§3 W1 row) |
| wcon-wiring-strategy | Wiring Strategy | parent (§4.1) |
| wcon-architecture | System Architecture | constrains (§4.1 connection model) |

*WACP Workspace — authored by AKIL Abderrahim and Claude Opus 4.6*
