---
id: wcon-wiring-phases
type: impl
status: final
created: 2026-04-15T04:15:00
revised: 2026-04-15T04:15:00
authors: [AAkil98, Claude Opus 4.6]
tags: [wiring, phases, deliverables, tests, cross-cutting]
depends_on: [wcon-wiring-strategy, wcon-architecture, wcon-sessions, wcon-highway]
---

# Wiring Phases — Deliverables, Tests, Acceptance Bars

> Operational companion to `impl/wiring-strategy.md`. The strategy doc frames the "why" (the hollow-code inventory, the architectural shape). This doc frames the "what, in what order, done when." Each W-phase is one row in §3 below with explicit deliverables, a test strategy layered mock-then-real, and a binary acceptance bar. Coding specs at `wacp-console/specs/coding/wcon-w{1..7}-*.md` drill further into function signatures, types, and test cases.
>
> **Ground rule — no shortcuts.** Mock-runtime tests are the fast feedback loop; real-runtime integration tests are the ground truth. Both layers must be green before a phase closes. Skipping the real-runtime layer is how stream protocol bugs reach production — §7 of `wcon-wiring-strategy.md` calls this risk out explicitly.

## Table of Contents

- 1. Orientation
- 2. Dependency Graph
- 3. Phase Breakdown
- 4. Testing Strategy
- 5. Exit Criteria & Phase-Close Checklist
- 6. Timeline & Sequencing
- 7. Risk Deltas vs. Strategy §6

---

## 1. Orientation

W0 is closed (tag `monorepo-v0` at `d26ec80`). The remaining work is W1 → W7. Each phase produces at least one coding spec, a Rust code delta, a mock-layer test pass, and — for W2 onwards — a real-runtime integration test pass. Phases are **sequenced**, not parallel: W2 depends on W1's `AppState.grpc_pool` field; W3 depends on W2's workspace IDs being populated; W4–W6 depend on W3's broadcast channel and `ActiveSessions` map; W7 sweeps all of them.

Per-phase outputs:
- One coding spec at `wacp-console/specs/coding/wcon-w<N>-<slug>.md` — scope, types, signatures, test cases, acceptance.
- One or more commits under the scope `feat(wN): …` or `refactor(wN): …` so git history maps 1:1 to the phase plan.
- Updates to this file (mark the phase row "DONE" with commit SHA and any deviations).

## 2. Dependency Graph

```
W1 (pool → AppState)
 │
 ├──▶ W2 (launch flow) ──▶ W3 (monitor) ──▶ W4, W5, W6 (can parallelize once W3 broadcast ships)
 │                                 │
 │                                 └──▶ W7 (integration tests — real runtime child process)
 │
 └──▶ W4 reads pool for highway gRPC (can start W4 read-only tests once W1 is green, but cannot
      merge W4 before W3 because pending-gate and pending-escalation surfaces need the monitor's
      in-memory state — see W6 for cross-session endpoints that share the same dependency)
```

**Practical order:** W1 → W2 → W3 → (W4, W5, W6 in any order, ideally parallel) → W7.

**The critical path is W3.** Its length sets the end-date; everything else is ≤ 1 day of work.

## 3. Phase Breakdown

### W1 — gRPC Pool → AppState   *(DONE — commit TBD by post-commit amendment)*

**Estimate:** 2 hours (actual: ~1h30 end-to-end). **Coding spec:** `wcon-w1-grpc-pool`.

**Deviation noted in the coding spec §6:** the current `GrpcPool` has no background reconnect loop, so per-channel status is refreshed only when the pool is explicitly told to reconnect. After a runtime crash, health reports `runtime_rest: "error"` immediately (live HTTP probe) but the three gRPC rows stay at their last-known `"ok"` until a handler or the W3 monitor triggers `reconnect_*()`. This is acceptable for W1; a dedicated tick-based refresh is not in scope but tracked in §7 risk deltas below.

