# WACP: Workspace

## Metadata

```yaml
title: "WACP: Workspace"
id: wacp-spec-workspace
type: constituent-spec
tier: abstract
category: primitives
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §6.1–§6.10 (workspace lifecycle, state machine, creation, tree, resources, concurrency, visibility, migration, graceful termination)
depends_on:
  - wacp-spec-clock
  - wacp-spec-identity
  - wacp-spec-roles
  - wacp-spec-user
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, workspace, lifecycle, state-machine, isolation, resources]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Internal Model](#2-internal-model)
3. [Lifecycle](#3-lifecycle)
4. [Creation](#4-creation)
5. [Workspace Tree](#5-workspace-tree)
6. [Resource Management](#6-resource-management)
7. [Concurrency](#7-concurrency)
8. [Visibility and Authority](#8-visibility-and-authority)
9. [Groups](#9-groups)
10. [Admission Control](#10-admission-control)
11. [Agent Migration](#11-agent-migration)
12. [System Lifecycle](#12-system-lifecycle)
13. [Trail Events](#13-trail-events)
14. [Conformance Requirements](#14-conformance-requirements)
15. [Implementation Notes](#15-implementation-notes)
16. [References](#16-references)

---

## 1. Purpose

The workspace is the protocol's unit of isolation. Every agent operates inside exactly one workspace. The workspace defines what the agent can see, what it can do, how long it may run, and what resources it may consume. It is the boundary that makes parallel execution safe — agents cannot interfere with each other because they cannot reach outside their workspace except through the protocol's communication primitives (envelopes and signals).

This spec defines the workspace's internal structure, its lifecycle from creation to terminal state, the tree that organizes workspaces into a hierarchy, and the resource management model that governs budgets, timeouts, and liveness. It is the protocol's process model — the equivalent of a process in an operating system, with its own address space, resource limits, and lifecycle managed by a supervisor.

## 2. Internal Model

A workspace is not a black box. It has a defined internal structure — nine components that together constitute the workspace's state. This model serves three purposes: it tells implementers what a workspace must contain, it gives recovery a precise target (reconstruct these nine components and the workspace is restored), and it makes agent migration definable (transfer these nine components and the agent can resume in a new runtime).

The nine components:

| # | Component | Mutability | Access |
|---|-----------|------------|--------|
| 1 | Directive | Immutable | Agent: read. Coordinator: read. |
| 2 | Inbox | Append by runtime, consume by agent | Agent: read/consume. Runtime: append. |
| 3 | Context | Immutable | Agent: read. Coordinator: read. |
| 4 | Working Memory | Mutable | Agent: read/write. Others: none (unless visibility granted). |
| 5 | Checkpoint Register | Append-only | Agent: read own. Coordinator: read. Integration: read. |
| 6 | Resource Meter | Runtime-managed | Agent: read. Runtime: read/write. Coordinator: read. |
| 7 | Local Trail | Append-only | Agent: read own. Coordinator: read. Observer: read (if designated). |
| 8 | Visibility Set | Frozen at creation, extendable by coordinator | Runtime: read (for permission checks). Agent: read (to know what it can see). |
| 9 | Authority Set | Frozen at creation | Runtime: read (for permission checks). |

**Directive.** The task assignment — the envelope that tells the agent what to do. Set at creation by the coordinator, constructed from the task's `description` field (PROTOCOL §4.6). The directive's metadata carries a `task_ref` linking this workspace to the task it executes. Immutable for the workspace's lifetime. The agent reads the directive to understand its task; it cannot modify or replace it. If a different task is needed, a new workspace is created. The directive is a reference to an envelope, not a copy — it carries the envelope's ID (Identity spec §2, rule 5: referenceable).

**Inbox.** The priority queue of envelopes waiting to be processed. The runtime appends envelopes that pass permission validation (Roles spec §6). The agent consumes envelopes in priority order — `blocking` before `urgent` before `normal` — with FIFO ordering within each priority level (Envelope spec, §3). Envelopes remain in the inbox until the agent acknowledges them. The inbox is the workspace's input channel — everything the agent hears from the outside world arrives here.

**Context.** Read-only information provided at creation alongside the directive. This may include references to checkpoints from completed dependency tasks (PROTOCOL §4.6), configuration parameters, domain-specific data, or anything the coordinator wants the agent to have access to without granting full workspace visibility. Context is immutable — it captures what was known at the time of task assignment.

**Working Memory.** The agent's mutable state — files, intermediate results, scratch data, any artifacts in progress. This is the only component the agent can modify. "Modify workspace files" in the permission matrix (Roles spec §6) means modify working memory. No other workspace can write to this space. Read access is governed by the visibility set — if another workspace has visibility into this one, it can read working memory but not write to it.

**Checkpoint Register.** The ordered sequence of checkpoints the agent has produced. Append-only — once a checkpoint is created and recorded, it cannot be modified or removed (PROTOCOL §7.1). Each entry carries a checkpoint ID (Identity spec §2), a type (constrained by the agent's role), a status (`provisional` or `final`), and a reference to the content in working memory. The register is the workspace's output record — it captures what the agent has produced.

**Resource Meter.** Tracks the workspace's consumption against its budgets across the four physical dimensions (PROTOCOL §6.6): compute (tokens consumed, wall time elapsed), memory (context tokens), storage (checkpoint bytes, trail bytes), and network (delivery bytes), plus derived cost. The runtime updates the meter; the agent can read it to gauge remaining capacity. When consumption crosses a warning threshold or hard limit, the runtime acts (PROTOCOL §6.6). The meter is the workspace's accounting system.

**Local Trail.** The workspace's portion of the global trail. Every protocol event involving this workspace — state transitions, envelope deliveries, signal emissions, checkpoint creations, permission denials, budget events — is recorded here in timestamp order (Clock spec §3, invariant 1). The local trail is a projection of the global trail, filtered to this workspace. The agent can read its own local trail to understand its history.

**Visibility Set.** The set of workspace IDs this workspace can read. Populated at creation by the coordinator. Determines what the agent can see beyond its own workspace — which other workspaces' working memory, checkpoint registers, and local trails are accessible (read-only). The visibility set is frozen at creation but can be extended by the coordinator through dynamic visibility grants (PROTOCOL §6.8). It can never be reduced. A visibility grant adds to the set; nothing removes from it.

**Authority Set.** The set of actions this workspace is permitted to perform. Determined by two inputs: the role assigned at creation (Roles spec §3) and the delegation capability if granted (Roles spec §4). The authority set is frozen at creation — it cannot be modified during the workspace's lifetime. This is the workspace's capability set, evaluated by the runtime at every action. The permission matrix (Roles spec §6) is the authority set's lookup table.

## 3. Lifecycle

A workspace progresses through a linear lifecycle. There are nine states: two productive (`idle`, `active`), three suspended (`blocked`, `migrating`, `suspended`), two resolution states (`integrating`, `conflicted`), and two terminal states (`closed`, `failed`). No backward transitions exist — except for `migrating` and `suspended`, which return the workspace to its pre-suspension state.

```
idle → active → integrating → closed
  │       │         │
  │       │         └→ conflicted → closed
  │       │                      → failed
  │       ├→ blocked → active
  │       │     │
  │       │     ├→ migrating → blocked
  │       │     │          └→ failed
  │       │     ├→ suspended → blocked
  │       │     │          └→ failed
  │       │     └→ failed
  │       ├→ migrating → active
  │       │          └→ failed
  │       ├→ suspended → active
  │       │          └→ failed
  │       └→ failed ─────────────x
  └→ failed ─────────────────────x
