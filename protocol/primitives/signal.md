# WACP: Signal

## Metadata

```yaml
title: "WACP: Signal"
id: wacp-spec-signal
type: constituent-spec
tier: abstract
category: primitives
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4.3 (signal types, propagation)
depends_on:
  - wacp-spec-workspace
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, signal, propagation, lifecycle, coordination, escalation]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Signal Types](#2-signal-types)
3. [Propagation](#3-propagation)
4. [Delivery Guarantees](#4-delivery-guarantees)
5. [Schema](#5-schema)
6. [Compound Patterns](#6-compound-patterns)
7. [Resource Signals](#7-resource-signals)
8. [Trail Events](#8-trail-events)
9. [Conformance Requirements](#9-conformance-requirements)
10. [Implementation Notes](#10-implementation-notes)
11. [References](#11-references)

---

## 1. Purpose

Signals are the protocol's notification primitive. Where envelopes carry content, signals carry state. They are the minimum viable unit of coordination — just enough information to tell the parent workspace what happened, without burdening it with detail it did not ask for. Signals also drive task state transitions — the runtime maps workspace signals to task lifecycle changes automatically (PROTOCOL §4.6).

An agent completing its work emits a signal, not an envelope. An agent that cannot continue emits a signal, not an envelope. An agent that needs human input emits a signal, not an envelope. The distinction is structural: signals declare what happened; envelopes explain what to do. Signals propagate upward through the workspace tree; envelopes are addressed to specific workspaces. Signals are cheap, fast, and typed; envelopes carry structured payloads with permission validation.

The protocol defines eleven base signal types across four categories. The signal set is closed — applications cannot register new signal types through the taxonomy. This is deliberate: every signal maps to a workspace state transition or a defined protocol operation. A signal the runtime does not recognize would have no defined effect. Domain-specific information is carried through the `reason` and `ref` fields, not through new signal types. If richer notification is needed, that is what envelopes are for.

## 2. Signal Types

Eleven signals, four categories. Each signal has a defined emitter set (Roles spec §6) and a defined effect on the workspace state machine (Workspace spec §3).

**Lifecycle signals** — track workspace state transitions. These are universal: every workspace, regardless of role, emits them as part of the state machine.

| Signal | Emitter | Meaning | State effect |
|--------|---------|---------|--------------|
| `ready` | All roles | Workspace initialized, waiting for input | Confirms `idle` state |
| `started` | All roles | Agent has begun or resumed working | Triggers `blocked` → `active` |
| `complete` | Worker, Observer (not Coordinator) | Agent considers its work finished | Triggers `active` → `integrating` |
| `failed` | All roles, Runtime | Unrecoverable error | Triggers → `failed` (terminal) |

`ready`, `started`, and `failed` are emitted by all roles including the coordinator — the coordinator's workspace goes through the same lifecycle as any other. `complete` is not emitted by the coordinator — its closure is governed by the system lifecycle (Workspace spec §12), not by the `complete` signal. `failed` may be emitted by an agent (unrecoverable error), by the coordinator (abort), or by the runtime (timeout, budget exceeded). The signal semantics are identical regardless of emitter; the `actor` field in the trail entry (PROTOCOL §9.1) distinguishes the source.

**Coordination signals** — facilitate interaction between workspaces without requiring a full envelope exchange:

| Signal | Emitter | Meaning | State effect |
|--------|---------|---------|--------------|
| `blocked` | Worker (and derived roles) | Cannot continue, reason attached | Triggers `active` → `blocked` |
| `checkpoint` | Worker (and derived roles) | New checkpoint available for reading | None (stays `active`) |
| `integrate` | Coordinator | Merge operation initiated | Marks integration start |
| `acknowledged` | Runtime | Envelope received by target | None (delivery confirmation) |
| `suspend` | Coordinator | Workspace paused to reclaim resources or reprioritize | Triggers → `suspended` (Workspace spec §3) |
| `migrate` | Coordinator | Agent migration initiated | Triggers → `migrating` (Workspace spec §11) |

`acknowledged` is emitted by the runtime, not by an agent — it is the protocol's delivery receipt. The agent does not choose to acknowledge; the runtime confirms delivery automatically.

**Escalation signal** — bridges the agent layer and the human layer:

| Signal | Emitter | Meaning | State effect |
|--------|---------|---------|--------------|
| `escalation` | Worker, Observer (and derived roles) | Agent needs human input to proceed | Activates the human highway |

An `escalation` signal requires a `reason` field explaining what the agent needs. It follows normal upward propagation and additionally activates the human highway (PROTOCOL §8) — the emitting agent's workflow pauses until a human responds or the escalation times out. The coordinator does not emit escalation signals — it is the root of the workspace tree and has no parent to escalate to. If the coordinator needs human input, it activates the human highway directly through a gate.

The escalation's destination is ultimately a user — the workspace owner or a designated human (Human Highway spec). The user's state gates what happens at the human boundary: if the user is `active`, the escalation is delivered; if `suspended` or `blocked`, it is queued (the user is expected to return); if `deactivated`, it is rejected (User spec §3.1). The signal spec defines propagation to the highway; the User spec and Human Highway spec define what happens when the escalation reaches the human.

**Signals drive task state.** Workspace signals are the mechanism through which task lifecycle transitions occur (PROTOCOL §4.6). When a workspace emits `started`, the bound task transitions to `in_progress`. When it emits `complete`, the task transitions to `completed`. When it emits `failed`, the task transitions to `failed`. There are no task-specific signal types — the existing eleven signals are sufficient. The runtime maps workspace signals to task transitions automatically, keeping the signal set closed.

**Signal count is fixed at eleven.** No mechanism exists to add, remove, or modify signal types. The `reason` field (free-form string) and `ref` field (reference to a related object) carry domain-specific information within the fixed types. This is the protocol's equivalent of Unix signals — a closed set where richer communication uses a different primitive.

## 3. Propagation

Signals propagate upward through the workspace tree. An agent emits a signal within its workspace. The runtime delivers it to the parent workspace — if one exists. This is not addressing — it is propagation. The distinction matters: an envelope is routed to a target by workspace ID; a signal is emitted from a source and flows upward by structure.

**Five propagation rules:**

**Rule 1: Signals are delivered to the immediate parent.** A child workspace's signal reaches its parent — whether that parent is the root coordinator or a delegate. The root workspace is the exception: it has no parent, so its signals are recorded in the global trail but delivered to no one (see rule 4). The signal also appears in the global trail regardless of tree depth, so the root coordinator sees all signals even from deeply nested subtrees.

**Rule 2: Signals do not propagate downward.** The coordinator cannot signal a child through the signal system. Downward communication uses envelopes. This asymmetry is intentional: upward signals are lightweight status reports; downward messages carry instructions that require structure.

**Rule 3: Signals do not propagate sideways.** Sibling workspaces are invisible to each other. A worker completing its work does not notify another worker. Only the parent (coordinator or delegate) sees the full picture and decides what to communicate to whom.

**Rule 4: The root workspace's signals are recorded, not delivered.** The coordinator's workspace has no parent (`parent: null`). Its signals — `ready`, `started`, `failed`, `integrate`, `acknowledged` — are recorded in the global trail as audit markers. They serve observability and recovery, not communication.

**Rule 5: Escalation signals additionally activate the human highway.** An `escalation` signal follows normal upward propagation (rules 1–3) and is recorded in the trail. Additionally, the runtime flags the escalation for human attention through the highway mechanism (PROTOCOL §8). This dual delivery — to the parent and to the human — is the only case where a signal reaches two destinations.

**Propagation and delegation.** When a delegate's child emits a signal, the delegate receives it as the immediate parent (rule 1). The delegate may act on it — for example, initiating integration of the child's work. The signal is also recorded in the global trail, making it visible to the root coordinator. The delegate does not "forward" the signal — the trail provides global visibility without explicit forwarding.

## 4. Delivery Guarantees

Signals are simpler than envelopes and carry stronger delivery properties.

**At-least-once delivery.** The runtime guarantees that every signal reaches the parent workspace's trail. If the parent is in a state where it cannot immediately process signals (e.g., it is itself in `migrating` or `blocked`), the signal is queued and delivered when the parent resumes. A signal is never silently dropped — it either reaches the parent or is recorded in the global trail as undeliverable (parent in terminal state).

**No acknowledgment.** Signals are not acknowledged. The `acknowledged` signal is the protocol's delivery receipt for envelopes, not for signals. Acknowledging a signal with another signal would create infinite recursion. The runtime guarantees delivery; the sender trusts the guarantee and moves on.

**Ordering within a workspace.** Signals from a single workspace are delivered in emission order (Workspace spec §7, invariant 4). If an agent emits `checkpoint` followed by `complete`, the parent receives them in that order. The runtime never reorders signals from the same source.

**No ordering across workspaces.** Signals from different workspaces carry no ordering guarantee relative to each other. The coordinator may receive `complete` from workspace A before `started` from workspace B, even if B started first. Cross-workspace ordering is reconstructed from trail timestamps when needed (Clock spec §4), not from delivery order.

**Signals to terminal parents.** If a parent workspace reaches a terminal state (`closed` or `failed`) while a child's signal is in flight, the signal is recorded in the global trail but cannot be delivered. This is not an error — it is a consequence of the parent's lifecycle ending. The child's signal is preserved for audit; it simply has no recipient to act on it.

## 5. Schema

Every signal carries the same structure. There are no signal-type-specific fields — the schema is uniform across all eleven types.

```yaml
signal:
  id: string                      # runtime-assigned, globally unique (Identity spec §2, rule 1; Identity spec §3)
  from: workspace_id              # the emitting workspace
  type: enum                      # one of the eleven fixed types
    - ready
    - started
    - blocked
    - checkpoint
    - complete
    - failed
    - integrate
    - acknowledged
    - escalation
    - suspend
    - migrate
  reason: string                  # required for: blocked, failed, escalation
                                  # optional for: all others
  ref: string                     # optional reference to a related object —
                                  # checkpoint ID, envelope ID, or workspace ID
                                  # depending on context
  timestamp: datetime             # runtime-assigned at emission (Clock spec §3, invariant 1)
  delivered_to: workspace_id      # set by runtime upon delivery (null for root workspace signals)
  delivered_at: datetime          # set by runtime upon delivery or trail recording
