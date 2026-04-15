---
id: wcon-w3-stream-shapes
type: impl
status: final
created: 2026-04-15T06:10:00
revised: 2026-04-15T06:10:00
authors: [AAkil98, Claude Opus 4.6]
tags: [w3, proto, streams, highway, review]
depends_on: [wcon-w3-session-monitor]
---

# W3 — Stream-Shape Review (gate for session_monitor.rs)

> Review of the four `HighwayService` server-streaming RPCs the session monitor consumes. Cites proto file + line number for every field. Documents observed runtime-side behavior so the monitor codes against reality, not aspiration. This note is the W3.1 deliverable — the spec at `wcon-w3-session-monitor.md` gates impl on it.

## Table of Contents

- 1. Scope
- 2. StreamTrail
- 3. StreamGates
- 4. StreamEscalations
- 5. StreamWorkspaceChanges
- 6. Runtime Behavioral Reality — No Server-Side Filtering
- 7. Supporting Unary RPCs
- 8. Corrected Monitor Architecture
- 9. Frontend Frame Schema

---

## 1. Scope

Before the monitor coroutine is written, four open questions must be answered from the proto + runtime:

1. What are the Item types for each stream, and which fields drive enrichment / filtering?
2. Do streams support server-side filtering (e.g., `workspace_ids`)?
3. How do streams terminate cleanly?
4. What unary RPCs does the monitor need for gap recovery on reconnect?

This note answers each, then points at the corrected architecture the monitor implements.

## 2. StreamTrail

**Proto:** `wacp/proto/highway.proto:39`, `168-172` (request), primitives.proto `163-172` (TrailEntry).

```proto
rpc StreamTrail(StreamTrailRequest) returns (stream TrailEntry);

message StreamTrailRequest {
    string workspace_id = 1;   // empty for global trail
    string event_type = 2;     // empty for all types
    bool from_beginning = 3;   // true = replay history then stream live
}

message TrailEntry {
    string id = 1;
    Timestamp timestamp = 2;
    string workspace_id = 3;
    string actor = 4;             // role name, "protocol", or user_id
    string event_type = 5;        // open set — see §6 for the canonical list
    bytes body = 6;               // type-specific payload
    uint64 sequence_number = 7;
    bytes chain_hash = 8;
}
```

**Monitor usage:** subscribe once per session. Client-side filter against the session's workspace set (see §6 below — runtime ignores the request filter). `sequence_number` is the canonical ordering key; duplicate suppression relies on it.

**Per-entry enrichment** (from `wcon-highway.md` §3.1):
- Resolve `workspace_id` → `workspace_label` via taxonomy (role display name).
- For checkpoint trail entries, decode `body` and resolve the checkpoint `type` → schema from `VerticalManifest.checkpoint_types`.
- Detect refusal markers in `body` and fan out as a synthesized `refusals` channel frame (§4A).

**Terminal conditions:** the runtime's mpsc channel is bounded at `64`; if the server can't push, the whole stream ends. No graceful "closed" frame — the stream just yields `None`. Treated as a disconnect; monitor reconnects with backoff.

## 3. StreamGates

**Proto:** `highway.proto:42`, `76-85`.

```proto
rpc StreamGates(StreamGatesRequest) returns (stream GateEvent);

message StreamGatesRequest {}                 // no filter

message GateEvent {
    string gate_id = 1;
    GateType type = 2;                        // task_approval, workspace_create, …
    bytes subject = 3;                        // serialized subject object
    string workspace_id = 4;
    string task_id = 5;
    uint64 timeout_ms = 6;
    string fallback_action = 7;
    Timestamp created_at = 8;
}
```

**Monitor usage:** subscribe once per session. Filter by `workspace_id` membership in the session's set. Update pending-gates state; broadcast to WS clients; optionally start a timeout timer for ui-only purposes (the runtime's fallback_action still applies authoritatively).

**Gate removal semantics:** `StreamGates` emits *creation* events only. Resolution (approve/reject/timeout) surfaces via the `StreamTrail` stream — event_type `"gate_resolved"`. The monitor listens for those trail entries and removes the matching `gate_id` from `PendingState.gates`.

