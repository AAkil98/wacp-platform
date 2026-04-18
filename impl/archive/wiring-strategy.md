---
id: wcon-wiring-strategy
type: impl
status: final
created: 2026-04-15T03:00:00
revised: 2026-04-15T04:10:00
authors: [AAkil98, Claude Opus 4.6]
tags: [strategy, runtime, integration, monorepo, cross-cutting]
depends_on: [wcon-architecture, wcon-sessions, wcon-highway, wcon-merge-plan]
---

# Wiring Strategy — Console ↔ Runtime Integration

> **Scope.** This is a **cross-cutting document** spanning both `wacp/` (runtime, protocol, taxonomy) and `wacp-console/` (workbench, SPA). It describes how the two halves connect across process boundaries via gRPC and REST.
>
> **Positioning.** Lives at the **monorepo root** (`impl/archive/wiring-strategy.md` of `wacp-platform/`) — the canonical wiring plan for the workspace. Relocated here at M5; see `wacp-console/impl/merge-plan.md` for the merge procedure and `impl/merge-execution-log.md` for the execution record.
>
> **Relationship to `merge-plan.md`.** W0 (monorepo merge) is the precondition for the wiring phases W1–W6. The detailed merge procedure is at `wacp-console/impl/merge-plan.md`; this document only summarizes it and defers for detail.

## Table of Contents

- 1. Situation Assessment
- 2. Inventory of Hollow Code
- 3. The Monorepo Question
- 4. Wiring Plan
- 5. Execution Order
- 6. Risk Map

---

## 1. Situation Assessment

Six phases of code exist. The backend has 66 endpoints and 99 tests. The frontend has 37 files and 9,367 lines. But the system has never talked to the WACP runtime. Every gRPC call is stubbed. The session lifecycle transitions in SQLite without creating a single workspace. Gate approvals write to an audit log and go nowhere. The WebSocket sends a welcome message and then sits idle.

This is not a cosmetic gap. The Console's core value proposition — launch sessions, stream trail events, approve gates, see refusals — is entirely decorative right now. Phase 7 (distribution) would package a binary that can't coordinate anything.

The correct next step is not distribution. It's wiring.

---

## 2. Inventory of Hollow Code

### 2.1 gRPC Pool: Built, Never Instantiated

`console-runtime/src/grpc_pool.rs` defines `GrpcPool` with 3 Tonic channels (AgentService, HighwayService, CoordinatorService), connect/reconnect methods, and per-service health tracking.

**Problem:** It's never created. `AppState` doesn't have a `grpc_pool` field. `main.rs` doesn't instantiate it. No handler ever calls `.agent()`, `.highway()`, or `.coordinator()`.

### 2.2 Launch Flow: State Transitions Without Runtime

`console-api/src/routes/sessions.rs:445-454`:
```
// The actual gRPC launch sequence (4.6) runs here.
// For now, transition directly to active
sessions::transition_state(..., LAUNCHING, ACTIVE, ...).await?;
```

The spec says launch is a 5-step atomic sequence: CreateSession → SubmitGoal → per-assignment Dispatch + SendEnvelope → subscribe to 4 streams → mark active. Currently it's: SQLite UPDATE state='active'. No workspaces exist. `coordinator_workspace_id` stays NULL. Per-assignment `workspace_id` stays NULL.

### 2.3 Highway Actions: Audit-Only

All four highway endpoints (gate resolve, batch resolve, escalation respond, directive inject) have the same pattern:

```rust
// TODO: Forward to HighwayService.ResolveGate via gRPC when connected
log_audit(&state.db, AuditEntry { ... }).await.ok();
```

They record the user's intent in the audit log but never forward it to the runtime. A gate approval doesn't actually unblock anything. An injected directive doesn't reach any agent.

### 2.4 Session Monitor: Not Started

The spec calls for one Tokio task per active session, subscribing to 4 gRPC streams (StreamTrail, StreamGates, StreamEscalations, StreamWorkspaceChanges), aggregating events into in-memory state, driving lifecycle transitions, and broadcasting to WebSocket subscribers.

**Current state:** The WebSocket upgrade endpoint holds the connection open. The session Zustand store has fields for trail/gates/escalations/refusals/workspaces. But there's no Tokio task, no stream subscription, no event aggregation. The `useSessionStream` hook connects to a WebSocket that never sends anything after the welcome message.