| Task | Deliverable | Test layer | Acceptance bar |
|------|-------------|------------|----------------|
| W1.1 Add `grpc_pool: Arc<GrpcPool>` to `AppState` | `console-api/src/lib.rs` | type-check only | `cargo check -p console-api` green |
| W1.2 Instantiate pool + `connect()` in `main.rs`; fail startup on non-transient errors, retry on transient | `console/src/main.rs` | mock: in-memory `GrpcPool` with injected dial failure → retry exits with backoff; real: boot against running runtime, assert pool reports 3 `Ready` channels | Boot logs show `grpc pool ready agent=Ready highway=Ready coordinator=Ready` within 5s of runtime availability |
| W1.3 Health endpoint reads pool status instead of TCP probes | `console-api/src/routes/health.rs` | mock: flip a channel to `Failed` → `/api/health` reports that channel `"degraded"`; real: kill runtime, observe `"error"` transition within 2s | All existing health tests pass; new tests cover `Ready` / `Connecting` / `Failed` / `TransientFailure` states |
| W1.4 Graceful shutdown: drain pool on SIGTERM | `console/src/main.rs` | mock: send SIGTERM during active connections → all channels closed cleanly | No panics; shutdown log `grpc pool drained in <N>ms` |

**Phase-close bar:**
- `cargo test -p console-api --lib health::` — all green.
- Manual: `wacp-console serve` in one shell, `wacp-runtime serve` in another, `curl /api/health` → all four runtime channels `"ok"`.
- Kill runtime → `curl /api/health` within 5s shows the three gRPC channels as `"error"` and `rest` as `"error"` too; restart runtime → all return to `"ok"` within 5s (pool reconnect).

---

### W2 — Launch Flow

**Estimate:** 1 working day. **Coding spec:** `wcon-w2-launch-flow`.

| Task | Deliverable | Test layer | Acceptance bar |
|------|-------------|------------|----------------|
| W2.1 Review proto shapes for `CreateSession`, `SubmitGoal`, `Dispatch`, `SendEnvelope` | notes in coding spec | — | Request/response fields documented; validation points identified |
| W2.2 `session_launcher.rs` implementing the 5-step atomic sequence | new `console-core/src/session_launcher.rs` | mock: all 5 steps happy path, then one failure injection per step (5 cases) | Each failure path produces a `LaunchError` variant with `step` + `reason` + recoverable/terminal flag |
| W2.3 Per-step compensation / rollback | same | mock: step 4 fails after steps 1–3 succeed → step 5 issues `AbortWorkspace` for every dispatched workspace; no orphan workspaces remain | Real runtime test: force dispatch #2 to fail (via mock LLM rejection) → session FAILED, runtime shows 0 active workspaces within 10s |
| W2.4 Replace SQLite-only transition at `sessions.rs:445-454` with `launcher.launch(session_id)` | `console-api/src/routes/sessions.rs` | mock + real: successful launch populates `sessions.coordinator_workspace_id` and every assignment's `workspace_id` non-NULL | Real: runtime `GET /v1/workspaces` lists created workspaces; `session_launcher.rs` returns `LaunchOutcome::Active` |
| W2.5 Metrics / structured logging per step | same | — | Each step emits one log event with `session_id`, `step`, `duration_ms`, `outcome` |

**Phase-close bar:**
- `cargo test -p console-core --lib session_launcher::` — all green, including 6+ failure-injection tests.
- Real: launch a session end-to-end against `wacp-runtime serve`; verify SQLite state (`sqlite3 console.db "SELECT coordinator_workspace_id FROM sessions WHERE id=?"`) and runtime state (`curl /v1/workspaces`).
- No `// TODO` remains in `sessions.rs:445-454`.

---

### W3 — Session Monitor *(critical path)*

**Estimate:** 2 working days. **Coding spec:** `wcon-w3-session-monitor`.