```

**States:**

**`idle`** — Workspace exists, agent is bound, no work has begun. The agent has emitted `ready` and is waiting for its first envelope. The directive, context, visibility set, and authority set are populated. Working memory, checkpoint register, and resource meter are at their initial values. This is the only state in which the coordinator may modify visibility and authority grants before work begins.

**`active`** — The agent has received input and is working. Envelopes flow through the inbox. Checkpoints may be created. The resource meter is accumulating. Most of a workspace's lifetime is spent here.

**`blocked`** — The agent cannot continue and has emitted a `blocked` signal with a reason. The workspace is suspended — no checkpoints may be created, but the inbox still accepts envelopes (typically feedback from the coordinator to unblock). Transitions back to `active` when the agent emits `started` after receiving unblocking input. The resource meter continues to accumulate (wall time passes even when blocked).

**`migrating`** — The coordinator has initiated an agent migration (§11). The workspace is frozen — no new checkpoints, no new envelopes processed, no signals emitted. The runtime is transferring the workspace's live state to a new agent. On success, the workspace returns to its pre-migration state (`active` or `blocked`). On failure, the workspace transitions to `failed`.

**`suspended`** — The coordinator has suspended the workspace. All nine components are preserved but processing stops — no new checkpoints, no envelope consumption, no signals emitted by the agent. The inbox continues to accept envelopes (they queue for later delivery, unlike `integrating` where the inbox is sealed). The resource meter pauses — wall time does not accrue during suspension (unlike `blocked`, where it does). Suspension is coordinator-initiated, not agent-initiated — the agent cannot suspend itself (that would be `blocked`). On resumption, the workspace returns to its pre-suspension state (`active` or `blocked`). Suspension serves operational needs: load shedding, priority reordering, human review of agent behavior, system-level maintenance windows, or user-state propagation — the coordinator may suspend workspaces in a user's originator tree when that user is suspended or blocked (User spec §3.5).

**`integrating`** — The agent has emitted `complete` and the coordinator has begun merging this workspace's checkpoints into the parent context. The workspace is read-only — no new checkpoints, no new envelopes accepted. The coordinator reads the checkpoint register and working memory. If the merge succeeds without conflicts, the workspace transitions to `closed`. If conflicts are detected, the workspace transitions to `conflicted`.

**`conflicted`** — The coordinator detected a conflict during integration that cannot be resolved mechanically. The workspace remains read-only. The coordinator evaluates the conflict and selects a resolution strategy (PROTOCOL §7.8). The Human Highway may be activated through a conflict resolution gate. Entry into this state produces a `conflict_detected` trail entry; exit produces a `conflict_resolved` trail entry.

**`closed`** — Terminal. Integration is complete. The entire workspace — all nine components — is immutable and archived. Trail, checkpoints, and envelopes are preserved permanently. No further activity can occur. A closed workspace cannot be reopened; if the work needs revision, a new workspace is created.

**`failed`** — Terminal. The agent encountered an unrecoverable error, the coordinator aborted the workspace, a timeout expired, or a budget limit was exceeded. The trail is preserved for diagnosis. No further transitions are possible.

**Transition rules:**

| From | To | Trigger | Initiator |
|------|----|---------|-----------|
| *(none)* | `idle` | Coordinator creates workspace | Coordinator |
| `idle` | `active` | Agent receives first envelope | Protocol (auto) |
| `idle` | `failed` | Creation error or timeout | Protocol (auto) |
| `active` | `migrating` | Coordinator initiates migration | Coordinator |
| `active` | `blocked` | Agent emits `blocked` signal | Agent |
| `active` | `integrating` | Agent emits `complete` signal | Agent + Coordinator |
| `active` | `failed` | Agent emits `failed`, coordinator aborts, timeout expires, or budget exceeded | Agent / Coordinator / Runtime |
| `blocked` | `active` | Agent emits `started` signal | Agent |
| `blocked` | `migrating` | Coordinator initiates migration | Coordinator |
| `blocked` | `suspended` | Coordinator suspends workspace | Coordinator |
| `blocked` | `failed` | Coordinator aborts, timeout expires, or budget exceeded | Coordinator / Runtime |
| `migrating` | `active` | Migration completes (was active before migration) | Runtime |
| `migrating` | `blocked` | Migration completes (was blocked before migration) | Runtime |
| `migrating` | `failed` | Migration error — new agent cannot be bound | Runtime |
| `active` | `suspended` | Coordinator suspends workspace | Coordinator |
| `suspended` | `active` | Coordinator resumes workspace (was active before suspension) | Coordinator |
| `suspended` | `blocked` | Coordinator resumes workspace (was blocked before suspension) | Coordinator |
| `suspended` | `failed` | Coordinator aborts, or system-level forced shutdown | Coordinator / Runtime |
| `integrating` | `closed` | Integration succeeds without conflict | Coordinator |
| `integrating` | `conflicted` | Coordinator detects conflict during merge | Coordinator |
| `integrating` | `failed` | Integration error (non-conflict) | Coordinator |
| `conflicted` | `closed` | Conflict resolved, integration complete | Coordinator |
| `conflicted` | `failed` | Conflict unresolvable or resolution aborted | Coordinator |

**No backward transitions — with two exceptions.** A workspace never moves from `integrating` or `conflicted` back to `active`. If a conflict requires agent rework, the coordinator fails the current workspace and creates a new workspace with a feedback directive referencing the conflict. The original workspace's trail preserves the full conflict record. The two exceptions are `migrating` and `suspended`, which both return the workspace to its pre-suspension state — these are not backward transitions in the lifecycle sense; they are suspension and resumption. `migrating` resumes with a different agent; `suspended` resumes with the same agent.

**Internal model through the lifecycle.** At creation, the directive, context, visibility set, and authority set are populated and frozen. During `active`, the inbox, working memory, checkpoint register, resource meter, and local trail are live — receiving input, accumulating state, producing output. At terminal states (`closed`, `failed`), all nine components are frozen. Recovery reconstructs the nine components from the trail to resume a workspace at its last known state.

**Graceful termination.** The coordinator may request that a workspace wind down before being forcibly failed. This is the protocol's equivalent of SIGTERM — a cooperative shutdown signal. The coordinator sends a feedback envelope with `intent: graceful_termination` and a `grace_period` duration. The agent is expected to produce a final checkpoint capturing its current progress within the grace period, then emit `complete`. If the grace period expires without the agent emitting `complete`, the runtime transitions the workspace to `failed` with `reason: graceful_termination_expired`. The agent's partial checkpoint (if any) is preserved in the register. Graceful termination is distinct from abort (which is immediate, no grace period) and from timeout (which is a safety net, not a request). The coordinator uses graceful termination when it wants to reclaim resources without losing in-progress work — the agent has time to save state.

## 4. Creation

Only the coordinator — or a delegate (Roles spec §4) — can create workspaces. Creation populates the workspace's nine internal components and places the workspace in `idle` state.

**Creation parameters:**

```yaml
workspace_create:
  id: string                   # runtime-assigned (Identity spec §2, rule 1)
  owner: user_id               # the human principal (User spec §4)
  originator: user_id | "system" # required: the causal origin (User spec §5, PROTOCOL §4.8)
  role: string                 # base or derived role from the taxonomy
  parent: workspace_id         # parent in the workspace tree
  directive: envelope_id       # the task assignment
  task_ref: task_id            # optional: the task this workspace executes (PROTOCOL §4.6)
  context: object              # read-only information for the agent
  visibility:                  # read access grants (workspace IDs)
    - workspace_id
  authority:                   # write access grants (own workspace files)
    - resource_ref
  delegate: boolean            # optional: grant delegation capability (default: false)
  timeout: duration            # max time in active/blocked/conflicted before auto-fail
  budget:                      # optional: resource limits (§6), structured per PROTOCOL §6.6
    compute_budget:
      token_limit: integer       # max tokens (input + output) across workspace lifetime
      compute_timeout: duration  # max wall-clock time in active state
    memory_budget:
      context_limit: integer     # max context window tokens (physical: bytes of RAM)
    storage_budget:
      checkpoint_limit: integer  # max checkpoint bytes
      trail_limit: integer       # optional: max trail entry bytes
    network_budget:              # optional
      bandwidth_limit: integer   # max delivery bytes (envelopes + signals)
    cost_limit: number           # derived ceiling: provider-defined function over physical dimensions
  priority: enum               # critical | interactive | normal | background (default: normal)
  group: string                # optional: group ID for batch operations (§9)
  liveness_interval: duration  # optional: max silence before liveness warning (§6)
