# WACP — Workspace Agent Coordination Protocol

```yaml
id: wacp-protocol
type: protocol
status: complete
created: 2026-02-23
revised: 2026-02-24
authors:
  - Akil Abderrahim
  - Claude Opus 4.6
tags: [coordination, multi-agent, protocol]
```

## Table of Contents

1. Scope and Purpose
2. Vocabulary
3. Design Principles
4. Core Primitives
5. Roles and Permissions
6. Workspace Lifecycle
7. Integration and Checkpoints
8. Human Highway
9. Trail
10. Recovery and Fault Handling
11. Security Model
12. Conformance and Terminology

---

## 1. Scope and Purpose

WACP (Workspace Agent Coordination Protocol) defines the rules by which autonomous agents coordinate work. It is a coordination protocol — it specifies what agents say, how they communicate, and the structures they operate within. It does not specify how the underlying system schedules, allocates, or manages resources; that is the domain of the runtime layer.

The protocol answers five questions:

1. **Where does an agent work?** — Workspaces: isolated, bounded contexts.
2. **How do agents communicate?** — Envelopes and signals: structured messages and typed notifications.
3. **How is progress recorded?** — Checkpoints: immutable snapshots of work.
4. **How is work organized?** — Tasks: units of work forming dependency graphs.
5. **What happened?** — The trail: an append-only audit log.

WACP is deliberately silent on execution. It does not define how inference is scheduled, how context is managed, or how physical resources are consumed. It defines the coordination surface — the contracts between agents, between agents and the coordinator, and between all parties and the audit record. The runtime implements these contracts; the protocol only specifies them.

---

## 2. Vocabulary

The following terms have precise meanings throughout this specification. When a term appears in body text, it carries exactly the definition given here.

**Workspace.** The unit of isolation. A bounded context assigned to exactly one agent.

**Agent.** An autonomous entity operating within a workspace. Makes decisions within protocol constraints. The coordinator is also an agent.

**Coordinator.** The agent that orchestrates work. Creates workspaces, dispatches directives, evaluates results. There is exactly one coordinator per protocol instance.

**Envelope.** The unit of communication. A structured message passed between workspaces.

**Signal.** The unit of notification. A lightweight, typed event that an agent emits to declare a state change.

**Checkpoint.** The unit of progress. An immutable snapshot of an agent's work within a workspace.

**Task.** The unit of work. A discrete assignment within a dependency graph.

**Trail.** The append-only audit log. The single source of truth for everything that has occurred.

**Role.** What an agent is permitted to do within its workspace. Assigned at creation, immutable thereafter.

**Runtime.** The software that implements this protocol. Enforces rules, delivers messages, manages state. The protocol specifies; the runtime enforces.

**Topology.** The structural relationships between protocol objects. Workspaces form trees, tasks form graphs, visibility and port rights form directed graphs, envelopes form conversation threads, the trail forms a linear sequence. The protocol's topology category defines these structures formally — their properties, operations, and invariants.

**Coordinator model.** The coordinator is the system — categorically different from users. It is not the most privileged user; it is a different kind of entity. Its root workspace carries `originator: "system"`. Human injections create causal boundaries within the tree: the injected workspace carries `originator: <user_id>`. The `originator` field on every workspace is the discriminator — `user_id` for human-rooted subtrees, `"system"` for system-rooted subtrees.

**Usage discipline:** "The protocol requires" (specification). "The runtime enforces" (implementation). "The coordinator decides" (orchestration). "The agent emits" (workspace-level action).

---

## 3. Design Principles

Every design decision in WACP traces back to at least one of these principles.

### 3.1 Messages Over Mutations

Agents never modify shared state directly. All coordination happens through explicit, typed messages. If an agent needs something to change, it sends a message declaring intent — it does not reach in and change it. This eliminates an entire class of race conditions and makes every interaction auditable.

### 3.2 Roles Are Structural, Not Suggested

An agent's role is enforced by the runtime, not by a system prompt that can be ignored. The protocol defines what each role can send, receive, and act on. Permissions are walls, not guidelines. Roles are extensible through single-level inheritance: a domain-specific role extends a base role and applies overrides.

### 3.3 Explicit Lifecycle, No Inference

No agent should have to guess whether another agent is working, waiting, done, or stuck. Every state transition is declared. If the runtime does not know an agent's state, the agent has not declared it — and that itself is a diagnosable fault.

### 3.4 Context Is Scoped, Not Shared

Agents do not share a global context. Each workspace has a defined boundary of visibility. An agent sees exactly what its role and workspace grant — nothing else unless explicitly permitted. The coordinator sees across workspaces but acts only through messages. Least privilege, applied to attention.

### 3.5 History Is First-Class

Every message, checkpoint, and signal is persisted and queryable. The trail is not a log to be parsed — it is a structured record with typed entries. Post-mortem analysis, replay, and comparison between runs should be trivial, not archaeological.

### 3.6 Protocol Over Tooling

WACP defines a protocol, not an application. The primitives should be implementable over files, over HTTP, over pipes, over anything. The coordination model is independent of transport.

### 3.7 Human Access Is Architectural

The protocol does not assume humans are absent or present. It guarantees that a human *can* observe any communication, approve any transition, inject any message, and receive any escalation — without the protocol needing to be stopped or reconfigured. Autonomy is a spectrum configured per workflow, not a binary switch.

### 3.8 Ordering Requires a Clock

Every timestamp in the protocol is produced by a clock. The clock is a foundational dependency — trail integrity, signal ordering, timeouts, concurrency resolution, and replay all depend on it. Without a well-defined clock, none of these guarantees hold.

---

## 4. Core Primitives

The core primitives form the foundation of WACP. These are the protocol's building blocks: every coordination operation is composed from them.

### 4.1 Workspace

The unit of isolation. A workspace is a bounded context assigned to exactly one agent, containing everything that agent can see and act on.

A workspace has:
- **id** — unique identifier within the workspace tree
- **role** — the role operating within it (§5)
- **parent** — the workspace that spawned it (null for root)
- **status** — lifecycle state (§6)
- **owner** — user_id of the human on whose behalf this workspace exists
- **originator** — required; `user_id` of the human who caused this workspace's creation, or `"system"` for system-initiated workspaces. Propagates unconditionally through subtrees (§4.8). At injection points, the injecting human's `user_id` overrides inheritance.
- **visibility** — list of resources this workspace can read
- **authority** — list of resources this workspace can modify
- **priority** — workspace priority band
- **mounts** — list of tools available to this workspace

Workspaces form a tree. The root workspace belongs to the coordinator. Child workspaces are created for each delegated unit of work. A workspace is created for a task, closed when the task completes or fails, and never reopened. If the work needs revision, a new workspace is created. Visibility is set at creation and may be expanded by the coordinator; authority is set at creation and frozen.

### 4.2 Envelope

The unit of communication. An envelope wraps any message passed between agents. Nothing travels between workspaces without one.

An envelope has:
- **id** — unique identifier
- **from** — source workspace id
- **to** — target workspace id
- **type** — extensible; base types: directive, feedback, query
- **payload** — the message content
- **in_reply_to** — envelope id this responds to (null if unsolicited)
- **timestamp** — creation time
- **priority** — normal | urgent | blocking
- **origin** — `agent` (default) or `human` (injected via the highway); system-assigned, immutable