| Task | Deliverable | Test layer | Acceptance bar |
|------|-------------|------------|----------------|
| W3.1 Proto review: `StreamTrail`, `StreamGates`, `StreamEscalations`, `StreamWorkspaceChanges` request + response types | notes in coding spec | — | Each stream's `Item` shape, ordering guarantees, and terminal conditions documented |
| W3.2 `SessionMonitor` task: 4 streams in a `tokio::select!`, internal command channel for cancel/shutdown | new `console-core/src/session_monitor.rs` | mock: stream mock that feeds deterministic frames; assert monitor forwards them in-order | Unit test: feed 100 trail + 10 gates + 3 escalations + 5 workspace-change events, monitor broadcasts 118 frames in expected order |
| W3.3 Event enrichment: workspace label lookup, checkpoint schema resolution | new `console-core/src/event_enricher.rs` | mock with taxonomy fixture | Enriched events match `wcon-highway.md` §3 / §4 JSON schema |
| W3.4 Refusal synthesis: detect refusal trail entries, resolve policy metadata | new `console-core/src/refusal_synthesizer.rs` | mock: feed three refusal variants (tool-layer, agent-layer, coordinator-layer) | Each produces a correctly typed frame per `wcon-highway.md` §4A |
| W3.5 Broadcast fan-out via `tokio::sync::broadcast` (bounded) | monitor module | mock: 4 receivers, 1 slow (blocks 2s); assert `Lagged` on the slow one, fast three unaffected | `slow_receiver_dropped=true` audit entry; no backpressure on the hot path |
| W3.6 WS endpoint subscribes to the monitor's broadcast | `console-api/src/routes/ws.rs` | mock: start monitor, open WS, assert `welcome` then subsequent frames arrive | Real: launch session, connect WS, observe trail events land in < 500ms of runtime emission |
| W3.7 Reconnect + gap recovery (exponential backoff, re-fetch workspace state via `GetWorkspace`, task state via `GetTaskGraph`) | monitor module | mock: kill stream server after N events, restart, assert monitor resumes without duplicates and without gap | Real: kill `wacp-runtime`, restart → monitor resubscribes; session state stays consistent; WS clients see no dropped frames modulo explicit `lag` markers |
| W3.8 Completion detection: terminal workspace state → session `COMPLETED` → monitor drops | monitor module | mock: feed terminal workspace-change → assert DB row updates to `completed` and monitor task terminates | Real: complete session end-to-end; session row becomes `completed`; monitor handle removed from `ActiveSessions` |
| W3.9 Failure backoff ceiling: 30 consecutive stream failures → session `FAILED` | monitor module | mock: fail all 4 streams 30x in a row | Session row transitions to `failed` with `reason='stream_backoff_exceeded'`; monitor drops |

**Phase-close bar:**
- `cargo test -p console-core --lib session_monitor::` and `event_enricher::` and `refusal_synthesizer::` — all green.
- Real-runtime integration test: launch → trail → gate → approve → workspace resume → complete, verifying both DB state and WS frame stream.
- Monitor memory bounded: 100-task workload profile shows < 50 MB resident per monitor (exit criterion from `wcon-wiring-strategy.md` §6 risk row on concurrent monitors).

---

### W4 — Highway Forwarding

**Estimate:** 4 hours. **Coding spec:** `wcon-w4-highway-forwarding`.

| Task | Deliverable | Test layer | Acceptance bar |
|------|-------------|------------|----------------|
| W4.1 `resolve_gate` + `batch_resolve` — `HighwayService::RespondToGate` before audit log | `console-api/src/routes/highway.rs` | mock: assert gRPC call happens first, then audit; runtime-reject surfaces as 502 Bad Gateway | Real: approve gate → workspace resumes within 2s; audit entry carries `gate_id` + `outcome=approved` |
| W4.2 `respond_escalation` — `HighwayService::RespondToEscalation` | same | mock + real | Real: resolve escalation → trail shows ack event; escalation removed from pending |
| W4.3 `inject_directive` — `HighwayService::InjectEnvelope` | same | mock + real | Real: inject directive → target workspace receives envelope; trail entry appears |
| W4.4 Partial-failure semantics for `batch_resolve` | same | mock: 3 gates, 1 fails at runtime; outcome reports `[ok, err, ok]` | Response schema matches `wcon-api.md` batch contract |
| W4.5 Ordering invariant: if gRPC call fails, do not audit | same | mock: runtime rejects gate → no audit row inserted | New test explicit; acts as spec of the invariant |