```

**What creation does to the internal model:**

| Component | Initialized to |
|-----------|---------------|
| Owner | Explicit `owner` field, or inherited from parent workspace (User spec §4). Immutable once set (changes only via ownership transfer, User spec §4). |
| Originator | Required. `"system"` for system-initiated workspaces, `user_id` for human-caused workspaces. Inherited from parent unconditionally, except at injection points where the injecting human's `user_id` overrides inheritance (PROTOCOL §4.8, rule 4). Immutable permanently. |
| Directive | The referenced envelope — immutable from this point. References the task via `task_ref` in the directive's metadata (PROTOCOL §4.6). |
| Inbox | Empty — first envelope delivery triggers `idle` → `active` |
| Context | The provided context object — immutable from this point |
| Working Memory | Empty (or seeded by implementation) |
| Checkpoint Register | Empty |
| Resource Meter | Zeroed — token count, compute time, cost all at 0 |
| Local Trail | First entry: `workspace_created` |
| Visibility Set | The provided workspace IDs, plus own workspace (always implicit) |
| Authority Set | Computed from role + delegation capability — frozen |

**Freezing rules.** Once the workspace transitions out of `idle`, authority is frozen — an agent cannot gain write access it was not granted at creation. Visibility may be expanded by the coordinator during execution (§8) but never reduced. This asymmetry is deliberate: granting read access to additional context is safe; granting write access to additional resources is not.

**Delegation at creation.** If `delegate: true`, the workspace gains the delegation capability (Roles spec §4). This adds coordinator-like envelope permissions scoped to the workspace's subtree. Delegation cannot be acquired after leaving `idle` state and cannot be revoked.

**Destruction does not exist.** Workspaces are never destroyed. They reach a terminal state (`closed` or `failed`) and become immutable records. Destroying a workspace would leave gaps in the global trail. Every workspace that ever existed remains queryable.

The coordinator may force a workspace into `failed` state at any time — this is the abort mechanism. Abort is immediate: the agent receives no further envelopes, and a `failed` signal is recorded in the trail with `reason: aborted_by_coordinator`.

**Rejection.** Workspace creation may be rejected by the runtime if system capacity is exhausted (§10). Rejection is recorded in the trail. A rejected workspace never enters `idle` — it never exists.

**Priority changes.** The coordinator may change a workspace's priority while it is in `active` or `blocked` state. Priority changes are recorded as a `priority_changed` trail entry. Priority cannot be changed once the workspace enters `integrating`, `conflicted`, or a terminal state.

## 5. Workspace Tree

Workspaces form a strict tree rooted at the coordinator's workspace. The tree is the protocol's process hierarchy — it expresses dependency, determines signal propagation, and enforces containment.

**Structure.** Every workspace except the root has exactly one parent, identified by the `parent` field at creation (Identity spec §2, rule 5: referenceable). The root workspace is the coordinator's — created by the runtime at initialization (PROTOCOL §5.1). The tree grows as the coordinator (or delegates) create child workspaces.

```
ws-root (coordinator)
 ├── ws-task-01 (worker)
 ├── ws-task-02 (worker, delegate: true)
 │    ├── ws-task-02a (worker)
 │    └── ws-task-02b (worker)
 └── ws-task-03 (observer)
