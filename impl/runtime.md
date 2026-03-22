# WACP Implementation: Runtime Architecture

```yaml
id: wacp-impl-runtime
type: implementation-spec
status: complete
created: 2026-03-17
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4 (core primitives)
  - §5 (roles and permissions)
  - §6 (workspace lifecycle)
  - §9 (trail)
  - §10 (recovery and fault handling)
  - §11 (security model)
depends_on:
  - wacp-spec-workspace
  - wacp-spec-envelope
  - wacp-spec-signal
  - wacp-spec-checkpoint
  - wacp-spec-task
  - wacp-spec-trail
  - wacp-spec-clock
  - wacp-spec-roles
  - wacp-spec-identity
  - wacp-spec-recovery
  - wacp-spec-security
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, runtime, rust, state-machine, concurrency, trust-root]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Runtime Boundaries](#2-runtime-boundaries)
3. [Process Model](#3-process-model)
4. [State Machine Engine](#4-state-machine-engine)
5. [Permission Engine](#5-permission-engine)
6. [Trail Write-Ahead Path](#6-trail-write-ahead-path)
7. [Clock Implementation](#7-clock-implementation)
8. [Workspace Isolation](#8-workspace-isolation)
9. [Envelope Delivery](#9-envelope-delivery)
10. [Signal Propagation](#10-signal-propagation)
11. [Taxonomy Loader](#11-taxonomy-loader)
12. [Resource Enforcement](#12-resource-enforcement)
13. [Recovery Engine](#13-recovery-engine)
14. [Concurrency Model](#14-concurrency-model)
15. [Error Model](#15-error-model)
16. [Crate Structure](#16-crate-structure)
17. [References](#17-references)

## 1. Purpose

This spec defines how the WACP runtime becomes a Rust program. It answers "how does this become code" — not "what must be true" (that's the protocol's job) or "how is data stored" (that's the storage spec's job).

The runtime is the trust root (§11.1). Every protocol guarantee — permissions, lifecycle transitions, trail integrity, envelope delivery, budget enforcement — flows from the assumption that the runtime is correct. This spec defines the internal architecture that makes that assumption hold.

**Scope.** The runtime process: its internal components, their relationships, how they enforce protocol invariants, and how they expose services to external clients (agents, highway). What is inside the Rust binary.

**Not in scope.** Storage backends (storage spec). Protobuf message definitions and gRPC service contracts (protocol-interface spec). Agent SDK surface area (sdk-agent spec). These depend on the runtime spec but are defined separately.

**Design constraint.** The runtime is a single logical process. It may use multiple threads internally, but it presents as one process to the outside world. Distribution (multiple runtime instances coordinating) is a future concern — the protocol supports it, but the first implementation targets a single-node runtime.

## 2. Runtime Boundaries

The runtime has exactly two external boundaries. Everything inside is trusted; everything outside is untrusted.

**Boundary 1: Agent-facing.** Agents connect to the runtime to receive envelopes, emit signals, create checkpoints, and query their local trail. Every agent connection is bound to exactly one workspace. The runtime authenticates the agent at connection time and associates the connection with the workspace's role. Every message from the agent is validated against the permission engine before it takes effect. Agents are untrusted — a well-behaved agent and a rogue agent receive identical enforcement.

**Boundary 2: Highway-facing.** Humans connect through the highway interface to observe the trail, respond to gates, inject envelopes, and handle escalations. The runtime authenticates the human and validates every action. Highway clients are untrusted — injection envelopes are validated (structure, target existence, type registration) even though they bypass role-based send restrictions (§8.1).

**No other boundaries exist.** The storage layer is internal to the runtime process — not an external service. The clock is internal. The taxonomy is loaded at initialization and held in memory. The runtime does not call out to external services during protocol operations. External dependencies (LLM providers, tools) are the agent's concern, not the runtime's.

**Transport abstraction.** Both boundaries are served through a transport trait. The default implementation is gRPC (protocol-interface spec). The trait is narrow: accept connections, authenticate, send messages, receive messages, disconnect. The runtime's internal logic is transport-agnostic — it operates on typed Rust structs, not serialized bytes.

## 3. Process Model

The runtime is structured as an **event-driven actor system** built on `tokio`.

**Why actors.** The protocol's natural unit of concurrency is the workspace. Each workspace has its own state, its own inbox, its own trail, and single-writer serialization (§6.7, invariant 1). This maps directly to an actor: a workspace actor owns its state, processes messages sequentially, and communicates with other actors only through message passing. No shared mutable state between workspaces.

**Three actor types:**

1. **Coordinator actor.** Singleton. Owns the workspace tree, the task graph, the global trail index, and the taxonomy. Receives signals from workspace actors, makes orchestration decisions (dispatch, integration, abort, migration), and sends directives. This is the protocol's coordinator (§5.2) realized as a runtime component — it is both a protocol participant and a runtime internal.

2. **Workspace actor.** One per active workspace. Owns the nine internal components (§6.1): directive, inbox, context, working memory, checkpoint register, resource meter, local trail, visibility set, authority set. Processes envelopes from the agent connection and from the coordinator actor. Emits signals to the coordinator actor. Enforces its own state machine transitions. Created by the coordinator actor, destroyed (frozen as immutable record) when the workspace reaches a terminal state.

3. **Transport actor.** One per external boundary (agent-facing, highway-facing). Accepts connections, authenticates, routes incoming messages to the correct workspace or coordinator actor, and routes outgoing messages to the correct connection. Stateless with respect to protocol logic — it is a router, not an enforcer.

**Actor communication.** Actors communicate through `tokio::mpsc` channels. Each actor owns a receiver; senders are cloned and distributed to other actors that need to send it messages. Channel capacity is bounded — backpressure propagates naturally. The coordinator actor holds senders to all workspace actors. Workspace actors hold a sender to the coordinator actor. Transport actors hold senders to the coordinator and to individual workspaces.

**Lifecycle.** The runtime starts by spawning the coordinator actor, which loads the taxonomy, initializes the clock, opens the trail store, and begins accepting connections through the transport actors. The coordinator actor is the last to shut down — it drains all workspace actors first, ensuring every workspace reaches a terminal state or is explicitly aborted.

## 4. State Machine Engine

The protocol defines three lifecycle state machines: workspace (9 states), envelope (5 states), and task (8 statuses). Rather than implementing each independently, the runtime uses a single generic state machine engine that all three instantiate.

**The generic FSM.** A state machine is defined by three types: `State` (an enum of valid states), `Trigger` (an enum of valid triggers), and `Context` (the data carried through transitions). The engine provides one operation: `transition(current_state, trigger, context) -> Result<State, TransitionError>`.

```rust
trait StateMachine {
    type State: Copy + Eq;
    type Trigger;
    type Context;

