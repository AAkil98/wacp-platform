---
id: wacp-audit-runtime-001
type: impl
status: final
created: 2026-04-13T00:00:00
authors: [Akil Abderrahim, Claude Opus 4.6]
tags: [audit, runtime, console, limitations]
depends_on: [wacp-impl-runtime]
---

# Runtime Implementation Audit

## Table of Contents
---
## 1. Context
## 2. Audit Scope
## 3. Resolved Limitations
## 4. Console-Facing Gaps
## 5. Coordinator Gaps
## 6. Agent-Facing Gaps
## 7. Severity Matrix
## 8. Recommendations
## References

---

## 1. Context

This audit was conducted against the `dev` branch at commit `cc26728` (2026-04-13),
immediately after landing real envelope routing, SHA-256 checkpoint persistence,
and exhaustive trail emission. The mock runtime (`wacp-mock-runtime`) has been
removed in `0f90e04`; Console development now targets the real runtime via
`dev/runtime.yaml`.

The purpose of this audit is to identify every remaining stub, placeholder, or
incomplete handler in the runtime event loop, gRPC service implementations, and
coordinator request path -- with particular attention to what the Console will
hit first.

### Methodology

Exhaustive search of `crates/wacp-runtime/src/` and `crates/wacp-transport/src/`
for: `unimplemented`, `todo!`, `TODO`, `FIXME`, empty match arms (`=> {}`),
`Default::default()` responses, `vec![]` returns in request handlers, hardcoded
IDs, and `String::new()` in response fields that should carry data.

---

## 2. Audit Scope

| Crate | Files examined | In scope |
|-------|---------------|----------|
| `wacp-runtime` | `init.rs`, `main.rs`, `channel_backend.rs`, `config.rs` | Yes |
| `wacp-transport` | `grpc_agent.rs`, `grpc_highway.rs`, `grpc_coordinator.rs`, `rest_gateway.rs` | Yes |
| `wacp-coordinator` | `orchestrator.rs`, `handler.rs`, `tree.rs` | Coordinator internals noted but not primary focus |

Out of scope: `wacp-trail`, `wacp-recovery`, `wacp-permissions`, `wacp-workspace`,
`wacp-taxonomy`, `wacp-fsm`, `wacp-clock` (these are self-contained crates with
their own test suites).

---

## 3. Resolved Limitations

These were listed as known limitations in the Console STATUS.md (rev 3) and are
now fully addressed:

| # | Limitation | Resolution | Commit |
|---|-----------|------------|--------|
| R1 | Trail fan-out only for Signal/StateChanged | `emit_trail_entry()` fires for all 6 event variants; escalation signals fan out to dedicated subscribers | `cc26728` |
| R2 | SendEnvelope stub (no coordinator routing) | Builds real `Envelope`, calls `coordinator.route_envelope()`, returns 404/503 on failure | `cc26728` |
| R3 | CreateCheckpoint stub (no persistence) | SHA-256 via `sha2`, `FileCheckpointStorage` content-addressable store on disk | `cc26728` |
| R4 | Mock runtime divergence | `wacp-mock-runtime` binary removed; `dev/runtime.yaml` points real runtime at `ecosystem/` verticals | `0f90e04` |

---

## 4. Console-Facing Gaps

These are limitations the Console will encounter through the REST gateway or
gRPC Highway service when rendering its UI.

### C1. GetCheckpoint always returns 404

**Location:** `init.rs:707-708`
**Severity:** High
**Current behavior:** `HighwayRequest::GetCheckpoint` unconditionally returns
`Status::not_found("checkpoint not found")`.
**Impact:** Checkpoint storage is written to by `CreateCheckpoint` but never read.
Console checkpoint viewer cannot display checkpoint content.
**Required:** Read from `self.checkpoint_storage` using the content hash or
checkpoint ID from the request.

### C2. QueryTrail returns empty (both paths)

