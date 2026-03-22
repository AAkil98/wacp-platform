# WACP Implementation: Agent Migration

```yaml
id: wacp-impl-migration
type: implementation-spec
status: complete
created: 2026-03-21
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §6.9 (agent migration)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-storage
  - wacp-impl-protocol-interface
  - wacp-spec-workspace
  - wacp-spec-signal
  - wacp-spec-trail
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, migration, agent, workspace, handoff, atomicity]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Migration Coordinator Procedure](#2-migration-coordinator-procedure)
3. [Workspace State Snapshot](#3-workspace-state-snapshot)
4. [Agent Unbind Sequence](#4-agent-unbind-sequence)
5. [New Agent Bind with State Restore](#5-new-agent-bind-with-state-restore)
6. [Atomic Rollback on Failure](#6-atomic-rollback-on-failure)
7. [Connection Handoff Mechanics](#7-connection-handoff-mechanics)
8. [Resource Meter Continuity](#8-resource-meter-continuity)
9. [Concurrency and Ordering Guarantees](#9-concurrency-and-ordering-guarantees)
10. [Trail Events](#10-trail-events)
11. [SDK Integration](#11-sdk-integration)
12. [References](#12-references)

## 1. Purpose

This spec defines how the WACP runtime replaces a workspace's agent while preserving all nine workspace components. It answers "how does migration become code" — not "what migration means" (that's the workspace spec §11 and the protocol §6.9) or "how state is persisted" (that's the storage spec's job).

The protocol defines migration as atomic: the workspace enters `migrating`, the runtime snapshots state, unbinds the old agent, binds the new agent, and returns the workspace to its pre-migration state. If any step fails, the workspace transitions to `failed`. There is no partial migration — the workspace never exists in a state where neither agent is bound.

This spec defines the internal procedure that makes atomicity real: the coordinator's migration orchestration, the state snapshot format and transfer, the connection management for unbind and bind, the rollback path on failure, and the guarantees around resource meter continuity, inbox ordering, and trail integrity across the migration boundary.

**Scope.** The coordinator's migration decision and procedure. Workspace state snapshot for handoff — what is captured, serialization format, transfer mechanism. Agent unbind — connection teardown, signal draining, final state capture. New agent bind — state restoration, connection establishment, inbox replay. Rollback on failure — what state is restored, what trail entries are written. Connection handoff mechanics at the transport layer. Resource meter continuity — budgets, timers, liveness tracking across the migration boundary. Concurrency constraints during migration.

**Not in scope.** Why migration is initiated — that is a coordinator policy decision (model upgrade, cost optimization, agent failure recovery). Distributed migration across runtime instances — migration within a single runtime process only. Agent SDK migration hooks — the new agent receives the standard `Bind` response with full workspace state; no SDK-level migration API exists. The new agent is expected to inspect the workspace's history (local trail, checkpoint register, working memory) to understand context.

**Design constraint.** Migration is a runtime-internal operation — no protocol message from the agent triggers it. Only the coordinator initiates migration. The workspace's identity (id, role, visibility set, authority set) does not change. The resource meter is continuous — the new agent inherits the old agent's consumption. The trail is continuous — migration events are part of the same local trail. The inbox is continuous — unprocessed envelopes are preserved and delivered to the new agent.

---

## 2. Migration Coordinator Procedure

Migration is a coordinator-driven operation. The coordinator actor owns the decision, orchestrates every step, and is the sole arbiter of success or failure. No other actor can initiate, advance, or cancel a migration. This section defines the complete procedure from the coordinator's perspective.

### 2.1 Migration Request

Migration begins when the coordinator decides to replace a workspace's agent. The decision is a coordinator-internal policy concern (§1, not in scope), but the request has a fixed structure:

```rust
pub struct MigrationRequest {
    pub workspace_id: WorkspaceId,
    pub new_agent: AgentRef,
    pub reason: String,
}

pub struct AgentRef {
    pub agent_type: String,         // implementation-defined: model name, binary path, etc.
    pub config: Option<bytes::Bytes>, // agent-specific configuration, opaque to the runtime
}
```

**`AgentRef`** identifies the replacement agent. The runtime does not interpret `agent_type` or `config` — it passes them to the agent launch mechanism (for PSK auth) or includes them in the external auth request (for external auth). The meaning of these fields is deployment-defined.

**Precondition check.** Before proceeding, the coordinator validates:

1. The workspace exists and is in `active` or `blocked` state. All other states are rejected — `idle` (no agent bound yet), `suspended` (frozen, not a candidate for replacement), `integrating` (mid-merge), `conflicted` (mid-resolution), `migrating` (already migrating), and terminal states.
2. The workspace is not in the middle of a trail write. The coordinator waits for any in-progress operation in the workspace actor to complete before sending the migration command.

If the precondition check fails, the coordinator logs the rejection and does not proceed. No trail entry is written for a rejected migration request — the request never reached the protocol layer.

### 2.2 The Seven-Step Procedure

Migration is a linear sequence — no branching, no parallelism. Each step must complete before the next begins. Failure at any step triggers rollback (§6).

```
Step  What                              Actor            Trail event
────  ──────────────────────────────    ───────────────  ────────────────────
 1    Record pre-migration state         Coordinator      —
 2    Transition to migrating            Workspace        migration_started
 3    Drain in-flight operations         Workspace        —
 4    Snapshot workspace state           Workspace        —
 5    Unbind old agent                   Transport        —
 6    Bind new agent                     Transport        —
 7    Transition to pre-migration state  Workspace        migration_completed