    fn transition(
        state: Self::State,
        trigger: Self::Trigger,
        ctx: &Self::Context,
    ) -> Result<Self::State, TransitionError>;
}
```

**Exhaustive matching.** Each implementation of `transition` is a `match` on `(state, trigger)`. Rust's exhaustiveness checking guarantees every combination is handled — either as a valid transition returning the new state, or as an explicit rejection returning `TransitionError::IllegalTransition`. Adding a state or trigger to the enum forces every match to be updated. An unhandled transition is a compile error, not a runtime surprise.

**Transition validation sequence.** Every transition follows the same four-step sequence, enforced by the engine:

1. **Permission check.** Is this trigger permitted for the current actor? (Permission engine, §5.)
2. **Precondition check.** Are the trigger's preconditions satisfied? (e.g., a workspace cannot transition to `integrating` without a final checkpoint.)
3. **Trail write.** The transition is recorded in the trail before it takes effect. (Write-ahead, §6.)
4. **State update.** The state is changed. Downstream effects (signal emission, envelope delivery) are triggered.

If any step fails, the transition does not occur. Step 3 is the commit point — once the trail entry is written, the transition will take effect even if the process crashes immediately after (recovery replays it).

**Three instantiations:**

| FSM | States | Triggers | Owner |
|-----|--------|----------|-------|
| Workspace lifecycle | `Idle`, `Active`, `Blocked`, `Suspended`, `Migrating`, `Integrating`, `Conflicted`, `Closed`, `Failed` | Agent signals, coordinator actions, timeouts, budget exceeded | Workspace actor |
| Envelope lifecycle | `Created`, `Validated`, `Delivered`, `Acknowledged`, `Rejected` | Submission, validation pass/fail, delivery, agent ack | Workspace actor (sender side) + coordinator |
| Task lifecycle | `Draft`, `Pending`, `Assigned`, `InProgress`, `Completed`, `Failed`, `Integrated`, `Cancelled` | Gate approval, workspace assignment, signals, integration result | Coordinator actor |

No backward transitions in any of the three. Terminal states are absorbing — once entered, no trigger produces a new state.

## 5. Permission Engine

The permission engine is the runtime's enforcement core. Every action an agent or human attempts passes through it before taking effect. The engine answers one question: given an actor, an action, and a target, is this permitted?

**Data structures.** The engine holds three lookup tables, built at initialization from the base protocol definitions plus the loaded taxonomy:

1. **Permission matrix.** `HashMap<(RoleName, EnvelopeType, RoleName), bool>` — can a sender with this role send this envelope type to a receiver with that role? Base rows from the protocol (§5.5), extended by taxonomy-registered envelope types (TAXONOMY.md §4). Lookup is O(1) per envelope send.

2. **Checkpoint type table.** `HashMap<CheckpointType, HashSet<RoleName>>` — which roles may create checkpoints of this type? Base entries (`artifact` → `worker`, `observation` → `observer`), extended by taxonomy. Lookup is O(1) per checkpoint creation.

3. **Port rights table.** `HashMap<WorkspaceId, Vec<PortRight>>` — active send rights, receive rights, and send-once rights per workspace. Mutated at runtime as rights are created, transferred, and revoked. Every envelope send checks this table in addition to the permission matrix — the matrix says the role *could* send; the port rights table says the workspace *currently can* send.

**Evaluation order.** For every agent action, the engine evaluates in fixed order:

1. **Authentication.** Is the actor's identity verified? (Reject if not.)
2. **Role lookup.** What role does this workspace hold? (Resolved from the workspace record, including derived role resolution: base → remove → add.)
3. **Action-specific check.** Depends on the action type:
   - *Envelope send:* permission matrix lookup + port rights check.
   - *Signal emission:* is this signal type in the role's emit set?
   - *Checkpoint creation:* checkpoint type table lookup.
   - *Trail query:* is the query scope within the role's access set?
   - *Visibility read:* is the target in the workspace's visibility set?
4. **Decision.** Allow or deny. Both outcomes produce a trail entry.

**Deny is the default.** If an action does not match an explicit allow rule, it is denied. There is no default-allow. This is the protocol's design (§5.1): "If an action is not explicitly permitted, it is denied."

**Highway override.** Human-injected envelopes bypass step 3's permission matrix check — humans can send any envelope type to any workspace (§8.1). All other validation (authentication, structure, target existence, type registration) remains enforced. The engine distinguishes human actions by the `origin: human` field, which is system-assigned and immutable.

**Immutability.** The permission matrix and checkpoint type table are immutable after initialization — they are built from the taxonomy, which is static within a run (TAXONOMY.md §9). The port rights table is the only mutable structure in the permission engine.

## 6. Trail Write-Ahead Path

The trail write is the runtime's commit point. The protocol requires that every event produces a trail entry *before* the event takes effect (§9.1). This is the write-ahead rule — if the write fails, the event does not proceed. The trail write path is the most latency-sensitive operation in the runtime.

**The write path.** Every protocol operation follows the same sequence:

1. The originating actor (workspace or coordinator) constructs a trail entry with all fields populated: id (runtime-assigned), timestamp (from the clock), workspace, actor, event_type, body.
2. The entry is passed to the trail writer.
3. The trail writer computes the hash chain link: `hash = H(previous_hash || entry_bytes)`. This extends the tamper-evident chain (§11.5).
4. The entry + hash are written to durable storage. The write is **synchronous** — the call does not return until the data is durable (fsync or equivalent). No write batching, no async flush.
5. On success, the trail writer returns the assigned entry id and the updated chain head hash. The originating operation proceeds.
6. On failure, the trail writer returns an error. The originating operation is aborted. The runtime does not silently skip trail writes.

**Why synchronous.** Async writes with batching would improve throughput but violate the write-ahead guarantee. If the runtime crashes between accepting an operation and flushing the batch, operations that appeared to succeed have no trail record. Recovery would reconstruct a state that diverges from what agents observed. The correctness cost is absolute; the performance cost is bounded (one fsync per operation, mitigated by storage spec choices).

**Hash chain.** Each trail entry carries the hash of the previous entry, forming a chain from the first entry to the most recent. The hash function is SHA-256. The chain is per-scope: the global trail has one chain, each local trail has its own. Cross-scope anchoring (local trail heads periodically recorded in the global trail) is a Level 3 conformance feature — supported but not required for initial implementation.

**Failure handling.** If a trail write fails:

- The operation that triggered it is rejected. The actor receives an error.
- If writes fail persistently, the runtime enters degraded mode (§10.3): no new operations are initiated, active agents may continue but their output cannot be recorded.
- When writes recover, a `system_degraded` entry is recorded, followed by a `system_recovered` entry. The gap between them represents a period where operations may have occurred without trail coverage — the runtime logs this explicitly.

**Concurrency.** The trail writer is a single-writer component — all trail entries from all actors are funneled through it. This is a serialization point. Within a workspace, entries are strictly ordered by timestamp. Across workspaces, the writer assigns a global sequence number in addition to the timestamp, providing a total order when needed for recovery. The writer is not an actor — it is a synchronous service called by actors, ensuring the write completes before the actor's message processing continues.

## 7. Clock Implementation

The protocol requires a clock that provides monotonic timestamps within a workspace, partial ordering across workspaces, sufficient resolution to avoid collisions, and durability across restarts (clock spec, four invariants).

**Choice: Hybrid Logical Clock (HLC).** The runtime uses an HLC — a combination of physical wall time and a logical counter. An HLC timestamp is a pair `(physical_time, logical_counter)`.

- `physical_time` is read from the system clock (`std::time::SystemTime`). It provides wall-clock proximity — timestamps are meaningful to humans and to external systems.
- `logical_counter` is incremented when two events would otherwise share the same `physical_time`. It provides the monotonicity guarantee within and across message exchanges.

**Why HLC over pure Lamport.** A pure Lamport clock gives monotonicity but produces timestamps with no relation to wall time. Trail queries by time range ("show me everything between 14:00 and 14:05") would be impossible. HLC preserves wall-clock proximity while guaranteeing the causal ordering that the protocol requires.

**Why HLC over pure wall clock.** A wall clock can go backward (NTP corrections), produce duplicates (insufficient resolution), and has no causal relationship to message delivery. HLC corrects for all three: physical time is bounded below by the previous timestamp (no backward movement), the logical counter breaks ties (no duplicates), and message receipt advances the clock (causal ordering through message passing, per causation spec).

**Timestamp generation rules:**

1. **Local event.** Read system clock. If it exceeds the current HLC physical component, advance physical and reset counter to 0. Otherwise, increment counter.
2. **Message send.** Generate a local event timestamp. Attach it to the message.
3. **Message receive.** Take the max of (local physical, received physical). If physical advanced, reset counter. Otherwise, take max of counters and increment. This ensures the receive timestamp is strictly greater than the send timestamp.

**Durability.** On startup, the runtime reads the last trail entry's timestamp. The clock is initialized to at least that timestamp + 1 logical tick. This prevents post-restart timestamps from collapsing into the pre-restart range, satisfying the durability invariant.

**Resolution.** Physical time is stored as microseconds since Unix epoch (64-bit integer). The logical counter is a 16-bit integer, allowing 65,535 events within the same microsecond before physical time must advance. This exceeds any realistic single-node throughput.

**Representation.** An HLC timestamp is a single 80-bit value: 64 bits physical + 16 bits logical. For storage and serialization, it is encoded as a 10-byte big-endian byte array — lexicographic byte ordering matches temporal ordering, enabling efficient range queries in the trail store.

## 8. Workspace Isolation

Each workspace is an actor that owns its state exclusively. This section defines how the nine internal components (§6.1) are represented in memory and how isolation is enforced.

**Ownership model.** A workspace actor holds its nine components as owned fields in a struct. No component is shared with any other actor. No reference, pointer, or handle to a workspace's internal state exists outside that workspace's actor. Communication between workspaces goes through channels — always copies or moves, never borrows.

```rust
struct WorkspaceState {
    id: WorkspaceId,
    status: WorkspaceStatus,          // current FSM state
    role: ResolvedRole,               // effective permissions (base + taxonomy overrides)
    parent: WorkspaceId,              // parent workspace id
    owner: UserId,                    // human on whose behalf this workspace exists
    originator: Originator,           // user_id or "system"

