---
id: wcon-w3-session-monitor
type: coding
status: final
created: 2026-04-15T04:30:00
revised: 2026-04-15T04:30:00
authors: [AAkil98, Claude Opus 4.6]
tags: [wiring, w3, monitor, streams, broadcast, websocket, highway, critical-path]
depends_on: [wcon-w2-launch-flow, wcon-wiring-phases, wcon-highway, wcon-sessions]
---

# W3 — Session Monitor *(critical path)*

## Table of Contents

- 1. Scope
- 2. Dependencies
- 3. Types & Signatures
- 4. Internal Design
- 5. Test Cases
- 6. Acceptance Criteria

---

## 1. Scope

Stand up one `SessionMonitor` Tokio task per ACTIVE session. Subscribe to `HighwayService`'s four streams, aggregate events, enrich with taxonomy data, drive session lifecycle transitions, and broadcast enriched frames to WebSocket subscribers via a bounded `tokio::sync::broadcast`. Handle stream disconnects with exponential backoff + gap recovery. Detect session completion from terminal workspace states and cleanly retire the monitor.

**This is the load-bearing piece.** Every other wiring phase either depends on the monitor's broadcast channel (W6) or interacts with the same lifecycle surface (W4, W5).

**Out of scope.** Highway endpoint forwarding (W4). Cancellation gRPC calls (W5). Cross-session aggregation endpoints (W6).

**Files touched.**
- New: `wacp-console/crates/console-core/src/session_monitor.rs`.
- New: `wacp-console/crates/console-core/src/event_enricher.rs`.
- New: `wacp-console/crates/console-core/src/refusal_synthesizer.rs`.
- Modified: `wacp-console/crates/console-api/src/routes/ws.rs` (subscribe to broadcast).
- Modified: `wacp-console/crates/console-api/src/lib.rs` (add `active_sessions` field — see W6, but the field lands in W3 because the monitor owns registration).
- Modified: `wacp-console/crates/console-core/src/lib.rs` (pub mod exports).

## 2. Dependencies

- **`wcon-w1-grpc-pool`**, **`wcon-w2-launch-flow`**: monitor needs pool + populated workspace IDs.
- **`wcon-highway` §3, §4, §4A, §5**: defines frame JSON schemas and refusal taxonomy the monitor must emit.
- **`wcon-sessions` §6 (monitor), §8.2 (recovery)**: lifecycle + completion semantics.
- **`wcon-architecture` §7**: concurrency model and bounded-buffer constraints.

## 3. Types & Signatures

### 3.1 Monitor lifecycle

```rust
pub struct SessionMonitor {
    session_id: SessionId,
    handle: SessionMonitorHandle,
}

#[derive(Clone)]
pub struct SessionMonitorHandle {
    session_id: SessionId,
    cmd_tx: mpsc::Sender<MonitorCmd>,
    broadcast_tx: broadcast::Sender<Frame>,
    pending: Arc<PendingState>,
}

impl SessionMonitor {
    pub async fn spawn(
        session_id: SessionId,
        workspace_ids: WorkspaceSet,
        pool: Arc<GrpcPool>,
        db: Arc<ConsoleDb>,
        enricher: Arc<EventEnricher>,
        refusals: Arc<RefusalSynthesizer>,
        cfg: MonitorConfig,
    ) -> SessionMonitorHandle;
}

#[derive(Debug)]
pub enum MonitorCmd {
    Shutdown,
    Snapshot(oneshot::Sender<MonitorSnapshot>),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Frame {
    pub session_id: SessionId,
    pub ts: chrono::DateTime<Utc>,
    pub payload: FramePayload,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FramePayload {
    Trail(EnrichedTrailEntry),
    Gate(EnrichedGate),
    Escalation(EnrichedEscalation),
    Refusal(Refusal),
    WorkspaceChange(WorkspaceChange),
    Lag { missed: u64 },
    MonitorError { reason: String, transient: bool },
}
```

### 3.2 Pending state (shared with W6)

```rust
pub struct PendingState {
    pub gates: RwLock<Vec<EnrichedGate>>,
    pub escalations: RwLock<Vec<EnrichedEscalation>>,
    pub refusals: RwLock<Vec<Refusal>>,
}
```

W6 reads from these to serve `/api/{gates,escalations,refusals}/pending`. W3 is the only writer.

### 3.3 MonitorConfig

