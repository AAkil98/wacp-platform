# WACP: Human Highway

## Metadata

```yaml
title: "WACP: Human Highway"
id: wacp-spec-human-highway
type: constituent-spec
tier: abstract
category: mechanisms
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §8
depends_on:
  - wacp-spec-workspace
  - wacp-spec-signal
  - wacp-spec-envelope
  - wacp-spec-checkpoint
  - wacp-spec-task
  - wacp-spec-integration
  - wacp-spec-user
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, human-highway, supervisory-control, gates, injection, escalation, autonomy]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [The Four Capabilities](#2-the-four-capabilities)
3. [Gate Schema and Mechanics](#3-gate-schema-and-mechanics)
4. [Task Approval Gate](#4-task-approval-gate)
5. [Autonomy Spectrum](#5-autonomy-spectrum)
6. [Edge Cases and Invariants](#6-edge-cases-and-invariants)
7. [Interface Boundary](#7-interface-boundary)
8. [Trail Events](#8-trail-events)
9. [Conformance Requirements](#9-conformance-requirements)
10. [Implementation Notes](#10-implementation-notes)
11. [References](#11-references)

---

## 1. Purpose

The human highway is the protocol's supervisory control layer. It is where all routes join — every primitive, every lifecycle transition, every communication channel passes through a point where a human *can* intervene. The highway does not require human presence. It guarantees human *access* — the ability to observe, approve, inject, and respond at any point in the system's operation, without redesigning or restarting the protocol.

The highway is not a new mechanical primitive. It is a policy layer composed from existing primitives: envelopes for injection, signals for escalation, the trail for visibility, checkpoints for review, tasks for planning approval, and integration for merge gating. What makes it a highway rather than another road is its cross-cutting nature — the human is not a participant in a single workspace but a supervisor with access to the entire system.

Multi-agent coordination systems face a tension. Fully autonomous systems are efficient but opaque — when something goes wrong, the human discovers the failure after the fact, in the wreckage. Fully human-supervised systems are safe but slow — the human becomes the bottleneck, and agents wait. The highway resolves this by making human involvement a **spectrum**, not a binary. At one end, the system runs autonomously with zero human interaction. At the other end, every transition requires human approval. Between these extremes, the workflow configures exactly which transitions are gated, which signals trigger human attention, and how long the system waits before falling back to autonomous behavior.

The design pattern is **supervisory control**, drawn from robotics and industrial systems. The operator does not control every actuator. They have a supervisory channel that lets them monitor, intervene, and override — without disrupting the system's autonomous operation. The highway is supervisory control for multi-agent coordination.

The highway provides four capabilities:
- **Visibility** — the human can observe any communication in the system in real time, through the global trail.
- **Gates** — the human can approve, reject, or modify specific transitions before they execute, including task approval (`draft` → `pending`), workspace creation, integration, and conflict resolution.
- **Injection** — the human can create and send envelopes to any workspace at any time, bypassing role-based send restrictions.
- **Escalation handling** — the human receives and responds to escalation signals from agents who cannot proceed without human input.

Each capability is independently configurable per workflow. A workflow may enable visibility and injection but disable gates — the human watches and can intervene, but nothing pauses for approval. Another workflow may gate every integration — the human approves every merge. The four capabilities compose freely along the autonomy spectrum.

The highway sits at Layer 6 in the dependency graph — above all primitives, below the systems. It depends on Workspace (lifecycle transitions to gate), Signal (escalation propagation), Envelope (injection mechanism), Checkpoint (human reviews artifacts), Task (planning approval gate), and Integration (merge gating). This is not incidental — the highway is the convergence point because human oversight touches every primitive.

## 2. The Four Capabilities

The highway provides four capabilities. Each is built on existing protocol primitives. Each is independently configurable. Together they give the human full supervisory access without coupling the protocol to any specific interface.

### 2.1 Visibility

The human can observe any communication in the system in real time.

**What is visible:**
- All envelopes — sent, delivered, rejected, undeliverable (Envelope spec, §10)
- All signals — emitted, delivered (Signal spec, §8)
- All checkpoints — created, with full payload accessible by reference (Checkpoint spec, §7)
- All task state transitions — creation, approval, assignment, completion, failure (Task spec, §8)
- All integration events — started, completed, conflicts detected and resolved (Integration spec, §8)
- All workspace state transitions — the full lifecycle of every workspace (Workspace spec, §13)
- All trail entries — as they are appended to the global trail

**How it works.** The global trail already records every protocol event. Visibility extends the trail from a post-hoc audit record to a live feed. The protocol emits trail entries as they occur; the highway makes them available to the human in real time. The implementation decides how — push notifications, polling, streaming — but the protocol guarantees the data is available.

**What visibility is not.** Visibility is passive. Observing an event does not alter it, delay it, or require acknowledgment. The system does not know or care whether a human is watching. Visibility has zero impact on protocol behavior. It is read-only access to the global trail, nothing more.

### 2.2 Gates

The human can approve, reject, or modify specific transitions before they execute.

**What can be gated:**

| Gate type | Transition gated | Subject |
|-----------|-----------------|---------|
| `task_approval` | Task `draft` → `pending` (Task spec, §4) | The task definition |
| `workspace_create` | Coordinator creates a workspace | The workspace creation parameters |
| `envelope_delivery` | Envelope reaches receiver's inbox | The envelope |
| `integration` | Coordinator initiates merge | The integration record |
| `conflict_resolution` | Coordinator resolves a conflict | The conflict and proposed resolution |
| `workspace_abort` | Coordinator aborts a workspace | The workspace and abort reason |

**How it works.** Gates are configured per-workflow (§5). When a gated transition is triggered, the runtime pauses the transition and emits a gate event to the highway. The human reviews the subject and responds with one of three actions:
- **Approve** — the transition proceeds as proposed.
- **Reject** — the transition is cancelled. The effect depends on the gate type: a rejected task approval transitions the task to `cancelled`; a rejected integration transitions the workspace to `failed` with `reason: gate_rejected_by_human`.
- **Modify** — the human alters the subject before approving. For example: adjusting a workspace's budget before creation, changing a task's priority before approval, or modifying an envelope's payload before delivery.

If no response arrives within the configured timeout, the fallback action executes automatically (§3). This ensures the system never deadlocks waiting for a human who has stepped away.

**What gates are not.** Gates are not checkpoints. They do not produce artifacts. They are synchronization points — they pause, wait, then proceed or cancel. Gate events and their resolutions are recorded in the trail.

### 2.3 Injection

The human can create and send envelopes at any time, targeting any workspace.

**What can be injected:**
- A new directive to any worker (or derived roles that receive directives)
- Feedback to any agent mid-work
- A query to the coordinator
- Any envelope type registered in the taxonomy

**How it works.** The human constructs an envelope through the application interface. The runtime validates the envelope against the permission matrix — with one exception: **human-injected envelopes bypass role-based send restrictions.** A human can send a `directive` even though only the coordinator role normally can. The envelope is still validated for structure, target existence, and type registration. It is still recorded in the trail. The only rule relaxed is the sender-role check.

**User state check.** Only `active` users may inject envelopes. The runtime rejects injections from `suspended`, `blocked`, or `deactivated` users — the highway is gated by user state (User spec §3.1). A `capability_denied` trail event is recorded with `reason: user_not_active`.

The injected envelope carries `origin: human` (Envelope spec, §3) — an immutable, system-assigned field that distinguishes human-injected messages from agent-generated ones. Any trail analysis can filter on this field.

The injected envelope also carries `originator: user_id` of the injecting human (User spec, §5.1). If the injection triggers workspace creation (e.g., a new directive to the coordinator), the resulting workspace inherits the `originator`. This is how the protocol traces work back to the human who initiated it — `origin: human` says "a human sent this," `originator` says "which human."

**What injection is not.** Injection is not modification. The human cannot alter an envelope that has already been sent. They cannot edit a checkpoint that has already been created. They can only add new messages to the flow. The immutability guarantees of the protocol — checkpoints, trail entries, delivered envelopes — are never violated by human action.

### 2.4 Escalation Handling

The human receives and responds to escalation signals from agents who cannot proceed without human input.

**When escalation occurs.** An agent emits an `escalation` signal (Signal spec, §2) when it cannot proceed without human input. The signal carries a `reason` explaining what the agent needs — "the directive is ambiguous," "I found a conflict exceeding my authority," "I have low confidence and want human review." The escalation signal propagates upward through the workspace tree (Signal spec, §3, rule 5) and is additionally delivered to the highway.

**Routing.** Escalation signals route to the workspace's owner by default (User spec, §4.1). In a multi-user deployment, different workspaces may be owned by different humans — an escalation from a database workspace routes to the database domain owner, not to all users. The routing target is determined by the `owner` field on the escalating workspace. Deployment may override default routing with domain-specific rules (e.g., route by team, by expertise, by availability). The protocol defines the default (owner); the deployment configures exceptions.

**User state gates escalation delivery.** The target user's state determines what happens to the escalation at the human boundary (User spec §3.1):

| User state | Escalation behavior |
|------------|-------------------|
| `active` | Delivered immediately. The user sees the escalation and can respond. |
| `suspended` | Queued. The escalation accumulates until the user is resumed, then delivers. The agent remains `blocked`. |
| `blocked` | Queued. Same as suspended — the user is expected to return. |
| `deactivated` | Rejected. The escalation cannot reach this user. The coordinator is notified and must reroute — either to a different user (via ownership transfer) or handle the escalation itself. A `capability_denied` trail event is recorded. |

When escalations are queued (user suspended or blocked), the escalation timeout continues to run. If the timeout expires before the user returns, the fallback action executes regardless of user state. This prevents an indefinitely suspended user from indefinitely blocking workspaces.

**Paired signals.** An agent that escalates emits two signals: a `blocked` signal (triggering `active` → `blocked` in the workspace state machine) followed by an `escalation` signal (activating the highway). The `blocked` signal handles the state transition; the `escalation` signal activates human attention. The workspace is suspended until the human responds or the escalation times out.

**The human responds with one of:**
- **An envelope** — feedback, a new directive, or clarification, delivered to the blocked workspace. The agent receives it, unblocks, and resumes.
- **An abort** — the human decides the workspace should fail. The runtime transitions it to `failed` with `reason: aborted_by_human`.
- **A delegation** — the human decides the coordinator should handle it. The escalation is downgraded to a coordinator concern — the coordinator processes it using its normal blocked-agent logic.

If no human response arrives within the workflow-configured timeout, the fallback behavior executes (§5).

**The coordinator does not escalate.** The coordinator is the root of the workspace tree and has no parent to escalate to. If the coordinator needs human input, it activates the highway directly through a gate — not through the escalation signal.

## 3. Gate Schema and Mechanics

Gates are the highway's active control mechanism. Where visibility is passive and injection is opportunistic, gates are synchronous — they pause a transition, wait for human input, and resume or cancel based on the response. This section defines the gate event schema, the resolution protocol, and the timeout/fallback mechanics.

**Gate event schema:**

```yaml
gate_event:
  id: string                      # runtime-assigned, globally unique (Identity spec, rule 1)
  type: enum
    - task_approval               # task draft → pending
    - workspace_create            # coordinator creates workspace
    - envelope_delivery           # envelope reaches receiver's inbox
    - integration                 # coordinator initiates merge
    - conflict_resolution         # coordinator resolves a conflict
    - workspace_abort             # coordinator aborts a workspace
  subject: object                 # the full object awaiting approval
                                  # (task, workspace config, envelope, integration record,
                                  #  conflict context, or abort context)
  workspace: workspace_id         # the workspace context (null for task_approval before binding)
  task_ref: task_id               # the task context, if applicable (null if not task-related)
  graph_ref: task_graph_id        # the task graph context, if applicable
  timestamp: datetime             # runtime-assigned at gate trigger
  timeout: duration               # how long to wait before fallback (null = wait indefinitely)
  fallback: enum
    - approve                     # transition proceeds automatically
    - reject                      # transition is cancelled
    - escalate_to_coordinator     # coordinator decides