## 4. StreamEscalations

**Proto:** `highway.proto:45`, `100-106`.

```proto
rpc StreamEscalations(StreamEscalationsRequest) returns (stream EscalationEvent);

message StreamEscalationsRequest {
    string user_id = 1;                       // empty for all escalations the user owns
}

message EscalationEvent {
    string escalation_id = 1;
    string workspace_id = 2;
    string owner = 3;
    bytes context = 4;
    Timestamp created_at = 5;
}
```

**Monitor usage:** subscribe once per session. Filter by workspace membership. The `user_id` request filter is ignored by the runtime (see §6); monitor enforces ownership downstream via the `owner` field if needed.

## 5. StreamWorkspaceChanges

**Proto:** `highway.proto:48`, `180-190`.

```proto
rpc StreamWorkspaceChanges(StreamWorkspaceChangesRequest) returns (stream WorkspaceStateChange);

message StreamWorkspaceChangesRequest {
    string workspace_id = 1;                  // empty for all workspaces
}

message WorkspaceStateChange {
    string workspace_id = 1;
    WorkspaceState previous = 2;
    WorkspaceState current = 3;
    string trigger = 4;                       // free-form
    Timestamp timestamp = 5;
}
```

**Monitor usage:** subscribe once per session. Filter by workspace membership. Drives the `workspaces` channel frame AND synthesizes the session-level lifecycle channel (`session_active`, `session_completed`, `session_failed`, `session_cancelled`) per `wcon-sessions` §6.2.

**Completion detection:** the coordinator workspace (root) reaching a terminal `WorkspaceState` (`CLOSED`, `FAILED`) drives session COMPLETED / FAILED transitions. The monitor watches for `current ∈ {CLOSED, FAILED}` on the root workspace specifically.

## 6. Runtime Behavioral Reality — No Server-Side Filtering

**`wacp/crates/wacp-transport/src/grpc_highway.rs:180-235`** — all four `stream_*` handlers:

```rust
async fn stream_trail(
    &self,
    _request: Request<wacp_v1::StreamTrailRequest>,   // request ignored
) -> Result<Response<Self::StreamTrailStream>, Status> {
    let (tx, rx) = mpsc::channel(64);
    self.coordinator_tx.send(HighwayRequest::SubscribeTrail { tx }).await …
}
```

Note the `_request`: none of the 4 streams read their request filters. Every subscriber receives every event.

**Consequences for the monitor:**

1. **Filtering is client-side, always.** The monitor tracks a `HashSet<WorkspaceId>` for the session and drops every event whose `workspace_id` isn't in the set.
2. **Every monitor task is a full firehose consumer.** At N concurrent sessions, we have 4N stream consumers, but only 4 upstream broadcasts (since all subscribers see everything). No performance risk from per-session subscribe — the runtime already has a fan-out internally.
3. **The original W3 spec mentioned `StreamTrail(workspace_ids)`** — this was aspirational. Fix: the monitor accepts the session's `WorkspaceSet` as a constructor arg, subscribes once globally per stream, and filters.
4. **Runtime buffer is 64.** If the monitor stalls, the runtime drops the whole stream. Two implications:
   - The monitor must drain events promptly into its broadcast channel (no blocking operations in the select loop).
   - On a stall-drop, the monitor will observe a clean stream end and reconnect. Gap recovery (§W3.7) catches up.

## 7. Supporting Unary RPCs

For gap recovery after a reconnect, the monitor calls:

### 7.1 GetWorkspace — `highway.proto:28`, `134-151`

```proto
rpc GetWorkspace(GetWorkspaceRequest) returns (WorkspaceView);

message GetWorkspaceRequest { string workspace_id = 1; }

message WorkspaceView {
    string id = 1;
    WorkspaceState state = 2;
    // … + role, parent, owner, originator, task_id, budget/usage, timestamps
}
```

Used on `StreamWorkspaceChanges` reconnect to rebuild the state map. The monitor calls it once per workspace in the session set; if any return `NotFound`, the monitor synthesizes a `WorkspaceState::CLOSED` event and treats that workspace as gone.

### 7.2 GetTaskGraph — `highway.proto:31`, `153-157`