**Location:** `init.rs:627-633` (Agent path), `init.rs:669-675` (Highway path)
**Severity:** High
**Current behavior:** Both handlers return `QueryTrailResponse { entries: [], has_more: false }`.
**Impact:** Live streaming works (trail entries are pushed to subscribers via
`emit_trail_entry`), but historical trail queries return nothing. Console cannot
populate the trail view on initial page load -- only events arriving after
subscription appear.
**Required:** Query `FileTrailStorage` (or `InMemoryTrailStorage`) for entries
matching the request's workspace ID, time range, and pagination cursor.

### C3. GetTaskGraph returns empty

**Location:** `init.rs:652-653`
**Severity:** Medium
**Current behavior:** `TaskGraphView { tasks: vec![] }`.
**Impact:** Console task graph visualization is empty. The coordinator maintains
task state internally, but this handler doesn't query it.
**Required:** Walk `self.coordinator.tree` to build a task graph view from active
workspaces and their task assignments.

### C4. Three Highway gRPC handlers return UNIMPLEMENTED

**Location:** `init.rs:656-667`
**Severity:** Medium (REST path works)
**Handlers:**
- `InjectEnvelope` -- "envelope injection via gRPC pending full wiring"
- `RespondToGate` -- "gate response pending"
- `RespondToEscalation` -- "escalation response pending"

**Impact:** The REST gateway handles all three via `ChannelBackend`, so Console
works over HTTP. The gRPC Highway path for these operations is broken. If Console
or any other Highway client calls gRPC directly, it fails.
**Required:** Mirror the `ChannelBackend` logic: build the corresponding domain
object from the request and route through the coordinator.

### C5. GetWorkspace response has empty fields

**Location:** `init.rs:680-701`
**Severity:** Medium
**Empty fields:**
- `role: String::new()` -- workspace role not resolved
- `originator: String::new()` -- origin workspace unknown
- `checkpoint_count: 0` -- never incremented
- `current_usage: None` -- no resource tracking
- `budget: None` -- no budget information
- `created_at: None` -- timestamp not recorded
- `last_activity: None` -- timestamp not recorded

**Impact:** Console workspace detail panel will show incomplete data. The
coordinator tree node has some of this information (`owner`, `parent`, `task_id`,
`status`) but not all.

### C6. GetAllocatable returns None

**Location:** `init.rs:814-817`
**Severity:** Low
**Current behavior:** `GetAllocatableResponse { remaining: None }`.
**Impact:** Console resource budget views are empty. Resource budgets are defined
in the protocol and config (`ResourceConfig`, `BudgetConfig`) but not wired into
this response.

### C7. Authenticate is a pass-through stub

**Location:** `init.rs:646-649`
**Severity:** Low (auth is a future Console spec concern)
**Current behavior:** `user_id: format!("user-{}", request.auth_token)`. Any
token is accepted; capabilities are hardcoded to `["observe", "inject", "gate"]`.
**Impact:** No real authentication. Acceptable for dev; Console auth spec
(`wcon-auth`) will define the real flow.

### C8. State change trigger field empty

**Location:** `init.rs:381`
**Severity:** Low
**Current behavior:** `trigger: String::new()` in `WorkspaceStateChange` proto.
**Impact:** Console state change timeline cannot show what caused each transition.

---

## 5. Coordinator Gaps

These affect whether the runtime actually performs operations requested through
the Coordinator gRPC/REST path.

### K1. SubmitGoal ID is not unique

**Location:** `init.rs:760`
**Severity:** High
**Current behavior:** `goal_id = format!("goal-{}", request.description.len())`.
**Impact:** Two goals with the same description length produce the same ID.
Collision breaks any downstream logic that keys on goal ID.
**Required:** Monotonic counter or UUID generation, consistent with envelope/checkpoint ID patterns.

### K2. SubmitGoal does not create a workspace tree

**Location:** `init.rs:758-765`
**Severity:** High
**Current behavior:** Returns `root_workspace_id: "ws-root"` (the pre-existing
root) without spawning a new goal workspace.
**Impact:** Goal submission appears to succeed but nothing happens. Console submits
a goal, gets back an ID, but no workspace appears in the tree.
**Required:** Coordinator should decompose the goal into tasks and spawn workspace
actors.