Delivery requires the sender to hold a valid send right to the receiver's workspace. Send rights are created by the coordinator at workspace creation, transferable between workspaces, and revocable by the coordinator at any time (§5.6).

**Lifecycle.** An envelope moves through five states: `created`, `validated`, `delivered`, `acknowledged`, `rejected`. Each transition is recorded as a trail entry.

**Delivery guarantees.** All envelopes are addressed (no broadcasts), validated before delivery, and acknowledged automatically. Envelopes are durable — once accepted by the runtime, they are written to the trail before delivery. The protocol guarantees at-least-once delivery with idempotent processing. Maximum redelivery attempts: 3 (protocol constant). An agent processes each envelope at most once regardless of redelivery.

**Signal idempotency.** Signals are idempotent with respect to state transitions. A duplicate signal that would trigger a transition the workspace has already made is recorded in the trail but does not alter state.

### 4.3 Signal

The unit of notification. A signal is a lightweight, typed event that an agent emits to declare a state change. Signals carry status, not content.

Signal types (closed set):
- **ready** — "I exist and am waiting for input"
- **started** — "I have begun work"
- **blocked** — "I cannot continue" (carries a reason)
- **checkpoint** — "I have produced a new checkpoint"
- **complete** — "My work is finished"
- **failed** — "I have encountered an unrecoverable error"
- **integrate** — "I am beginning to merge this workspace's work into the parent" (coordinator-emitted; marks the start of integration)
- **acknowledged** — "I have received your message"
- **escalation** — "I need human input to proceed"
- **suspend** — "This workspace is being paused" (coordinator-emitted)
- **migrate** — "This workspace's agent is being replaced" (coordinator-emitted)

Signals propagate upward through the workspace tree — from child to parent, never laterally and never downward.

### 4.4 Checkpoint

The unit of progress. A checkpoint is a structured, immutable snapshot that an agent produces to declare: "here is what I have done, and here is the state of my work."

A checkpoint has:
- **id** — unique within its workspace
- **workspace** — the workspace that produced it
- **type** — extensible; base types: artifact, observation
- **payload** — the content (code, prose, structured data, files)
- **intent** — free-text explanation of why this checkpoint exists
- **parent_checkpoint** — the checkpoint this builds on, if any
- **status** — provisional | final
- **confidence** — high | medium | low

Checkpoints are immutable once created. An agent cannot alter a past checkpoint — it can only create a new one that supersedes it. Artifacts within checkpoints are independently addressable via artifact_id.

### 4.5 Trail

The unit of history. The trail is the ordered, append-only record of everything that has occurred: every envelope sent, every signal emitted, every checkpoint created, every state transition declared.

A trail has:
- **scope** — workspace-level (local) or system-wide (global)
- **entries** — ordered list of typed events
- **queryable** — filterable by workspace, role, event type, time range

Each workspace maintains its own local trail. The root workspace maintains the global trail. No entry can be deleted or modified.

The trail serves three purposes:
1. **Audit** — what happened, when, and by whom
2. **Replay** — reconstruct the sequence of events
3. **Comparison** — evaluate different workflow configurations against the same directive set

### 4.6 Task

The unit of work. A task is a structured, coordinator-defined unit with explicit dependencies, lifecycle state, and assignment tracking.

A task has:
- **id** — unique identifier, globally unique across runs
- **name** — human-readable label
- **description** — what needs to be done
- **depends_on** — list of task IDs that must complete first (DAG edges)
- **parent_task** — the task this was decomposed from (null for root tasks)
- **status** — draft | pending | assigned | in_progress | completed | failed | integrated | cancelled
- **workspace_ref** — the workspace currently executing this task
- **workspace_history** — all workspaces that have attempted this task, in order
- **checkpoint_ref** — the final checkpoint from the completing workspace

Tasks form directed acyclic graphs (DAGs). The coordinator constructs the graph; the protocol maintains it. The task-workspace relationship is one-to-many across time — if a workspace fails, the coordinator may retry by creating a new workspace for the same task.

### 4.7 Identity

Every protocol object carries an identifier. Six rules govern all identifiers:

1. **Runtime-assigned.** All identifiers are generated by the runtime, never by agents.
2. **Opaque.** No prescribed format — UUIDs, ULIDs, integers, URIs are all valid. Semantic relationships are expressed through explicit fields, not ID structure.
3. **Unique within scope.** Each identifier type has a defined uniqueness scope (global or per-run).
4. **Immutable.** Once assigned, an identifier never changes.
5. **Referenceable.** Any identifier can appear in another object's reference fields.
6. **Non-recyclable.** An identifier is never reused after the object reaches a terminal state.

Agents address messages by workspace_id, not by role or agent identity. There is no broadcast — the coordinator sends individual envelopes to each target.

### 4.8 User Identity

The protocol defines three principals: agents (who do the work), the coordinator (who orchestrates), and users (on whose behalf the work exists).

user_id is a protocol-level identifier. It follows the six identifier rules with one adaptation: user_id is assigned at the authentication boundary, not generated by the runtime. The protocol carries it; the deployment layer assigns it.

**Ownership.** Every workspace has an `owner: user_id` — the human on whose behalf the work exists. Ownership determines who receives escalations by default, who is accountable for outcomes, and what "my workspaces" means.

Ownership rules:
1. **Explicit assignment.** The coordinator specifies `owner` at workspace creation. This is the normal case.
2. **Inheritance from parent.** If no owner is specified, the workspace inherits the owner of its parent. This is the default for delegated subtrees.
3. **Tree ownership.** A user transitively owns the entire subtree beneath their workspace — every workspace created beneath it, at every depth.
4. **Explicit override.** The coordinator may set a different owner on a child workspace, creating an ownership boundary within the tree. This supports handoff scenarios where a subtask belongs to a different user's domain.
5. **Ownership transfer.** The coordinator may transfer ownership of a workspace from one user to another. Transfer applies to a single workspace and does not cascade to children. Transfer does not rewrite history — trail entries before the transfer retain the original owner's attribution. Transfer is transparent to agents. Produces a `workspace_ownership_transferred` trail event.

**Originator.** The `originator` field answers "which human caused this?" — distinct from `owner` which answers "who is accountable?" The field is required on every workspace. Its type is `user_id | "system"`.

Originator rules:
1. **Required.** Every workspace MUST carry an originator. There is no workspace without a declared causal origin.
2. **The root workspace carries `originator: "system"`.** The coordinator is the system — categorically different from users. Its root workspace is system-originated. There is no deployment in which the coordinator is a user.
3. **Autonomous actions set originator to `"system"`.** When the coordinator creates workspaces based on its own logic (recovery, bootstrap, maintenance — not in response to a human injection), the workspace carries `originator: "system"`. The system is an explicit causal origin, not an absence.
4. **Human injection sets originator to `user_id`.** When a human injects a directive through the highway, the resulting workspace carries `originator: user_id` of the injecting human. **This overrides inheritance at the injection point** — the injected workspace has a different originator than its structural parent (the coordinator). This is the only mechanism that creates causal boundaries.
5. **Unconditional inheritance.** When a workspace creates child workspaces (via delegation, not injection), the children inherit the originator automatically. Unlike owner, originator cannot be overridden at creation. A human-originated workspace's children are always human-originated. A system-originated workspace's children are always system-originated.
6. **Immutable.** Once set, originator never changes. Ownership transfer does not change it. Reparenting does not change it. The causal chain is a permanent record.