```

**Step 1: Record pre-migration state.** The coordinator records the workspace's current state (`active` or `blocked`) in the migration context. This is the state the workspace will return to on success. It also records the current agent identity — the old agent — for the trail entry.

```rust
struct MigrationContext {
    workspace_id: WorkspaceId,
    pre_migration_state: WorkspaceStatus,  // Active or Blocked
    old_agent: AgentIdentity,
    new_agent: AgentRef,
    reason: String,
    snapshot: Option<MigrationSnapshot>,   // populated at step 4
}
```

**Step 2: Transition to `migrating`.** The coordinator sends a `MigrateWorkspace` command to the workspace actor on the high-priority channel (runtime spec, §14). The workspace actor validates the transition via the FSM engine (`active → migrating` or `blocked → migrating`), writes a `migration_started` trail entry, and transitions. From this point, the workspace is frozen — the agent's normal-priority channel is drained but no new messages are processed.

```rust
// Sent on the coordinator → workspace high-priority channel
pub enum CoordinatorCommand {
    // ... other commands (abort, suspend, budget increase, etc.)
    Migrate(MigrationContext),
}
```

The workspace actor acknowledges the transition by responding to the coordinator. If the FSM rejects the transition (a race — the workspace changed state between the precondition check and the command delivery), the coordinator aborts the migration with no trail entry beyond the rejection.

**Step 3: Drain in-flight operations.** The workspace actor completes any operation that was in progress when the migration command arrived. Specifically:

- If a trail write is in progress (the workspace was mid-operation when `Migrate` arrived on the high-priority channel, but `biased` select picked `Migrate` first because the operation hadn't started its write yet), the operation is abandoned — no trail write means no commit, no side effect.
- If an envelope delivery to this workspace's inbox was in progress (the delivery pipeline was between "trail write committed" and "inbox append"), the delivery completes. The envelope enters the inbox. The new agent will see it.
- If an outbound envelope from this workspace was in the delivery pipeline (between "workspace sent" and "receiver delivered"), the delivery continues independently — it is already committed in the trail and does not require the sending workspace to remain active.

After draining, the workspace actor holds no in-progress protocol operations. Its state is quiescent.

**Step 4: Snapshot workspace state.** The workspace actor serializes the five live components that require capture (§3). The four immutable components (directive, context, visibility set, authority set) are already fixed — they are shared via `Arc` and do not need serialization. The snapshot is sent to the coordinator as part of the migration acknowledgment.

**Step 5: Unbind old agent.** The coordinator instructs the transport actor to disconnect the old agent's gRPC connection. The transport actor closes the connection. The old agent receives a gRPC `CANCELLED` status on its streaming RPCs and a connection drop. The old agent cannot reconnect to this workspace — the PSK token is revoked (deployment spec, §5.2) or the external auth provider is expected to reject re-authentication for the old identity.

**Step 6: Bind new agent.** The coordinator instructs the transport actor to expect a new connection for this workspace. For PSK auth, the coordinator generates a new token and launches the replacement agent process with the token. For external auth, the coordinator signals readiness and the new agent connects independently. The new agent calls `Bind`, authenticates, and receives the full workspace state in the `BindResponse` (protocol-interface spec, §4) — including the directive, context, role, visibility set, authority set, and current budget. The workspace actor's inbox, working memory, checkpoint register, and local trail are accessible through the standard RPCs (`ReceiveEnvelopes`, `QueryTrail`, `ReadResource`).

If the new agent does not connect within a configurable timeout (default: 30 seconds), the migration fails (§6).

**Step 7: Transition to pre-migration state.** The coordinator sends a `MigrationComplete` command to the workspace actor. The workspace actor transitions from `migrating` back to the recorded pre-migration state (`active` or `blocked`), writes a `migration_completed` trail entry, and resumes normal message processing. The agent's normal-priority channel is re-enabled. Queued envelopes in the inbox are delivered to the new agent via the `ReceiveEnvelopes` stream.

### 2.3 Coordinator State During Migration

The coordinator tracks active migrations in a map:

```rust
struct CoordinatorState {
    // ... other state (workspace tree, task graph, etc.)
    active_migrations: HashMap<WorkspaceId, MigrationContext>,
}
```

Only one migration per workspace at a time. A second migration request for a workspace that is already migrating is rejected. The map entry is created at step 1 and removed at step 7 (success) or during rollback (failure).

**No parallel migrations of the same workspace.** This is a direct consequence of the protocol's state machine — a workspace in `migrating` cannot transition to `migrating` again.

**Parallel migrations of different workspaces.** Permitted. Each migration is independent — different workspaces, different actors, different channels. The coordinator processes migration steps for different workspaces concurrently via its normal message loop.

### 2.4 Migration Timeout

The overall migration has a timeout: the maximum duration from step 2 (transition to `migrating`) to step 7 (transition back). Default: 60 seconds. If the timeout expires, the coordinator triggers rollback (§6).

The timeout protects against a migration that hangs — typically because the new agent cannot be launched or fails to connect (step 6). The timeout is not configurable per-migration in the initial implementation — it is a global setting. The coordinator manages migration timeouts using the same `FuturesUnordered` mechanism as workspace timeouts (runtime spec, §12).

---

## 3. Workspace State Snapshot

The snapshot captures the workspace's live state at the moment of migration — everything the new agent needs to resume where the old agent left off. This section defines what is captured, what is shared, and the serialization format.

### 3.1 Component Classification

The nine workspace components (runtime spec, §8) fall into two categories during migration:

| Category | Components | Why |
|----------|-----------|-----|
| **Shared (Arc, no capture needed)** | Directive, context, visibility set, authority set | Immutable after creation — already `Arc`-shared, the new agent receives references to the same data |
| **Captured (serialized into snapshot)** | Inbox, working memory, checkpoint register, resource meter, local trail handle | Mutable during execution — their current state must be transferred |

The snapshot contains only the five captured components. The four shared components are passed to the new workspace actor via `Arc::clone` — zero-copy, no serialization.

### 3.2 What Is Captured

**Inbox (`VecDeque<Envelope>`).** All envelopes currently in the inbox — delivered but not yet consumed by the agent. The queue order is preserved. Envelopes that were in-flight during drain (§2.2, step 3) have already landed in the inbox by snapshot time. The new agent receives these envelopes through the `ReceiveEnvelopes` stream after binding.

**Working memory (`WorkingMemory`).** The agent's scratch space — files on disk, structured data in memory, intermediate results. The runtime does not interpret working memory contents (runtime spec, §8). For in-memory working memory, the snapshot serializes the raw bytes. For file-backed working memory, the snapshot records the directory path — the files remain on disk and the new agent accesses the same path. No file copying occurs.

**Checkpoint register (`Vec<Checkpoint>`).** The append-only chain of checkpoints the agent has created. Checkpoint metadata (id, type, intent, confidence, content hash) is serialized. Checkpoint payloads are not included in the snapshot — they live in the content-addressable checkpoint store (storage spec, §5) and are accessed by content hash. The new agent can read payloads through the standard `ReadResource` RPC.

**Resource meter (`ResourceMeter`).** Current consumption across all budget dimensions — tokens, wall time, storage bytes, network bytes, cost. The meter is transferred as-is. The new agent inherits the old agent's consumption (§8).

**Local trail handle (`TrailHandle`).** The handle carries the workspace's trail partition metadata — the sequence range, the most recent entry id, and the chain head hash. The new agent does not receive the trail contents in the snapshot — it queries the trail through `QueryTrail`. The handle ensures the workspace actor continues writing to the same trail partition after migration.

### 3.3 Serialization Format

The snapshot is serialized using the same protobuf-based binary encoding as workspace state persistence (storage spec, §6). This reuses the existing serialization infrastructure and ensures compatibility with the recovery engine — a migration snapshot and a persistence snapshot have the same format.

```rust
pub struct MigrationSnapshot {
    pub inbox: Vec<Envelope>,
    pub working_memory: WorkingMemorySnapshot,
    pub checkpoint_register: Vec<CheckpointMetadata>,
    pub resource_meter: ResourceMeterState,
    pub trail_handle: TrailHandleState,
    pub pre_migration_state: WorkspaceStatus,
    pub snapshot_sequence: u64,     // trail sequence number at snapshot time
}