```

In this example, `ws-task-02` is a delegate — it created its own children (`ws-task-02a`, `ws-task-02b`). The root coordinator created the top-level workspaces. The tree captures both relationships.

**Three invariants:**

1. **A child cannot outlive its parent — within ownership.** If a parent workspace transitions to `failed`, its children with the same `owner` are aborted immediately — they transition to `failed` with `reason: parent_failed`. This cascades recursively within the ownership boundary. Children with a different owner are not aborted; they are reparented to the coordinator (User spec §4). The coordinator's workspace is the root; if it fails, all workspaces fail regardless of ownership (PROTOCOL §6.5) — there is no parent to reparent to.

2. **Visibility flows downward by default.** A parent can see all descendants (the coordinator sees everything). A child can only see what was explicitly granted at creation through its visibility set. Siblings cannot see each other unless the coordinator (or their parent delegate) grants cross-visibility.

3. **Containment flows upward.** A child's visibility set must be a subset of its parent's. A child's authority set must be a subset of its parent's. A child's budget must be carved from its parent's remaining budget (Roles spec §4). No workspace can exceed the boundaries of its parent. The tree is a containment hierarchy — permissions narrow as you go deeper.

**The tree has two roots.** The workspace tree has a structural root — the coordinator's workspace (`originator: "system"`) — and zero or more causal roots — the humans who inject work. The structural tree (parent → child) determines signal propagation and containment. The causal tree (originator → workspace) determines user-state impact. The coordinator is the system, not a user; its autonomous workspaces carry `originator: "system"`. Human injections create causal boundaries: the injected workspace carries `originator: <user_id>`, overriding inheritance at that point (PROTOCOL §4.8, rule 4). When a user's state changes — `active` → `suspended`, `active` → `blocked`, or any state → `deactivated` — the coordinator is notified and evaluates the originator tree for downstream action (User spec §3.5). The protocol does not prescribe cascade behavior; the coordinator decides what to do.

**Signals propagate upward.** When a workspace emits a signal, it propagates to the parent. If the parent is a delegate, the delegate handles it locally (for its subtree management) and the signal also appears in the global trail. The coordinator observes all signals — via direct delivery from its children, and via the global trail for signals emitted deeper in the tree. Trail observation is not delivery.

**The tree is stable once formed, with one exception.** A workspace's parent cannot change under normal operation. The one exception is ownership-boundary reparenting: when a parent is aborted and a child with a different owner survives, the child is reparented to the coordinator (User spec §4). Reparenting is recorded in the trail as a `workspace_reparented` event. Beyond this exception, workspaces cannot be moved between parents or detached. The trail's parent references include the reparenting history, and recovery uses this to reconstruct the tree deterministically.

## 6. Resource Management

Three mechanisms govern a workspace's resource boundaries: timeouts catch workspaces that run too long, budgets catch workspaces that consume too much, and liveness monitoring catches workspaces that have stopped making progress. They are independent — each operates on its own dimension, and all three may be active simultaneously.

| Mechanism | What it catches | Trigger | Response |
|-----------|----------------|---------|----------|
| Timeout | Hung or abandoned workspaces | Wall-clock duration in `active` + `blocked` + `conflicted` exceeds limit | Automatic `failed` with `reason: timeout` |
| Budget | Cost overruns, context exhaustion | Token, compute, or cost consumption exceeds hard limit | Automatic `failed` with `reason: budget_exceeded` |
| Liveness | Stalled agents producing no output | No trail activity within interval | Advisory warning to coordinator |

**Timeouts.** Every workspace is created with a timeout (§4). If the workspace remains in `active`, `blocked`, or `conflicted` state beyond this duration, the runtime transitions it to `failed`. Timeout is a safety net, not a performance target — it prevents abandoned workspaces from consuming resources indefinitely. The timeout clock never resets; it is a monotonic countdown from the moment the workspace leaves `idle`.

**Budgets.** A workspace's budget declares maximum resource consumption across the four physical dimensions plus a derived cost ceiling (PROTOCOL §6.6):

| Physical resource | Field(s) | Meaning |
|-------------------|----------|---------|
| Compute | `token_limit`, `compute_timeout` | Maximum tokens (input + output) across workspace lifetime; maximum wall-clock time in `active` state only — time in `blocked` does not accrue |
| Memory | `context_limit` | Maximum context window tokens — the physical RAM ceiling for the workspace's live context |
| Storage | `checkpoint_limit`, `trail_limit` | Maximum checkpoint bytes; maximum trail entry bytes (optional) |
| Network | `bandwidth_limit` | Maximum delivery bytes (envelopes + signals) — optional |
| Cost (derived) | `cost_limit` | Maximum monetary cost — a provider-defined function over the four physical dimensions |

Budgets are optional — a workspace created without a budget operates without resource limits (though implementations may set defaults). Each dimension is enforced independently.

When the workspace executes a task with a `resource_estimate` (PROTOCOL §4.6), the coordinator may use the estimate to inform the workspace's budget — allocating proportionally, with safety margins, or weighted by critical path position. The estimate is the input; the budget is the constraint.

**Budget enforcement is layered:**

- **Agent-side (cooperative).** The agent reports consumption in each checkpoint's `resource_usage` field (PROTOCOL §7.1). Informational — the agent declares what it believes it has consumed.
- **Runtime-side (mandatory).** The runtime tracks actual consumption independently. The runtime is the authoritative source — if agent and runtime disagree, the runtime's numbers are correct. Hard limits are enforced regardless of agent self-report.
- **Coordinator-side (strategic).** The coordinator reads consumption data from checkpoints and the trail to make macro decisions: grant more budget, abort, or adjust dispatch parallelism.

**Budget lifecycle:**

1. Workspace created with budget — runtime begins tracking.
2. Consumption reaches warning threshold (implementation-defined, recommended 80%) — runtime records `budget_warning` trail entry, coordinator notified. Coordinator may send feedback ("wrap up"), increase budget, or wait.
3. Consumption reaches hard limit — runtime allows the current in-flight operation to complete (no mid-generation interruption), then transitions workspace to `failed` with `reason: budget_exceeded` and `budget_dimension: <which limit>`.

**Budget modification.** The coordinator may increase a workspace's budget while it is in `idle`, `active`, or `blocked` state. Modification is always additive — limits cannot be decreased below current consumption. Each modification is recorded in the trail. Budget cannot be modified once the workspace enters `integrating`, `conflicted`, or a terminal state.

**Budget inheritance for delegates.** When a delegate creates a child workspace with a budget, the child's budget is carved from the delegate's remaining budget (Roles spec §4). The runtime enforces: `child.budget ≤ delegate.remaining_budget`. The delegate's consumption includes all children's consumption.

**Liveness monitoring.** A workspace may be created with a `liveness_interval` — the maximum duration between consecutive trail entries before the runtime raises a concern. The liveness clock resets whenever any trail entry is recorded for the workspace. If the interval elapses with no trail entry, the runtime records a `liveness_warning` and notifies the coordinator.

Liveness is advisory, not terminal. Unlike timeout (which forces `failed`), a liveness warning lets the coordinator decide: send feedback to check on the agent, abort the workspace, or wait (the task legitimately requires long silent processing). This advisory model is deliberate — agents legitimately vary in processing time, and a one-size-fits-all rule would generate noise.

**All three mechanisms may be active simultaneously.** A workspace with a 30-minute timeout and a 5-minute liveness interval will receive liveness warnings at 5, 10, 15, 20, and 25 minutes of silence — but will be forcibly failed at 30 minutes regardless. Budget limits operate orthogonally: the workspace may hit its token limit at minute 12, well before the timeout.

## 7. Concurrency

The protocol operates in environments where signals, envelopes, and coordinator actions may arrive concurrently. Six invariants ensure deterministic behavior. Together they guarantee that no race condition can produce an ambiguous state.

**Invariant 1: Single-writer serialization.** Within a workspace, state transitions are serialized. If two events arrive simultaneously that would each trigger a transition, the runtime processes them in receipt order. The first transition that succeeds determines the new state; the second event is evaluated against that new state. Implementations must guarantee that no two state transitions are processed concurrently for the same workspace.

**Invariant 2: Abort precedence.** The coordinator's abort is a priority action. When a coordinator abort and an agent signal arrive concurrently for the same workspace, the abort wins — it is processed before queued agent signals. The workspace transitions to `failed`. Any agent signals that arrive after the abort are recorded in the trail but cannot trigger transitions (terminal state). This ensures the coordinator can always halt a workspace, regardless of what the agent is doing.

**Invariant 3: External failure precedence.** Runtime-initiated failures (budget exhaustion, §6) and protocol-initiated failures (timeout, §6) follow the same precedence rules as coordinator abort. When an external failure and an agent signal arrive concurrently, the external failure wins. The workspace transitions to `failed`. The agent signal is recorded in the trail but cannot trigger a transition. Resource limits are walls, not guidelines.

**Invariant 4: Signal emission ordering.** Signals from the same workspace are delivered to the parent in emission order. If an agent emits `checkpoint` followed by `complete`, the parent receives them in that order — guaranteed by the transport layer. Signals from different workspaces carry no ordering guarantee relative to each other. Cross-workspace ordering is reconstructed from trail timestamps when needed, not from delivery order.

**Invariant 5: Trail monotonicity.** Trail entries within a workspace are strictly ordered — timestamps are monotonic, with no duplicates (Clock spec §3, invariant 1). This enables deterministic replay and total ordering of events within a workspace. Across workspaces, timestamps provide a partial order only (Clock spec §3, invariant 2). Two events in different workspaces with the same timestamp are concurrent.

**Invariant 6: Timeout race resolution.** A timeout may fire at the exact moment an agent completes. The protocol resolves this deterministically:
1. If the `complete` signal is processed before the timeout fires, the timeout is cancelled. The workspace enters `integrating`.
2. If the timeout fires before the `complete` signal is processed, the workspace enters `failed` with `reason: timeout`. The late `complete` signal is recorded in the trail but cannot trigger a transition.

The timeout is a scheduled event, not a continuous check. Implementations should schedule the timeout at workspace creation and cancel it when the workspace reaches a terminal state or exits the timeout-applicable states (`active`, `blocked`, `conflicted`).

**Precedence summary.** When multiple events compete for the same workspace:

| Priority | Source | Effect |
|----------|--------|--------|
| 1 (highest) | Coordinator abort | Immediate `failed` |
| 2 | Runtime/protocol failure (budget, timeout) | Immediate `failed` |
| 3 | Agent signals | Processed in emission order |

Lower-priority events that arrive after a higher-priority event has transitioned the workspace to a terminal state are recorded in the trail but cannot trigger transitions.

## 8. Visibility and Authority

Visibility and authority are the two dimensions of a workspace's access control. Visibility governs what the workspace can read; authority governs what it can modify. They are set at creation (§4) and follow different mutability rules — visibility can be expanded, authority cannot.

**Visibility set.** The set of workspace IDs this workspace can read (§2, component 8). Populated at creation by the coordinator. The workspace can always see itself — own workspace is implicit in every visibility set. Beyond that, the coordinator decides what each workspace can see: peer workspaces, parent checkpoints, specific resources.

**Authority set.** The set of actions this workspace is permitted to perform (§2, component 9). Computed from the role and delegation capability at creation. Frozen permanently — authority never changes during the workspace's lifetime.

**The asymmetry is deliberate.** Granting read access to additional context mid-task is safe — the agent gains information but cannot affect other workspaces. Granting write access mid-task is not safe — it would enable scope creep and break the isolation guarantee that makes parallel execution deterministic.

**Dynamic visibility grants.** The coordinator may grant additional visibility to a workspace in `active` or `blocked` state:

```yaml
visibility_grant:
  workspace: workspace_id     # the workspace receiving additional visibility
  target: workspace_id        # the workspace being made visible
  reason: string              # why the grant is needed (recorded in trail)