### 2.5 Cross-Session Endpoints: Hardcoded Empty

```rust
// GET /api/gates/pending
Ok(Json(serde_json::json!({ "items": [] })))
```

The nav badge in the frontend reads from these endpoints. They always return empty arrays because no monitor populates them.

### 2.6 Cancellation Cleanup: Empty Match Arms

```rust
CancelAction::AbortWorkspace => {
    // Full abort via CoordinatorService.AbortWorkspace
    // (will be implemented with the gRPC client pool integration)
}
```

Cancelling an active session marks it `cancelled` in SQLite but doesn't tell the runtime to stop.

### 2.7 Recovery: Query Exists, Wiring Doesn't

`sessions::list_active()` can find sessions in `state = 'active'`. But startup doesn't call it, doesn't re-subscribe to streams, doesn't verify workspace state with the coordinator.

### 2.8 Summary Table

| Component | Lines of Code | Functional? |
|-----------|--------------|-------------|
| Auth/users/tokens/audit/settings | ~3,200 | Yes — self-contained, no runtime dependency |
| Taxonomy index + discovery endpoints | ~1,800 | Partially — loads from REST API, serves queries, but requires running runtime for verticals |
| Profile validation/CRUD/export/import | ~2,400 | Yes — self-contained, validates against taxonomy index |
| Session state machine + validation | ~500 | Yes — logic is correct, tested |
| Session CRUD endpoints | ~600 | Partially — creates/reads from SQLite, but launch/cancel don't contact runtime |
| gRPC pool | ~160 | No — never instantiated |
| Launch flow | ~50 | No — skips gRPC entirely |
| Session monitor | 0 | No — doesn't exist |
| Highway actions | ~250 | No — audit log only |
| WebSocket forwarding | ~100 | No — sends welcome, then idle |
| Cross-session queries | ~30 | No — hardcoded empty |
| Recovery | ~10 | No — not wired |
| Frontend (all surfaces) | ~9,400 | Structurally complete — renders UI, makes API calls, but receives no real-time data |

---

## 3. The Monorepo Question

### 3.1 Current Cross-Repo Coupling

```
mada/
├── wacp/                  # Protocol runtime (16 crates)
│   ├── crates/
│   │   ├── wacp-runtime/  # gRPC services + REST gateway
│   │   ├── wacp-taxonomy/ # ← console depends on this (path dep)
│   │   ├── wacp-types/    # ← console depends on this (path dep)
│   │   └── …13 more
│   ├── proto/             # .proto files (5)
│   ├── highway-ui/        # Connect-Web SPA (separate from console's frontend)
│   └── tests/
│
└── wacp-console/          # Management workbench (6 crates + frontend)
    ├── crates/
    │   ├── console-runtime/ # build.rs reads ../../../wacp/proto/*.proto
    │   └── …5 more
    └── frontend/          # REST + OpenAPI codegen
```

**Coupling points:**
- `wacp-taxonomy = { path = "../wacp/crates/wacp-taxonomy" }` — relative path dep
- `wacp-types = { path = "../wacp/crates/wacp-types" }` — relative path dep
- `tonic-build` reads `../../../wacp/proto/*.proto` — fragile 3-level relative path
- `wacp-console`'s CI already checks out both repos side-by-side (merge was anticipated)

### 3.2 The Decision

**The two repos are not independent.** They share proto contracts, type crates, and must version-lock. A proto change on one side breaks the other immediately. There is no scenario where you ship one without the other — the Console is the product layer on top of the runtime.

**Merge is the right call.** Key benefits in one table:

| Factor | Separate | Merged |
|--------|----------|--------|
| Proto contract changes | Break console silently; discovered later in CI | Caught in same commit; `cargo check` fails immediately |
| Type changes (`wacp-types`) | Path dep works but no lockfile coordination | Single `Cargo.lock`; types are workspace members |
| Integration tests | Must check out both repos | Single workspace; test crate imports both sides |
| CI | Two workflows, coordinated checkout | Per-project workflows with `paths:` filters, one checkout |
| Proto codegen | `../../../wacp/proto` from console's `build.rs` | Shared `wacp-proto` crate, one codegen pass |
| Release | Two binaries, two repos, manual version sync | One workspace, `cargo-dist` from same tag |