```proto
rpc GetTaskGraph(GetTaskGraphRequest) returns (TaskGraphView);

message GetTaskGraphRequest {}                // no filter

message TaskGraphView { repeated Task tasks = 1; }   // Task from primitives.proto:151
```

Used on `StreamTrail` reconnect to re-seed task-id → task name mapping for enrichment. A more targeted RPC would be nicer but doesn't exist; the global `TaskGraphView` is what we get.

### 7.3 No `ListPendingGates` / `ListPendingEscalations`

The spec §3 (W3.7) envisioned these for gap recovery on the Gates / Escalations streams. They don't exist — the runtime only surfaces pending items via the streams themselves. Consequence for reconnect: the monitor can't resync gate / escalation state deterministically.

Practical handling:
- **Gates:** on reconnect, emit a `Lag` frame telling the client "re-fetch pending gates". The existing W6 endpoint `/api/gates/pending` is a cross-session read; it suffices. Alternative (deferred): a wacp-side `ListActiveGates` RPC.
- **Escalations:** same pattern — `Lag` frame; the client re-queries `/api/escalations/pending`.

## 8. Corrected Monitor Architecture

The coding spec's Section 4.1 diagram stands, with two clarifications from this review:

```
SessionMonitor task
 ├── 4 stream driver sub-tasks (one per highway stream — GLOBAL subscribe)
 │    each filters against WorkspaceSet locally, sends StreamEvent into
 │    a single mpsc<StreamEvent>
 ├── main select! loop:
 │    ├── recv StreamEvent → enrich + refusal-detect → broadcast_tx.send
 │    ├── recv MonitorCmd (shutdown | snapshot)
 │    └── tick(heartbeat) → periodic DB state write + prune stale pending
 └── on exit: update sessions.state, remove from active_sessions, drop broadcast_tx
```

Deviations from the W3 spec's Section 4.1:

- **Filtering is client-side.** Spec showed `StreamTrail(workspace_ids)`; runtime ignores. Monitor holds the `WorkspaceSet` and filters events post-receive.
- **Gap recovery does not include `ListPendingGates` / `ListPendingEscalations`** — those RPCs don't exist. Monitor emits a `Lag` frame with `refresh_hint: ["gates","escalations"]` instead.
- **No `ReceiveCommands` subscription.** Spec §4.3 mentioned it in passing; that's `AgentService` (per-workspace) not `HighwayService`. Out of scope for the monitor.

## 9. Frontend Frame Schema

Per `wcon-highway.md` §2.2 and §3.1, the WebSocket frame envelope is:

```json
{ "channel": <str>, "session_id": <uuid>, "event": { … } }
```

Seven channels the monitor emits:

| Channel | Source | Event body |
|---------|--------|-----------|
| `trail` | StreamTrail | enriched TrailEntry per §3.1 (workspace_label, summary, body) |
| `gates` | StreamGates | enriched GateEvent per §4.1 (gate_id, type, timeout, subject) |
| `escalations` | StreamEscalations | enriched EscalationEvent per §5 |
| `workspaces` | StreamWorkspaceChanges | raw change event (previous, current, trigger) |
| `refusals` | synthesized from StreamTrail | refusal event per §4A |
| `session` | synthesized from StreamWorkspaceChanges | session lifecycle transitions (active / completed / failed / cancelled) |
| `notification` | synthesized cross-stream | nav-badge deltas, toast triggers |

**Lag marker.** On reconnect, the monitor emits a control frame on the affected channel(s):

```json
{
  "channel": "control",
  "session_id": "…",
  "event": { "type": "lag", "refresh_hint": ["gates", "escalations"], "missed": <u64> }
}
```

The frontend handles `control.lag` by re-querying the relevant REST endpoints.

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-w3-session-monitor | W3 — Session Monitor | parent (this note is the §3 W3.1 gate artifact) |
| wcon-highway | Highway Integration | defines frame schemas for each channel |
| wcon-sessions | Session System | defines completion detection rules (§6.2) |
| wcon-wiring-phases | Wiring Phases | parent (§3 W3 row) |

*WACP Workspace — authored by AKIL Abderrahim and Claude Opus 4.6*