```

The grant takes effect immediately and is recorded as a `visibility_granted` trail entry.

**Visibility grant rules:**

1. **Additive only.** Visibility can be granted but not revoked. Once a resource is visible, it remains visible for the workspace's lifetime. This prevents a class of race conditions where an agent reads a resource, acts on it, and then loses visibility — decisions would be based on context the agent can no longer verify.

2. **Scoped to the grantor's visibility.** The coordinator can only grant visibility to resources within its own visibility scope. For the root coordinator (visibility: all), this is trivially satisfied. For delegates, this enforces containment — a delegate cannot grant visibility it does not have (§5, invariant 3).

3. **No authority grants.** Write access cannot be expanded after creation. This is the firewall between visibility and authority.

4. **Only available in `active` or `blocked` state.** Visibility cannot be granted to workspaces in any other state — `idle` (initial setup, not dynamic grant), `migrating`, `suspended`, `integrating`, `conflicted`, `closed`, or `failed`. Dynamic grants apply to workspaces actively executing or waiting on a dependency.

**Use case.** A worker discovers mid-task that it needs context from a peer workspace. It sends a `query` envelope to the coordinator explaining what it needs. The coordinator evaluates the request and grants visibility to the relevant resource. The worker reads directly from the source — more efficient than copying large artifacts into an envelope.

## 9. Groups

A workspace group is a named set of workspaces that share a fate. Groups enable batch operations — aborting an entire pipeline, adjusting priority for a set of related workspaces, or tracking resource consumption across a directive's full lifecycle.

**Membership.** Groups are declared at workspace creation via the `group` field (§4). Multiple workspaces with the same group ID form a group. Membership is set at creation and cannot change. Groups are flat — no group hierarchy. A workspace belongs to at most one group. Workspaces without a `group` field are ungrouped and managed individually.

The group ID is a free-form string chosen by the coordinator (Identity spec §2, rule 2: opaque; Identity spec §3: uniqueness scope per-run). The protocol does not assign group IDs — the coordinator names groups according to its own conventions (e.g., `directive-042-pipeline`, `batch-3`).

**Batch operations.** The coordinator can perform batch operations on all workspaces in a group:

- **Batch abort.** Abort all non-terminal workspaces in a group. Each workspace transitions to `failed` with `reason: batch_abort`. The trail records a `batch_abort` entry listing the group ID and all affected workspace IDs.
- **Batch priority change.** Adjust the priority of all `active` or `blocked` workspaces in a group. Each affected workspace's priority is updated and a trail entry is recorded.

Batch operations are atomic in intent but sequential in execution — each workspace is individually transitioned. If a workspace reaches a terminal state between the batch request and its execution (e.g., it completes naturally), it is skipped. The trail records which workspaces were affected and which were skipped.

**Group resource accounting.** The trail supports querying resource consumption by group — aggregating `budget_warning`, `budget_exceeded`, and checkpoint `resource_usage` entries across all workspaces sharing a group ID. This gives the coordinator a directive-level view of cost, not just a workspace-level view.

## 10. Admission Control

The runtime may reject workspace creation when system capacity is exhausted. This is the protocol's backpressure mechanism — it prevents the coordinator from overloading the system by spawning more workspaces than the runtime can support.

**Rejection mechanism.** When the coordinator requests workspace creation and the runtime determines that capacity is insufficient:

1. The runtime records a `workspace_rejected` trail entry with `reason: capacity_exhausted` and any implementation-specific capacity details.
2. The runtime returns the rejection to the coordinator. The workspace never enters `idle` — it never exists.

The coordinator then adapts:
- **Queue** the directive for later dispatch when capacity frees up.
- **Reduce parallelism** — switch from parallel to sequential dispatch for remaining directives.
- **Escalate** to the human highway if the capacity constraint is unexpected.

**Capacity model.** The capacity model is grounded in the four physical resources (PROTOCOL §6.6). The runtime MUST evaluate physical capacity across all four dimensions before admitting a workspace:

| Physical resource | Capacity question |
|-------------------|-------------------|
| Compute | Can the scheduler serve another workspace's turns? |
| Memory | Can another context window be allocated? |
| Storage | Can another workspace's checkpoints and trail entries be stored? |
| Network | Can another workspace's envelopes and signals be delivered? |

A workspace is admitted only when all four physical dimensions have sufficient capacity. Thresholds for each dimension are implementation-defined; the dimensions themselves are protocol-defined.

**Admission and priority.** When the system is at capacity, the runtime may consider the requested workspace's priority:

- A `critical` workspace may be admitted even at capacity, if the runtime supports priority-based admission.
- A `normal` or `background` workspace may be rejected even below capacity, if the runtime reserves headroom for higher-band requests.

Priority-based admission is optional. The simplest conformant implementation rejects all creation requests when a hard capacity limit is reached, regardless of priority.

## 11. Agent Migration

The workspace-agent binding may change during execution. A model upgrade becomes available, an agent proves underpowered for the task, or cost optimization requires switching to a cheaper model. Migration replaces the agent while preserving the workspace — all nine components survive intact.

**The `migrating` state.** A workspace in `active` or `blocked` may transition to `migrating` when the coordinator initiates a migration. During `migrating`, the workspace is frozen — no new checkpoints, no new envelopes processed, no signals emitted by the agent. The runtime serializes the workspace's live state, binds a new agent, and transitions the workspace back to its pre-migration state (`active` or `blocked`).

```
active ──→ migrating ──→ active
blocked ──→ migrating ──→ blocked
                     └──→ failed  (migration error)