pub enum WorkingMemorySnapshot {
    InMemory(bytes::Bytes),         // serialized in-memory data
    FileBacked(PathBuf),            // path to working directory on disk
}

pub struct ResourceMeterState {
    pub tokens: u64,
    pub wall_time_ms: u64,
    pub storage_bytes: u64,
    pub network_bytes: u64,
    pub cost_micros: u64,
    pub active_since: Option<Timestamp>,  // for wall-time timer reconstruction
}

pub struct TrailHandleState {
    pub first_sequence: u64,
    pub last_sequence: u64,
    pub last_entry_id: String,
    pub chain_head_hash: [u8; 32],
}
```

### 3.4 Snapshot Lifetime

The snapshot is created at step 4 and consumed at step 6 (when the new agent binds). It lives in memory — it is never written to disk. If the migration fails, the snapshot is used for rollback state verification (§6), then dropped. If the migration succeeds, the snapshot is dropped after step 7. The snapshot is owned by the `MigrationContext` struct in the coordinator's `active_migrations` map.

**No durability requirement.** If the runtime crashes during migration, recovery reconstructs the workspace from the trail. The `migration_started` trail entry exists — recovery detects a workspace in `migrating` state with no `migration_completed` or `migration_failed` event and transitions it to `failed` with `reason: migration_interrupted`.

---

## 4. Agent Unbind Sequence

Unbinding disconnects the old agent from the workspace. After unbind, the old agent has no channel to the runtime and cannot affect the workspace's state.

### 4.1 Connection Teardown

The coordinator sends an `UnbindAgent` command to the transport actor for the workspace's current connection:

```rust
pub enum TransportCommand {
    // ... other commands
    UnbindAgent {
        workspace_id: WorkspaceId,
        reason: UnbindReason,
    },
}