### K3. Dispatch does not create a workspace

**Location:** `init.rs:771-777`
**Severity:** High
**Current behavior:** Returns `ws_id = format!("ws-{}", request.task_id)` without
creating a workspace node or starting an actor.
**Impact:** Task dispatch is a no-op. The returned workspace ID doesn't exist in
the tree.
**Required:** Create a child workspace in the coordinator tree, assign the task,
start the workspace actor.

### K4. Five coordinator operations are no-ops

**Location:** `init.rs:786-807`
**Severity:** Medium
**Handlers returning `Default::default()` without modifying state:
- `SuspendWorkspace` -- does not pause the workspace actor
- `ResumeWorkspace` -- does not resume a suspended workspace
- `Decompose` -- does not break a goal into sub-tasks
- `CancelTask` -- does not cancel a running task
- `SendDirective` / `SendFeedback` -- does not deliver to the workspace

**Impact:** Console action buttons (suspend, resume, cancel) appear to succeed
but have no effect. Only `AbortWorkspace` (`init.rs:779-784`) is real -- it calls
`self.coordinator.abort_workspace()`.

### K5. GetReadyTasks always empty

**Location:** `init.rs:767-769`
**Severity:** Medium
**Current behavior:** `GetReadyTasksResponse { tasks: vec![] }`.
**Impact:** No task scheduling can occur. Downstream of K2/K3 -- if goals and
dispatch don't create tasks, there are no ready tasks to return.

### K6. TriggerIntegration is a stub

**Location:** `init.rs:808-812`
**Severity:** Low
**Current behavior:** Returns `result: "accepted"` with empty detail. No
integration is triggered.

---

## 6. Agent-Facing Gaps

These affect agent-to-runtime communication over the Agent gRPC service.

### A1. workspace_id never extracted from gRPC metadata

**Location:** `grpc_agent.rs:87, 108, 129, 150, 180, 199`
**Severity:** High
**Current behavior:** Every `AgentRequest` variant sets
`workspace_id: String::new()`. The comment at line 87 reads:
`// TODO: extract from connection metadata`.
**Impact:** The runtime cannot associate agent requests with their workspace.
`SendEnvelope` uses this as `from_workspace`; `EmitSignal` uses it to identify
the source. All agent operations are effectively unattributed.
**Required:** Extract workspace ID from gRPC request metadata (set during agent
bind) or from the `BindRequest`/connection state.

### A2. SubscribeEnvelopes / SubscribeCommands silently dropped

**Location:** `init.rs:635-636`
**Severity:** High
**Current behavior:** Empty match arms `=> {}`. The transport layer creates
subscriber channels and returns streams to the agent, but the runtime never
registers the sender side. Agents open a stream that will never receive data.
**Impact:** Envelope delivery to agents is broken. An agent calls
`ReceiveEnvelopes`, gets a stream, and waits forever.
**Required:** Register the `tx` sender in a subscriber list (similar to
`trail_subs`, `gate_subs`, etc.) and push envelopes/commands when they arrive
for that workspace.

### A3. ReadResource unimplemented

**Location:** `grpc_agent.rs:163-168`
**Severity:** Low
**Current behavior:** Returns `Status::unimplemented("read_resource not yet implemented")`.
**Impact:** Agents cannot read resources via gRPC. Not Console-blocking.

---

## 7. Severity Matrix