**Phase-close bar:**
- `cargo test -p console-api --lib routes::highway::` — all green (existing + new).
- Real: full integration test launches session, drives gate, approves, watches trail resume.
- Audit table invariant check: `SELECT COUNT(*) FROM audit WHERE kind='gate_decision' AND decision IS NULL` = 0.

---

### W5 — Cancel & Recovery

**Estimate:** 4 hours. **Coding spec:** `wcon-w5-cancel-recovery`.

| Task | Deliverable | Test layer | Acceptance bar |
|------|-------------|------------|----------------|
| W5.1 Fill `CancelAction::BestEffortAbort` — `CoordinatorService::AbortWorkspace`, tolerate failure | `console-api/src/routes/sessions.rs` | mock: runtime returns Unavailable → session still cancels with `reason='best_effort_abort_failed'` | Real: cancel active session → runtime workspace stops OR best-effort reason recorded |
| W5.2 Fill `CancelAction::AbortWorkspace` — same call but cancel fails hard if abort fails | same | mock: runtime returns Unavailable → session stays in prior state, 502 to client | Real: equivalent behavior on a live workspace |
| W5.3 `recovery.rs` — on startup, `sessions::list_active()` → for each, `CoordinatorService::GetWorkspace` → respawn monitor OR mark FAILED | new `console-core/src/recovery.rs` | mock: mixed inputs (live workspace, dead workspace) → correct partition | Real: restart console while 2 sessions active; both monitors resume; WS clients reconnect |
| W5.4 Hook `recovery::run()` after pool connect in `main.rs` | `console/src/main.rs` | — | Startup sequence order: pool → recovery → listen. Recovery failure does not block startup but logs + metrics |
| W5.5 Recovery boundary test: workspace that existed pre-restart but no longer in runtime | same | mock: `GetWorkspace` returns NotFound → session marked FAILED with `reason='recovery_workspace_missing'` | Real: kill runtime, drop its data dir, restart both → recovery surfaces FAILED sessions |

**Phase-close bar:**
- `cargo test -p console-core --lib recovery::` and `cargo test -p console-api --lib routes::sessions::cancel::` — all green.
- Real: crash-restart chaos test (kill console during session, restart → session visible, monitor running, WS reconnects).
- No empty `match` arms remain in `sessions.rs` cancel handler.

---

### W6 — Cross-Session Endpoints

**Estimate:** 2 hours. **Coding spec:** `wcon-w6-cross-session`.

| Task | Deliverable | Test layer | Acceptance bar |
|------|-------------|------------|----------------|
| W6.1 Add `active_sessions: Arc<RwLock<HashMap<session_id, SessionMonitorHandle>>>` to AppState | `console-api/src/lib.rs` | — | Monitor spawn adds entry; monitor drop removes entry; atomic under racing spawns |
| W6.2 `GET /api/gates/pending` reads from monitor handles, filters by owner | `console-api/src/routes/highway.rs` | mock: 2 monitors, each with 3 pending gates, 1 non-owner → response lists 4 owner-visible | Response conforms to `wcon-api.md`; pagination metadata correct |
| W6.3 `GET /api/escalations/pending` — same pattern | same | mock | Same bar |
| W6.4 `GET /api/refusals/pending` — same pattern | same | mock | Same bar |
| W6.5 Ownership enforcement (403 on non-owner queries when scope is session-specific) | same | mock: forge a cross-owner request → 403 | Response body matches `wcon-auth.md` error schema |

**Phase-close bar:**
- `cargo test -p console-api --lib routes::highway::pending::` — all green.
- Real: two concurrent sessions with different owners → each sees only their own pending items.
- No hardcoded `Ok(Json(json!({"items": []})))` left in the three endpoints.