### 3.3 Retrospective — Merge Executed 2026-04-15

**The merge (W0) is done.** The umbrella repo at `/home/aakil98/mada/wacp-platform/` carries the full merged state on `main`, tagged `monorepo-v0` at commit `d26ec80`. Both `wacp/` and `wacp-console/` are subtrees with preserved history; source repos are frozen at `pre-monorepo-{wacp,console}` for 30-day rollback.

- **Procedure:** `wacp-console/impl/merge-plan.md` (7 milestones M0 → M7).
- **Execution record:** `impl/merge-execution-log.md` — per-milestone commit anchors, 6 deviations (§4.1–§4.6), §6 validation outcomes.
- **Outcome:** §6 checklist green modulo pre-existing drift (logged §4.5) and Docker/UI items deferred to W1 browser smoke + pre-release infra (logged §4.6).
- **Actual effort:** ~4 working hours end-to-end vs. the 1–2 day estimate, largely because §10 pre-flight surfaced every ambiguity before execution.

### 3.4 What NOT to Do

Do **not** collapse the architectural boundary. The Console process and the Runtime process remain separate binaries. The Console connects to the Runtime via gRPC and REST. Merging repos does not mean merging processes.

The merge is about **development ergonomics** — one checkout, one build, one test suite, one lockfile. The runtime is still `wacp-runtime serve` and the console is still `wacp-console serve` (or whatever the final binary names are). They just live in the same workspace.

---

## 4. Wiring Plan

Assuming the merge is done (or not — wiring works either way), here's how to make the Console talk to the runtime.

### 4.1 Phase W1: gRPC Pool → AppState

**What:** Instantiate `GrpcPool` in `main.rs`, add it to `AppState`, connect on startup.

**Files:**
- `console-api/src/lib.rs` — add `pub grpc_pool: Arc<GrpcPool>` to `AppState`
- `console/src/main.rs` — create pool, call `pool.connect()`, inject into AppState

**Validation:** Health endpoint reports `runtime_agent: ok/error` from pool status instead of TCP probe.

### 4.2 Phase W2: Launch Flow

**What:** Replace the SQLite-only state transition with the real 5-step gRPC sequence.

**Steps:**
1. `CoordinatorService::CreateSession` → get `coordinator_workspace_id` → store in sessions table
2. `CoordinatorService::SubmitGoal` → submit workflow description
3. For each assignment: `CoordinatorService::Dispatch` → get `workspace_id` → store in session_assignments
4. For each assignment: `AgentService::SendEnvelope` → send directive with LLM config, tools, context
5. If all succeed → transition to ACTIVE. If any fail → transition to FAILED with reason.

**Files:**
- New: `console-core/src/session_launcher.rs` — the 5-step sequence
- Modified: `console-api/src/routes/sessions.rs` — `launch_session` calls the launcher instead of direct state transition

**Validation:** After launch, `sessions.coordinator_workspace_id` is non-NULL. Each assignment's `workspace_id` is non-NULL. The runtime's `GET /v1/workspaces` shows the created workspaces.

### 4.3 Phase W3: Session Monitor

**What:** One Tokio task per active session. Subscribes to 4 gRPC streams from HighwayService. Aggregates events. Updates DB on state transitions. Broadcasts to WebSocket subscribers.

**Architecture:**
```
SessionMonitor
├── spawn(session_id, grpc_pool, db, ws_broadcast)
│   ├── HighwayService::StreamTrail(workspace_ids) → trail channel
│   ├── HighwayService::StreamGates(workspace_ids) → gates channel
│   ├── HighwayService::StreamEscalations(workspace_ids) → escalations channel
│   └── HighwayService::StreamWorkspaceChanges(workspace_ids) → workspaces channel
│
├── Event loop:
│   ├── Trail entry → enrich (workspace label, checkpoint schema) → broadcast
│   ├── Gate event → add to pending_gates → broadcast → start timeout timer
│   ├── Escalation → add to pending_escalations → broadcast
│   ├── Workspace change → update state map → check completion → broadcast
│   ├── Completion detected → transition session to COMPLETED → broadcast → stop
│   └── Stream error → retry with backoff → 30 failures → FAILED → stop
│
└── Cleanup:
    ├── Drop broadcast channel
    ├── Update session state in DB
    └── Remove from active sessions map
```