pub enum UnbindReason {
    Migration,
    WorkspaceTerminated,
    ConnectionTimeout,
}
```

The transport actor:

1. Closes the `ReceiveEnvelopes` server stream — the agent receives a gRPC `CANCELLED` status.
2. Closes the `ReceiveCommands` server stream — same.
3. Rejects any in-flight unary RPCs from the agent with gRPC `UNAVAILABLE` and message "workspace migrating."
4. Drops the connection. The gRPC channel is closed.
5. Removes the workspace-to-connection mapping from the transport actor's routing table.

**The agent receives no migration notification.** The old agent sees a connection drop — it does not know whether the workspace is migrating, terminating, or the runtime crashed. This is deliberate: the agent is untrusted (runtime spec, §2). Notifying the agent of a pending migration would give it an opportunity to interfere — racing to emit signals, writing corrupted checkpoints, or refusing to disconnect. The connection is simply closed.

### 4.2 Token Revocation

For PSK authentication, the coordinator calls `psk.revoke_agent(workspace_id)` (deployment spec, §5.2). The old token is removed from the authenticator's table. If the old agent attempts to reconnect with its token, authentication fails.

For external authentication, revocation is the external provider's responsibility. The runtime does not call the external provider to revoke — it simply stops accepting the old agent's identity for this workspace. The workspace actor rejects `Bind` attempts from any identity that does not match the new agent's identity.

### 4.3 Signal Drain

Between the workspace entering `migrating` (step 2) and unbind (step 5), the old agent's connection may still be open. The workspace actor's frozen state means:

- Inbound signals from the agent are rejected — the workspace does not process them. The agent receives gRPC `FAILED_PRECONDITION` with message "workspace migrating."
- Outbound streams (`ReceiveEnvelopes`, `ReceiveCommands`) stop delivering new messages — the streams are still technically open until unbind, but no new items are pushed.

No signal drain delay is needed. The workspace actor stops processing agent messages the moment it transitions to `migrating` (the high-priority channel takes precedence). Any agent messages that arrived on the normal-priority channel between the last processed message and the migration command are discarded — they are uncommitted (no trail write), so they never happened from the protocol's perspective.

---

## 5. New Agent Bind with State Restore

After the old agent is unbound, the coordinator arranges for the new agent to connect and receive the workspace's full state.

### 5.1 Agent Launch

The coordinator does not connect to the new agent — the new agent connects to the runtime. The coordinator's role is to make the workspace available for binding and ensure the new agent can authenticate.

**PSK auth.** The coordinator generates a new token via `psk.register_agent(workspace_id, role)` and launches the new agent process. The launch mechanism is implementation-defined — it may be a child process (`tokio::process::Command`), a container start, or a message to an agent orchestrator. The token is passed to the agent through the launch mechanism (environment variable, file, or command-line argument).

**External auth.** The coordinator does not launch the agent. The external system is responsible for starting the replacement agent and providing it with credentials that the external auth provider will accept. The coordinator sets a flag on the workspace indicating that a new agent is expected — when a `Bind` RPC arrives for this workspace with a valid identity, the migration proceeds.

### 5.2 Bind with Migration Context

The new agent calls `Bind` like any other agent — the `Bind` RPC is identical whether the workspace is freshly created or mid-migration. The transport actor routes the `Bind` to the workspace actor. The workspace actor detects that it is in `migrating` state and handles the bind as a migration bind rather than a first bind:

1. Authenticate the token (same flow as a normal bind).
2. Verify the authenticated identity matches the `new_agent` in the `MigrationContext`. If it does not, reject with `PERMISSION_DENIED`.
3. Associate the new connection with the workspace.
4. Construct the `BindResponse` with the workspace's current state — same fields as a normal bind, including the directive, context, role, visibility set, authority set, and budget.

The `BindResponse` does not carry a special "this is a migration" flag. The new agent receives the same response as if it were binding to a freshly created workspace. The difference is observable through the workspace's history: the local trail contains previous entries, the checkpoint register is non-empty, the inbox may contain unprocessed envelopes, and working memory contains in-progress artifacts.

### 5.3 Inbox Delivery

After the `BindResponse`, the new agent opens the `ReceiveEnvelopes` stream. The workspace actor delivers all unprocessed envelopes from the inbox in FIFO order — the same order the old agent would have received them. New envelopes that arrive after migration (from the coordinator or other workspaces) are appended to the inbox and delivered after the backlog.

There is no "replay" marker in the envelope stream. The new agent receives envelopes as if it had always been connected. The migration boundary is invisible in the envelope stream — it is visible only in the trail.

### 5.4 Working Memory Access

For file-backed working memory, the new agent inherits the same file paths. The `authority_set` (unchanged by migration) determines which files the agent can read and write. The agent accesses files through the standard `ReadResource` RPC or through direct filesystem access if the launch mechanism provides the paths.

For in-memory working memory, the snapshot's `WorkingMemorySnapshot::InMemory` bytes are loaded into the workspace actor's state. The agent accesses them through `ReadResource`.

### 5.5 Bind Timeout

If no `Bind` arrives within the migration timeout window (§2.4), the coordinator treats the migration as failed and triggers rollback (§6). The timeout begins at step 5 (unbind) and expires at the migration-level timeout (default 60 seconds from step 2).

---

## 6. Atomic Rollback on Failure

Migration is atomic — it succeeds completely or the workspace transitions to `failed`. There is no partial state where the workspace has a new agent but missing state, or no agent but is still `active`. This section defines the failure paths.

### 6.1 Failure Points

| Failure point | Step | Cause | Recovery action |
|--------------|------|-------|-----------------|
| FSM rejection | 2 | Workspace changed state between precondition check and command delivery (race) | Abort — no trail entry, no state change |
| Trail write failure | 2 | `migration_started` trail entry cannot be written (storage failure) | Abort — workspace remains in pre-migration state |
| Snapshot failure | 4 | Serialization error (should not happen — all types are serializable) | Workspace transitions to `failed` |
| Unbind failure | 5 | Transport error (connection already dropped) | Non-fatal — proceed, the old agent is already gone |
| Bind timeout | 6 | New agent does not connect within timeout | Workspace transitions to `failed` |
| Bind auth failure | 6 | New agent presents invalid credentials | Workspace transitions to `failed` |
| Bind identity mismatch | 6 | New agent authenticates but identity does not match expected `new_agent` | Reject bind, wait for correct agent or timeout |
| Trail write failure | 7 | `migration_completed` trail entry cannot be written | Workspace transitions to `failed` — migration is logically complete but unrecordable |

### 6.2 The `failed` Transition

When a migration fails after the workspace has entered `migrating` (steps 4–7), the coordinator:

1. Writes a `migration_failed` trail entry with the failure reason, old agent, new agent, and the step that failed.
2. Transitions the workspace to `failed` with `reason: migration_error`.
3. Disconnects the new agent if it connected (close the gRPC connection).
4. Removes the migration from `active_migrations`.

**Why `failed` and not rollback to pre-migration state.** The protocol defines migration as atomic (workspace spec, §11): "it either completes fully or the workspace transitions to `failed`." Rolling back to `active` or `blocked` would require rebinding the old agent — but the old agent has been disconnected, its token revoked, and it may have already exited. Reconnecting the old agent would itself be a migration, creating recursive failure modes. The clean answer is: migration failed, the workspace is failed, the coordinator may create a new workspace with a fresh agent and the old workspace's task.

### 6.3 Pre-Step-2 Failures

Failures before the workspace enters `migrating` (step 1 precondition check, step 2 FSM rejection or trail write failure) are aborts, not failures. The workspace remains in its current state (`active` or `blocked`). No trail entry is written. The migration never began from the protocol's perspective. The coordinator may retry the migration or abandon it.

### 6.4 Recovery After Crash During Migration

If the runtime crashes while a migration is in progress, recovery (runtime spec, §13) encounters one of three trail states:

| Trail state | Interpretation | Recovery action |
|------------|----------------|-----------------|
| `migration_started` with no `migration_completed` or `migration_failed` | Migration was interrupted by the crash | Transition workspace to `failed` with `reason: migration_interrupted` |
| `migration_started` + `migration_completed` | Migration succeeded, crash happened after | Workspace is in pre-migration state — reconstruct normally |
| `migration_started` + `migration_failed` | Migration failed, crash happened after | Workspace is `failed` — reconstruct normally |

The first case is the interesting one. Recovery cannot resume a half-completed migration — the old agent is gone, the new agent may or may not have connected, and the in-memory snapshot is lost. The safe action is to fail the workspace. The coordinator can then re-evaluate and create a new workspace.

---

## 7. Connection Handoff Mechanics

This section defines the transport-layer mechanics that support the migration procedure. The transport actor manages connections and routing; it does not understand migration semantics — it follows commands from the coordinator.

### 7.1 Routing Table

The transport actor maintains a mapping from workspace id to active connection:

```rust
struct TransportState {
    connections: HashMap<WorkspaceId, ConnectionHandle>,
}