---

### W7 — Integration Tests *(validation of W1–W6)*

**Estimate:** 1 working day. **Coding spec:** `wcon-w7-integration-tests`.

| Task | Deliverable | Test layer | Acceptance bar |
|------|-------------|------------|----------------|
| W7.1 Test harness: spawn `wacp-runtime` as child process with fixture data dir + temp SQLite | new test crate or `wacp-console/crates/console-test-support::real_runtime` module | — | `start_runtime()` returns a handle that owns the child, exposes ports, implements `Drop` for cleanup; no zombie processes on test panic |
| W7.2 Full happy-path lifecycle test: boot runtime, boot console, login, create profile, launch, stream, approve gate, complete | test crate | real | Single test runs end-to-end in ≤ 30s; artifacts cleaned up |
| W7.3 Runtime-restart mid-session | same | real | Session survives, WS reconnects, monitor resubscribes |
| W7.4 Partial-launch failure | same | real (forced via LLM mock) | Session FAILED, no orphan workspaces |
| W7.5 Concurrent sessions: 10 in parallel | same | real | All 10 complete, no deadlocks, memory per-monitor < 50 MB |
| W7.6 WebSocket slow-consumer drop | same | real + forced client lag | Slow consumer `Lagged`, others unaffected, audit entry present |
| W7.7 Recovery on console restart | same | real | Restart console mid-session → monitor resubscribes, WS clients reconnect, no state drift |

**Phase-close bar:**
- `cargo test -p console-integration` (or wherever the harness lives) — all green, ≤ 2 min total runtime on developer hardware.
- CI wires the integration suite gated on `paths: wacp-console/**` or `wacp/**` so proto / runtime changes get tested end-to-end.
- No flakiness observed over 10 consecutive runs.

---

## 4. Testing Strategy

### 4.1 Layering Principle

| Layer | Substrate | Speed | Use For |
|-------|-----------|-------|---------|
| Unit | in-crate fakes, no I/O | milliseconds | state machine logic, enrichment, validation |
| Mock runtime | `console-test-support::mock_grpc` (in-process tonic server) | ~100 ms | gRPC request/response shape assertions, error-path coverage |
| Real runtime (child proc) | `wacp-runtime` binary, fixture verticals, temp data dir | ~2–5 s per test | end-to-end semantics, stream ordering, reconnect, completion detection |

**Rule.** Every phase delivers tests at layers 1 + 2 at minimum. W2 onwards adds layer 3. W7 is pure layer-3 sweep.

**Anti-rule.** Do not "promote" a layer-2 test to certify a layer-3 scenario. If the assertion is about reconnect / stream gap / concurrent sessions, it runs against the real binary. Mock runtime is for request-shape and error-path coverage, not for protocol semantics.

### 4.2 Mock Runtime Capabilities

`console-test-support::mock_grpc` (already present — see M3 commit `c713fcc`) provides:
- Injectable per-method response stubs (return Ok, Err, stream, stream-with-failure-at-N).
- Request capture for assertion.
- Per-stream controls: emit frame, close cleanly, close with error.

W3 tests in particular depend on injectable stream controls — verify this is in place before W3 starts.

### 4.3 Real Runtime Harness (W7)

Lives in the integration test crate. Responsibilities:
- Build `wacp-runtime` if not present; spawn with fixture config in a tempdir.
- Wait for `/healthz` `ready` before returning (bounded timeout).
- Forward runtime logs to test output on failure.
- On `Drop`: `SIGTERM`, then `SIGKILL` after 5s.
- Isolation: each test gets its own runtime instance on a random port (prevents flakes from shared state).

## 5. Exit Criteria & Phase-Close Checklist

Every phase closes on these five items — any red item blocks the merge.