```

**Field semantics:**

- **`id`** — globally unique, runtime-assigned. Each gate event is a distinct protocol object with its own trail entry.
- **`type`** — one of six gate types. The set is closed — applications cannot register new gate types through the taxonomy. New gate types require a protocol revision. This is consistent with signals (closed set) and distinct from envelopes and checkpoints (open sets).
- **`subject`** — the complete object awaiting approval. For `task_approval`, this is the full task definition (Task spec, §2). For `workspace_create`, this is the creation parameters (Workspace spec, §4). For `integration`, this is the integration record including source workspace, strategy, and checkpoint reference. The human sees everything needed to make an informed decision.
- **`workspace`** — the workspace context. For `task_approval` gates, this is null — the task has not yet been bound to a workspace. For all other gate types, this identifies the workspace involved in the transition.
- **`task_ref`** — links the gate to a task when applicable. Set for `task_approval` (the task being approved), `workspace_create` (the task the workspace will execute), and `integration` (the task being integrated). Null for gates unrelated to tasks.
- **`graph_ref`** — links the gate to the task graph when applicable. Enables the human to see not just the individual task but its position in the broader work plan.
- **`timestamp`** — assigned by the runtime at gate trigger. The timeout countdown begins at this moment.
- **`timeout`** — how long the system waits for a human response. Null means wait indefinitely — the system blocks until the human responds. This is the only configuration where a missing human can deadlock the system.
- **`fallback`** — the action executed when the timeout expires. Three options: `approve` (proceed automatically — the gate is advisory), `reject` (cancel the transition — the gate is mandatory), or `escalate_to_coordinator` (let the coordinator decide — the gate is delegatable).

**Gate resolution schema:**

```yaml
gate_resolution:
  gate_id: string                 # references the gate event
  action: enum
    - approve
    - reject
    - modify
  modifications: object           # present only when action is modify
                                  # contains the fields altered by the human
  actor: user_id | "fallback" | "protocol"   # user_id when human acts; system token otherwise
  timestamp: datetime             # when the resolution occurred