```rust
pub struct MonitorConfig {
    pub broadcast_capacity: usize,                 // default 256
    pub reconnect_initial: Duration,               // default 200 ms
    pub reconnect_max: Duration,                   // default 30 s
    pub reconnect_failure_cap: u32,                // default 30 (→ FAILED after 30 consecutive failures)
    pub gap_recovery_min_elapsed: Duration,        // default 5 s (below this, skip re-fetch)
}
```

### 3.4 ActiveSessions in AppState

```rust
pub type ActiveSessionsMap = Arc<RwLock<HashMap<SessionId, SessionMonitorHandle>>>;

pub struct AppState {
    // …W1 fields…
    pub active_sessions: ActiveSessionsMap,   // NEW at W3 (used by W6)
}
```

Monitor spawn inserts; monitor drop removes.

## 4. Internal Design

### 4.1 Task topology

```
SessionMonitor task
 ├── 4 stream driver sub-tasks (one per highway stream)
 │    each sends into a single mpsc<StreamEvent>
 ├── main select! loop:
 │    ├── recv StreamEvent → dispatch to enricher / refusal synth → broadcast_tx.send
 │    ├── recv MonitorCmd (shutdown | snapshot)
 │    └── tick(heartbeat) → periodic DB state write + prune stale pending
 └── on exit: update sessions.state, remove from active_sessions, drop broadcast_tx
```

Stream driver sub-tasks own reconnect policy per-stream. A driver crash is contained — only its stream reopens.

### 4.2 Completion detection

On each `WorkspaceChange`, inspect `new_state`. When the *coordinator workspace* reaches a terminal state (`Completed`, `Failed`, `Aborted`), transition the session accordingly and broadcast a final frame:
- `Completed` → session `COMPLETED`.
- `Failed` → session `FAILED` with `reason` from runtime.
- `Aborted` → session `CANCELLED` if prior user-initiated cancel, else `FAILED`.

Monitor exits after final frame is broadcast + DB updated. Entry is removed from `active_sessions`.

### 4.3 Reconnect & gap recovery