- [ ] **Coding spec finalized.** Frontmatter `status: final`, deliverables list matches this doc's phase row.
- [ ] **All phase tests green.** Unit + mock + (for W2+) real-runtime integration subset.
- [ ] **No stub / TODO remains in scope.** Grep the files listed in "Deliverable" columns; zero residual `TODO` / `unimplemented!()` / empty match arms.
- [ ] **`wcon-wiring-phases.md` row marked DONE.** Append commit SHA(s) and any deviation notes inline.
- [ ] **Retrospective note.** One paragraph on what surprised, what took longer than estimate, what the next phase inherits.

## 6. Timeline & Sequencing

| Phase | Estimate | Earliest start | Latest finish (if sequential) |
|-------|----------|----------------|-------------------------------|
| W1 | 2 h | now | +2 h |
| W2 | 1 d | after W1 | +1 d 2 h |
| W3 | 2 d | after W2 | +3 d 2 h |
| W4 | 4 h | after W3 start (read-only) — but merge gated on W3 | +3 d 6 h |
| W5 | 4 h | after W3 | +4 d 2 h |
| W6 | 2 h | after W3 (needs `ActiveSessions` map) | +4 d 4 h |
| W7 | 1 d | after W6 | +5 d 4 h |

Parallelization opportunity after W3 ships: W4 / W5 / W6 can run concurrently if three people or three sessions exist. For single-thread execution, total = ~5 working days.

## 7. Risk Deltas vs. Strategy §6

Strategy §6 lists six risks. This table tracks mitigations per phase and flags the residuals.

| §6 Risk | Mitigated in | Residual after phase close |
|---------|--------------|----------------------------|
| Proto shape mismatch | W2.1, W3.1 coding spec task | Low — review tasks gate the code |
| Mock/real runtime divergence | W2, W3, W4, W5 each require real test | Low — layering rule enforces real-path coverage |
| Stream reconnect gap | W3.7 | Low if W3.7 tests green; monitor open-question: can `GetTaskGraph` return a *too-large* payload on long sessions? Profile during W7. |
| Pool status freshness (no background refresh in W1) | deferred | Low — every gRPC caller from W2 onward gets `Option<Client>`; `None` triggers `reconnect_*`. W3 monitor driver sub-tasks reconnect on stream failure. Escalate to a tick-based refresh only if the health endpoint's gRPC rows feel stale in practice. |
| Concurrent monitor resource pressure | W7.5 | Medium — profile against 10-session fixture; 100+ untested until after W7 |
| WS broadcast backpressure | W3.5 | Low — `Lagged` drop is the intended semantics |
| Stale session on restart | W5.3, W5.5 | Low — recovery partition test covers it |

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-wiring-strategy | Wiring Strategy | framed by (this doc operationalizes §4 and §5) |
| wcon-merge-execution-log | Merge Execution Log | precedes (W0 closed 2026-04-15) |
| wcon-architecture | System Architecture | constrains (§4.1 connection model, §7 monitor, §8.6 auth) |
| wcon-sessions | Session System | implements (§4 launch, §6 monitor, §7.3 cancel, §8.2 recovery) |
| wcon-highway | Highway Integration | implements (§2.2 WebSocket, §4 gates, §5 escalations, §4A refusals) |
| wcon-api | Console REST API | contracts (response schemas for pending + batch endpoints) |
| wcon-auth | Authentication & Authorization | constrains (ownership filters, 403 paths) |
| wcon-w1-grpc-pool | W1 coding spec | implements (§3 W1 row) |
| wcon-w2-launch-flow | W2 coding spec | implements (§3 W2 row) |
| wcon-w3-session-monitor | W3 coding spec | implements (§3 W3 row) |
| wcon-w4-highway-forwarding | W4 coding spec | implements (§3 W4 row) |
| wcon-w5-cancel-recovery | W5 coding spec | implements (§3 W5 row) |
| wcon-w6-cross-session | W6 coding spec | implements (§3 W6 row) |
| wcon-w7-integration-tests | W7 coding spec | implements (§3 W7 row) |

*WACP Workspace — authored by AKIL Abderrahim and Claude Opus 4.6*