```

**Resolution semantics:**

- **`approve`** — the transition proceeds exactly as proposed. The subject is unchanged.
- **`reject`** — the transition is cancelled. The effect depends on the gate type:
  - `task_approval` rejected → task transitions to `cancelled` (Task spec, §4)
  - `workspace_create` rejected → workspace is never created; coordinator is notified
  - `envelope_delivery` rejected → envelope transitions to `rejected` with `reason: gate_rejected_by_human`
  - `integration` rejected → workspace transitions to `failed` with `reason: gate_rejected_by_human`
  - `conflict_resolution` rejected → workspace remains `conflicted`; coordinator must select a different strategy
  - `workspace_abort` rejected → abort is cancelled; workspace continues in its current state
- **`modify`** — the human alters specific fields of the subject before approving. The `modifications` object contains only the changed fields. The runtime applies the modifications and proceeds with the transition. Not all fields are modifiable — runtime-assigned fields (`id`, `timestamp`) and structural fields (`graph_ref`, `depends_on`) cannot be modified through gates. Modifiable fields depend on the gate type: budget, priority, visibility grants for `workspace_create`; priority, description for `task_approval`; payload for `envelope_delivery`.
- **`actor`** — identifies who resolved the gate. When a human resolves the gate, `actor` carries their `user_id` (User spec, §2). When the timeout expires, `actor` is `"fallback"`. When the runtime resolves due to subject invalidation (§6.5), `actor` is `"protocol"`. Trail queries filtering on `actor` values that are not `"fallback"` or `"protocol"` return exactly the set of human-resolved gates, with the specific human identified.

**Gate queuing.** Multiple gates may fire in quick succession — for example, when the coordinator submits a full task graph for approval (Task spec, §7, upfront decomposition) and each task triggers a `task_approval` gate. The protocol handles concurrent gates by queuing:

1. Gates are queued FIFO by timestamp. The human sees one pending gate at a time.
2. If a gate's timeout expires while queued (before the human sees it), the fallback executes immediately. The gate is dequeued and the next gate is presented.
3. The trail records queue position: `gate_triggered` entries include a `queue_position` field indicating how many gates were ahead of it.

## 4. Task Approval Gate

The task approval gate is where the highway meets the task primitive. Every task begins in `draft` status (Task spec, §4) — a proposal from the coordinator, awaiting human review. The `draft` → `pending` transition is gated by default: no task becomes actionable without human approval or an explicit timeout fallback.

This gate is structurally different from the other five. The other gates pause transitions that are already in motion — a workspace being created, an envelope being delivered, an integration being performed. The task approval gate pauses a transition that has not yet begun — no workspace exists, no agent is bound, no tokens are consumed. It is the protocol's **planning checkpoint** — the point where the human reviews the coordinator's work plan before any resources are committed.

**What the human sees.** The gate event's `subject` contains the full task definition (Task spec, §2): name, description, dependencies, priority, resource estimate, and position in the task graph. When the coordinator submits a full graph for approval (upfront decomposition, Task spec, §7), each task in the graph triggers its own `task_approval` gate. The `graph_ref` field on the gate event links each task to the broader plan — the human can see not just this task but where it sits in the dependency structure.

**Three responses:**

**Approve** — the task transitions from `draft` to `pending`. It is now eligible for workspace binding (Task spec, §5) once its dependencies are met. A `task_approved` trail entry is recorded with `approval_source: human` (Task spec, §8).

**Reject** — the task transitions from `draft` to `cancelled`. It will never be executed. Dependent tasks that relied solely on this task become unresolvable — the coordinator must cancel them too or restructure the graph. A `task_approved` trail entry is NOT recorded — instead, a `task_status_changed` entry records the `draft` → `cancelled` transition with the gate rejection as the trigger.

**Modify** — the human alters the task before approving. Modifiable fields: `name`, `description`, `priority`, `resource_estimate`. The human cannot modify `depends_on` (dependencies are structural and validated against the graph — modifying them through a gate could introduce cycles or cross-graph references). The human cannot modify `id`, `timestamp`, or `graph_ref` (runtime-assigned or structural). After modification, the task transitions to `pending` with the modified values. The `task_approved` trail entry records `approval_source: human` and the `modifications` field on the gate resolution captures what was changed.

**Batch approval.** When the coordinator submits a full task graph, the human may want to approve all tasks at once rather than one by one. The protocol supports this through the gate queue — each task is a separate gate event, and the human resolves each independently. Implementations may offer a batch interface that lets the human approve, reject, or modify multiple gates in a single action (Task spec, §10, batch approval note). The protocol processes each resolution independently — batch presentation is an interface concern, not a protocol mechanism.

**Timeout behavior.** The task approval gate's timeout and fallback are configured in the workflow's highway block (§5). Three common configurations:

| Configuration | Timeout | Fallback | Use case |
|---------------|---------|----------|----------|
| Mandatory review | null (indefinite) | — | High-stakes work where no task should proceed without human review |
| Advisory review | short (seconds) | approve | Low-stakes work where the human sees the plan but doesn't block progress |
| Supervised review | medium (minutes) | escalate_to_coordinator | The coordinator proceeds if the human is unavailable, but the human is preferred |

**Progressive decomposition and the gate.** When the coordinator decomposes a task mid-execution (Task spec, §7, progressive decomposition), the new subtasks enter as `draft` and trigger their own `task_approval` gates. The human sees scope expansion in real time — new tasks appearing in the graph that were not part of the original plan. This prevents silent scope creep: the coordinator cannot add work without the human's awareness.

**The gate is the protocol's safety valve.** Before any workspace is spawned, before any tokens are consumed, before any agent begins working — a human has seen the plan. The `draft` state ensures that the coordinator's decomposition is reviewed, not rubber-stamped after the fact. This is the most consequential gate in the highway: it controls what work enters the system.

## 5. Autonomy Spectrum

The highway's four capabilities are independently configurable per workflow. The configuration determines where the system sits on the autonomy spectrum — from fully autonomous (no human interaction) to fully gated (every transition requires approval). The workflow's `highway` block is the control surface.

**Highway configuration schema:**

```yaml
highway:
  visibility: boolean               # whether the live trail feed is active (default: true)
  gates:
    task_approval: boolean           # default: true (the only gate enabled by default)
    workspace_create: boolean        # default: false
    envelope_delivery: boolean       # default: false
    integration: boolean             # default: false
    conflict_resolution: boolean     # default: false
    workspace_abort: boolean         # default: false
  gate_defaults:
    timeout: duration                # default timeout for all gates (overridable per gate type)
    fallback: enum                   # default fallback for all gates (overridable per gate type)
  gate_overrides:                    # optional per-gate-type timeout/fallback overrides
    task_approval:
      timeout: duration
      fallback: enum
    integration:
      timeout: duration
      fallback: enum
    # ... other gate types
  injection: boolean                 # whether human injection is enabled (default: true)
  escalation:
    enabled: boolean                 # whether agents can escalate (default: true)
    timeout: duration                # how long to wait for human response
    fallback: enum                   # approve | reject | delegate_to_coordinator