| ID | Gap | Severity | Console-blocking? | Depends on | Status |
|----|-----|----------|-------------------|------------|--------|
| C1 | GetCheckpoint returns 404 | High | Yes | -- | Resolved (phase 1) |
| C2 | QueryTrail returns empty | High | Yes | -- | Resolved (phase 1) |
| K1 | SubmitGoal ID collision | High | Yes | -- | Resolved (phase 2) |
| K2 | SubmitGoal no-op | High | Yes | -- | Resolved (phase 2) |
| K3 | Dispatch no-op | High | Yes | K2 | Resolved (phase 2) |
| A1 | workspace_id not extracted | High | Indirect | -- | Resolved (phase 3) |
| A2 | Envelope/command subscriptions dropped | High | No (agent-side) | -- | Resolved (phase 3) |
| C3 | GetTaskGraph empty | Medium | Yes | K2, K3 | Open (phase 5) |
| C4 | Highway gRPC UNIMPLEMENTED (3) | Medium | No (REST works) | -- | Resolved (phase 4) |
| C5 | GetWorkspace incomplete fields | Medium | Yes (partial) | -- | Partial (phase 5) |
| K4 | 5 coordinator ops are no-ops | Medium | Yes (actions fail silently) | -- | Resolved (phase 2) |
| K5 | GetReadyTasks empty | Medium | Indirect | K2, K3 | Resolved (phase 2) |
| C6 | GetAllocatable returns None | Low | Cosmetic | -- | Resolved (phase 2) |
| C7 | Authenticate stub | Low | No (future spec) | -- | Open (phase 5) |
| C8 | State change trigger empty | Low | Cosmetic | -- | Open (phase 5) |
| K6 | TriggerIntegration stub | Low | No | -- | Resolved (phase 2) |
| A3 | ReadResource unimplemented | Low | No | -- | Open (phase 5) |

### Console critical path

The Console's first render depends on: workspace listing (works), vertical
listing (works), health (works), trail history (C2), and checkpoint retrieval (C1).

The Console's first interactive flow (submit a goal, watch it execute) depends on:
SubmitGoal actually creating workspaces (K1, K2), Dispatch creating actors (K3),
and state changes flowing through streams (works).

**Minimum viable set for Console dev: C1, C2, K1, K2, K3.**

---

## 8. Recommendations

### Phase 1: Console read path (C1, C2, C5)

Wire GetCheckpoint to read from `checkpoint_storage`. Wire QueryTrail to read
from trail storage with workspace filtering and pagination. Populate GetWorkspace
fields from coordinator tree node data and workspace actor metadata.

### Phase 2: Coordinator write path (K1, K2, K3, K4)

Implement real SubmitGoal with monotonic ID generation and workspace tree
creation. Implement Dispatch with workspace actor spawning. Wire
Suspend/Resume/Cancel to coordinator FSM transitions.

### Phase 3: Agent plumbing (A1, A2)

Extract workspace_id from gRPC connection metadata (requires a bind-state
registry keyed by connection). Register envelope/command subscribers in the
event loop.

### Phase 4: Highway gRPC parity (C4)

Wire InjectEnvelope, RespondToGate, RespondToEscalation through the coordinator
-- same logic as the REST ChannelBackend path.

### Phase 5: Remaining gaps (C3, C5, C7, C8, A3)

Wire GetTaskGraph to walk the coordinator task graph and return real task views.
Populate the three remaining empty GetWorkspace fields (`current_usage`,
`created_at`, `last_activity`) from workspace actor metadata. Replace the
Authenticate pass-through with token validation against a configurable auth
backend (or at minimum reject empty/malformed tokens). Populate the `trigger`
field on WorkspaceStateChange from the event that caused the transition. Wire
ReadResource in the Agent gRPC service to read workspace-scoped resources from
the coordinator.

Note: C6 (GetAllocatable), K4 (Decompose/CancelTask), K5 (GetReadyTasks), and
K6 (TriggerIntegration) were resolved during phases 2-4 and are no longer open.

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wacp-impl-runtime | Runtime Architecture | implements |
| wacp-spec-workspace | Workspace Lifecycle | constrains C5, K2, K3, K4 |
| wacp-spec-envelope | Envelope Spec | constrains C4, A1, A2 |
| wacp-spec-checkpoint | Checkpoint Spec | constrains C1 |
| wacp-spec-trail | Trail Spec | constrains C2 |
| wacp-spec-task | Task Scheduling | constrains C3, K5 |

*WACP Runtime Audit -- authored by Akil Abderrahim and Claude Opus 4.6*