    // The nine components
    directive: Directive,             // 1. immutable task assignment
    inbox: VecDeque<Envelope>,        // 2. append by runtime, consume by agent
    context: Context,                 // 3. immutable read-only info from coordinator
    working_memory: WorkingMemory,    // 4. mutable agent workspace (files, intermediate results)
    checkpoint_register: Vec<Checkpoint>,  // 5. append-only checkpoint chain
    resource_meter: ResourceMeter,    // 6. runtime-managed consumption tracking
    local_trail: TrailHandle,         // 7. handle to this workspace's trail partition
    visibility_set: VisibilitySet,    // 8. frozen at creation, extendable by coordinator
    authority_set: AuthoritySet,      // 9. frozen at creation
}
```

**Freezing rules.** The protocol defines different mutability rules for each component. The runtime enforces these structurally:

- `directive`, `context`, `authority_set`: set at creation, never modified. Stored as owned values behind no mutable reference. Any attempt to modify after initialization is a type error — these fields are not exposed through any `&mut` method after construction.
- `visibility_set`: set at creation, expandable only by the coordinator. The workspace actor accepts a `GrantVisibility` message from the coordinator actor. The set grows monotonically — the `add` method exists, no `remove` method exists.
- `inbox`: append-only from the runtime's perspective (envelopes arrive), consume-only from the agent's perspective (agent processes them). The workspace actor mediates — it appends envelopes received from the transport and yields them to the agent connection in order.
- `checkpoint_register`: append-only. The `push` method exists, no `pop`, `remove`, or `replace` method exists. The runtime validates each checkpoint against the chain head (parent must be the current tail) before appending.
- `resource_meter`: mutated only by the runtime (the workspace actor itself), never by the agent. Updated on every trail write, envelope delivery, and checkpoint creation.

**Terminal state freezing.** When a workspace reaches `Closed` or `Failed`, the workspace actor converts `WorkspaceState` into an `ArchivedWorkspace` — an immutable snapshot with no mutable methods. The actor stops processing messages. The archived state is persisted to storage and the actor task completes. The `tokio` task is dropped. The channel senders held by other actors become disconnected — sends to a terminated workspace return an error, which the coordinator handles.

**Working memory.** The `WorkingMemory` component is the agent's scratch space. Its representation depends on what the agent works on — files on disk, structured data in memory, or a combination. The runtime does not interpret working memory contents. It tracks its size for budget enforcement (storage dimension) and persists it for recovery, but does not validate or constrain its structure. The working memory boundary is the authority set — the agent can modify anything within its authority and nothing outside it.

## 9. Envelope Delivery

Envelope delivery is the runtime's message passing operation. An envelope moves from sender to receiver through a validated, trail-recorded pipeline. No shortcut exists — every envelope traverses the full pipeline regardless of sender, receiver, or priority.

**The delivery pipeline.** Six steps, executed in order:

1. **Receive.** The transport actor receives a serialized envelope from an agent connection or the highway. It deserializes into a typed `Envelope` struct and forwards to the workspace actor (for agent-sent envelopes) or the coordinator actor (for highway injections).

2. **Validate.** The permission engine checks:
   - Sender workspace exists and is in a state that permits sending (`Active` or `Blocked`).
   - Envelope type is registered (base type or taxonomy-registered).
   - Sender role is permitted to send this type to the receiver's role (permission matrix).
   - Sender holds a valid port right to the receiver (port rights table).
   - For `send_once` rights: the right exists and has not been consumed.
   - For highway injections: `origin: human` is set, permission matrix check is skipped, all other checks apply.

   Validation failure produces a trail entry (`envelope_rejected`) and an error to the sender. The envelope does not advance.

3. **Assign identity.** The runtime assigns the envelope's `id` and `timestamp`. These are runtime-assigned, never agent-supplied (identity spec, rule 1). If the agent included an id, it is overwritten.

4. **Trail write.** The `envelope_created` trail entry is written (write-ahead, §6). This is the commit point. After this write, the envelope *will* be delivered — even if the runtime crashes, recovery will replay the delivery.

5. **Deliver.** The envelope is sent to the receiver's workspace actor through its channel. The workspace actor appends it to the inbox. An `envelope_delivered` trail entry is written. For `send_once` rights, the right is consumed (destroyed) after delivery.

6. **Acknowledge.** The receiver's workspace actor emits an `acknowledged` signal automatically upon delivery. An `envelope_acknowledged` trail entry is written. The agent does not manually acknowledge — the runtime handles it.

**Priority handling.** Envelopes carry a priority: `normal`, `urgent`, or `blocking`. Priority affects inbox ordering within the workspace actor, not the delivery pipeline itself. The workspace actor maintains a priority queue — `blocking` envelopes are processed before `urgent`, which are processed before `normal`. The delivery pipeline treats all envelopes identically.

**Redelivery.** If delivery fails (receiver's channel is full, workspace actor is temporarily unreachable), the runtime retries up to 3 times with linear backoff. Each attempt is recorded in the trail. After 3 failures, the envelope transitions to `Rejected` with `reason: delivery_failed`. The sender and coordinator are notified.

**Idempotency.** The receiver workspace tracks delivered envelope ids. If a redelivery arrives for an already-processed envelope (possible after a crash-recovery cycle), it is recorded in the trail as a duplicate but does not enter the inbox. The agent sees each envelope exactly once.

**Threading.** Envelopes carry an `in_reply_to` field referencing a previous envelope id. The runtime does not enforce threading structure — it records the reference in the trail for query support. Thread reconstruction is a trail query operation, not a delivery concern.

---

## 10. Signal Propagation

Signals are lightweight state notifications that propagate upward through the workspace tree. Unlike envelopes, signals are not addressed — they are emitted by a workspace and delivered to its parent, recursively up to the coordinator.

**Emission.** A workspace actor emits a signal in two ways:

- **Explicit.** The agent requests a signal emission (e.g., `blocked`, `complete`, `escalation`). The workspace actor validates that the signal type is in the role's emit set (permission engine, §5). If valid, the signal is created.
- **Automatic.** The runtime emits signals without agent involvement. A `checkpoint` signal is emitted when a checkpoint is created (§7.2, rule 5). An `acknowledged` signal is emitted on envelope delivery. The workspace actor handles these internally — no agent action required.

**The propagation path.** Once emitted:

1. The workspace actor constructs the signal: type, workspace id, timestamp (from the clock), and payload (for `blocked`: the reason; for `escalation`: the escalation context; for others: empty).
2. A trail entry (`signal_emitted`) is written (write-ahead).
3. The signal is sent to the parent workspace's actor through the coordinator's channel. The coordinator actor is the hub — all signals route through it because it owns the workspace tree and needs to observe all state changes.
4. The coordinator actor processes the signal. Depending on the type:
   - `complete`: initiates integration (workspace transitions to `Integrating`).
   - `failed`: records the failure, may trigger retry or cascade.
   - `blocked`: records the block, may dispatch unblocking input or escalate.
   - `escalation`: routes to the highway for human handling, in addition to coordinator processing.
   - `checkpoint`: records the new checkpoint for integration planning.
   - Others: recorded and processed per coordinator logic.
5. If the workspace has a delegate parent (not the root coordinator), the signal is delivered to the delegate first. The delegate may handle it locally or propagate it further. Propagation stops when a handler processes the signal or it reaches the root.

**Signal ordering.** Signals from the same workspace are delivered in emission order (§6.7, invariant 4). The workspace actor emits signals sequentially (single-writer), and the channel preserves FIFO order. Signals from different workspaces have no guaranteed relative order — the coordinator processes them in arrival order, which reflects the partial ordering of the clock.

**Idempotency.** Signals are idempotent with respect to state transitions (§4.2). If a duplicate signal arrives (possible after recovery replay), the coordinator checks whether the transition it would trigger has already occurred. If so, the signal is recorded in the trail but does not alter state. This prevents recovery from double-applying transitions.

**Escalation routing.** An `escalation` signal is special — it crosses from the protocol layer to the highway layer. The coordinator actor forwards escalation signals to the highway transport actor, which delivers them to connected highway clients. Escalations route to the workspace's owner by default (§8.1). If no highway client is connected, the escalation is queued until one connects or the timeout expires (per highway configuration).

## 11. Taxonomy Loader

The taxonomy is loaded once at initialization, validated, and converted into the runtime's lookup tables. It does not change during a run. The loader is a startup component, not a runtime service.

**Input.** A YAML or JSON file conforming to the taxonomy schema (TAXONOMY.md §2). The file path is provided as a runtime configuration parameter. An absent or empty taxonomy is valid — the runtime operates with base types only.

**Loading sequence.** Five steps, all-or-nothing:

1. **Parse.** Deserialize the file into typed Rust structs (`TaxonomyDefinition`, `RoleDefinition`, `EnvelopeTypeDefinition`, `CheckpointTypeDefinition`). Parse failure aborts the run with a configuration error.

2. **Protocol version check.** The taxonomy's `protocol_version` field must match the runtime's compiled protocol version. A mismatch aborts the run. This prevents stale taxonomies from operating against a newer runtime.

3. **Validate.** The eleven checks from TAXONOMY.md §7, executed in order:
   - Name uniqueness within each registry (roles, envelope types, checkpoint types).
   - No collision with base names (`coordinator`/`worker`/`observer`, `directive`/`feedback`/`query`, `artifact`/`observation`).
   - Inheritance validity: every `extends` field names a base role, not a derived role.
   - No privilege escalation: no derived role adds coordinator-level capabilities.
   - Cross-registry consistency: derived roles referencing custom types must reference types registered in the same taxonomy.
   - Envelope type role references exist (base or derived).
   - Checkpoint type role references exist (base or derived).
   - Non-empty permissions for every envelope type.
   - Non-empty role list for every checkpoint type.

   Any check failure rejects the entire taxonomy. The runtime does not load a partial taxonomy. The error message identifies the failing entry and the violated check.

4. **Resolve derived roles.** For each derived role, apply the resolution algorithm: start with the base role's capabilities, apply `remove`, then apply `add` (TAXONOMY.md §3). The result is a `ResolvedRole` — the effective permission set. This is computed once and cached.

5. **Build lookup tables.** Three tables, as defined in the permission engine (§5):
   - Permission matrix: base rows + taxonomy envelope type rows.
   - Checkpoint type table: base entries + taxonomy checkpoint type entries.
   - Role table: base roles + resolved derived roles.

**Output.** A `Taxonomy` struct containing the three lookup tables plus metadata (taxonomy id, version, protocol version). This struct is immutable — it is `Arc`-shared with the coordinator actor and all workspace actors. No actor can modify it. The taxonomy id and version are recorded in the first trail entry of the run.

**No hot reloading.** The taxonomy is not watched for changes. Modifying the taxonomy file during a run has no effect. A new taxonomy takes effect at the next run's initialization. This is a deliberate design choice (TAXONOMY.md §9) — mid-run taxonomy changes would invalidate the permission matrix, which would invalidate every in-flight permission check.

## 12. Resource Enforcement

The runtime enforces three resource boundaries per workspace: timeouts, budgets, and liveness (§6.6). These are independent mechanisms — any one can trigger failure independently of the others.

**Timeouts.** Every workspace has a timeout — the maximum cumulative duration in `Active` + `Blocked` + `Conflicted` states. The runtime tracks this with a per-workspace timer managed by the coordinator actor.

- On transition to `Active`: start or resume the timer.
- On transition to `Blocked` or `Conflicted`: timer continues running (wall time counts against the budget in these states).
- On transition to `Suspended` or `Migrating`: pause the timer. These states freeze the workspace — the agent is not running, so wall time should not count.
- On timeout expiry: the coordinator actor sends an abort to the workspace actor, which transitions to `Failed` with `reason: timeout`. The abort follows the precedence rules (§6.7, invariant 2) — it is processed before queued agent signals.
- The timeout clock never resets. Extensions are additive — the coordinator can increase the timeout, but the elapsed time is never zeroed.

Implementation: `tokio::time::sleep_until` for each workspace, recalculated on state transitions that pause or resume the timer. The coordinator actor holds a `FuturesUnordered` of timeout futures, polled alongside its message channel.

**Budgets.** Optional per-workspace limits across four physical dimensions plus a derived cost ceiling:

| Dimension | What it measures | Updated on |
|-----------|-----------------|------------|
| Compute (tokens) | LLM tokens consumed | Agent-reported via checkpoint `resource_usage` field |
| Compute (wall time) | Cumulative active processing time | Timer (same mechanism as timeout) |
| Memory (context) | Context window usage | Agent-reported |
| Storage | Checkpoint bytes + trail bytes | Trail writer (trail bytes) + checkpoint creation (checkpoint bytes) |
| Network | Envelope payload bytes delivered | Envelope delivery pipeline |
| Cost | Derived monetary ceiling | Computed from dimension usage × configured rates |

The resource meter is updated by the workspace actor on every measurable operation. The runtime tracks consumption independently of agent self-reporting for the dimensions it can observe (storage, network, trail bytes). Agent-reported dimensions (tokens, context) are trusted but recorded — the trail makes over-reporting or under-reporting detectable in post-hoc analysis.

Warning at a configurable threshold (default 80%). Hard failure at the limit. Warning produces a `resource_warning` trail entry and a feedback envelope to the agent. Hard failure transitions the workspace to `Failed` with `reason: budget_exceeded`, following the same precedence as timeout (§6.7, invariant 3).

The coordinator may increase budgets additively during execution — the workspace actor accepts a `BudgetIncrease` message from the coordinator. Budgets are never decreased.

**Liveness.** Optional monitoring of agent activity. The coordinator tracks the most recent trail entry timestamp for each active workspace. If no entry is recorded within the configured liveness interval, a `liveness_warning` trail entry is produced and the coordinator is notified. Liveness is advisory — the coordinator decides whether to escalate, send a ping, or abort. The runtime does not automatically fail a workspace on liveness timeout.

## 13. Recovery Engine

The recovery engine reconstructs runtime state from the trail after a crash. The trail is the single source of truth (§10.2) — recovery is trail replay.

**When recovery runs.** At every startup, unconditionally. The runtime does not distinguish between a clean start and a crash recovery. It always replays the trail. On a clean start with no prior trail, replay is a no-op. On a restart after a crash, replay reconstructs the last consistent state. This eliminates the need for a separate "was the shutdown clean?" check.

**The recovery procedure.** Five steps, matching the recovery spec (§10.4):

1. **Trail integrity check.** Walk the hash chain from the first entry to the last. Verify each link: `H(previous_hash || entry_bytes) == stored_hash`. If a broken link is found, recovery halts — trail corruption requires human intervention. The runtime refuses to start with a corrupted trail.

2. **State reconstruction.** Replay trail entries in global sequence order. For each entry, apply the corresponding state change:
   - `workspace_created`: create workspace state in memory with initial components.
   - `workspace_state_changed`: advance the workspace FSM.
   - `envelope_created` / `envelope_delivered` / `envelope_acknowledged`: reconstruct envelope lifecycle state. If `envelope_created` exists but `envelope_delivered` does not, the envelope is in-flight — it will be redelivered (step 3).
   - `signal_emitted`: reconstruct signal state. Idempotency (§10, §4.2) ensures replayed signals do not double-apply transitions.
   - `checkpoint_created`: rebuild the checkpoint register for the workspace.
   - `task_*` events: rebuild the task graph.
   - `port_right_*` events: rebuild the port rights table.
   - `resource_*` events: rebuild resource meters.

   After replay, every workspace is in the state recorded by its most recent trail entry. Workspaces in terminal states (`Closed`, `Failed`) are loaded as archived records. Workspaces in non-terminal states are loaded as live actors.

3. **In-flight recovery.** Identify operations that were committed (trail entry written) but not completed:
   - Envelopes with `envelope_created` but no `envelope_delivered`: redeliver.
   - Workspaces with `workspace_state_changed` to `Integrating` but no integration result: restart integration.
   - Gate events with no response: re-emit to the highway.

   These are the operations that were in progress when the crash occurred. The write-ahead guarantee ensures their trail entries exist. Redelivery and re-execution are safe because all operations are idempotent.

4. **Timer reconstruction.** For each active workspace, compute remaining timeout from the trail: sum all durations in timer-active states (`Active`, `Blocked`, `Conflicted`) from trail timestamps. Subtract from the configured timeout. Set the timer to the remainder. If already exceeded, transition to `Failed` immediately.

5. **Clock recovery.** Initialize the HLC to at least the last trail entry's timestamp + 1 logical tick (clock implementation, §7). This ensures post-recovery timestamps are strictly greater than all pre-crash timestamps.

**Recovery invariants enforced:**

| Invariant | How enforced |
|-----------|-------------|
| Trail-authoritative state (§10.4, #1) | State is built exclusively from trail replay — no other source consulted |
| Write-ahead trail (§10.4, #2) | Trail entries exist for all committed operations — the write-ahead path guarantees this |
| Idempotent recovery (§10.4, #3) | Running recovery twice produces the same result — replay is deterministic, signals are idempotent |
| No silent data loss (§10.4, #4) | In-flight operations are detected and completed — nothing is silently dropped |
| Degradation over catastrophe (§10.4, #5) | Only trail corruption halts recovery — all other failures are recoverable |

**Performance.** Full replay scales linearly with trail size. For long-running systems, periodic snapshots can accelerate recovery — the runtime replays from the most recent snapshot instead of the beginning. Snapshot implementation is defined in the storage spec.

## 14. Concurrency Model

The runtime runs many workspaces simultaneously. This section defines how concurrency is structured to satisfy the protocol's six concurrency invariants (§6.7) without locks on shared state.

**No shared mutable state.** This is the foundational decision. No two actors share a mutable data structure. The coordinator actor owns the workspace tree and task graph. Each workspace actor owns its nine components. The trail writer is a serialization point accessed through a channel, not a shared mutex. The permission engine's immutable tables are `Arc`-shared (read-only). The port rights table is owned by the coordinator actor — workspace actors request permission checks through messages.

**How each invariant is satisfied:**

**Invariant 1: Single-writer serialization.** Each workspace actor processes messages sequentially from its `tokio::mpsc` channel. One message at a time, one state transition at a time. No concurrent mutations within a workspace. This is the actor model's fundamental guarantee — sequential message processing without locks.

**Invariant 2: Abort precedence.** The coordinator actor's abort message must be processed before queued agent signals. Implementation: workspace actors use a two-channel design. A high-priority `tokio::mpsc` channel for coordinator commands (abort, suspend, migrate, budget increase, visibility grant). A normal-priority channel for agent messages (envelopes, signal requests, checkpoint submissions). The actor's select loop checks the high-priority channel first:

```rust
loop {
    tokio::select! {
        biased;  // check in declared order, not randomly
        Some(cmd) = coordinator_rx.recv() => self.handle_coordinator_cmd(cmd).await,
        Some(msg) = agent_rx.recv() => self.handle_agent_msg(msg).await,
    }
}
```

The `biased` select ensures coordinator commands always take priority over agent messages when both are available.

**Invariant 3: External failure precedence.** Budget and timeout failures are coordinator commands — they arrive on the high-priority channel. Same mechanism as abort precedence.

**Invariant 4: Signal emission ordering.** Signals from the same workspace are emitted sequentially (single-writer actor) and sent through a single channel to the coordinator. `tokio::mpsc` preserves FIFO order. Ordering is guaranteed without additional mechanism.

**Invariant 5: Trail monotonicity.** The trail writer assigns a global sequence number to every entry. Within a workspace, entries are strictly ordered because the workspace actor is single-threaded — it writes one trail entry at a time and waits for the write to complete before proceeding. Across workspaces, the global sequence number provides a total order when needed. The HLC timestamp provides the partial order for concurrent events.

**Invariant 6: Timeout race resolution.** The timeout future and the `complete` signal both arrive at the coordinator actor. The coordinator processes messages sequentially. If `complete` arrives first, it transitions the workspace to `Integrating` and cancels the timeout future. If the timeout fires first, it transitions the workspace to `Failed` — a subsequent `complete` is recorded in the trail but cannot trigger a transition (the workspace is in a terminal state, and the FSM rejects all triggers from terminal states).

**Thread allocation.** The `tokio` runtime uses a multi-threaded scheduler (default: one thread per CPU core). Workspace actors are `tokio` tasks — lightweight, cooperatively scheduled, multiplexed across threads. The runtime does not pin actors to threads. A system with 100 active workspaces runs 100 tasks across, say, 8 threads. The coordinator actor and transport actors are also tasks in the same pool. No dedicated threads for any component.

## 15. Error Model

The runtime distinguishes between errors that are protocol events (expected, handled within the coordination model) and errors that are infrastructure failures (unexpected, handled by the runtime itself). The two categories never mix — a protocol error does not crash the runtime, and an infrastructure failure does not produce a protocol signal.

**Protocol errors.** These are expected outcomes within the coordination model. They produce trail entries and protocol responses. They do not propagate as Rust panics or unhandled errors.

| Error | Produced by | Trail event | Response |
|-------|------------|-------------|----------|
| Permission denied | Permission engine | `permission_denied` | Error to agent |
| Illegal transition | State machine engine | `transition_rejected` | Error to agent |
| Envelope validation failed | Delivery pipeline | `envelope_rejected` | Error to sender |
| Checkpoint type invalid | Workspace actor | `checkpoint_rejected` | Error to agent |
| Budget exceeded | Resource enforcement | `workspace_state_changed` (→ `Failed`) | Workspace terminated |
| Timeout | Resource enforcement | `workspace_state_changed` (→ `Failed`) | Workspace terminated |
| Delivery failed (after retries) | Delivery pipeline | `envelope_rejected` | Notification to sender + coordinator |

All protocol errors are `Result::Err` values in Rust. They are handled at the call site — never bubbled up as panics. The workspace actor's message processing loop catches all protocol errors, records them, and continues processing the next message.

**Infrastructure errors.** These are failures in the runtime's own machinery. They cannot be expressed as protocol events because the protocol's recording mechanism may itself be affected.

| Error | Cause | Response |
|-------|-------|----------|
| Trail write failure | Storage I/O error | Operation rejected; persistent failure → degraded mode (§6) |
| Channel disconnected | Actor task panicked or was dropped | Coordinator treats workspace as failed; records in trail if possible |
| Clock failure | System clock unavailable | Runtime halts — no operations can be timestamped |
| Taxonomy parse failure | Malformed configuration | Runtime refuses to start |
| Transport failure | Network error, connection drop | Connection dropped; agent must reconnect; workspace enters `Blocked` if no reconnection within liveness interval |

Infrastructure errors use Rust's `Result` type at the boundary where they occur. They are never silently swallowed. The escalation path: try to record in the trail → if trail is unavailable, log to stderr → if persistent, halt the runtime.

**Panic policy.** The runtime does not panic under normal operation. `unwrap()` and `expect()` are forbidden outside of initialization code (where a failure means the runtime cannot start). All fallible operations return `Result`. If a workspace actor task panics despite this policy (a bug), `tokio`'s task join handle detects it. The coordinator actor treats the panicked workspace as failed — recording a `workspace_state_changed` to `Failed` with `reason: internal_error`. The runtime continues operating. A coordinator actor panic is fatal — the runtime shuts down.

---

## 16. Crate Structure

The runtime is a Cargo workspace — multiple crates with defined dependency relationships. Crate boundaries follow trust and abstraction boundaries. Each crate has a single responsibility and a clear public API.

```
wacp-runtime/
├── Cargo.toml                  # workspace manifest
├── crates/
│   ├── wacp-types/             # shared type definitions
│   ├── wacp-clock/             # HLC implementation
│   ├── wacp-trail/             # trail writer, hash chain, query
│   ├── wacp-fsm/               # generic state machine engine
│   ├── wacp-permissions/       # permission engine, port rights
│   ├── wacp-taxonomy/          # taxonomy loader and validation
│   ├── wacp-workspace/         # workspace actor and state
│   ├── wacp-coordinator/       # coordinator actor logic
│   ├── wacp-transport/         # transport trait + gRPC implementation
│   ├── wacp-recovery/          # recovery engine
│   └── wacp-runtime/           # binary: wires everything together
```

**Crate responsibilities:**

| Crate | Owns | Depends on |
|-------|------|------------|
| `wacp-types` | Protocol enums (signals, states, priorities), identifier types, envelope/checkpoint/task/workspace structs. No logic — pure data definitions. | None (leaf crate) |
| `wacp-clock` | HLC implementation. Timestamp generation, comparison, serialization. | `wacp-types` |
| `wacp-trail` | Trail writer, hash chain computation, trail query interface. Storage backend trait (implementation in storage spec). | `wacp-types`, `wacp-clock` |
| `wacp-fsm` | Generic `StateMachine` trait. Workspace, envelope, and task FSM implementations. Transition validation sequence. | `wacp-types` |
| `wacp-permissions` | Permission matrix, checkpoint type table, port rights table. Evaluation logic. | `wacp-types`, `wacp-taxonomy` |
| `wacp-taxonomy` | Taxonomy parsing, validation, derived role resolution, lookup table construction. | `wacp-types` |
| `wacp-workspace` | Workspace actor: state struct, nine components, message handling, agent interaction. | `wacp-types`, `wacp-clock`, `wacp-trail`, `wacp-fsm`, `wacp-permissions` |
| `wacp-coordinator` | Coordinator actor: workspace tree, task graph, orchestration decisions, integration initiation. | `wacp-types`, `wacp-clock`, `wacp-trail`, `wacp-fsm`, `wacp-permissions`, `wacp-workspace` |
| `wacp-transport` | Transport trait definition. gRPC server implementation (tonic). Connection management, authentication, routing. | `wacp-types` |
| `wacp-recovery` | Recovery procedure: trail integrity check, state reconstruction, in-flight recovery, timer reconstruction. | `wacp-types`, `wacp-trail`, `wacp-fsm`, `wacp-clock`, `wacp-workspace`, `wacp-coordinator` |
| `wacp-runtime` | The binary. Initialization sequence: parse config, load taxonomy, open trail, run recovery, spawn coordinator, start transport, accept connections. Shutdown sequence. | All crates |

**Dependency rule.** Dependencies flow downward — leaf crates (`wacp-types`, `wacp-clock`) depend on nothing internal. Higher-level crates depend on lower-level ones. No circular dependencies. The `wacp-runtime` binary crate is the only crate that depends on everything.

**Testing boundary.** Each crate is independently testable. `wacp-fsm` can be tested with mock states and triggers. `wacp-trail` can be tested with an in-memory storage backend. `wacp-workspace` can be tested with a mock coordinator channel. Integration tests live in the `wacp-runtime` crate, where all components are wired together.

## 17. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §4 (core primitives) | §1, §4, §8 | Primitive definitions — workspace, envelope, signal, checkpoint, task, trail |
| §4.2 (envelope) | §9, §10 | Envelope lifecycle, delivery guarantees, signal idempotency |
| §4.3 (signal) | §10 | Signal types (closed set), upward propagation |
| §4.7 (identity) | §9 | Runtime-assigned identifiers |
| §4.8 (user identity) | §8 | Originator, ownership |
| §5 (roles and permissions) | §5, §10, §11 | Permission matrix, role resolution, delegation |
| §5.1 (assignment rules) | §5 | Default-deny evaluation |
| §5.5 (permission matrix) | §5 | Base envelope permissions |
| §5.6 (port rights) | §5, §9 | Send, receive, send-once rights |
| §6.1 (internal model) | §3, §8 | Nine workspace components |
| §6.2–§6.3 (states and transitions) | §4 | Workspace state machine |
| §6.6 (resource management) | §12 | Timeouts, budgets, liveness |
| §6.7 (concurrency) | §14 | Six concurrency invariants |
| §6.8 (visibility and authority) | §8 | Dynamic visibility, frozen authority |
| §6.9 (agent migration) | §1 | Migration as connection management |
| §7.2 (checkpoint rules) | §10 | Auto-signal emission on checkpoint creation |
| §8.1 (highway capabilities) | §2, §5, §9 | Human injection, escalation routing |
| §9.1 (trail entry schema) | §6 | Write-ahead rule, entry structure |
| §10 (recovery) | §13 | Recovery model, failure classification |
| §10.3 (partial failures) | §6, §15 | Degraded mode |
| §10.4 (recovery invariants) | §13 | Five recovery invariants |
| §11.1 (trust root) | §1, §2 | Runtime as trust root |
| §11.3 (identity and authentication) | §2, §5 | Agent, coordinator, human authentication |
| §11.5 (trail integrity) | §6 | Hash chain, tamper evidence |

### Constituent Specs

| Spec | Referenced in | Topic |
|------|--------------|-------|
| Clock spec | §7 | Four clock invariants, HLC design basis |
| Roles spec | §5, §11 | Base roles, derived role resolution order |
| Identity spec | §9 | Opaque identifiers, runtime-assigned rule |
| Trail spec | §6, §13 | Write-ahead, hash chain, scopes, tiered storage |
| Recovery spec | §13 | Five-step recovery procedure |
| Causation spec | §7 | Causal ordering through message passing |

### TAXONOMY.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §2 (contents) | §11 | Taxonomy structure |
| §3 (derived role registry) | §11 | Role resolution: base → remove → add |
| §4 (envelope type registry) | §5, §11 | Permission matrix extension |
| §7 (validation) | §11 | Eleven validation checks |
| §9 (taxonomy lifecycle) | §11 | Immutable within a run |

### Implementation Specs

| Spec | Relationship | Topic |
|------|-------------|-------|
| Storage spec (`impl/storage.md`) | Downstream | Trail backend, checkpoint store, workspace persistence, snapshots |
| Protocol interface spec (`impl/protocol-interface.md`) | Downstream | Protobuf definitions, gRPC service contracts, serialization |
| Agent SDK spec (`impl/sdk-agent.md`) | Downstream | Python + Rust SDK surface, connection lifecycle |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../protocol/PROTOCOL.md) | Implementation Journal: [IMPLEMENTATION.md](../IMPLEMENTATION.md)*