```

**Default behavior.** With no highway configuration, the defaults are: visibility on, `task_approval` gate enabled (all other gates disabled), injection on, escalation enabled. This means the human sees everything, approves the work plan, can inject at any time, and receives escalations — but workspace creation, envelope delivery, integration, and conflict resolution proceed without gates. This is the **supervised** baseline: the human controls what enters the system (task approval) and can intervene at any point (injection), but does not block execution once work begins.

**`task_approval` is the only gate enabled by default.** This is deliberate. The task approval gate is the protocol's planning checkpoint — it controls what work enters the system before any resources are consumed (§4). All other gates control transitions that are already in motion. Defaulting `task_approval` to enabled ensures that human oversight begins at the planning stage, not after the fact.

**Three presets:**

| Preset | Visibility | Gates | Injection | Escalation |
|--------|-----------|-------|-----------|------------|
| **autonomous** | on | task_approval only, short timeout, approve fallback | on | enabled, short timeout, delegate fallback |
| **supervised** | on | task_approval + integration | on | enabled, medium timeout, reject fallback |
| **gated** | on | all gates enabled | on | enabled, no timeout (waits indefinitely) |

**Autonomous.** The system runs with minimal human involvement. Task approval gates fire but fall through quickly on timeout — the human sees the plan but does not block it. Escalations are delegated to the coordinator if the human does not respond. The human can still inject at any time. This preset is for workflows where speed matters more than per-transition oversight.

**Supervised.** The human approves the work plan (task approval) and every merge (integration). Everything else runs autonomously. Escalations wait longer for human response and reject on timeout rather than delegating — the system prefers to stop rather than proceed without human input on escalated issues. This is the recommended starting point for most workflows.

**Gated.** Every transition requires human approval. Nothing moves without the human's consent. Escalations wait indefinitely. This preset is for high-stakes, low-volume workflows where correctness is paramount and speed is secondary. It is the only preset where a missing human can deadlock the system.

**The spectrum is continuous, not discrete.** Presets are starting points, not the only options. A workflow can gate task approval and integration but not workspace creation. It can enable escalation with a long timeout but set all gates to short timeouts. The four capabilities compose independently — any combination is valid. The presets are named configurations for common patterns; the schema supports arbitrary combinations.

**Per-gate-type overrides.** The `gate_overrides` block allows different timeout and fallback settings for different gate types within the same workflow. A workflow might set a long timeout for `task_approval` (the human should carefully review the plan) but a short timeout for `workspace_create` (workspace creation is routine). This granularity lets the workflow designer tune human involvement per transition type, not just globally.

## 6. Edge Cases and Invariants

The highway's interaction with the rest of the protocol creates edge cases that must be handled deterministically. Four situations require explicit rules.

### 6.1 Fallback Loop Prevention

A gate's fallback may be `escalate_to_coordinator`. If the coordinator's response re-triggers the same gate, an infinite loop results. The protocol prevents this with a **gate re-entry guard**.

**Invariant: A gate that was resolved by fallback cannot be re-triggered for the same subject within the same lifecycle.** The runtime tracks `(gate_type, subject_id)` tuples and rejects re-entry. If the coordinator attempts to re-trigger a gate that already fell through, the re-trigger is silently converted to the fallback's secondary action:

- If fallback was `escalate_to_coordinator` and the coordinator re-triggers → treated as `approve`
- If fallback was `approve` → no re-trigger possible (transition already proceeded)
- If fallback was `reject` → no re-trigger possible (transition already cancelled)

A `gate_reentry_blocked` trail event is recorded when the guard fires, for diagnostic visibility.

### 6.2 Injection to Terminal Workspaces

A human may attempt to inject an envelope into a workspace that has reached a terminal state (`closed` or `failed`). The protocol **rejects** such injections — consistent with the envelope validation rules (Envelope spec, §4, rule 2).

**Rules:**
1. Injections to terminal workspaces are rejected with `reason: workspace_terminal`.
2. The attempted injection IS recorded in the trail as `human_injection` with `status: rejected` — the human's intent is preserved for audit even though the action did not execute.
3. The human is notified of the rejection through the interface (application concern).
4. If the human intends to revive work from a terminal workspace, the correct mechanism is to inject a directive into the coordinator's workspace, requesting that a new workspace be created with appropriate context from the failed one.

### 6.3 Injection to Sealed Workspaces

A workspace in `integrating` state has a sealed inbox (Envelope spec, §4). Human injection follows the same rule — the human cannot inject envelopes into a workspace that is being integrated. The envelope is rejected with `reason: workspace_sealed`. The human may inject into the coordinator's workspace instead, influencing the integration decision through feedback.

A workspace in `migrating` state has a frozen inbox — envelopes are queued, not rejected (Envelope spec, §4). Human-injected envelopes to a migrating workspace are queued like any other envelope and delivered when migration completes.

### 6.4 Human Availability

Human availability operates at two levels: **user state** (protocol-level, explicit) and **presence** (interface-level, inferred from timeouts).

**User state** is the protocol's explicit availability model (User spec §3.1). A user who is `suspended`, `blocked`, or `deactivated` is definitively unavailable — the runtime knows this and acts accordingly (§2.3 for injection, §2.4 for escalation routing). User state transitions are recorded in the trail and the coordinator is notified. This is deterministic: the system knows the user's state and gates behavior accordingly.

**Presence** is the interface-level availability model, handled by the **timeout/fallback mechanism**. An `active` user who does not respond to a gate or escalation within the timeout is treated as absent — the fallback executes. This is probabilistic: the system does not know whether the user is watching but not acting, away from the interface, or deliberately waiting. The timeout resolves the ambiguity.

The two levels compose: user state is checked first (is the user able to act?), then presence is inferred by timeout (is the user choosing to act?). A `suspended` user never reaches the timeout — their escalations are queued immediately. An `active` user who ignores a gate reaches the timeout and the fallback fires.

**Guidance for workflow designers:**
- Set short timeouts (seconds) for gates that should not block: the system prefers progress over human input.
- Set long timeouts (minutes to hours) for gates that genuinely require human judgment: the system prefers correctness over speed.
- Set no timeout (`timeout: null`) only in the `gated` preset where human involvement is mandatory. This is the only configuration where a missing human can deadlock the system — use with awareness.
- Consider user state in multi-user deployments: if the primary owner is frequently `suspended` or `blocked`, configure escalation routing to a delegate user or set shorter timeouts with `escalate_to_coordinator` fallback.

### 6.5 Gate During Workspace Failure Race

A gate may be pending when its subject becomes invalid — for example, an `integration` gate is waiting for human approval when the source workspace transitions to `failed` due to a timeout. The protocol handles this deterministically:

1. The gate is automatically resolved with `action: reject` and `actor: protocol` (not `human` or `fallback`).
2. A `gate_resolved` trail entry records the automatic resolution with `reason: subject_invalidated`.
3. The human is notified that the gate is no longer pending.
4. The transition that was paused is cancelled — the system proceeds with the failure path instead.

This prevents stale gates from accumulating — every gate is eventually resolved, whether by human action, timeout fallback, or subject invalidation.

## 7. Interface Boundary

The protocol defines the highway's **capabilities** — what a human can do — and its **semantics** — what happens when they do it. The protocol does not define the **interface** — how the human sees and interacts with the system. This boundary is deliberate and load-bearing.

**What the protocol defines:**
- Gate event schema and resolution options — approve, reject, modify (§3)
- Task approval mechanics — the `draft` → `pending` gate with its three responses (§4)
- Escalation signal semantics and response types — envelope, abort, delegation (§2.4)
- Injection rules — bypasses sender-role check, preserves all other validation (§2.3)
- Visibility scope — full trail access in real time (§2.1)
- Timeout and fallback mechanics — per-gate-type configuration (§3, §5)
- Trail recording of all highway events (§8)
- Edge case handling — deterministic resolution for every edge case (§6)

**What the protocol does not define:**
- Whether the interface is a CLI, a web dashboard, a chat window, or an IDE panel
- How gate events are rendered to the human
- How the human composes injected envelopes
- How the task graph is visualized for task approval
- Notification mechanisms — push notifications, polling, email alerts
- Authentication or authorization of the human — the protocol trusts its boundary; the application authenticates
- Batch presentation of multiple gates — the protocol processes resolutions individually; the interface may present them in batch

**Why this boundary exists.** The highway is a protocol capability; the interface is an application concern. Different applications will implement different interfaces — a research platform may use a web dashboard with graph visualization, a coding tool may use an IDE panel, a devops system may use a chat integration. The protocol ensures they all have the same capabilities and the same trail semantics, regardless of how the human interacts.

**Authentication.** The protocol requires that human actions are authenticated — the trail must record which human performed each action. But the authentication mechanism is application-defined. The protocol defines what identity information must be recorded (the `actor` field in trail entries); the application defines how identity is verified. This is consistent with the security model (PROTOCOL §11) — the protocol specifies requirements, not mechanisms.

**The interface must not violate protocol semantics.** An interface may present gates in batch, but the protocol processes each resolution independently. An interface may let the human compose envelopes with rich formatting, but the envelope must conform to the schema (Envelope spec, §3). An interface may display checkpoints with syntax highlighting, but it must not modify the checkpoint content. The interface is a lens — it shapes how the human sees the system, but it does not change what the system does.

## 8. Trail Events

Every human action through the highway is recorded in the trail as a first-class event. The trail does not distinguish between "important" agent events and "side-channel" human events — all are entries with equal standing. The highway generates eight event types across three categories.

**Gate events:**

| Event | Trigger | Key fields |
|-------|---------|------------|
| `gate_triggered` | A gated transition is paused | `gate_id`, `gate_type`, `subject`, `workspace`, `task_ref`, `graph_ref`, `timeout`, `fallback`, `queue_position` |
| `gate_resolved` | Human approved, rejected, or modified | `gate_id`, `gate_type`, `action` (approve/reject/modify), `modifications` (if modify), `actor` (human/fallback/protocol) |
| `gate_timeout` | No human response; fallback executed | `gate_id`, `gate_type`, `fallback_action`, `elapsed` |
| `gate_reentry_blocked` | Re-entry guard prevented a loop | `gate_id`, `gate_type`, `subject_id`, `converted_to` |

**Injection events:**

| Event | Trigger | Key fields |
|-------|---------|------------|
| `human_injection` | Human sent an envelope via the highway | `envelope_id`, `actor: user_id`, `to`, `type`, `status` (delivered/rejected) |

**Escalation events:**

| Event | Trigger | Key fields |
|-------|---------|------------|
| `escalation_received` | Escalation signal delivered to highway | `signal_id`, `workspace`, `reason` |
| `escalation_resolved` | Human responded to escalation | `signal_id`, `workspace`, `actor: user_id`, `response_type` (envelope/abort/delegation), `response_ref` (envelope_id if envelope) |
| `escalation_timeout` | No human response to escalation; fallback executed | `signal_id`, `workspace`, `fallback_action`, `elapsed` |

**Event semantics:**

- **`gate_triggered`** and **`gate_resolved`** are paired — every triggered gate is eventually resolved, whether by human action (`actor: human`), timeout fallback (`actor: fallback`), or subject invalidation (`actor: protocol`, §6.5). The pair forms a complete record of the gate's lifecycle.

- **`gate_timeout`** is recorded in addition to `gate_resolved` when the timeout fires — it captures the timing detail (`elapsed`) separately from the resolution action. The `gate_resolved` entry records `actor: fallback`; the `gate_timeout` entry records the timeout mechanics.

- **`human_injection`** records both successful and rejected injections. The `status` field distinguishes them. A rejected injection (§6.2, §6.3) has `status: rejected` — the human's intent is preserved even though the action did not execute.

- **`escalation_received`** and **`escalation_resolved`** are paired like gate events. The `response_type` field distinguishes the three response kinds: `envelope` (feedback sent to workspace), `abort` (workspace failed by human), `delegation` (escalation delegated to coordinator). When the response is an envelope, `response_ref` carries the envelope ID — linking the escalation trail to the envelope trail.

- **`escalation_timeout`** is recorded when the human does not respond to an escalation within the configured window. The fallback action executes and the `escalation_resolved` entry records `actor: fallback`.

**The `actor` field.** All highway trail entries use the `actor` field to identify the source. For human actions, `actor` carries the acting human's `user_id` (User spec, §2) — not a generic "human" marker, but the specific identity. For automated actions, `actor` is a system token: `"fallback"` (timeout expired, fallback executed) or `"protocol"` (runtime resolved due to subject invalidation, §6.5). Trail queries filtering on `actor` values that are not system tokens return exactly the set of human actions, with the specific human identified. This supports multi-user audit: "what did user X approve?" is a direct trail query, not an application-layer join.

**No payload duplication.** Like other trail events in the protocol, highway trail entries reference objects by ID rather than duplicating their content. A `gate_triggered` entry references the subject by ID; a `human_injection` entry references the envelope by ID. The trail records what happened, not the full content of what was involved.

## 9. Conformance Requirements

Three tiers: **Core** (minimum viable highway — visibility and task approval), **Standard** (full gate system and escalation), **Full** (injection, edge case handling, and configuration granularity).

**Core (7 requirements):**

| # | Requirement |
|---|-------------|
| C-1 | Visibility MUST provide real-time access to the global trail. All protocol events MUST be observable by the human (§2.1). |
| C-2 | The `task_approval` gate MUST be supported. Tasks in `draft` status MUST trigger a gate event before transitioning to `pending` (§4). |
| C-3 | Gate events MUST follow the schema defined in §3. All fields MUST be present; `id` and `timestamp` runtime-assigned. |
| C-4 | Gate resolution MUST support three actions: approve, reject, modify (§3). |
| C-5 | Gate timeout MUST be enforced. When the configured timeout expires, the fallback action MUST execute automatically (§3). |
| C-6 | Every gate MUST be eventually resolved — by human action, timeout fallback, or subject invalidation (§6.5). No gate may remain pending indefinitely unless `timeout: null` is configured. |
| C-7 | All highway events MUST be recorded in the trail per §8. Gate events MUST be paired (`gate_triggered` → `gate_resolved`). |
| C-8 | All human highway actions MUST carry the acting human's `user_id` in the `actor` field of trail events (User spec, §2). No highway event attributed to a human MAY be anonymous. |

**Standard (9 requirements):**

| # | Requirement |
|---|-------------|
| S-1 | All six gate types MUST be supported: `task_approval`, `workspace_create`, `envelope_delivery`, `integration`, `conflict_resolution`, `workspace_abort` (§2.2). |
| S-2 | Gates MUST be independently configurable per workflow through the highway configuration schema (§5). |
| S-3 | Escalation signals MUST activate the highway in addition to normal upward propagation (§2.4). |
| S-4 | Escalation handling MUST support three response types: envelope, abort, delegation (§2.4). |
| S-5 | Escalation timeout MUST be enforced. When the configured timeout expires, the fallback action MUST execute (§2.4, §5). |
| S-6 | Gate queuing MUST be FIFO by timestamp. Gates that timeout while queued MUST have their fallback executed immediately (§3). |
| S-7 | The fallback loop prevention guard MUST be enforced. Re-triggered gates MUST be converted per §6.1. |
| S-8 | Subject invalidation MUST automatically resolve pending gates with `actor: protocol` (§6.5). |
| S-9 | Escalation signals MUST route to the workspace's owner by default (User spec, §4.1). Deployment MAY override routing with domain-specific rules. |
| S-10 | User state MUST gate highway interactions. Injection MUST be rejected for non-active users (§2.3). Escalation delivery MUST follow the user state routing table: delivered for `active`, queued for `suspended`/`blocked`, rejected for `deactivated` (§2.4). |

**Full (5 requirements):**

| # | Requirement |
|---|-------------|
| F-1 | Injection MUST be supported. Human-injected envelopes MUST bypass role-based send restrictions while preserving all other validation (§2.3). |
| F-2 | Injected envelopes MUST carry `origin: human` (Envelope spec, §3). The field MUST be immutable and system-assigned. |
| F-3 | Rejected injections (to terminal or sealed workspaces) MUST be recorded in the trail with `status: rejected` (§6.2, §6.3). |
| F-4 | Per-gate-type overrides MUST be supported in the highway configuration (§5). |
| F-5 | The three presets (autonomous, supervised, gated) MUST be available as named configurations (§5). |

**23 requirements total.** Core ensures the human can see the system and approve the work plan. Standard ensures full gate coverage, escalation handling, user state gating, and deterministic edge case resolution. Full ensures injection capability and configuration granularity.

## 10. Implementation Notes

*This section is non-normative. It captures practical guidance for implementers — approaches that have proven useful but are not protocol requirements.*

**Gate as a state machine.** Each gate is a mini state machine: `pending` → `resolved` (by human, fallback, or protocol). Implementations should track pending gates in a queue keyed by gate ID, with a scheduled timer for each gate's timeout. When the timer fires, the fallback executes and the gate transitions to resolved. When the human responds, the timer is cancelled and the gate transitions to resolved. The gate is never in an ambiguous state — it is always either pending or resolved.

**Visibility as a trail subscription.** The simplest visibility implementation is a subscription to the global trail's append stream. The interface subscribes at startup and receives new entries as they are written. For web-based interfaces, Server-Sent Events or WebSockets provide the push mechanism. For CLI interfaces, tailing the trail file is sufficient. The protocol does not prescribe the subscription mechanism — it guarantees the data is available.

**Injection as envelope creation with elevated permissions.** Injection reuses the envelope creation path (Envelope spec, §5) with one modification: the permission check for sender role is skipped. All other validation remains. Implementations should implement injection as a flag on the envelope creation path (`origin: human` → skip role check) rather than as a separate code path. This ensures injected envelopes follow the same lifecycle as agent envelopes — validation, trail recording, delivery, acknowledgment.

**Gate queue as a priority queue.** Although the protocol specifies FIFO ordering by timestamp, implementations may benefit from a priority queue that surfaces urgent gates first. A `task_approval` gate for an `urgent` task could be presented before a `workspace_create` gate for a `normal` task, even if the workspace gate was triggered first. This is an interface-level optimization — the protocol processes resolutions in the order they arrive, regardless of presentation order.

**Batch task approval interface.** When a full task graph is submitted for approval (Task spec, §7, upfront decomposition), each task triggers its own gate. A naive interface would present them one at a time. A better interface presents the full graph with approve/reject/modify controls per task, plus an "approve all" action. The protocol processes each resolution independently — the interface batches the presentation but sends individual resolutions. This is the most impactful interface optimization for the highway, as task graphs may contain tens of tasks.

**Escalation notification.** Escalation signals require human attention — unlike gates, which may fall through on timeout, escalations represent an agent that is stuck. Implementations should use the most intrusive notification available for the interface: push notifications, audible alerts, or prominent UI indicators. A gate that times out proceeds or cancels; an escalation that times out may result in a failed workspace and wasted work.

**Timeout precision.** Gate timeouts are durations, not deadlines. The runtime converts the duration to a deadline at trigger time (`deadline = trigger_timestamp + timeout`). For queued gates, the timeout starts at trigger time, not at presentation time — a gate may expire before the human ever sees it. Implementations should display the remaining time to the human, not the original duration, to avoid confusion when gates have been queued.

**Human identity in trail entries.** The protocol requires that highway trail entries carry the acting human's `user_id` (User spec, §2). The `user_id` is assigned at the authentication boundary (User spec, §2.1) and carried into every highway action. Implementations must provide the authenticated `user_id` to the runtime when resolving gates, injecting envelopes, or responding to escalations. The `actor` field on highway trail entries is typed — it carries a `user_id` for human actions and a system token (`"fallback"`, `"protocol"`) for automated actions. No highway event is anonymous — every human action is attributed.

## 11. References

This spec depends on seven WACP specs. Cross-references are inline throughout; this section collects them for navigability.

| Spec | Sections referenced | Relationship |
|------|-------------------|--------------|
| [Workspace](../primitives/workspace.md) | §4 (creation parameters), §13 (trail events) | Gates pause workspace transitions; injection targets workspaces |
| [Signal](../primitives/signal.md) | §2 (escalation type), §3 (propagation rule 5), §8 (trail events) | Escalation signals activate the highway |
| [Envelope](../primitives/envelope.md) | §3 (`origin` field), §4 (validation rules, sealed inbox), §5 (creation path), §10 (trail events) | Injection creates envelopes; gates may pause delivery |
| [Checkpoint](../primitives/checkpoint.md) | §7 (trail events) | Visibility includes checkpoint creation |
| [Task](../primitives/task.md) | §2 (task definition), §4 (`draft` → `pending` transition), §5 (workspace binding), §7 (decomposition), §8 (trail events), §10 (batch approval) | Task approval gate is the primary planning checkpoint |
| [Integration](../mechanisms/integration.md) | §8 (trail events) | Integration gate pauses merge transitions |
| [User](../primitives/user.md) | §2 (`user_id`), §3 (state model, §3.1 states), §4.1 (ownership, escalation routing), §5.1 (`originator`) | User state gates injection and escalation; `actor` field traces human identity; `originator` traces causal origin |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