Per stream:
1. On disconnect, back off `reconnect_initial → reconnect_max` with ×2 + jitter.
2. Count consecutive failures. At `reconnect_failure_cap`, emit `MonitorError { transient: false }`, transition session to FAILED, exit.
3. On successful reconnect: if elapsed since last success > `gap_recovery_min_elapsed`, call the one-shot recovery RPC for that stream:
   - `StreamTrail` gap → skip replay (trail entries are high-volume; don't backfill — just resume and emit a `Lag` frame).
   - `StreamGates` gap → `ListPendingGates(workspace_ids)` to get current pending set; diff against our pending state; emit new gates, drop resolved ones.
   - `StreamEscalations` gap → same pattern via `ListPendingEscalations`.
   - `StreamWorkspaceChanges` gap → `GetWorkspace(workspace_id)` for each workspace; rebuild state map; emit synthesized change events for deltas.
4. After recovery, broadcast a `Lag { missed }` frame to signal clients — they decide whether to reload.

### 4.4 Broadcast fan-out

- `tokio::sync::broadcast` with capacity `broadcast_capacity`.
- Slow consumer (`Lagged(n)`) — log `slow consumer dropped missed=n`, drop the receiver, do not retry on the sender side.
- Fast path — `broadcast_tx.send(frame)` is non-blocking (returns immediately if no receivers or if bounded buffer has room).

### 4.5 Event enrichment (`event_enricher.rs`)

```rust
pub struct EventEnricher {
    taxonomy: Arc<TaxonomyIndex>,
    workspace_labels: Arc<WorkspaceLabelCache>,
}

impl EventEnricher {
    pub fn enrich_trail(&self, raw: wacp_proto::TrailEntry, ctx: &SessionContext) -> EnrichedTrailEntry;
    pub fn enrich_gate(&self, raw: wacp_proto::Gate, ctx: &SessionContext) -> EnrichedGate;
    pub fn enrich_escalation(&self, raw: wacp_proto::Escalation, ctx: &SessionContext) -> EnrichedEscalation;
}
```

Enrichment adds:
- Workspace label (role display name from taxonomy).
- Checkpoint schema (for checkpoint trail entries — from `VerticalManifest.checkpoint_types`).
- Task graph context (task_id → task name) via a small LRU cache seeded from `GetTaskGraph`.

### 4.6 Refusal synthesis (`refusal_synthesizer.rs`)

```rust
pub struct RefusalSynthesizer {
    taxonomy: Arc<TaxonomyIndex>,
}

impl RefusalSynthesizer {
    /// Inspect a trail entry — if it represents a refusal, return a synthesized Refusal frame.
    pub fn detect(&self, entry: &wacp_proto::TrailEntry) -> Option<Refusal>;
}
```

Detection rules per `wcon-highway.md` §4A:
- Tool-layer refusal: entry with `kind=ToolCall` + `refusal_policy_id` set.
- Agent-layer refusal: entry with `kind=AgentDecision` + `refusal_reason` field.
- Coordinator-layer refusal: entry with `kind=CoordinatorDecision` + `refusal_kind`.

Each produces a `Refusal` payload with policy metadata resolved from taxonomy.

## 5. Test Cases

### 5.1 Unit — enricher / synthesizer

- **T3.1** `enrich_trail` with known role → sets `role_display_name`; with unknown role → sets `role_display_name = "unknown"` and logs once per session.
- **T3.2** `enrich_gate` with known checkpoint type → schema populated; unknown → schema `None`.
- **T3.3** `RefusalSynthesizer::detect` — three refusal variants produce correct `Refusal::{ToolLayer, AgentLayer, CoordinatorLayer}`.
- **T3.4** Non-refusal trail entry → `None`.

### 5.2 Mock runtime — streams

- **T3.5** Feed 100 trail + 10 gates + 3 escalations + 5 workspace-change events → monitor broadcasts 118 frames in stream-observed order (per-stream FIFO preserved; no cross-stream ordering promise).
- **T3.6** Slow receiver blocks 2s → `Lagged` emitted; fast receivers unaffected; audit log entry `slow_consumer_dropped session=… missed=…`.
- **T3.7** Reconnect: stream server closes, monitor reconnects within `reconnect_max`, resumes; broadcast shows `Lag` frame followed by recovered events.
- **T3.8** 30 consecutive stream failures → monitor transitions session to FAILED, emits `MonitorError { transient: false }` frame, exits, removes from `active_sessions`.
- **T3.9** Terminal workspace state → session `COMPLETED`, final frame sent, monitor exits.
- **T3.10** Shutdown command mid-stream → monitor drains broadcast, updates DB, exits within 500ms.

### 5.3 Mock runtime — enriched payloads

- **T3.11** Enriched trail entry matches the WebSocket JSON schema defined in `wcon-highway.md` §3.
- **T3.12** Enriched gate entry includes vertical context badges per `wcon-highway.md` §4.5.

### 5.4 Real runtime (some here, sweep in W7)

- **T3.13** End-to-end: launch session against `wacp-runtime serve`; assert trail events land on WS in ≤ 500ms of runtime emission (measured by timestamp delta).
- **T3.14** Runtime restart chaos: kill runtime mid-session, restart; monitor resumes, no duplicate frames, single `Lag` frame on recovery.

### 5.5 Memory

- **T3.15** 100 active monitors against mock runtime; each task's resident memory (approximate via `jemalloc` or `sysinfo`) < 50 MB. Regression test — run post-impl, record baseline; alert if > 1.5× baseline in future.

## 6. Acceptance Criteria

- [ ] Proto-review notes at `impl/notes/w3-stream-shapes.md` land before `session_monitor.rs`. Cite proto file + line for all four streams' Item types, ordering guarantees, terminal conditions.
- [ ] `cargo test -p console-core --lib session_monitor::` — all green, ≥ 15 tests.
- [ ] `cargo test -p console-core --lib event_enricher::` and `refusal_synthesizer::` — green.
- [ ] `cargo test -p console-api --lib routes::ws::` — green (WS subscribes to broadcast).
- [ ] Manual: launch session, open WS at `ws://[::1]:8080/api/ws?session_id=…`, observe `welcome` then trail/gate frames; kill runtime → `MonitorError { transient: true }` frame; restart runtime → `Lag` frame → normal resumption.
- [ ] Memory regression test T3.15 baseline recorded in this doc's §6 or a sibling note.
- [ ] No empty `ws::handle_socket` body; no `// TODO: subscribe to monitor` comments.

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-w2-launch-flow | W2 — Launch Flow | precedes (requires populated workspace_ids) |
| wcon-wiring-phases | Wiring Phases | parent (§3 W3 row) |
| wcon-highway | Highway Integration | constrains (§3 trail, §4 gates, §4A refusals, §5 escalations) |
| wcon-sessions | Session System | constrains (§6 monitor, §8.2 recovery completion detection) |
| wcon-architecture | System Architecture | constrains (§7 concurrency, bounded buffers) |

*WACP Workspace — authored by AKIL Abderrahim and Claude Opus 4.6*