**Rule precedence.** Rule 4 (injection) overrides rule 5 (inheritance) at injection points. Rule 5 applies everywhere else. This is how human causal origins enter the tree — injection is the single exception to unconditional inheritance.

**The coordinator model.** The coordinator is the system. It has no `user_id`. Its trail events use `actor: "system"`. No user can become the coordinator. The boundary between "user action" and "coordinator action" is absolute: users request through the highway, the coordinator decides. The workspace tree always contains a system-originated sub-forest (rooted at the coordinator) and zero or more human-originated subtrees (one per injection point). The originator on each workspace declares whether the system or a human is the causal origin.

"Show me everything user X caused" queries originator for a specific `user_id`. "Show me everything the system initiated" queries originator for `"system"`. "Show me everything user Y is responsible for" queries owner. All are first-class trail queries.

---

## 5. Roles and Permissions

Every agent operates under a role. A role is a named set of capabilities bound to a workspace at creation. It governs what the agent can send, receive, emit, create, and access. These five dimensions are exhaustive — every protocol action falls into one of them.

### 5.1 Assignment Rules

A role is assigned to a workspace at creation by the coordinator. It cannot change during the workspace's lifetime — if a different role is needed, a new workspace is created. One workspace, one role. The coordinator role is unique: exactly one workspace holds it, assigned by the runtime at initialization.

The runtime evaluates permissions at the moment of action. If an action is not explicitly permitted, it is denied. There is no default-allow. Every denial is recorded in the trail.

### 5.2 Base Roles

The protocol defines three base roles representing three fundamental capability levels.

**Coordinator.** The root agent. Exactly one per run. Decomposes tasks, delegates work, evaluates results, integrates outcomes. Sees across all workspaces but modifies none directly — orchestrates through envelopes and protocol operations.

- Send: `directive` and `feedback` to workers
- Receive: `query` from workers; all signals from child workspaces
- Emit: `ready`, `started`, `failed`, `integrate`, `acknowledged`
- Create: no checkpoints
- Access: read all workspaces and the global trail; modify none directly

**Worker.** The producing agent. Receives a directive, works within its workspace, declares progress through checkpoints. Deliberately isolated — knows its task and nothing else unless granted additional visibility.

- Send: `query` to coordinator
- Receive: `directive` and `feedback` from coordinator
- Emit: `ready`, `started`, `blocked`, `checkpoint`, `complete`, `failed`, `escalation`
- Create: `artifact` checkpoints
- Access: read own workspace, own directive, own local trail; modify own workspace files

**Observer.** The monitoring agent. Read-only access to designated workspaces and the trail. Produces no artifacts but may record observations. The foundation for dashboards, metrics, and human-facing interfaces.

- Send: none
- Receive: none
- Emit: `ready`, `started`, `complete`, `failed`, `escalation`
- Create: `observation` checkpoints
- Access: read designated workspaces and trail (configurable scope); modify none

### 5.3 Role Inheritance

Applications define specialized roles through single-level inheritance. A derived role extends exactly one base role, then applies overrides: `remove` capabilities, then `add` capabilities. Resolution order is fixed and deterministic.

Constraints: a derived role cannot itself be extended (no deep hierarchies). A derived role cannot add capabilities that exceed its base role's ceiling — no privilege escalation. All derived roles must be registered in the taxonomy before use.

### 5.4 Delegation

Delegation is a capability, not a role. The coordinator grants `delegate: true` at workspace creation, frozen like authority. A delegate can create child workspaces within its subtree, assign roles to them, send directives, receive queries, and integrate their checkpoints. A delegate is a local coordinator for its subtree.