```

**Initiation.** Only the coordinator (or a delegate for its subtree) can initiate migration. The agent cannot migrate itself. Migration is requested through a protocol operation:

```yaml
workspace_migrate:
  workspace: workspace_id     # the workspace to migrate
  new_agent: agent_ref        # the replacement agent (implementation-defined)
  reason: string              # why migration is needed (recorded in trail)
```

**What happens during migration:**

1. The workspace transitions to `migrating`. A `migration_started` trail entry is recorded with the old agent, the new agent, and the reason.
2. The runtime snapshots the nine components. Immutable components (directive, context, visibility set, authority set) require no serialization — they are already fixed. Live components (inbox, working memory, checkpoint register, resource meter, local trail) are captured at the moment of the snapshot.
3. The old agent is unbound. It receives no further envelopes and can emit no further signals.
4. The new agent is bound and given access to the workspace's full state — the same directive, the same working memory, the same checkpoint history, the same inbox.
5. The workspace transitions back to its pre-migration state (`active` or `blocked`). A `migration_completed` trail entry is recorded.

**The new agent picks up where the old one left off.** It reads the directive to understand the task, reads working memory to see in-progress artifacts, reads the checkpoint register to see what has been produced, and reads the local trail to understand the workspace's history — including the migration event itself. The inbox may contain unprocessed envelopes; the new agent processes them.

**Migration is atomic.** It either completes fully or the workspace transitions to `failed` with `reason: migration_error`. There is no partial migration — the workspace never exists in a state where neither agent is bound. If the new agent cannot be bound (unavailable, incompatible, capacity exhausted), the migration fails and the workspace is failed. The coordinator may then create a new workspace using the replacement workaround.

**Constraints:**

- Migration can only be initiated from `active` or `blocked` state. Workspaces in `idle`, `integrating`, `conflicted`, or terminal states cannot be migrated.
- The workspace's role, visibility set, and authority set do not change during migration. The new agent inherits the same permissions as the old one.
- The resource meter is continuous across migration — consumption by the old agent counts toward the same budget. Migration does not reset budgets.
- The trail is continuous across migration — the `migration_started` and `migration_completed` entries are part of the same local trail. No gap, no separate trail.
- Multiple migrations of the same workspace are permitted. Each is recorded in the trail.

**Trail events:**

| Event | When | Body |
|-------|------|------|
| `migration_started` | Workspace enters `migrating` | `old_agent`, `new_agent`, `reason` |
| `migration_completed` | Workspace exits `migrating` | `old_agent`, `new_agent`, `duration` |
| `migration_failed` | Migration cannot complete | `old_agent`, `new_agent`, `reason`, `error` |

## 12. System Lifecycle

The protocol defines individual workspace lifecycles (§3) but the system as a whole also has a lifecycle: it is initialized, it runs, and it shuts down. This section specifies the bookends.

**Initialization.** A WACP system begins with a bootstrap sequence. The following steps occur in order:

1. **Runtime starts.** The runtime initializes the clock (Clock spec §3), the trail store, the transport layer, and the security subsystem (PROTOCOL §11). No protocol operations are possible until the runtime is ready.

2. **Root workspace is created.** The runtime creates the root workspace and assigns it the `coordinator` role (PROTOCOL §5.1). This is the only workspace not created by a coordinator — it is created by the runtime itself. The root workspace has:
   - No parent (`parent: null`)
   - Full visibility (can see all descendants and the global trail)
   - Full authority (can create workspaces, send directives, initiate integration)
   - No budget (the coordinator is not resource-bounded by the protocol; the runtime may impose external limits)
   - No timeout (the coordinator lives for the duration of the run)

3. **Coordinator agent is bound.** The runtime binds an agent to the root workspace. The binding mechanism is implementation-defined. The agent emits `ready`.

4. **Workflow is loaded.** The coordinator receives a workflow definition — either as its first envelope or through an implementation-defined mechanism. The workflow defines the stages, roles, dispatch patterns, and highway configuration for the run.

5. **The coordinator is now active.** It may create child workspaces, send directives, and begin orchestrating work. The global trail records all initialization events.

The trail's first entry MUST be the `workspace_created` event for the root workspace. This anchors the trail integrity chain (PROTOCOL §11.5) and establishes the zero point for all timestamps.

**Normal shutdown** proceeds when the coordinator determines the run is complete:

1. All child workspaces reach terminal states — every workspace is either `closed` or `failed`. No workspace is in `active`, `blocked`, `migrating`, `integrating`, or `conflicted` state.
2. The root workspace transitions to `closed`. No further protocol operations are possible.
3. The runtime records the final trail entry — the root workspace's state change to `closed`. The integrity chain is complete.
4. The runtime shuts down. Transport, storage, and clock are released. The trail is durable and available for post-run analysis.

**Forced shutdown** occurs when the runtime must stop before the coordinator has completed:

1. The runtime transitions all non-terminal workspaces to `failed` with `reason: system_shutdown`.
2. The root workspace transitions to `failed` (not `closed` — the run did not complete normally).
3. A `system_degraded` trail event (PROTOCOL §10.3) is recorded with `reason: forced_shutdown`.
4. The runtime shuts down. The trail is recoverable per the recovery spec.

**Abandoned runs** occur when the runtime crashes without executing a shutdown sequence. Recovery reconstructs state from the trail (PROTOCOL §10.2). The run may be resumed or declared failed — that is the runtime's decision.

**Three invariants:**

1. **The root workspace is the first and last.** Its creation is the first trail entry; its terminal state change is the last. No workspace may exist before the root or after the root closes.
2. **No orphan workspaces.** Every workspace except the root has a parent. If the root fails, all descendants fail (§5, invariant 1).
3. **Shutdown is recorded.** Whether normal, forced, or recovered, the trail always contains evidence of how the system ended.

## 13. Trail Events

The workspace produces the following trail events. All events follow the standard trail entry schema (PROTOCOL §9.1) with workspace-specific body payloads.

**Lifecycle events:**

| Event | Trigger | Body |
|-------|---------|------|
| `workspace_created` | Workspace enters `idle` | `workspace_id`, `role`, `parent`, `delegate`, `originator`, `owner`, `visibility_set`, `authority_set`, `timeout`, `budget`, `priority`, `group` |
| `workspace_state_changed` | Any state transition | `workspace_id`, `from_state`, `to_state`, `trigger`, `initiator` |
| `workspace_rejected` | Runtime rejects creation | `reason`, `capacity_details` (implementation-defined) |

**Resource events:**

| Event | Trigger | Body |
|-------|---------|------|
| `budget_warning` | Consumption reaches warning threshold | `workspace_id`, `budget_dimension`, `consumed`, `limit`, `threshold_percent` |
| `budget_exceeded` | Consumption reaches hard limit | `workspace_id`, `budget_dimension`, `consumed`, `limit` |
| `budget_modified` | Coordinator increases budget | `workspace_id`, `budget_dimension`, `old_limit`, `new_limit` |
| `liveness_warning` | No trail activity within liveness interval | `workspace_id`, `interval`, `last_activity_timestamp` |
| `priority_changed` | Coordinator changes workspace priority | `workspace_id`, `old_priority`, `new_priority` |

**Ownership events:**

| Event | Trigger | Body |
|-------|---------|------|
| `workspace_ownership_transferred` | Coordinator transfers workspace ownership (User spec §4) | `workspace_id`, `from_user`, `to_user`, `reason`, `transferred_by` |
| `workspace_reparented` | Cross-ownership abort reparents orphaned workspace (User spec §4) | `workspace_id`, `old_parent`, `new_parent`, `reason` |

**Visibility events:**

| Event | Trigger | Body |
|-------|---------|------|
| `visibility_granted` | Coordinator grants dynamic visibility | `workspace_id`, `target`, `reason` |

**Group events:**

| Event | Trigger | Body |
|-------|---------|------|
| `batch_abort` | Coordinator aborts all workspaces in a group | `group_id`, `affected_workspaces`, `skipped_workspaces` |
| `batch_priority_changed` | Coordinator changes priority for a group | `group_id`, `old_priority`, `new_priority`, `affected_workspaces`, `skipped_workspaces` |

**Migration events:**

| Event | Trigger | Body |
|-------|---------|------|
| `migration_started` | Workspace enters `migrating` | `workspace_id`, `old_agent`, `new_agent`, `reason` |
| `migration_completed` | Workspace exits `migrating` successfully | `workspace_id`, `old_agent`, `new_agent`, `duration` |
| `migration_failed` | Migration cannot complete | `workspace_id`, `old_agent`, `new_agent`, `reason`, `error` |

**Suspension events:**

| Event | Trigger | Body |
|-------|---------|------|
| `suspension_started` | Workspace enters `suspended` | `workspace_id`, `pre_suspension_state`, `reason` |
| `suspension_resumed` | Workspace exits `suspended` | `workspace_id`, `resumed_to_state`, `duration` |

**Graceful termination events:**

| Event | Trigger | Body |
|-------|---------|------|
| `graceful_termination_initiated` | Coordinator sends graceful termination request | `workspace_id`, `grace_period`, `reason` |
| `graceful_termination_expired` | Grace period expires without `complete` | `workspace_id`, `grace_period`, `partial_checkpoint` (if any) |

**Integration events:**

| Event | Trigger | Body |
|-------|---------|------|
| `conflict_detected` | Workspace enters `conflicted` | `workspace_id`, `conflict_type`, `resources` (affected), `description` |
| `conflict_resolved` | Workspace exits `conflicted` | `workspace_id`, `conflict_type`, `resolution_strategy`, `resolution` (description), `outcome` (`closed` or `failed`) |

**No new event types beyond what the protocol already defines** — workspace events use the standard trail entry schema with workspace-specific bodies. The `workspace_state_changed` event is the workhorse: every lifecycle transition produces one, creating a complete state history that recovery can replay.

## 14. Conformance Requirements

| Requirement | Level | Description |
|-------------|-------|-------------|
| Nine-component internal model | Core | Runtime MUST implement all nine workspace components as defined in §2. |
| Nine workspace states | Core | Runtime MUST implement all nine states (`idle`, `active`, `blocked`, `migrating`, `suspended`, `integrating`, `conflicted`, `closed`, `failed`) with the transition rules defined in §3. |
| Coordinator-only creation | Core | Only the coordinator (or a delegate with the delegation capability) MAY create workspaces. |
| Terminal state immutability | Core | Workspaces in `closed` or `failed` state MUST be immutable. No further transitions, envelopes, or checkpoints. |
| No workspace destruction | Core | Workspaces MUST NOT be destroyed. Terminal workspaces remain queryable indefinitely. |
| Workspace tree invariants | Core | Runtime MUST enforce: child cannot outlive parent, visibility flows downward, containment flows upward. |
| Timeout enforcement | Core | Runtime MUST transition workspaces to `failed` when timeout expires. |
| Concurrency invariants | Core | Runtime MUST enforce all six concurrency invariants (§7): single-writer serialization, abort precedence, external failure precedence, signal emission ordering, trail monotonicity, timeout race resolution. |
| Authority frozen at creation | Core | Authority set MUST NOT change after the workspace leaves `idle` state. |
| Budget enforcement | Standard | Runtime MUST track resource consumption and enforce hard limits independently of agent cooperation. |
| Budget warning before failure | Standard | Runtime MUST produce a `budget_warning` trail entry before a `budget_exceeded` failure. |
| Budget modification | Standard | Coordinator MAY increase budgets additively in `idle`, `active`, or `blocked` states. |
| Liveness monitoring | Standard | Runtime MUST support optional `liveness_interval` with advisory warnings to the coordinator. |
| Dynamic visibility grants | Standard | Runtime MUST support additive visibility grants to workspaces in `active` or `blocked` state. |
| Workspace groups | Standard | Runtime MUST support group membership, batch abort, and batch priority change. |
| Admission control | Standard | Runtime MUST be able to reject workspace creation and record rejections in the trail. |
| Agent migration | Standard | Runtime MUST support the `migrating` state with atomic agent replacement preserving all nine components. |
| Workspace suspension | Standard | Runtime MUST support the `suspended` state with coordinator-initiated suspension and resumption. Resource meter MUST pause during suspension. |
| Graceful termination | Standard | Runtime MUST support graceful termination — delivering the coordinator's feedback envelope with `intent: graceful_termination`, enforcing the grace period, and transitioning to `failed` with `reason: graceful_termination_expired` if the agent does not emit `complete` within the grace period. |
| Workspace owner field | Core | Every workspace MUST carry an `owner: user_id` field set at creation (User spec §4). |
| Ownership-boundary abort | Core | Abort MUST cascade within ownership boundaries and reparent cross-owned children to the coordinator (User spec §4). |
| Originator propagation | Standard | If a parent workspace has an `originator`, child workspaces MUST inherit it unconditionally (User spec §5). |
| User-state awareness | Standard | The coordinator SHOULD evaluate workspaces in a user's originator tree when that user's state changes (User spec §3.5). The response (suspend, abort, transfer, no action) is a coordinator policy decision. |
| Priority-based admission | Full | Runtime MAY implement priority-based admission control. |
| Group resource accounting | Full | Trail SHOULD support querying resource consumption aggregated by group. |

## 15. Implementation Notes

These notes are non-normative. They capture practical guidance for implementers.

**Workspace as a directory.** The simplest implementation maps each workspace to a filesystem directory. Working memory is the directory's contents. The directive is a file. The inbox is a subdirectory of pending envelopes. The checkpoint register is a manifest file listing checkpoint IDs and paths. The local trail is a log file. The visibility and authority sets are metadata files. This approach is transparent (humans can inspect workspace state with standard tools), portable, and trivially serializable for migration.

**State machine as an enum + transition function.** The nine states map naturally to an enum. The transition table (§3) becomes a function `(current_state, event) → new_state | rejection`. Precompute the valid transitions at startup and reject invalid ones with a single lookup. The `migrating` and `suspended` states require storing the pre-suspension state so the workspace can return to it on success or resumption.

**Resource meter implementation.** Token counting depends on the LLM provider's API — most return token counts in their response. The runtime accumulates these. Compute timeout requires a wall-clock timer that runs only when the workspace is in `active` state — pause it on `blocked`, `migrating`, `suspended`, and resume on return to `active`. Cost is typically derived from tokens × price-per-token, but the protocol leaves the unit implementation-defined.

**Suspension implementation.** Suspension requires storing the pre-suspension state (as with migration). The key implementation difference from `blocked` is the resource meter: during `blocked`, wall time continues to accrue (the agent chose to block, it is still occupying resources); during `suspended`, the meter pauses entirely (the coordinator chose to suspend, the agent is not consuming resources). Implementations may release some resources during suspension (e.g., GPU memory) while preserving the nine-component state — this is an optimization, not a protocol requirement.

**Graceful termination implementation.** When the coordinator sends the graceful termination feedback envelope, the runtime starts a grace period timer. The agent processes the envelope like any other feedback — it reads `intent: graceful_termination` and the `grace_period` and decides what to save. Implementations should treat this as a scheduled timeout: if `complete` is emitted before expiry, cancel the timer; if the timer fires first, transition to `failed`. The agent may produce multiple provisional checkpoints during the grace period — the last one before terminal transition is preserved.

**Concurrency with a per-workspace lock.** The simplest way to satisfy invariant 1 (single-writer serialization) is a mutex per workspace. All state transitions acquire the workspace's lock. Abort precedence (invariant 2) is implemented by checking for pending aborts before processing agent signals. This is sufficient for single-runtime deployments. Multi-runtime deployments may need distributed locking or serialization through a shared event log.

**Migration snapshot.** For filesystem-based workspaces, migration is a directory copy (or move). For in-memory workspaces, it is a serialization of the workspace's data structures. The key implementation decision is whether the snapshot is taken synchronously (workspace is frozen, snapshot completes, new agent binds) or involves an async handoff. The protocol requires atomicity — the simplest path is synchronous.

**Group tracking.** Maintain a map from group ID to set of workspace IDs. Batch operations iterate the set. Group resource accounting queries the trail store with a group ID filter — this is a read-time aggregation, not a separate data structure.

**Budget warning threshold.** The protocol recommends 80% but does not require it. Implementations should make the threshold configurable per workspace or globally. Some workloads benefit from earlier warnings (70%) to give agents more time to wrap up; others benefit from later warnings (90%) to reduce noise.

## 16. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §4.6 | §2, §4, §6 | Task primitive — task-workspace binding, `task_ref`, `resource_estimate`, dependencies |
| §5.1 | §5, §12 | Assignment rules — coordinator role assigned by runtime at initialization |
| §6.1–§6.10 | §1–§15 | Workspace lifecycle (defines this spec) — state machine, creation, tree, resources, concurrency, visibility, migration, graceful termination |
| §6.4 | §4 | Creation — budget, priority, visibility, authority |
| §6.5 | §5 | Workspace tree — root failure cascades to all workspaces |
| §6.6 | §2, §6, §10 | Resource management — four physical dimensions, budget lifecycle, timeouts |
| §6.8 | §2, §8 | Visibility and authority — dynamic visibility grants, additive only |
| §7.1 | §2, §6 | Checkpoint anatomy — `resource_usage` field |
| §7.8 | §3 | Conflict resolution strategies |
| §9.1 | §13 | Trail entry schema — workspace event bodies |
| §10.2 | §3, §12 | Recovery model — nine-component reconstruction from trail |
| §10.3 | §12 | Partial failures — `system_degraded` trail event |
| §11 | §12 | Security model — system initialization |
| §11.5 | §12 | Trail integrity — integrity chain anchored by root workspace |

### Constituent Specs

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Clock spec | §3 | §2, §7, §12 | Clock invariants — monotonic timestamps within workspace, partial order across workspaces, initialization |
| Identity spec | §2 | §2, §4, §5, §9 | Identifier rules — runtime-assigned (rule 1), opaque (rule 2), referenceable (rule 5) |
| Roles spec | §3, §4, §6 | §2, §4, §5, §6 | Base roles, delegation capability, permission matrix — authority set, delegation grants, budget inheritance |
| User spec | §3, §4, §5 | §3, §4, §5, §13, §14 | User lifecycle — user-as-root-of-originator-tree, state mirroring (§3); ownership model, originator propagation, ownership transfer, cross-ownership reparenting (§4, §5) |
| Recovery spec | — | §3, §12 | Recovery procedure — workspace state reconstruction |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