**Files:**
- New: `console-core/src/session_monitor.rs` — the monitor task
- New: `console-core/src/refusal_synthesizer.rs` — detect refusal trail entries, resolve policy metadata
- New: `console-core/src/event_enricher.rs` — workspace labels, checkpoint field schemas
- Modified: `console-api/src/routes/ws.rs` — subscribe to monitor's broadcast channel instead of sitting idle
- Modified: `console-api/src/lib.rs` — add `ActiveSessions` map to AppState

**Validation:** Launch session against mock runtime → trail entries appear on WebSocket → gates appear → approve via API → workspace resumes.

### 4.4 Phase W4: Highway Forwarding

**What:** Replace audit-only stubs with real gRPC calls.

**Changes per endpoint:**
- `resolve_gate` → `HighwayService::RespondToGate(gate_id, decision, reason)` → then audit log
- `batch_resolve` → loop of `RespondToGate` calls with partial failure tracking
- `respond_escalation` → `HighwayService::RespondToEscalation(escalation_id, response)` → then audit log
- `inject_directive` → `HighwayService::InjectEnvelope(workspace_id, envelope)` → then audit log

**Files:**
- Modified: `console-api/src/routes/highway.rs` — add gRPC calls before audit logging

**Validation:** Approve gate → mock runtime confirms workspace resumes → workspace change event on WebSocket.

### 4.5 Phase W5: Cancellation + Recovery

**What:** Wire the empty cancel match arms and startup recovery.

**Cancel:**
- `BestEffortAbort` → `CoordinatorService::AbortWorkspace(coordinator_workspace_id)`, tolerate failure
- `AbortWorkspace` → same call, but fail the cancel if abort fails

**Recovery:**
- On startup, call `sessions::list_active()` → for each, verify workspace state via `CoordinatorService::GetWorkspace` → re-subscribe to 4 streams via new monitor task → if any verification fails, mark `FAILED` with `recovery_failed` reason

**Files:**
- Modified: `console-api/src/routes/sessions.rs` — fill cancel match arms
- New: `console-core/src/recovery.rs` — startup recovery sequence
- Modified: `console/src/main.rs` — call recovery after pool connect

**Validation:** Cancel active session → runtime confirms abort → workspaces stop. Restart server → active sessions resume streaming.

### 4.6 Phase W6: Cross-Session + Pending Endpoints

**What:** Wire the hardcoded-empty endpoints to the monitor's in-memory state.

**Changes:**
- Add `ActiveSessionsMap` (a `HashMap<session_id, SessionMonitorHandle>`) to AppState
- `GET /api/gates/pending` → iterate all monitors, collect pending gates, filter by ownership
- Same for escalations and refusals

**Files:**
- Modified: `console-api/src/routes/highway.rs` — read from monitor handles
- Modified: `console-api/src/lib.rs` — add `active_sessions: Arc<RwLock<HashMap<...>>>`

**Validation:** Two active sessions with pending gates → endpoint returns merged list, ownership-scoped.

---

## 5. Execution Order

```
[W0 DONE]  Merge repos — tag monorepo-v0 (2026-04-15)
  │        see impl/merge-execution-log.md for record
  │
W1: gRPC Pool → AppState                   ~2 hours
  │
W2: Launch flow                             ~1 day
  │     (requires understanding the runtime's
  │      CreateSession/Dispatch/SendEnvelope
  │      request/response shapes)
  │
W3: Session monitor                         ~2 days
  │     (critical path — 4 concurrent streams,
  │      event aggregation, lifecycle derivation,
  │      broadcast fan-out, reconnect + gap recovery)
  │
W4: Highway forwarding                      ~4 hours
  │     (mechanical — gRPC call before the existing
  │      audit log, on all 4 highway endpoints)
  │
W5: Cancel + recovery                       ~4 hours
  │
W6: Cross-session endpoints                 ~2 hours
  │
W7: Integration tests                       ~1 day
        (real wacp-runtime as child process,
         full lifecycle, 4-stream assertion)
```