struct ConnectionHandle {
    agent_identity: AgentIdentity,
    envelope_stream_tx: mpsc::Sender<Envelope>,
    command_stream_tx: mpsc::Sender<Command>,
}
```

During migration, the routing table entry for the workspace is in one of three states:

1. **Pre-unbind.** Entry points to the old agent's connection. Messages from the workspace actor are delivered to the old agent.
2. **Between unbind and bind.** Entry is removed. Messages from the workspace actor are queued in the workspace actor's outbound buffer — the workspace is frozen, so no new messages are generated, but messages from other workspaces targeting this one are buffered by the delivery pipeline.
3. **Post-bind.** Entry points to the new agent's connection. Messages are delivered to the new agent.

### 7.2 Concurrent Connection Prevention

Between unbind and bind, the transport actor must prevent stale connections. Two scenarios:

**Old agent reconnects.** If the old agent detects the connection drop and attempts to reconnect (calls `Bind` again), the transport actor routes the `Bind` to the workspace actor. The workspace actor is in `migrating` state. It checks the identity against the `MigrationContext`: the old agent's identity does not match the expected `new_agent`. The `Bind` is rejected with `PERMISSION_DENIED`. For PSK auth, the token was already revoked, so authentication fails before the `Bind` reaches the workspace actor.

**Unknown agent connects.** An unrelated agent attempts to bind to the migrating workspace. Same flow — identity mismatch, rejected.

**Multiple bind attempts from the new agent.** The new agent may retry its `Bind` if the first attempt times out at the client side. The workspace actor accepts the first successful `Bind` and rejects subsequent ones — the workspace is no longer in `migrating` state after the first bind completes step 7.

### 7.3 Message Buffering During Gap

Between unbind and bind (steps 5–6), envelopes addressed to the migrating workspace may arrive from other workspaces. These envelopes have already been committed in the trail (`envelope_created`) — they must be delivered. The delivery pipeline handles this:

- The delivery pipeline attempts to route the envelope to the workspace actor. The workspace actor's channel is still open (the actor is alive, just frozen).
- The workspace actor appends the envelope to its inbox. The inbox accepts envelopes in `migrating` state — it is append-only by design (runtime spec, §8).
- The envelope is not delivered to the agent stream (no agent is connected). It sits in the inbox.
- After the new agent binds and opens `ReceiveEnvelopes`, the inbox is drained in order — buffered envelopes first, then live ones.

No envelope is lost. No envelope is delivered out of order. The migration gap is invisible to the delivery guarantee.

---

## 8. Resource Meter Continuity

The resource meter is continuous across migration — the new agent inherits the old agent's consumption. Migration does not reset budgets, timers, or liveness tracking. This section defines how each resource dimension is preserved.

### 8.1 Budget Dimensions

The five budget dimensions (runtime spec, §12) are transferred in the `ResourceMeterState` within the snapshot (§3.3):

| Dimension | Transfer mechanism | Continuity guarantee |
|-----------|-------------------|---------------------|
| Tokens | `resource_meter.tokens` copied | Cumulative count continues from old agent's total |
| Wall time | `resource_meter.wall_time_ms` + timer reconstruction | Elapsed time includes old agent's active time |
| Storage | `resource_meter.storage_bytes` copied | Checkpoint and trail bytes are additive |
| Network | `resource_meter.network_bytes` copied | Envelope payload bytes are additive |
| Cost | `resource_meter.cost_micros` copied | Derived cost continues from old agent's total |

**Wall time timer reconstruction.** Wall time is tracked by a timer in the coordinator (runtime spec, §12). During `migrating`, the timer is paused — `migrating` is a frozen state, like `suspended`, and wall time should not accumulate. On transition back to `active` or `blocked`, the timer resumes. The elapsed time is: old agent's accumulated wall time + time in `active`/`blocked` after migration.

The `active_since` field in `ResourceMeterState` records when the workspace last entered a timer-active state. If the workspace was `active` before migration, `active_since` holds the timestamp of the most recent transition to `active`. The coordinator uses this to compute the correct elapsed duration when reconstructing the timer after migration.

### 8.2 Budget Warnings and Limits

Warning thresholds and hard limits do not change during migration. If the old agent consumed 79% of the token budget, the new agent starts at 79%. If the new agent's first checkpoint pushes consumption past the warning threshold (80% by default), the runtime emits a `resource_warning` trail entry and a feedback envelope to the new agent — exactly as it would without migration.

If the old agent was already past the warning threshold, the new agent inherits that state. No duplicate warning is emitted for the threshold crossing that the old agent already triggered — the warning is per-threshold-crossing, not per-agent.

### 8.3 Liveness Tracking

Liveness monitoring (runtime spec, §12) tracks the most recent trail entry timestamp for each active workspace. During `migrating`, the workspace produces no trail entries (it is frozen). The coordinator pauses liveness monitoring for the workspace during migration — a migrating workspace is not expected to produce activity.

On transition back to the pre-migration state (step 7), liveness monitoring resumes. The liveness clock resets to the `migration_completed` trail entry's timestamp — the new agent's first "activity" is the migration completion event. This prevents a false liveness timeout immediately after migration due to the gap.

---

## 9. Concurrency and Ordering Guarantees

Migration interacts with the runtime's concurrency model (runtime spec, §14). This section defines how the six concurrency invariants hold during migration.

### 9.1 Single-Writer Serialization (Invariant 1)

The workspace actor remains the single writer throughout migration. The actor does not stop — it transitions to `migrating` and continues processing messages from the high-priority (coordinator) channel. It stops processing messages from the normal-priority (agent) channel. The single-writer guarantee is preserved because:

- Only the workspace actor writes trail entries for this workspace.
- Only the workspace actor mutates the inbox, working memory, checkpoint register, and resource meter.
- The coordinator sends commands through the high-priority channel — it does not directly mutate workspace state.

No concurrent access to workspace state occurs during snapshot creation (§3). The snapshot is created by the workspace actor itself, within its message processing loop. The actor serializes its own state — no locking, no borrowing across actors.

### 9.2 Abort Precedence (Invariant 2)

An abort during migration takes priority. If the coordinator decides to abort the workspace while it is in `migrating` (e.g., a parent workspace fails, triggering cascade abort), the abort command arrives on the high-priority channel. The workspace actor processes it before any pending migration step. The workspace transitions from `migrating` to `failed` with `reason: aborted`, and the migration is cancelled.

The migration's own failure path (§6) is not an abort — it is a coordinator decision that flows through the same high-priority channel. The distinction: an abort is triggered by an external event (parent failure, budget exceeded, timeout). A migration failure is triggered by the migration itself (bind timeout, auth failure).

### 9.3 Signal Ordering (Invariant 4)

During `migrating`, no signals are emitted by the agent (the workspace is frozen). The only signals related to migration are the implicit state transitions recorded as trail entries (`migration_started`, `migration_completed`, `migration_failed`). These are not signals in the protocol sense — they are trail events. No signal ordering constraint is violated because no signals exist during the migration window.

After migration completes and the workspace returns to `active` or `blocked`, the new agent may emit signals. These signals are ordered after the `migration_completed` trail entry — the trail's sequence number guarantees this.

### 9.4 Trail Monotonicity (Invariant 5)

The trail remains monotonic across migration. The sequence is:

```
... → [last pre-migration entry] → migration_started → migration_completed → [first post-migration entry] → ...
```

All entries share the same workspace id, the same local trail partition, and the same hash chain. The `migration_started` entry's `prev_hash` links to the last pre-migration entry. The `migration_completed` entry's `prev_hash` links to `migration_started`. The first post-migration entry's `prev_hash` links to `migration_completed`. The chain is unbroken.

The HLC timestamp also advances monotonically — `migration_started` has a timestamp after the last pre-migration entry, and `migration_completed` has a timestamp after `migration_started`.

### 9.5 Timeout Race (Invariant 6)

The workspace's overall timeout (runtime spec, §12) is paused during `migrating` — the timer does not accumulate wall time in frozen states. If the workspace was close to its timeout before migration, the migration does not consume timeout budget. On return to `active` or `blocked`, the timer resumes from where it paused.

A race between the migration timeout (§2.4) and the workspace timeout is resolved by the coordinator's sequential message processing. Both timeouts are futures in the coordinator's `FuturesUnordered`. If the migration timeout fires first, the coordinator fails the migration (§6), which transitions the workspace to `failed`. The workspace timeout future is then cancelled. If the workspace timeout fires first (because it was very close to expiry and the timer was not properly paused due to a bug), the coordinator processes the workspace timeout — transitioning the workspace to `failed` with `reason: timeout`, which also cancels the migration.

---

## 10. Trail Events

Three trail events are defined for migration. They are part of the workspace's local trail and the global trail.

### 10.1 `migration_started`

Written at step 2 (§2.2), when the workspace transitions to `migrating`.

```rust
pub struct MigrationStartedBody {
    pub workspace_id: WorkspaceId,
    pub old_agent: String,          // agent identity or type description
    pub new_agent: String,          // agent identity or type description
    pub reason: String,             // why migration was initiated
    pub pre_migration_state: String, // "active" or "blocked"
}
```

**Actor field:** `"coordinator"` — migration is coordinator-initiated.

### 10.2 `migration_completed`

Written at step 7 (§2.2), when the workspace transitions back to its pre-migration state.

```rust
pub struct MigrationCompletedBody {
    pub workspace_id: WorkspaceId,
    pub old_agent: String,
    pub new_agent: String,
    pub duration_ms: u64,           // wall time from migration_started to migration_completed
    pub pre_migration_state: String, // the state being returned to
}
```

**Actor field:** `"coordinator"`.

### 10.3 `migration_failed`

Written when migration fails after the workspace entered `migrating` (§6.2).

```rust
pub struct MigrationFailedBody {
    pub workspace_id: WorkspaceId,
    pub old_agent: String,
    pub new_agent: String,
    pub reason: String,             // why migration was initiated (from the request)
    pub error: String,              // what went wrong (bind timeout, auth failure, etc.)
    pub failed_at_step: u32,        // which step failed (2–7)
    pub duration_ms: u64,           // wall time from migration_started to failure
}
```

**Actor field:** `"coordinator"`.

**Followed by:** A `workspace_state_changed` trail entry recording the transition from `migrating` to `failed`. The `migration_failed` entry provides the migration-specific context; the state change entry is the standard lifecycle event that recovery uses to reconstruct state.

### 10.4 No Trail Gap

The three migration events are part of the same local trail as all other workspace events. There is no separate "migration trail" or "migration log." The trail's hash chain is continuous — migration events link to the previous entry's hash like any other event. A trail query for this workspace returns migration events interleaved with all other events in sequence order.

---

## 11. SDK Integration

The new agent uses the standard SDK — no migration-specific API exists. This section defines what the new agent observes and how it is expected to behave.

### 11.1 What the New Agent Sees

The `BindResponse` is identical to a non-migration bind:

| Field | Value |
|-------|-------|
| `workspace_id` | Same workspace id (unchanged by migration) |
| `state` | `Active` or `Blocked` (the pre-migration state, set after step 7 completes) |
| `role` | Same role (unchanged) |
| `directive` | Same directive (immutable since creation) |
| `context` | Same context (immutable since creation) |
| `visibility` | Same visibility set (unchanged) |
| `authority` | Same authority set (unchanged) |
| `budget` | Current budget limits with current consumption (reflects old agent's usage) |

The new agent cannot distinguish a migration bind from a fresh bind by the `BindResponse` alone. The distinction is in the workspace's history — the local trail contains entries from before the new agent existed.

### 11.2 Expected New Agent Behavior

The new agent is expected to:

1. **Read the directive** to understand the task. The directive has not changed.
2. **Read the checkpoint register** (via `QueryTrail` filtering for `checkpoint_created` events, or via `ReadResource` for checkpoint payloads) to understand what work has been done.
3. **Read working memory** to see in-progress artifacts.
4. **Process inbox envelopes** delivered through `ReceiveEnvelopes`. These may include envelopes from before migration that the old agent did not process.
5. **Continue the task.** Create new checkpoints, emit signals, send envelopes — exactly as if it had been the agent from the beginning.

The SDK does not enforce any of this. The new agent may ignore the workspace's history and start fresh — this would be a poor agent implementation (duplicating work, ignoring context), but the protocol does not prohibit it. The trail records everything — post-hoc analysis will reveal whether the new agent utilized the old agent's work.

### 11.3 No Migration Callback

Neither the Python SDK nor the Rust SDK provides a "migration happened" callback or event. The new agent is simply bound and starts processing. If the agent implementation wants to detect migration, it can query the local trail for `migration_completed` events — but this is application logic, not SDK infrastructure.

The old agent receives no callback either. Its connection is dropped (§4.1). If the old agent's SDK has a disconnect handler, it fires on the connection drop — the handler cannot distinguish migration from any other disconnect cause.

---

## 12. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §6.9 (agent migration) | §1, §2 | Migration definition — atomic, coordinator-initiated, preserves nine components |

### Workspace Spec (`protocol/primitives/workspace.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §3 (states and transitions) | §2.1, §2.2, §6.1 | `migrating` state, valid transitions (`active`/`blocked` → `migrating` → `active`/`blocked`/`failed`) |
| §11 (agent migration) | §1, §6.2 | Atomicity guarantee, constraint list, trail events |