A delegate cannot: create workspaces outside its subtree, exceed its own budget (children draw from the delegate's remaining budget), grant visibility or authority it does not hold, or grant delegation to its own children without the root coordinator's approval.

### 5.5 Permission Matrix

The base envelope permissions:

| Sender → Envelope Type | Eligible Receivers |
|-------------------------|--------------------|
| Coordinator → `directive` | Worker |
| Coordinator → `feedback` | Worker |
| Worker → `query` | Coordinator |

Delegation extends this matrix at runtime — a delegate gains coordinator-like envelope permissions scoped to its subtree. Derived roles extend it through the taxonomy. The matrix is the runtime's lookup table: given a role and an action, it returns allow or deny.

### 5.6 Port Rights

Port rights govern workspace-to-workspace communication as transferable capabilities. A workspace cannot send an envelope without holding a valid right to the target.

Three right types:

- **Send right.** Allows the holder to send envelopes to a specific workspace. Created by the coordinator at workspace creation based on the permission matrix (§5.5). Transferable: a workspace may include a send right in an envelope, granting the receiver the ability to communicate with a third workspace.
- **Receive right.** Allows the holder to receive envelopes on its own inbox. Every workspace holds the receive right to its own inbox. Receive rights are not transferable — a workspace cannot delegate the ability to read its mail.
- **Send-once right.** A send right consumed upon use. After one envelope is sent, the right is destroyed. Used for one-time callbacks: "reply to me at this address, once."

Rights are created by the coordinator, active while held, and destroyed when consumed (send-once) or revoked. The coordinator can revoke any send right at any time — this is how the runtime enforces communication boundaries. Revocation is immediate: envelopes in flight are delivered, but no new envelopes may be sent on a revoked right.

The permission matrix (§5.5) defines the *default* right grants — which rights the coordinator creates when a workspace is created. Port rights extend this: the matrix is the initial configuration, rights are the runtime state. A workspace may hold rights beyond what the matrix grants (via transfer) or fewer (via revocation).

Right creation, transfer, and revocation are recorded in the trail. Invariant: no envelope may be delivered unless the sender holds a valid send right (or send-once right) to the receiver at the time of submission.

---

## 6. Workspace Lifecycle

A workspace progresses through a defined lifecycle from creation to terminal state. The lifecycle is the protocol's process model — it governs when an agent can act, what transitions are legal, and how the system responds to completion, failure, and interruption.

### 6.1 Internal Model

A workspace has nine internal components:

| # | Component | Mutability |
|---|-----------|------------|
| 1 | Directive | Immutable — the task assignment |
| 2 | Inbox | Append by runtime, consume by agent |
| 3 | Context | Immutable — read-only information from the coordinator |
| 4 | Working Memory | Mutable — the agent's files and intermediate results |
| 5 | Checkpoint Register | Append-only — the ordered sequence of checkpoints produced |
| 6 | Resource Meter | Runtime-managed — tracks consumption against budgets |
| 7 | Local Trail | Append-only — this workspace's portion of the global trail |
| 8 | Visibility Set | Frozen at creation, extendable by coordinator |
| 9 | Authority Set | Frozen at creation |

These nine components constitute the workspace's complete state. Recovery reconstructs them from the trail. Migration transfers them to a new agent.

### 6.2 States

Nine states: two productive, three suspended, two resolution, two terminal.

```
idle → active → integrating → closed
  │       │         │
  │       │         └→ conflicted → closed
  │       │                      → failed
  │       ├→ blocked → active
  │       ├→ migrating → active | blocked | failed
  │       ├→ suspended → active | blocked | failed
  │       └→ failed
  └→ failed
```

**`idle`** — Workspace exists, agent is bound, no work has begun. The agent has emitted `ready`. Visibility and authority grants may still be adjusted.

**`active`** — The agent has received input and is working. Envelopes flow, checkpoints are created, the resource meter is accumulating.

**`blocked`** — The agent cannot continue and has emitted `blocked` with a reason. The inbox still accepts envelopes. Returns to `active` when the agent emits `started` after receiving unblocking input.

**`migrating`** — The coordinator has initiated an agent migration. The workspace is frozen — no checkpoints, no envelope processing, no signals. On success, returns to its pre-migration state. On failure, transitions to `failed`.

**`suspended`** — The coordinator has paused the workspace. All components are preserved but processing stops. The resource meter pauses (unlike `blocked`, where wall time continues). The inbox queues envelopes for later delivery. Returns to its pre-suspension state on resumption.

**`integrating`** — The agent has emitted `complete` and the coordinator is merging checkpoints into the parent context. The workspace is read-only.

**`conflicted`** — The coordinator detected a conflict during integration. The workspace remains read-only while the coordinator selects a resolution strategy.

**`closed`** — Terminal. Integration complete. All nine components are immutable and archived.

**`failed`** — Terminal. Unrecoverable error, abort, timeout, or budget exceeded. The trail is preserved for diagnosis.

### 6.3 Transition Rules

| From | To | Trigger |
|------|----|---------|
| *(none)* | `idle` | Coordinator creates workspace |
| `idle` | `active` | Agent receives first envelope |
| `idle` | `failed` | Creation error or timeout |
| `active` | `blocked` | Agent emits `blocked` |
| `active` | `migrating` | Coordinator initiates migration |
| `active` | `suspended` | Coordinator suspends workspace |
| `active` | `integrating` | Agent emits `complete` |
| `active` | `failed` | Agent emits `failed`, coordinator aborts, timeout, or budget exceeded |
| `blocked` | `active` | Agent emits `started` |
| `blocked` | `migrating` | Coordinator initiates migration |
| `blocked` | `suspended` | Coordinator suspends workspace |
| `blocked` | `failed` | Coordinator aborts, timeout, or budget exceeded |
| `migrating` | `active` / `blocked` | Migration succeeds (returns to pre-migration state) |
| `migrating` | `failed` | Migration error |
| `suspended` | `active` / `blocked` | Coordinator resumes (returns to pre-suspension state) |
| `suspended` | `failed` | Coordinator aborts or system shutdown |
| `integrating` | `closed` | Integration succeeds |
| `integrating` | `conflicted` | Conflict detected |
| `integrating` | `failed` | Integration error |
| `conflicted` | `closed` | Conflict resolved |
| `conflicted` | `failed` | Conflict unresolvable |

No backward transitions exist. `migrating` and `suspended` return to the pre-suspension state — these are suspension and resumption, not lifecycle regression. If a conflict requires agent rework, the coordinator fails the workspace and creates a new one.

### 6.4 Creation

Only the coordinator — or a delegate — can create workspaces. Creation populates the nine components and places the workspace in `idle`. Key parameters: role, parent, owner, directive, context, visibility set, authority set, delegation capability, timeout, and budget.

Authority is frozen once the workspace leaves `idle`. Visibility may be expanded by the coordinator during execution but never reduced. Delegation is granted at creation and cannot be acquired or revoked later.

Workspaces are never destroyed. They reach a terminal state and become immutable records. The coordinator may abort a workspace at any time, forcing it to `failed`.

### 6.5 Workspace Tree

Workspaces form a strict tree rooted at the coordinator's workspace. Every workspace except the root has exactly one parent. The tree is the protocol's primary topological structure — it determines containment, signal propagation, failure cascade, and budget inheritance.

**Two roots.** The workspace tree has a structural root — the coordinator's workspace (`originator: "system"`) — and zero or more causal roots — the humans who inject work. The structural tree (parent → child) governs containment and signal propagation. The causal tree (originator → workspace) governs attribution and user-state impact. The structural root is always the system. Causal roots are the humans whose injections create subtrees within the system's tree.

**Three invariants:**

1. **A child cannot outlive its parent — within ownership.** If a parent fails, its children are aborted recursively, but only within the same ownership boundary. In multi-user runs, a child workspace owned by a different user is not aborted — it is reparented to the coordinator (the protocol's equivalent of Unix orphan reparenting to init). The coordinator then decides how to handle the orphan: reassign it to a new parent, leave it under coordinator management, or consult the owning user through the highway. The coordinator's workspace is the root — if it fails, all workspaces fail regardless of ownership.
2. **Visibility flows downward by default.** A parent can see all descendants. A child sees only what was explicitly granted.
3. **Containment flows upward.** A child's visibility, authority, and budget must be subsets of its parent's.

Signals propagate upward — from child to parent, never laterally.

The workspace tree is one of several topological structures built from the protocol's primitives. The formal definition of tree operations — traverse, cascade, reparent, prune — and the relationship between the structural tree and the other topological structures (task graph, visibility graph, port rights graph, ownership domains, envelope threads) are defined in the topology specs.

### 6.6 Resource Management

Three independent mechanisms govern resource boundaries:

**Timeouts.** Every workspace has a timeout — maximum duration in `active` + `blocked` + `conflicted`. When exceeded, the runtime transitions the workspace to `failed`. The timeout clock never resets.

**Budgets.** Optional per-workspace resource limits across four physical dimensions: compute (tokens, wall time), memory (context window), storage (checkpoint bytes, trail bytes), and network (delivery bytes), plus a derived cost ceiling. The runtime tracks consumption independently of agent self-reporting. Warning at a configurable threshold (recommended 80%), hard failure at the limit. The coordinator may increase budgets additively during execution.

**Liveness.** Optional monitoring of agent activity. If no trail entry is recorded within the liveness interval, the runtime warns the coordinator. Advisory, not terminal — the coordinator decides how to respond.

### 6.7 Concurrency

Six invariants ensure deterministic behavior when events arrive concurrently:

1. **Single-writer serialization.** State transitions within a workspace are serialized.
2. **Abort precedence.** Coordinator abort is processed before queued agent signals.
3. **External failure precedence.** Budget and timeout failures follow the same precedence as abort.
4. **Signal emission ordering.** Signals from the same workspace are delivered in emission order.
5. **Trail monotonicity.** Trail entries within a workspace are strictly ordered; across workspaces, timestamps provide a partial order.
6. **Timeout race resolution.** If `complete` is processed before the timeout fires, the timeout is cancelled. If the timeout fires first, the late `complete` is recorded but cannot trigger a transition.

### 6.8 Visibility and Authority

Visibility governs read access; authority governs write access. The asymmetry is deliberate: granting read access mid-task is safe; granting write access mid-task breaks isolation.

The coordinator may grant dynamic visibility to workspaces in `active` or `blocked` state. Grants are additive only — visibility can never be revoked. Authority is frozen at creation and cannot be modified.

### 6.9 Agent Migration

The coordinator may replace a workspace's agent while preserving all nine components. The workspace enters `migrating`, the runtime snapshots the state, unbinds the old agent, binds the new one, and returns the workspace to its pre-migration state. Migration is atomic — it succeeds fully or the workspace fails. The resource meter is continuous across migration; budgets do not reset.

### 6.10 Graceful Termination

The coordinator may request a workspace wind down before being forcibly failed. It sends a feedback envelope with `intent: graceful_termination` and a `grace_period`. The agent is expected to produce a final checkpoint and emit `complete` within the grace period. If the period expires, the workspace transitions to `failed` with `reason: graceful_termination_expired`.

---

## 7. Integration and Checkpoints

Checkpoints capture individual progress. Integration assembles that progress into a coherent whole. Together they form the protocol's production and assembly model — agents produce checkpoints; the coordinator integrates them.

### 7.1 Checkpoint Anatomy

Every checkpoint carries the same structure:

- **id** — runtime-assigned, globally unique
- **workspace** — the workspace that created it
- **type** — base types: `artifact`, `observation`; extensible via taxonomy
- **payload** — files changed, content, artifact references
- **intent** — why this checkpoint exists, in the agent's words
- **parent** — the checkpoint this builds upon (null for first)
- **status** — `provisional` (work in progress) or `final` (ready for integration)
- **confidence** — `high`, `medium`, or `low`
- **resource_usage** — optional: tokens consumed, wall time, cost since previous checkpoint
- **timestamp** — runtime-assigned at creation

### 7.2 Checkpoint Rules

Five rules govern checkpoint behavior:

1. **Immutable.** Once created, no field may be changed. If an agent needs to revise, it creates a new checkpoint referencing the old one as its parent. History is never rewritten.
2. **Final checkpoint for integration.** The coordinator uses the most recent checkpoint with `status: final`. Provisional checkpoints are preserved in the trail for audit but not integrated.
3. **Type-role compatibility.** The runtime rejects checkpoints whose type is not permitted for the workspace's role. Workers produce `artifact` checkpoints; observers produce `observation` checkpoints. Derived roles may produce taxonomy-registered types.
4. **Confidence is information, not a gate.** A `confidence: low` checkpoint is valid. The coordinator reads confidence levels and decides what to do — the runtime does not block or delay based on confidence.
5. **Auto-signal emission.** When a checkpoint is created, the runtime automatically emits a `checkpoint` signal to the parent workspace. The agent does not need to emit it manually.

### 7.3 Checkpoint Chains

Within a workspace, checkpoints form a linear chain through their `parent` field. Three invariants:

1. **Linear.** No branches, no forks. One parent, one child. If the coordinator wants to explore two approaches, it creates two workspaces.
2. **Rooted.** The first checkpoint has `parent: null`. Each subsequent checkpoint references the immediately preceding one. The runtime rejects any checkpoint whose parent is not the current chain head.
3. **Append-only.** Checkpoints are added to the end. Never inserted, removed, or reordered.

Chains do not span workspaces. Cross-workspace linkage is the coordinator's responsibility through integration.

### 7.4 Integration Process

Integration begins when a worker emits `complete` and the workspace enters `integrating`. The coordinator reads the final checkpoint and makes one of three decisions:

**Accept** — the work is satisfactory. The coordinator proceeds to merge artifacts into the parent workspace. If no conflicts arise, the workspace transitions to `closed`. If conflicts arise, it transitions to `conflicted`.

**Revise** — the work is insufficient but recoverable. The workspace is failed with `reason: revision_required`. The coordinator creates a new workspace with a feedback directive referencing the failed workspace's trail.

**Reject** — the work is unusable. The workspace is failed with `reason: rejected`. The coordinator decides whether to retry or abort.

No backward transitions. A workspace that has entered `integrating` cannot return to `active`. Revision means a new workspace.

### 7.5 Merge Strategies

The coordinator selects a merge strategy at integration time:

| Strategy | Mechanism | Conflict Detection |
|----------|-----------|-------------------|
| `direct` | Artifacts copied into parent as-is | None |
| `layered` | Artifacts applied on top of existing parent state | Content overlap detected |
| `evaluated` | Coordinator reads both states and synthesizes a result | All conflict types detected |

Strategy selection is the coordinator's decision based on how directives were scoped, whether overlap is expected, and how critical correctness is.

### 7.6 Integration Ordering

When multiple workspaces complete concurrently, the coordinator integrates them sequentially — one at a time, each seeing the result of all prior integrations. There is no parallel merge. The coordinator selects order based on dependencies, priority, confidence, or the task DAG's topological order.

### 7.7 Conflict Detection

Four conflict types (closed set):

| Type | Meaning |
|------|---------|
| `content_overlap` | Two workspaces modified the same resource |
| `semantic_contradiction` | Two workspaces reached incompatible conclusions |
| `dependency_violation` | A workspace's output invalidates an assumption from a prior integration |
| `constraint_breach` | Integration would violate a workflow-level constraint |

A detected conflict transitions the workspace from `integrating` to `conflicted`.

### 7.8 Conflict Resolution

Three resolution strategies:

**Coordinator resolve** — the coordinator examines conflicting artifacts, makes a judgment, completes the merge. Workspace transitions to `closed`.

**Escalate** — the conflict is routed to a human for resolution (§8). The human reviews and decides: approve, modify, or reject.

**Agent rework** — the workspace is failed, a new workspace is created with conflict context in the directive.

All conflicts must be resolved before the workspace can transition from `conflicted` to `closed`. If any conflict results in `agent_rework`, the entire workspace is failed.

### 7.9 Salvage Integration

When a workspace fails but has produced checkpoints, the coordinator may recover partial work through salvage integration. Three mandatory guardrails:

1. **Evaluated strategy only.** Salvaged artifacts cannot be blindly merged.
2. **Confidence treated as low.** Regardless of self-reported confidence — the agent was interrupted before completing its work.
3. **Trail transparency.** Every trail entry carries `mode: salvage` so downstream consumers can distinguish salvaged from normal integrations.

The source workspace remains in `failed` state throughout — salvage extracts artifacts from the wreckage without resurrecting the workspace.

---

## 8. Human Highway

The human highway is the protocol's supervisory control layer. It guarantees that a human *can* observe any communication, approve any transition, inject any message, and respond to any escalation — without requiring the protocol to be stopped or reconfigured. The highway does not require human presence. It guarantees human *access*.

The highway is not a new primitive. It is a policy layer composed from existing primitives: envelopes for injection, signals for escalation, the trail for visibility, checkpoints for review, tasks for planning approval, and integration for merge gating.

### 8.1 The Four Capabilities

**Visibility.** The human can observe any communication in the system in real time through the global trail. Visibility is passive — observing an event does not alter it, delay it, or require acknowledgment. The system does not know or care whether a human is watching.

**Gates.** The human can approve, reject, or modify specific transitions before they execute. Six gate types:

| Gate Type | Transition Gated |
|-----------|-----------------|
| `task_approval` | Task `draft` → `pending` |
| `workspace_create` | Coordinator creates a workspace |
| `envelope_delivery` | Envelope reaches receiver's inbox |
| `integration` | Coordinator initiates merge |
| `conflict_resolution` | Coordinator resolves a conflict |
| `workspace_abort` | Coordinator aborts a workspace |

When a gated transition is triggered, the runtime pauses and emits a gate event. The human responds with **approve** (proceed as proposed), **reject** (cancel the transition), or **modify** (alter specific fields before approving). If no response arrives within the configured timeout, a fallback action executes automatically — ensuring the system never deadlocks waiting for an absent human.

**Injection.** The human can create and send envelopes to any workspace at any time. Human-injected envelopes bypass role-based send restrictions — a human can send a directive even though only the coordinator role normally can. All other validation (structure, target existence, type registration) remains enforced. Injected envelopes carry `origin: human`, an immutable system-assigned field distinguishing them from agent-generated messages. Injection is additive — the human cannot alter envelopes already sent or checkpoints already created.

**Escalation handling.** When an agent emits an `escalation` signal, it is delivered to the highway in addition to propagating upward through the workspace tree. The human responds with an envelope (feedback to unblock the agent), an abort (fail the workspace), or a delegation (let the coordinator handle it). Escalations route to the workspace's owner by default.

### 8.2 Task Approval Gate

The task approval gate is structurally distinct from the others. It pauses a transition that has not yet begun — no workspace exists, no agent is bound, no tokens are consumed. It is the protocol's planning checkpoint.

Every task begins in `draft` status. The `draft` → `pending` transition is gated by default. When the coordinator submits a task graph, each task triggers its own `task_approval` gate. The human sees the full plan — name, description, dependencies, priority, resource estimate, and position in the graph — before any resources are committed.

When the coordinator decomposes tasks mid-execution, the new subtasks enter as `draft` and trigger their own gates. The human sees scope expansion in real time — preventing silent scope creep.

### 8.3 Autonomy Spectrum

The four capabilities are independently configurable per workflow. The configuration determines where the system sits on the autonomy spectrum.

| Preset | Description |
|--------|-------------|
| **autonomous** | Task approval with short timeout and approve fallback. Escalations delegated to coordinator on timeout. The human watches but does not block. |
| **supervised** | Task approval and integration gated. Escalations wait longer, reject on timeout. The recommended starting point. |
| **gated** | All gates enabled. Escalations wait indefinitely. Nothing moves without human consent. |

The spectrum is continuous — presets are starting points, not the only options. Any combination of capabilities, timeouts, and fallbacks is valid. Per-gate-type overrides allow different timeout and fallback settings for different transitions within the same workflow.

### 8.4 Gate Mechanics

Gate events carry: id, type, the full subject awaiting approval, workspace and task context, timeout duration, and fallback action. The gate type set is closed — new gate types require a protocol revision.

Every gate MUST be eventually resolved — by human action, timeout fallback, or subject invalidation. The system MUST NOT deadlock waiting for an absent human. Gate ordering, timeout behavior, and fallback loop prevention are defined in the highway spec.

### 8.5 Interface Boundary

The protocol defines the highway's capabilities and semantics. It does not define the interface — whether it is a CLI, a web dashboard, a chat window, or an IDE panel. The interface is an application concern. Different applications implement different interfaces; the protocol ensures they all have the same capabilities and the same trail semantics.

---

## 9. Trail

The trail is the protocol's memory. Every other primitive produces trail entries — workspaces record state transitions, signals record emissions, envelopes record deliveries, checkpoints record production, tasks record lifecycle changes, integration records decisions, and the highway records every human action. If an event is not in the trail, it did not happen. If it is in the trail, it is immutable.

### 9.1 Entry Schema

Every protocol event produces exactly one trail entry. All entries share a common header:

- **id** — globally unique, runtime-assigned
- **timestamp** — runtime-assigned at the moment the event occurs; monotonic within a workspace, partial order across workspaces
- **workspace** — the workspace this event belongs to (null for system-level events)
- **actor** — who caused the event: a role name (agent action), `protocol` (runtime action), or a `user_id` (human action)
- **event_type** — from the closed event registry
- **body** — type-specific payload defined by each primitive's spec

One event, one entry. No event produces zero entries; no event produces multiple entries.

**Write-ahead.** The trail entry is written before the event takes effect. A state transition is recorded before the state machine is updated. An envelope delivery is recorded before the envelope is placed in the inbox. If the write fails, the event does not proceed.

### 9.2 Scopes

**Local trail.** Every workspace has a local trail — the ordered sequence of entries where `workspace` equals that workspace's ID. Strictly ordered by timestamp. The agent can read its own local trail.

**Global trail.** The union of all local trails plus system-level entries. Ordered by timestamp, but the ordering is partial across workspaces — entries from different workspaces with the same timestamp are concurrent. The global trail is the master record; local trails are projections of it.

Access follows the role hierarchy: workers read their own local trail only; observers read trails per their visibility set; the coordinator reads the full global trail.

### 9.3 Integrity

Three invariants protect the trail's value:

1. **Append-only.** No entry may be deleted within the retention window. The trail only grows.
2. **Immutable.** No entry may be modified after creation. Corrections are recorded as new entries referencing the original.
3. **No gaps.** Every protocol event produces exactly one trail entry. Enforced by the write-ahead rule — if the write fails, the event does not proceed.

The trail MUST be tamper-evident. The mechanism for achieving tamper evidence is defined in the trail spec.

### 9.4 Querying

The trail is queryable — a database, not a file. Queries respect access rules — a worker querying outside its scope sees an empty result, not an error. The trail MUST be queryable during a run, not only after completion. Query mechanisms are defined in the trail spec.

### 9.5 Event Registry

The registry is the consolidated index of every trail event type. It is closed within a protocol version — unknown event types are rejected. The registry spans all primitives and mechanisms: workspace events, user events, signal events, envelope events, checkpoint events, task events, integration events, highway events, recovery events, security events, and trail-own events.

### 9.6 Storage and Retention

The trail grows monotonically. The runtime MUST support tiered storage to manage this growth. Compaction MUST relocate entries, never delete them within the retention window. Retention policies are configurable per deployment. Storage architecture and compaction procedures are defined in the trail spec.

---

## 10. Recovery and Fault Handling

The protocol's lifecycle, signal, and integration mechanisms handle failures within the coordination layer — an agent errors, a budget is exhausted, a conflict cannot be resolved. These are internal failures: the runtime is healthy, the failure is contained.

External failures are different. The LLM provider goes down. The runtime process crashes. Trail storage becomes unavailable. These failures strike the infrastructure the protocol depends on. Without recovery semantics, a restart would leave the system in an undefined state.

### 10.1 Failure Classification

Three classes, by scope and responsibility:

**Agent-local.** An agent's backing model returns an error, a tool times out, an external API is unreachable. The agent's responsibility is to translate external failures into protocol signals: `blocked` if recoverable (the reversible signal), `failed` if unrecoverable. The protocol does not see external dependencies — it sees workspace state. Liveness monitoring (§6.6) serves as the backstop for agents that stall without signaling.

**Systemic.** The LLM provider goes down, affecting multiple agents simultaneously. A shared dependency becomes unavailable. The coordinator detects systemic failures by observing patterns in the trail: temporal clustering of failures, reason similarity, group correlation. The coordinator's response: pause dispatch, avoid redundant retries, batch abort if appropriate, escalate to the human highway. The protocol does not automate systemic failure detection — it provides the trail data that makes detection possible.

**Runtime infrastructure.** The runtime itself crashes. Trail storage becomes unavailable. The clock stops. These are the most severe failures because the protocol's own machinery is affected. The protocol cannot handle these in real time — by definition, it is not running. Instead, it defines recovery invariants: what must be true after the runtime restarts.

### 10.2 The Recovery Model

The trail is the recovery source. Every protocol event produces a trail entry before it takes effect (§9.1, write-ahead). This means the trail contains the last known good state. Recovery is trail replay.

**Recovery axiom:** If an event is recorded in the trail, it happened. If an event is not recorded, it did not happen. There is no third category. Operations in flight when the runtime crashed — not yet recorded in the trail — are lost. The protocol treats them as if they never occurred.

### 10.3 Partial Failures

**Trail write failures.** When a trail write fails, the runtime MUST NOT proceed with the operation the entry was meant to record. The trail write is a precondition, not a side effect. If writes persistently fail, the runtime enters degraded mode: no new operations initiated, active agents may continue but their output cannot be recorded, a `system_degraded` trail entry is recorded when writes recover.

**Transport failures.** Transient failures are retried. Persistent failures are reported to the coordinator. The runtime MUST NOT silently drop messages.

### 10.4 Recovery Invariants

Five invariants govern failure and recovery:

1. **Trail-authoritative state.** After recovery, system state is exactly what the trail says — no more, no less.
2. **Write-ahead trail.** No operation takes effect before its trail entry is durably written.
3. **Idempotent recovery.** The recovery procedure produces the same result regardless of how many times it runs.
4. **No silent data loss.** The runtime never silently drops trail entries, envelopes, or signals. If an operation cannot be recorded or delivered, the runtime retries, escalates, or halts — never proceeds as if it succeeded.
5. **Degradation over catastrophe.** The runtime degrades gracefully rather than halting abruptly. Transient failures are retried. Persistent failures trigger notification and escalation. Only trail corruption warrants an immediate halt.

The recovery procedure — the specific steps by which the runtime reconstructs state from the trail — is defined in the recovery spec.

---

## 11. Security Model

The protocol's coordination mechanisms — roles, permissions, visibility, authority, trail immutability — are structural guarantees within a trusted runtime. They define what agents are allowed to do. The security model defines how that enforcement survives adversarial conditions.

### 11.1 Trust Root

The runtime is the trust root. The runtime delivers envelopes, enforces the permission matrix, manages workspace lifecycles, writes trail entries, and enforces budgets. The protocol assumes the runtime is correct and uncompromised — similar to a kernel trust model. If the runtime is compromised, all guarantees collapse. Defending against a compromised runtime is out of scope.

### 11.2 Threat Actors

Four classes, ordered by increasing severity:

**Rogue agent.** An agent that attempts to exceed its permissions — reading outside its visibility, modifying outside its authority, sending envelopes without valid rights, impersonating another agent. The permission matrix and port rights are designed for this threat. The runtime enforces permissions independently of agent cooperation.

**Compromised transport.** An adversary that can observe, modify, or inject messages on the communication channel. The protocol requires message integrity and origin authentication.

**Compromised storage.** An adversary that can read or modify trail entries, checkpoints, or workspace data at rest. The protocol requires trail integrity verification and defines confidentiality boundaries.

**Insider with elevated access.** A human operator who acts maliciously — modifying trail entries through direct storage access, injecting envelopes that bypass the highway. The protocol's non-repudiation requirements make such actions detectable, though not preventable.

Out of scope: compromised runtime, side-channel attacks, infrastructure-level denial of service, agent-internal vulnerabilities (prompt injection, model jailbreaking). The permission matrix ensures a compromised agent's blast radius is limited to its workspace.

### 11.3 Identity and Authentication

Every participant — agents, the coordinator, humans — must be authenticated. The protocol requires verified identity but does not prescribe a specific mechanism.

**Agent identity.** Unique, verifiable at message origin, bound to the workspace at creation. The `workspace_created` trail entry includes the authenticated agent identity — the foundation for non-repudiation.

**Coordinator identity.** Singleton verification (exactly one coordinator per instance), elevated authentication (established before any workspaces are created), and non-repudiable actions.

**Human identity.** Authenticated before any highway action. The `actor` field carries the authenticated human's `user_id` — the specific person, not a generic marker. Authentication mechanism is application-defined.

### 11.4 Message Integrity

Every message — envelopes, signals, checkpoints — must be integrity-protected.

**Envelopes** carry an integrity proof covering all fields, bound to the sender's identity. The runtime verifies integrity before delivery; failure results in rejection with `reason: integrity_violation`.

**Signals** rely on origin verification by the runtime. Since the runtime mediates all signal delivery, per-signal cryptographic proof is not required unless signals travel over an untrusted transport.

**Checkpoints** carry a content integrity proof computed at creation. The runtime verifies integrity at integration time; a checkpoint that fails verification is not integrated.

### 11.5 Trail Integrity

The trail MUST be tamper-evident (§9.3). The runtime MUST provide cryptographic integrity protection for trail entries. Implementations SHOULD sign trail entries. The mechanisms for achieving trail integrity — hash chains, signing, cross-scope anchoring — are defined in the security spec.

### 11.6 Confidentiality

The protocol defines confidentiality boundaries, not encryption requirements. Trail access follows the role hierarchy. Checkpoint content is accessible only to entities with visibility. Envelope payloads are accessible only to sender, receiver, and coordinator. Highway access is restricted to authenticated humans. Deployment models and their enforcement mechanisms are defined in the security spec.

### 11.7 Non-Repudiation

The combination of authenticated identity (§11.3), message integrity (§11.4), and trail integrity (§11.5) produces non-repudiation: an entity cannot deny having taken an action if the trail records it. The chain of proof: identity verified at workspace creation, integrity proof bound to identity on every action, action recorded in the trail with verified identity, trail entry linked into the hash chain.

### 11.8 Security Invariants

Seven invariants:

1. **Runtime is the trust root.** All security guarantees flow from the assumption that the runtime is correct and uncompromised.
2. **Authentication precedes authorization.** No entity may perform any protocol action without first being authenticated.
3. **Integrity is verified, not assumed.** Messages are integrity-protected at creation and verified at consumption. A message that fails verification is rejected and logged, never silently accepted.
4. **The trail is tamper-evident.** The hash chain makes tampering detectable. The cost of undetectable tampering scales with the chain length.
5. **Visibility is the minimum confidentiality guarantee.** An agent sees exactly what its visibility grants permit and nothing more.
6. **Security events are never silent.** Every authentication failure, integrity violation, and permission denial is recorded in the trail.
7. **Physical resources are protected per deployment model.** Compute, memory, storage, and network each have protection requirements that scale with the deployment context.

---

## 12. Conformance and Terminology

### 12.1 Terminology

This specification uses the following terms, consistent with RFC 2119:

- **MUST** / **REQUIRED** — absolute requirement. An implementation that violates a MUST is non-conformant.
- **SHOULD** / **RECOMMENDED** — there may exist valid reasons to ignore this requirement, but the implications must be understood.
- **MAY** / **OPTIONAL** — truly optional. An implementation may include or omit this feature.

### 12.2 Conformance Levels

WACP defines three conformance levels. Each level includes all requirements of the levels below it.

**Level 1: Core**

The minimum viable WACP implementation.

| Requirement | Section |
|-------------|---------|
| Six core primitives (workspace, envelope, signal, checkpoint, trail, task) | §4 |
| Three base roles with the permission matrix | §5 |
| Port rights (three right types, enforcement invariant) | §5.6 |
| Eleven signal types (closed set) | §4.3 |
| Envelope lifecycle (five states, all transitions recorded) | §4.2 |
| Redelivery (3 attempts, linear backoff, deduplication) | §4.2 |
| Workspace state machine (nine states, all defined transitions) | §6.2–§6.3 |
| Checkpoint creation and immutability | §7.1–§7.2 |
| Integration (at least `direct` strategy) | §7.4 |
| Trail recording (all events produce trail entries) | §9.1 |
| Write-ahead trail | §9.1 |
| Identity rules | §4.7 |

**Level 2: Standard**

Adds resource management, observability, and the human highway. The recommended level for production deployments.

| Requirement | Section |
|-------------|---------|
| All Level 1 requirements | — |
| Resource budgets, timeouts, and liveness monitoring | §6.6 |
| Dynamic visibility grants | §6.8 |
| Human highway (all three presets) | §8 |
| Conflict detection and resolution | §7.7–§7.8 |
| Trail tamper evidence | §11.5 |
| Authentication for all actors | §11.3 |
| Message integrity verification | §11.4 |
| Recovery procedure | §10.3 |
| User identity model (ownership, originator) | §4.8 |

**Level 3: Full**

Complete WACP conformance including all optional features.

| Requirement | Section |
|-------------|---------|
| All Level 2 requirements | — |
| Salvage integration | §7.9 |
| Trail lifecycle management (compaction, archival) | §9.6 |
| Trail signing | §11.5 |
| Cross-scope trail anchoring | §11.5 |
| Agent migration | §6.9 |
| Graceful termination | §6.10 |

### 12.3 Partial Conformance

An implementation MAY claim partial conformance by stating its level and listing any deviations. Deviations must be documented; undisclosed deviations are non-conformant.

### 12.4 Extension Rules

Conformant implementations MAY extend the protocol in two ways:

1. **Taxonomy extensions.** New envelope types, checkpoint types, and roles registered through the taxonomy. These do not affect conformance.
2. **Implementation-defined behaviors.** Where the protocol states a behavior is "implementation-defined," any behavior that satisfies the stated constraints is conformant.

Conformant implementations MUST NOT:
- Add signal types beyond the eleven defined in §4.3
- Add workspace states beyond the nine defined in §6.2
- Add backward transitions to the state machine
- Modify the base permission matrix (§5.5)
- Weaken any MUST requirement

### 12.5 Protocol Constants

These values are fixed by the protocol. Implementations MUST NOT change them.

| Constant | Value |
|----------|-------|
| Signal types | 11: `ready`, `started`, `blocked`, `checkpoint`, `complete`, `failed`, `integrate`, `acknowledged`, `escalation`, `suspend`, `migrate` |
| Workspace states | 9: `idle`, `active`, `blocked`, `suspended`, `migrating`, `integrating`, `conflicted`, `closed`, `failed` |
| Terminal states | 2: `closed`, `failed` |
| Base roles | 3: `coordinator`, `worker`, `observer` |
| Role inheritance depth | 1 (single-level only) |
| Base envelope types | 3: `directive`, `feedback`, `query` |
| Checkpoint statuses | 2: `provisional`, `final` |
| Confidence levels | 3: `high`, `medium`, `low` |
| Base checkpoint types | 2: `artifact`, `observation` |
| Integration strategies | 3: `direct`, `layered`, `evaluated` |
| Integration modes | 2: `normal`, `salvage` |
| Conflict types | 4: `content_overlap`, `semantic_contradiction`, `dependency_violation`, `constraint_breach` |
| Resolution strategies | 3: `coordinator_resolve`, `escalate`, `agent_rework` |
| Trail scopes | 2: local (workspace), global (system) |
| Trail storage tiers | 3: `hot`, `warm`, `cold` |
| Highway presets | 3: `autonomous`, `supervised`, `gated` |
| Gate types | 6: `task_approval`, `workspace_create`, `envelope_delivery`, `integration`, `conflict_resolution`, `workspace_abort` |
| Deployment security models | 3: `single-runtime`, `shared-infrastructure`, `zero-trust` |
| Port right types | 3: `send`, `receive`, `send_once` |
| Envelope states | 5: `created`, `validated`, `delivered`, `acknowledged`, `rejected` |
| Redelivery attempts | 3 (4 total including initial send) |
| Envelope priority levels | 3: `normal`, `urgent`, `blocking` |
| Envelope origin values | 2: `agent`, `human` |
| Originator values | 2 types: `user_id` (human-rooted), `"system"` (system-rooted) |
| Coordinator model | 1: the coordinator is the system. `originator: "system"` on the root workspace. No deployment in which the coordinator is a user. |

### 12.6 Glossary

Canonical definitions for all terms used in this specification. Alphabetical order.

| Term | Definition |
|------|-----------|
| **Agent** | An intelligent entity operating within a workspace. Makes autonomous decisions within protocol constraints. The coordinator is also an agent. |
| **Authority** | The set of resources a workspace may modify. Frozen after the workspace leaves `idle` state. |
| **Checkpoint** | The unit of progress. A structured, immutable snapshot of an agent's work within a workspace. |
| **Coordinator** | The root agent. Exactly one per run. Creates workspaces, dispatches directives, evaluates results, integrates outcomes. |
| **Causal tree** | The tree formed by `originator` propagation. Every workspace traces its causal origin to either a `user_id` or `"system"`. Distinct from the structural tree (parent → child). |
| **Delegate** | A workspace with `delegate: true` that can create child workspaces within its subtree. |
| **Coordinator model** | The coordinator is the system — categorically different from users. Its root workspace carries `originator: "system"`. There is one model, not a choice. |
| **Envelope** | The unit of communication. A structured message passed between workspaces. |
| **Gate** | A synchronization point where the runtime pauses a transition and waits for human approval. |
| **Highway** | The human highway — the policy layer enabling human visibility, gates, injection, and escalation handling. |
| **Integration** | The act of merging a workspace's final checkpoint into the parent workspace. |
| **Observer** | A base role with read-only access to designated workspaces and the trail. |
| **Mixed tree** | A workspace tree containing both the system-rooted sub-forest (`originator: "system"`) and one or more human-rooted subtrees (`originator: user_id`). The normal state of any tree with human interaction. |
| **Originator** | Required field on every workspace. `user_id` of the human who caused it, or `"system"` for system-initiated workspaces. Immutable once set. Inherited unconditionally by child workspaces, except at injection points where the injecting human's `user_id` overrides inheritance. Distinct from owner. |
| **Owner** | The `user_id` of the human on whose behalf a workspace exists. Determines escalation routing, accountability, and capability scoping. Transferable. |
| **Port right** | A transferable capability governing workspace-to-workspace communication. Three types: send, receive, send-once. |
| **Protocol** | This specification. Defines rules, requires behaviors, specifies invariants. Does not act. |
| **Role** | A named set of capabilities bound to a workspace at creation. Governs what an agent can send, receive, emit, create, and access. |
| **Runtime** | The software that implements the protocol. Enforces rules, delivers messages, manages state. The trust root. |
| **Signal** | The unit of notification. A lightweight, typed event declaring a state change. Eleven types (closed set). |
| **Task** | The unit of work. A structured assignment within a dependency graph. |
| **Trail** | The append-only audit log. The single source of truth. Exists at local (per-workspace) and global (system-wide) scope. |
| **Visibility** | The set of resources a workspace may read. Set at creation, expandable by the coordinator, never reducible. |
| **Worker** | A base role that receives directives, works within its workspace, and declares progress through checkpoints. |
| **Structural tree** | The workspace tree formed by parent → child relationships. Rooted at the coordinator's workspace. Governs containment, signal propagation, budget inheritance, failure cascade. |
| **Topology** | The structural relationships between protocol objects — trees, graphs, domains, threads, sequences. Defined formally in the topology specs. |
| **Workspace** | The unit of isolation. A bounded context assigned to exactly one agent. Nine lifecycle states. |

---

*WACP — authored by Akil Abderrahim and Claude Opus 4.6*