```

**Field semantics:**

- **`id`** — globally unique, runtime-assigned (Identity spec §2, rule 1). Never reused (Identity spec §2, rule 6).
- **`from`** — the workspace that emitted the signal. Always present — there are no anonymous signals (Identity spec §4).
- **`type`** — one of exactly eleven values. The enum is closed.
- **`reason`** — explains why the signal was emitted. Required for `blocked` (what is blocking), `failed` (what went wrong), and `escalation` (what the agent needs from a human). Optional for all other types — agents may include a reason for richer trail analysis, but the protocol does not require it.
- **`ref`** — points to a related protocol object. For `checkpoint` signals, this is the checkpoint ID. For `acknowledged`, this is the envelope ID. For `escalation`, this may reference the directive or context that prompted the escalation. The protocol does not prescribe which object `ref` points to for each signal type — it is a general-purpose reference field.
- **`timestamp`** — assigned by the runtime at emission, not by the agent (Clock spec §3). Monotonic within the emitting workspace.
- **`delivered_to`** — set by the runtime when the signal reaches its destination. For root workspace signals (propagation rule 4), this is null — no recipient exists.
- **`delivered_at`** — set by the runtime when the signal is processed. For delivered signals, this is the delivery timestamp. For root workspace signals, this is the timestamp when the signal was recorded in the global trail. Never null — every signal is processed at a definite moment.

## 6. Compound Patterns

Some workflows produce predictable signal sequences. These are not distinct signal types — they are patterns that emerge from the state machine. Recognizing them enables smarter coordinator decisions.

**Healthy work cycle:**
`ready → started → checkpoint → checkpoint → complete`

The most common pattern. The agent initializes, works, produces intermediate checkpoints, and completes. The number of `checkpoint` signals varies — some tasks produce one, others produce many.

**Blocked-then-recovered:**
`ready → started → checkpoint → blocked → started → complete`

The agent hits an obstacle, emits `blocked` with a reason, receives feedback from the coordinator, and resumes. The second `started` signal marks the resumption. Multiple blocked-recovered cycles are possible within a single workspace.

**Immediate failure:**
`ready → failed`

The agent fails before producing any work. Common causes: malformed directive, missing context, incompatible agent capabilities. The trail preserves the failure reason for diagnosis.

**Escalation-then-resumed:**
`ready → started → escalation → started → complete`

The agent needs human input, escalates through the human highway, receives a response, and resumes. The `escalation` signal activates the highway; the subsequent `started` signal indicates the human responded and the agent continues.

**Migration mid-work:**
`ready → started → checkpoint → [migrating] → started → complete`

The agent is replaced mid-task. The `migrating` state is a state transition triggered by the `migrate` signal (Workspace spec §11). The new agent emits `started` when it resumes, producing a signal gap that the trail fills with `migration_started` and `migration_completed` events.

The coordinator does not need to be programmed to handle each pattern explicitly. But pattern recognition in the trail enables smarter decisions — for example, an agent that frequently emits `blocked` signals may indicate a poorly scoped directive.

## 7. Resource Signals

The protocol does not add signal types for resource events — the eleven base signals are sufficient. Resource situations map to existing signals through the `reason` field.

| Situation | Signal | Reason | Initiated by |
|-----------|--------|--------|-------------|
| Agent approaching budget limit | `blocked` | `budget_warning: {dimension, consumed, limit}` | Agent (cooperative) |
| Agent hit hard budget limit | `failed` | `budget_exceeded: {dimension, consumed, limit}` | Runtime (mandatory) |
| Agent stalled (no trail activity) | *(none)* | — | Runtime records `liveness_warning` trail entry |
| Workspace creation rejected | *(none)* | — | Runtime records `workspace_rejected` trail entry |

A budget-aware agent that detects it is approaching its token limit should emit `blocked` with a budget-related reason. This gives the coordinator the opportunity to respond — grant more budget, send feedback to wrap up, or abort. If the agent does not self-report, the runtime enforces the hard limit regardless, transitioning the workspace to `failed` with `reason: budget_exceeded`.

Liveness warnings and admission rejections are not signals — they are trail entries that the coordinator reads. This is consistent with the signal design: signals declare *agent* state changes, while liveness and admission are *system* observations about agents. The boundary is clear: agents emit signals; the runtime writes trail entries.

## 8. Trail Events

Every signal produces exactly one trail entry at emission, and one at delivery. The signal system generates no other event types.

| Event | When | Body |
|-------|------|------|
| `signal_emitted` | Agent or runtime emits a signal | `signal_id`, `from`, `type`, `reason`, `ref`, `timestamp` |
| `signal_delivered` | Runtime delivers signal to parent | `signal_id`, `from`, `delivered_to`, `delivered_at` |

**Two entries per signal, not one.** Emission and delivery are separate events because they happen at different times, in different workspaces, and may be separated by queuing. The emission entry is written to the emitting workspace's local trail. The delivery entry is written to the receiving workspace's local trail. Both appear in the global trail. For root workspace signals (propagation rule 4), only the emission entry is produced — there is no delivery entry because there is no recipient.

**The trail is the signal's permanent record.** Once a signal is emitted and recorded, it exists in the trail regardless of what happens next. If the parent fails before processing the signal, the emission entry survives. If recovery reconstructs state, it replays signal entries to rebuild the coordination picture.

**No separate rejection event.** Signals are not subject to permission validation in the same way envelopes are — the runtime enforces signal permissions at emission time (Roles spec §6). If an agent attempts to emit a signal its role does not permit, the emission is rejected and a `permission_denied` trail entry is recorded. This uses the existing permission denial mechanism, not a signal-specific event type.

## 9. Conformance Requirements

| Requirement | Level | Description |
|-------------|-------|-------------|
| Eleven signal types | Core | Runtime MUST implement exactly the eleven signal types defined in §2. The set is closed. |
| Signal schema | Core | Every signal MUST carry all fields defined in §5. `reason` MUST be present for `blocked`, `failed`, and `escalation`. |
| Upward propagation | Core | Signals MUST propagate to the immediate parent workspace. Signals MUST NOT propagate downward or sideways. |
| At-least-once delivery | Core | Runtime MUST guarantee that every signal reaches the parent workspace's trail. No silent drops. |
| Emission ordering | Core | Signals from a single workspace MUST be delivered in emission order. |
| No acknowledgment | Core | Signals MUST NOT be acknowledged. The `acknowledged` signal is for envelopes only. |
| Runtime-assigned fields | Core | `id`, `timestamp`, `delivered_to`, and `delivered_at` MUST be assigned by the runtime, never by agents. |
| Closed signal set | Core | Runtime MUST reject any signal with a `type` value not in the eleven defined types. |
| Trail recording | Core | Every signal emission MUST produce a `signal_emitted` trail entry. Every signal delivery MUST produce a `signal_delivered` trail entry. |
| Permission enforcement | Standard | Runtime MUST enforce signal emission permissions per the Roles spec. Unauthorized emissions MUST be rejected and recorded. |
| Escalation highway activation | Standard | `escalation` signals MUST activate the human highway (PROTOCOL §8) in addition to normal upward propagation. |
| Root workspace signals | Standard | Root workspace signals MUST be recorded in the global trail with `delivered_to: null` and `delivered_at` set to the trail recording timestamp. |
| Reason field for resource signals | Full | Budget-aware agents SHOULD emit `blocked` with a budget-related reason when approaching limits, enabling cooperative resource management. |

## 10. Implementation Notes

These notes are non-normative. They capture practical guidance for implementers.

**Signal as a lightweight struct.** A signal is a fixed-size structure — eleven possible types, a handful of fields, no variable-length payload beyond `reason` and `ref`. Implementations should represent signals as plain structs or named tuples, not as a generic message type. The fixed schema means serialization and deserialization are trivial.

**Propagation as a tree walk.** The simplest propagation implementation walks the workspace tree from the emitting workspace to its parent. For flat trees (all children directly under the coordinator), this is a single hop. For delegated subtrees, the walk is longer but bounded by tree depth. The global trail receives a copy of every signal regardless of tree depth — propagation and trail recording are parallel operations, not sequential.

**Signal queue per workspace.** Each workspace maintains an inbound signal queue. The runtime appends signals as they arrive; the agent (or coordinator) consumes them. For the coordinator, this queue is the primary coordination input — it reads signals to learn what its children are doing. The queue is ordered by delivery timestamp (Clock spec §3, invariant 1 within the receiving workspace).

**Emission permission check.** Before recording a signal, the runtime checks the emitting workspace's role against the signal permissions (Roles spec §6). This is a single lookup: `(role, signal_type) → allow | deny`. For derived roles, the effective permission set is precomputed at workspace creation (Roles spec §5) — no inheritance resolution at emission time.

**Escalation dual delivery.** The `escalation` signal requires two actions: normal upward propagation (to the parent workspace) and human highway activation (PROTOCOL §8). Implementations should treat these as two independent side effects of the same emission event. If the highway is not configured or not available, the signal still propagates normally — the escalation is recorded in the trail and the coordinator can handle it without human intervention.

**Pattern detection.** Compound patterns (§6) can be detected by querying the trail for signal sequences filtered by workspace ID. A simple sliding window over a workspace's signal history is sufficient. Implementations may offer pattern-matching APIs for coordinator agents, but this is a convenience — the trail contains all the data needed for pattern recognition.

## 11. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §4.3 | §1–§10 | Signal primitive (defines this spec) — types, propagation, closed set |
| §4.6 | §1, §2 | Task primitive — signal-to-task state mapping |
| §8 | §2, §3, §9, §10 | Human highway — escalation signal activates highway |
| §9.1 | §2, §8 | Trail entry schema — `actor` field distinguishes signal source |

### Constituent Specs

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Clock spec | §3, §4 | §4, §5, §10 | Clock invariants — monotonic timestamps within workspace; causal ordering across workspaces |
| Identity spec | §2, §3, §4 | §5 | Identifier rules — runtime-assigned (rule 1), non-recyclable (rule 6), globally unique scope; no anonymous signals |
| Roles spec | §5, §6 | §2, §8, §10 | Role inheritance — precomputed permissions; signal emission permissions per role |
| Workspace spec | §3, §7, §11, §12 | §2, §4, §6, §8 | State machine — signal-triggered transitions; concurrency invariant 4 (emission ordering); migration; system lifecycle |
| User spec | §3.1 | §2 | User state gates escalation delivery — active (delivered), suspended/blocked (queued), deactivated (rejected) |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
