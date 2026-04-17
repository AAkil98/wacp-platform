---
id: wcon-w7-integration-tests
type: coding
status: final
created: 2026-04-15T04:50:00
revised: 2026-04-15T04:50:00
authors: [AAkil98, Claude Opus 4.6]
tags: [wiring, w7, integration, real-runtime, harness, end-to-end]
depends_on: [wcon-w6-cross-session, wcon-wiring-phases]
---

# W7 — Integration Tests

## Table of Contents

- 1. Scope
- 2. Dependencies
- 3. Types & Signatures
- 4. Internal Design
- 5. Test Cases
- 6. Acceptance Criteria

---

## 1. Scope

Stand up an integration test harness that spawns the real `wacp-runtime` binary as a child process, drives the console through a full session lifecycle, and verifies the four stream surfaces end-to-end. This is the ground-truth validation that complements the mock-runtime unit tests from W1–W6 — any divergence between mock and real behavior surfaces here.

**Files touched.**
- New crate: `wacp-console/crates/console-integration/` (or a new `tests/` directory within an existing crate if the team prefers — see §4.0 decision note).
- Possibly modified: `wacp-console/crates/console-test-support/src/lib.rs` — add `real_runtime` module if the harness lives there instead.
- New: CI workflow addition to run integration suite on PR (`ci-console.yml` gains a job).

## 2. Dependencies

- **W1 through W6 all merged.** W7 is a sweep; it does not constrain their design beyond the test-surface expectations in their specs.
- **`wacp-runtime` binary buildable** in the workspace (verified at M7 §6).
- **Fixture verticals** at `wacp/ecosystem/` (present and passing taxonomy load per M7 smoke).

## 3. Types & Signatures

### 3.1 Harness

```rust
pub struct RuntimeHarness {
    child: Child,
    pub agent_port: u16,
    pub highway_port: u16,
    pub coordinator_port: u16,
    pub rest_port: u16,
    data_dir: TempDir,
}

impl RuntimeHarness {
    /// Spawn wacp-runtime on random ports with fixture verticals in a tempdir.
    /// Blocks until /healthz returns `{"status":"ready"}` or timeout.
    pub async fn spawn() -> Result<Self, HarnessError>;

    pub async fn wait_ready(&self, timeout: Duration) -> Result<(), HarnessError>;

    pub fn runtime_url(&self) -> String;   // http://[::1]:<rest_port>
}

impl Drop for RuntimeHarness {
    fn drop(&mut self) {
        // SIGTERM, wait 5s, SIGKILL.
        let _ = self.child.kill();
    }
}
```

### 3.2 Console harness

```rust
pub struct ConsoleHarness {
    child: Child,
    pub port: u16,
    db_path: TempDir,
}

impl ConsoleHarness {
    pub async fn spawn(runtime: &RuntimeHarness) -> Result<Self, HarnessError>;
    pub fn base_url(&self) -> String;
}
```

Or the console may run in-process (via `wacp_console::serve_with_config(...)`) — choose based on whether in-process observability beats the cleaner isolation of a child process. For W7, default to in-process; child-process variant remains as an opt-in for rare cases.

### 3.3 Test client

A thin `reqwest`-based client for the REST API + `tokio-tungstenite` for the WS. Auth cookie handling via `reqwest::cookie_store`. Test user seeded via a helper that hits the bootstrap endpoint.

## 4. Internal Design

### 4.0 Decision — test crate vs. in-place tests