### Signal Spec (`protocol/primitives/signal.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §2 (signal types) | §9.3 | `migrate` signal — coordinator-emitted, triggers `migrating` state |

### Trail Spec (`protocol/primitives/trail.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §7 (event registry) | §10 | `migration_started`, `migration_completed`, `migration_failed` event types |

### Runtime Spec (`impl/runtime.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §2 (runtime boundaries) | §4.1 | Agents are untrusted — no migration notification |
| §4 (state machine engine) | §2.2 | FSM transition validation for `migrating` |
| §8 (workspace isolation) | §3.1, §3.2 | Nine components, ownership model, immutability rules |
| §12 (resource enforcement) | §8 | Timeouts, budgets, liveness, timer pausing in frozen states |
| §13 (recovery engine) | §6.4 | Crash recovery — detecting interrupted migrations |
| §14 (concurrency model) | §2.2, §9 | High-priority channel, biased select, six invariants |

### Storage Spec (`impl/storage.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §5 (checkpoint store) | §3.2 | Content-addressable payloads — not included in snapshot |
| §6 (workspace state persistence) | §3.3 | Serialization format reused for migration snapshot |

### Protocol Interface Spec (`impl/protocol-interface.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §4 (agent service contract) | §5.2 | `Bind` RPC, `BindResponse` fields |

### Deployment Spec (`impl/deployment.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §5.2 (PSK provider) | §4.2, §5.1 | Token generation, revocation, agent launch |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../protocol/PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../protocol/TAXONOMY.md)*