**Total remaining effort:** 5–6 working days.

**Critical path:** W3 (session monitor) — 4 concurrent streams, aggregation, reconnect, completion detection, broadcast fan-out. Every other phase is mechanical wiring.

**Phase breakdown with per-phase deliverables, test strategy, and acceptance bar:** see `impl/archive/wiring-phases.md`. Each phase has a coding spec at `wacp-console/specs/coding/wcon-w{1..7}-*.md` that drills into function signatures, types, and test cases.

### 5.1 Pre-Flight — Done at M7 §6

All four pre-flight checks in the pre-merge version of this document were verified at M7 validation; evidence is in `impl/merge-execution-log.md` §3 M7 table. Quick summary:

1. ✓ `cargo run -p wacp-runtime -- serve --config dev/runtime.yaml` starts, loads 7 verticals, serves REST on `[::1]:9093`.
2. ✓ `curl http://[::1]:9093/v1/verticals` returns fixture data (2397 bytes).
3. ✓ `cargo run -p wacp-console -- serve` boots, loads taxonomy via REST (roles=37, tools=68, verticals=7), serves rust-embed'd SPA on `[::1]:8080`.
4. ✓ `/api/health` reports `runtime_{agent,coordinator,highway,rest}` all `"ok"` — the four runtime ports are reachable from the console process.

The Console talks to the Runtime's REST surface cleanly. The gRPC surface is unused (no pool instantiated yet) — that's W1.

---

## 6. Risk Map

| Risk | Impact | Mitigation |
|------|--------|------------|
| Proto message shapes don't match expectations | Launch flow builds wrong requests → runtime rejects | Read the actual `.proto` files and runtime handler code before writing client calls. The proto definitions are the contract. |
| Mock runtime is too different from real runtime | Tests pass against mock but fail against real | Write integration tests that use the real `wacp-runtime` binary (started as a child process). Mock runtime stays for unit-level tests. |
| Stream reconnection during active session | Monitor loses events, session state drifts | Implement gap recovery: on reconnect, re-fetch workspace state via `GetWorkspace`, task state via `GetTaskGraph`. Don't replay trail — just resume from current point. |
| Concurrent session monitors compete for resources | Memory/CPU pressure with many active sessions | Each monitor is one Tokio task with bounded buffers (trail_buffer_size). Profile memory per session before scaling. |
| WebSocket broadcast backpressure | Slow frontend consumer blocks broadcast | Use `tokio::sync::broadcast` with bounded capacity. Slow receivers get `Lagged` error — drop them with warning (spec §2.2: "slow consumers dropped"). |
| Recovery on startup with stale sessions | Server crashes, restarts, finds "active" sessions whose runtime workspaces no longer exist | Recovery verifies each workspace via `GetWorkspace`. If verification fails → mark FAILED. Don't silently resume stale sessions. |

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-architecture | System Architecture | constrains (§4.1 connection model, §7 monitor, §8.6 auth) |
| wcon-sessions | Session System | implements (§4 launch, §6 monitor, §7.3 cancel, §8.2 recovery) |
| wcon-highway | Highway Integration | implements (§2.2 WebSocket, §4 gates, §5 escalations, §4A refusals) |
| wcon-merge-plan | Monorepo Merge Plan | precedes (W0 precondition, executed 2026-04-15) |
| wcon-merge-execution-log | Merge Execution Log | records W0 execution evidence + §6 outcomes |
| wcon-wiring-phases | Wiring Phases | derives (per-phase deliverables, tests, acceptance bars) |
| wcon-w1-grpc-pool | W1 — gRPC Pool → AppState | implements §4.1 |
| wcon-w2-launch-flow | W2 — Launch Flow | implements §4.2 |
| wcon-w3-session-monitor | W3 — Session Monitor | implements §4.3 |
| wcon-w4-highway-forwarding | W4 — Highway Forwarding | implements §4.4 |
| wcon-w5-cancel-recovery | W5 — Cancel & Recovery | implements §4.5 |
| wcon-w6-cross-session | W6 — Cross-Session Endpoints | implements §4.6 |
| wcon-w7-integration-tests | W7 — Integration Tests | validates W1–W6 |

*WACP Workspace — authored by AKIL Abderrahim and Claude Opus 4.6*