New `console-integration` crate gives:
- Isolation (doesn't pollute crate test trees).
- Workspace-level `cargo test -p console-integration` target.
- Single CI job.

Single-crate `tests/` gives:
- Simpler structure.
- Tests live close to what they test.

**Pick the crate approach.** Rationale: W7 harness needs to be reusable (future Wiring-phase-N regression tests, CI wiring, manual smoke scripts). Encapsulating in its own crate pays off as the suite grows.

### 4.1 Harness lifecycle

```
per test:
  let runtime = RuntimeHarness::spawn().await?;
  let console = ConsoleHarness::spawn(&runtime).await?;
  let client = TestClient::new(&console).await?;
  …test body…
  drop(client); drop(console); drop(runtime);
```

Tests cannot share harnesses (for isolation). Fast-test mode: reuse runtime harness if the test explicitly opts in via a `shared_runtime` fixture.

### 4.2 Port allocation

Random ephemeral port. Runtime's CLI takes `--listen-*` env-overrides (shipped in M5, §10.2). Harness sets these env-vars before spawning.

### 4.3 Test data

- Fixture user seed via bootstrap token.
- Fixture profile seed via REST (one profile, single-assignment, minimal tools).
- Fixture ecosystem from `wacp/ecosystem/` (taxonomy is pre-loaded by runtime).

### 4.4 Assertion utilities

```rust
pub async fn assert_ws_frame_within<F>(ws: &mut WebSocket, timeout: Duration, pred: F) -> Frame
    where F: Fn(&Frame) -> bool;
```

`assert_ws_frame_within` drains frames until predicate matches or timeout — essential for stream-based assertions that don't know exact inter-arrival timing.

### 4.5 CI integration

- New workflow job `integration` in `ci-console.yml` triggered on `wacp-console/**` and `wacp/**` path filters.
- Job runs `cargo test -p console-integration --test '*'` with `CARGO_BUILD_JOBS=2` (avoid memory pressure on GH runners).
- Integration suite is not on the lint CI (ci-lint.yml); only full console CI runs it.

## 5. Test Cases

### 5.1 Happy-path lifecycle

- **T7.1** Boot runtime + console, log in, create profile, launch session. Assert DB `coordinator_workspace_id` non-NULL and runtime `/v1/workspaces` lists the created workspace. Full sequence ≤ 30 s.
- **T7.2** Open WS, receive welcome, then trail entries for each task. Drive a gate (approve via REST), observe workspace resume on trail within 2 s.
- **T7.3** Session reaches COMPLETED (via scripted coordinator fixture) — WS emits final frame, session DB row shows `state='completed'`, monitor removed from `active_sessions`.

> **Deviation — T7.2 / T7.3 currently `#[ignore]`-ed.** The deterministic `wacp-llm` stub provider landed in audit §13.7.6 (see `wcon-llm-stub`) and is exercised end-to-end by `integration/tests/llm_stub_e2e.rs`. The remaining gap is runtime-side: `AgentService::Bind` does not return a real directive, `EmitSignal` does not advance the workspace FSM, and `CreateCheckpoint` does not fan into highway gates. Until those handlers are wired, T7.2 / T7.3 assert outcomes the runtime cannot produce. Each test carries an `#[ignore]` reason pointing at the specific missing wiring.

### 5.2 Failure / chaos

- **T7.4** Kill runtime mid-session → WS `MonitorError { transient: true }` → restart runtime → `Lag` frame → resumption.
- **T7.5** Partial-launch failure: mock LLM rejects on dispatch #2 → session FAILED; `curl /v1/workspaces` shows 0 live for this session (W2 rollback assertion).
- **T7.6** Console restart mid-session: kill console, restart → recovery runs, monitor resumes, new WS client connects and receives frames.

### 5.3 Concurrency & stress

- **T7.7** 10 concurrent sessions with staggered starts → all 10 complete; no deadlock; per-monitor memory bound.
- **T7.8** WS slow-consumer: one client sleeps 2 s between reads → `Lagged` log emitted; other clients unaffected.

### 5.4 Cross-session

- **T7.9** Two sessions launched by two users; user A's `/api/gates/pending` shows only A's gates; admin user sees both sets.
- **T7.10** W4/W6 sequence: approve a gate via W4 → W6 pending endpoint no longer lists it within 500 ms.

## 6. Acceptance Criteria

- [ ] `cargo test -p console-integration` green on developer machine in ≤ 2 min.
- [ ] All 10 test cases T7.1–T7.10 implemented.
- [ ] No flakes over 10 consecutive local runs (flaky tests either fixed or explicitly `#[ignore]`d with a linked issue).
- [ ] CI `integration` job added to `ci-console.yml`, gated on the right paths.
- [ ] Harness `Drop` impl cleans up runtime child process even on test panic (verified via `pgrep wacp-runtime` = 0 after test failure).
- [ ] Each W1–W6 phase's "Real runtime (W7 sweep)" note is satisfied by at least one of the W7 test cases.

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-wiring-phases | Wiring Phases | parent (§3 W7 row) |
| wcon-w1-grpc-pool | W1 — gRPC Pool → AppState | validates |
| wcon-w2-launch-flow | W2 — Launch Flow | validates |
| wcon-w3-session-monitor | W3 — Session Monitor | validates |
| wcon-w4-highway-forwarding | W4 — Highway Forwarding | validates |
| wcon-w5-cancel-recovery | W5 — Cancel & Recovery | validates |
| wcon-w6-cross-session | W6 — Cross-Session Endpoints | validates |

*WACP Workspace — authored by AKIL Abderrahim and Claude Opus 4.6*
