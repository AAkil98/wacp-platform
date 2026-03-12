# WACP: Checkpoint

## Metadata

```yaml
title: "WACP: Checkpoint"
id: wacp-spec-checkpoint
type: constituent-spec
tier: abstract
category: primitives
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §7.1–§7.2 (checkpoint anatomy, checkpoint chain)
depends_on:
  - wacp-spec-workspace
  - wacp-spec-signal
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, checkpoint, progress, artifact, immutability, chain]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Checkpoint Types](#2-checkpoint-types)
3. [Schema](#3-schema)
4. [Checkpoint Rules](#4-checkpoint-rules)
5. [Checkpoint Chains](#5-checkpoint-chains)
6. [Resource Tracking](#6-resource-tracking)
7. [Trail Events](#7-trail-events)
8. [Conformance Requirements](#8-conformance-requirements)
9. [Implementation Notes](#9-implementation-notes)

---

## 1. Purpose

Checkpoints are the protocol's progress primitive. Where envelopes carry communication and signals carry state, checkpoints carry work product. They are immutable records of what an agent has produced — code, reports, datasets, plans, observations — captured at a specific moment with explicit intent and confidence.

A checkpoint is not a save point. It is a declaration: "here is what I have produced, here is why, and here is how confident I am." The agent creates checkpoints as it works; the coordinator reads them to understand progress, evaluate output, and decide what to integrate. Checkpoints accumulate in a linear chain within each workspace — each one building on the last, none ever modified.

The protocol defines two base checkpoint types: `artifact` (the agent produced something) and `observation` (the agent recorded something it noticed). Applications register additional types through the taxonomy — a reviewer derived role might introduce `review`, a decision-making role might introduce `decision`. The type system is open, like envelopes and unlike signals.

Checkpoints serve three audiences. The **coordinator** reads the final checkpoint at integration time — it is the workspace's deliverable. The **trail** preserves every checkpoint for audit, recovery, and analysis — the full chain of revisions, not just the final answer. The **human** reads checkpoints through the highway to evaluate agent work, approve gates, or understand what happened in a failed workspace. When the workspace executes a task (Task spec, §5), the final checkpoint also becomes the task's deliverable — the task's `checkpoint_ref` points to it, linking the work product to the unit of work that produced it.

## 2. Checkpoint Types

The protocol defines two base checkpoint types. These cover the two fundamental things an agent does: produce and observe.

| Type | Purpose | Example |
|------|---------|---------|
| `artifact` | The agent produced something | Code, a report, a dataset, a plan, a configuration |
| `observation` | The agent recorded something it noticed without producing an artifact | A pattern detected in data, an anomaly in logs, a dependency discovered during analysis |

Two types, one distinction. An `artifact` checkpoint contains work product — something the coordinator can integrate into the parent workspace. An `observation` checkpoint contains information — something the coordinator can read and act on but does not merge as an artifact. Both are immutable, both carry intent and confidence, both appear in the trail.

**Why two and not more.** The base types map to the two base capabilities of producing roles: workers produce artifacts, observers produce observations. The coordinator does not create checkpoints — it reads and integrates them. Adding base types for every possible output category (review, decision, analysis, summary) would couple the protocol to specific domains. Instead, the protocol provides two structural categories and lets the taxonomy specialize.

**Extensibility through the taxonomy.** Applications register additional checkpoint types alongside the derived roles that produce them. Each new type specifies:
- A name (unique within the taxonomy)
- Which roles may create it
- Any required payload fields beyond the base schema

For example, a `reviewer` derived role (Roles spec, §5) introduces `review` — a checkpoint containing evaluation results. A decision-making derived role might introduce `decision` — a checkpoint recording a choice that affects downstream work. These types are registered in the taxonomy and validated at runtime — an unregistered type is rejected.

**The `type` field is a string, not a closed enum.** Like envelope types (Envelope spec, §2) and unlike signal types (Signal spec, §2), checkpoint types are open. The runtime validates checkpoint types against the taxonomy at creation time. The two base types are always available; taxonomy-registered types are available when the taxonomy is loaded.

## 3. Schema

Every checkpoint carries the same structure. The schema is uniform across all checkpoint types — base and taxonomy-registered alike.

```yaml
checkpoint:
  id: string                      # runtime-assigned, globally unique (Identity spec, rule 1)
  workspace: workspace_id         # the workspace that created this checkpoint
  type: string                    # base types: artifact, observation
                                  # extensible via taxonomy
  payload:
    files_changed: list           # paths relative to workspace root
    content: string               # summary or full content, depending on type
    artifacts: list               # references to produced files or objects
  intent: string                  # why this checkpoint exists, in the agent's words
  parent: checkpoint_id           # the checkpoint this builds upon (null for first)
  status: enum
    - provisional                 # work in progress, may be superseded
    - final                       # agent considers this complete
  confidence: enum
    - high                        # agent believes this is correct
    - medium                      # agent has reservations (noted in intent)
    - low                         # agent is uncertain, requests feedback
  resource_usage:                 # optional (§6)
    tokens_consumed: integer      # input + output tokens since previous checkpoint
    wall_time: duration           # elapsed active time since previous checkpoint
    cost: number                  # monetary cost (implementation-defined unit)
  timestamp: datetime             # runtime-assigned at creation (Clock spec, invariant 1)
```

**Field semantics:**

- **`id`** — globally unique, runtime-assigned (Identity spec, rule 1). Never reused (rule 6). Monotonic within the workspace — later checkpoints have later IDs.
- **`workspace`** — the workspace that created this checkpoint. Always present — there are no orphan checkpoints (Identity spec, §4).
- **`type`** — identifies the checkpoint type. Base types (`artifact`, `observation`) are defined by the protocol. Taxonomy-registered types are validated at runtime — an unregistered type is rejected.
- **`payload.files_changed`** — paths affected by this checkpoint, relative to the workspace root. For `observation` checkpoints, this may be empty — the agent noticed something but did not modify files.
- **`payload.content`** — the checkpoint's substance. For `artifact` checkpoints, this may be the full artifact or a summary with references. For `observation` checkpoints, this is the observation itself. The protocol does not constrain content structure — it is opaque to the runtime.
- **`payload.artifacts`** — references to produced files or objects. Typically file paths, URIs, or checkpoint IDs from other workspaces. The protocol does not fetch or validate artifacts — they are metadata for the coordinator and integrator.
- **`intent`** — the agent's explanation of why this checkpoint exists. Free-form text. Required — every checkpoint must state its purpose. This is the agent's voice in the trail: "I produced this because..." or "I noticed this because..."
- **`parent`** — links this checkpoint to its predecessor in the chain (§5). Null for the first checkpoint in a workspace. Forms the linear revision history.
- **`status`** — the agent's assessment of completeness. `provisional` means the agent may revise this work. `final` means the agent considers this checkpoint ready for integration. Set by the agent at creation and immutable — a provisional checkpoint is not promoted to final; instead, a new final checkpoint is created.
- **`confidence`** — the agent's assessment of correctness. `high`, `medium`, or `low`. This is information for the coordinator, not a gate — all confidence levels are valid checkpoints (§4, rule 4). An agent with reservations should note them in `intent`.
- **`resource_usage`** — optional resource consumption since the previous checkpoint (§6). Agent-reported. The runtime may independently verify; discrepancies are recorded in the trail.
- **`timestamp`** — assigned by the runtime at creation, not by the agent (Clock spec). Monotonic within the workspace.

**Integrity.** Every checkpoint carries a content integrity proof, verified at integration time. The integrity mechanism is implementation-defined; the protocol requires that it exists and is verified. See PROTOCOL §11.5 for the integrity model. A checkpoint that fails integrity verification is not integrated.

## 4. Checkpoint Rules

Five rules govern checkpoint behavior. These are normative — every runtime must enforce them.

**Rule 1: Checkpoints are immutable.** Once created, a checkpoint cannot be modified. No field may be changed — not the payload, not the status, not the confidence, not the intent. If an agent needs to revise its work, it creates a new checkpoint that references the previous one as its `parent`. The old checkpoint remains in the trail. History is never rewritten.

This is the protocol's strongest immutability guarantee. Envelopes are immutable after creation but have runtime-managed status transitions. Workspace state changes through defined transitions. Checkpoints do not change at all — they are append-only records. The chain grows forward; it never rewrites backward.

**Rule 2: Only the final checkpoint matters for integration.** When the coordinator integrates a workspace (Integration spec), it uses the most recent checkpoint with `status: final`. This checkpoint is also set as the task's `checkpoint_ref` when the task reaches `completed` status (Task spec, §2) — the task's deliverable is the workspace's final checkpoint. All provisional checkpoints are preserved in the trail for audit but are not included in the integration. If no `final` checkpoint exists when the workspace completes, normal integration cannot proceed — the coordinator may salvage a provisional checkpoint through salvage integration (Integration spec) or fail the workspace.

**Rule 3: Checkpoints must be type-compatible with the role.** The runtime rejects checkpoints whose type is not permitted for the workspace's role. Which types each role may produce is defined in the permission matrix (Roles spec, §6) and extensible through the taxonomy. Base roles produce base types — workers create `artifact` checkpoints and observers create `observation` checkpoints. Derived roles may produce additional types registered through the taxonomy. Enforcement happens at creation, not after the fact — an unauthorized checkpoint is never recorded.

**Rule 4: Confidence is information, not a gate.** A checkpoint with `confidence: low` is a valid checkpoint. The protocol does not block it, delay it, or treat it differently from a `confidence: high` checkpoint. But the coordinator can read confidence levels across all workspaces and use them to decide whether to proceed, request revision, or reassign. Low confidence is a signal from the agent that says "I have reservations" — the coordinator decides what to do with that information.

**Rule 5: Checkpoint creation automatically emits a signal.** When a checkpoint is created, the runtime automatically emits a `checkpoint` signal (Signal spec, §2) to the parent workspace. The signal's `ref` field carries the checkpoint ID. This emission is protocol-initiated — the agent does not need to emit it manually. The coordinator receives the signal and can immediately read the checkpoint. This is consistent with the protocol's auto-emission of `acknowledged` signals (Envelope spec, §4, rule 3): actions that the coordinator must always know about are not left to agent cooperation.

**Interaction between rules.** Rules 1 and 2 work together: because checkpoints are immutable (rule 1), a provisional checkpoint cannot be promoted to final. Instead, the agent creates a new checkpoint with `status: final` and `parent` pointing to the provisional one. The chain grows; nothing is overwritten. Rule 5 ensures the coordinator learns about every checkpoint — provisional or final — as it is created, giving the coordinator a live view of the agent's progress.

## 5. Checkpoint Chains

Within a workspace, checkpoints form a single-linked chain through their `parent` field. Each checkpoint points to the one before it. The chain tells the story of the agent's work — what it tried first, how it revised, and where it landed.

```
cp-001 (provisional, confidence: medium)
  └── cp-002 (provisional, confidence: medium)
        └── cp-003 (provisional, confidence: high)
              └── cp-004 (final, confidence: high)
```

**Three invariants:**

**Invariant 1: Chains are linear.** A checkpoint has at most one parent and at most one child. There are no branches, no forks, no DAGs. If the coordinator wants to explore two approaches, it creates two workspaces — each with its own linear chain — not two branches within one workspace. This mirrors the workspace concurrency model (Workspace spec, §7): one agent, one workspace, one chain of work.

**Invariant 2: The first checkpoint has no parent.** The chain starts with `parent: null`. Every subsequent checkpoint references the immediately preceding one. The runtime enforces this: a checkpoint whose `parent` is not the current chain head is rejected.

**Invariant 3: The chain is append-only.** Checkpoints are added to the end of the chain. They are never inserted in the middle, never removed, never reordered. Combined with immutability (§4, rule 1), this means the chain is a permanent, ordered record of every revision the agent made.

**Reading the chain.** The coordinator reads the chain in two ways:
- **Head only**: for integration, the coordinator reads the most recent `final` checkpoint (§4, rule 2). This is the deliverable.
- **Full chain**: for analysis, the coordinator (or a human via the highway) reads the entire chain to understand the agent's reasoning process. A chain that goes from `confidence: low` to `confidence: high` over four revisions tells a different story than one that produces a single `confidence: high` checkpoint immediately.

**Chain length is unbounded.** The protocol does not limit the number of checkpoints in a chain. An agent that iterates extensively produces a long chain. An agent that gets it right immediately produces a chain of one. The resource meter (Workspace spec, §6) constrains the agent's budget, which indirectly bounds chain length — but the protocol imposes no explicit limit.

**Chains across workspaces.** Chains do not span workspaces. Each workspace has its own independent chain starting from `parent: null`. The Integration spec defines how the coordinator connects work across workspaces — by reading each workspace's final checkpoint and merging the results. The `parent` field is workspace-local; cross-workspace linkage is the coordinator's responsibility, not the chain's.

## 6. Resource Tracking

The `resource_usage` field on a checkpoint captures resource consumption since the previous checkpoint (or since workspace start for the first checkpoint). It is optional, agent-reported, and serves the coordinator's cost visibility — not the runtime's enforcement.

```yaml
resource_usage:
  tokens_consumed: integer      # input + output tokens since previous checkpoint
  wall_time: duration           # elapsed active time since previous checkpoint
  cost: number                  # monetary cost (implementation-defined unit)
```

**Three dimensions, one purpose.** Tokens measure cognitive effort. Wall time measures elapsed duration. Cost measures monetary spend. Together they give the coordinator a per-checkpoint view of how expensive each iteration was — not just the workspace total, but the incremental cost of each revision.

**Delta, not cumulative.** Each checkpoint's `resource_usage` reports consumption since the previous checkpoint, not since workspace creation. The coordinator sums the chain to get the cumulative total. The delta model is more informative — it reveals whether costs are increasing (each revision more expensive), stabilizing (consistent cost per revision), or decreasing (agent converging). A cumulative model would hide these patterns.

**Agent-reported, runtime-verified.** The agent populates `resource_usage` based on its own accounting. The runtime may independently verify these numbers against its own resource meter (Workspace spec, §6). Discrepancies are recorded in the trail as `resource_discrepancy` entries but do not block checkpoint creation. The protocol trusts the agent's self-report for coordination purposes; the runtime's meter is the enforcement mechanism for hard budget limits.

**Relationship to workspace resource management.** The workspace resource meter (Workspace spec, §6) tracks aggregate consumption across three dimensions: tokens, wall time, and cost. Checkpoint resource tracking provides the same dimensions at checkpoint granularity. The workspace meter is the runtime's enforcement tool (hard limits, budget warnings). Checkpoint resource tracking is the coordinator's analysis tool (per-iteration cost, burn rate, convergence detection).

**When `resource_usage` is absent.** If a checkpoint omits `resource_usage`, the coordinator has no per-checkpoint cost data for that iteration. The workspace resource meter still tracks aggregate consumption — the absence of per-checkpoint data does not affect budget enforcement. Implementations should encourage agents to report resource usage, but the protocol does not require it.

## 7. Trail Events

Every checkpoint creation produces a trail entry. The checkpoint system generates three event types.

| Event | When | Body |
|-------|------|------|
| `checkpoint_created` | Agent creates a checkpoint | `checkpoint_id`, `workspace`, `type`, `status`, `confidence`, `parent`, `timestamp` |
| `checkpoint_rejected` | Runtime rejects checkpoint creation | `workspace`, `type`, `reason`, `timestamp` |
| `resource_discrepancy` | Runtime's resource meter disagrees with agent-reported `resource_usage` | `checkpoint_id`, `workspace`, `field`, `agent_value`, `runtime_value`, `timestamp` |

**One entry per successful checkpoint.** A successfully created checkpoint produces exactly one `checkpoint_created` trail entry. Unlike envelopes (which produce separate created and delivered entries), checkpoints have no delivery phase — they are written to the workspace's checkpoint register and the trail simultaneously. The `checkpoint` signal (§4, rule 5) is auto-emitted as a side effect of creation, producing its own `signal_emitted` trail entry (Signal spec, §8).

**Rejection recording.** A checkpoint that fails validation — wrong type for the role (§4, rule 3), unregistered type, or structural deficiency — produces a `checkpoint_rejected` trail entry. The checkpoint is never recorded in the workspace's checkpoint register. Rejection reasons:

| Reason | Meaning |
|--------|---------|
| `permission_denied` | Workspace's role cannot create this checkpoint type |
| `invalid_type` | Checkpoint type is not registered in the taxonomy |
| `invalid_structure` | Required fields missing or malformed |
| `invalid_parent` | `parent` field does not reference the current chain head |
| `workspace_not_active` | Workspace is not in `active` state |

**Resource discrepancy recording.** When the runtime verifies `resource_usage` and finds a discrepancy with its own meter, a `resource_discrepancy` entry is recorded per field that disagrees. This entry does not block checkpoint creation — it is an audit record. The `field` identifies which dimension disagrees (`tokens_consumed`, `wall_time`, or `cost`). The `agent_value` and `runtime_value` capture both numbers for comparison.

**No payload in trail entries.** Like envelope trail entries (Envelope spec, §10), checkpoint trail entries reference checkpoints by `id` without duplicating the payload. The trail records *that* a checkpoint was created — not *what* it contained.

## 8. Conformance Requirements

| Requirement | Level | Description |
|-------------|-------|-------------|
| Two base checkpoint types | Core | Runtime MUST implement `artifact` and `observation` as base types. |
| Checkpoint schema | Core | Every checkpoint MUST carry all fields defined in §3. `id` and `timestamp` MUST be runtime-assigned. `intent` MUST be present and non-empty. |
| Immutability | Core | Checkpoints MUST be immutable after creation. No field may be modified. |
| Final checkpoint for integration | Core | Integration MUST use the most recent checkpoint with `status: final`. Provisional checkpoints MUST NOT be used for normal integration. |
| Type-role compatibility | Core | Runtime MUST reject checkpoints whose type is not permitted for the workspace's role. |
| Auto-signal emission | Core | Runtime MUST emit a `checkpoint` signal upon checkpoint creation. The signal's `ref` field MUST carry the checkpoint ID. |
| Linear chains | Core | Chains MUST be linear — at most one parent, at most one child per checkpoint. Runtime MUST reject checkpoints whose `parent` is not the current chain head (or null for the first). |
| Append-only chains | Core | Checkpoints MUST only be appended to the end of the chain. No insertions, removals, or reordering. |
| Trail recording | Core | Every checkpoint creation MUST produce a `checkpoint_created` trail entry. Every rejection MUST produce a `checkpoint_rejected` trail entry. |
| Active workspace only | Standard | Checkpoints MUST only be created in workspaces in `active` state. Creation attempts in other states MUST be rejected. |
| Taxonomy validation | Standard | Runtime MUST reject checkpoint types not registered in the taxonomy. |
| Confidence preserved | Standard | The `confidence` field MUST be recorded and available to the coordinator. The runtime MUST NOT gate, delay, or filter checkpoints based on confidence level. |
| Resource verification | Full | Runtime SHOULD verify agent-reported `resource_usage` against its own meter. Discrepancies MUST be recorded as `resource_discrepancy` trail entries without blocking checkpoint creation. |
| No payload in trail | Full | Trail entries MUST reference checkpoints by `id` without duplicating payload content. |

## 9. Implementation Notes

These notes are non-normative. They capture practical guidance for implementers.

**Checkpoint as an immutable record.** Checkpoints should be stored as immutable objects — write-once, read-many. Content-addressed storage (keyed by a hash of the checkpoint's fields) is a natural fit: it enforces immutability by construction and provides the integrity proof required by PROTOCOL §11.5 for free. If the storage layer cannot modify an object after creation, rule 1 is enforced at the infrastructure level.

**Chain as a linked list.** The checkpoint chain is a singly-linked list through the `parent` field. The workspace's checkpoint register (Workspace spec, §2) needs only two pointers: the head (first checkpoint, `parent: null`) and the tail (most recent checkpoint, the current chain head). New checkpoints are appended at the tail. The `parent` validation check (invariant 2) is a single pointer comparison: does the new checkpoint's `parent` match the current tail?

**Checkpoint register vs. trail.** The checkpoint register is the workspace's live index of its checkpoints — an ordered list for fast access. The trail is the permanent record. Both contain the same checkpoints, but they serve different purposes: the register is for the agent and coordinator during execution; the trail is for audit, recovery, and analysis after the fact. Implementations should write to both atomically — the register and the trail entry in a single operation.

**Signal emission wiring.** Like the `acknowledged` signal for envelopes, the `checkpoint` signal is a runtime side effect, not an agent action. Implementations should wire signal emission into the checkpoint creation path: validate → write to register → write to trail → emit signal. The signal carries the checkpoint ID in its `ref` field, so it must be emitted after the ID is assigned.

**Resource usage verification.** The runtime's resource meter and the agent's self-reported `resource_usage` measure the same dimensions but from different vantage points. The runtime measures externally (API call counts, wall clock time); the agent measures internally (tokens processed, active work time). Small discrepancies are expected — the agent may not account for protocol overhead, and the runtime may not distinguish agent computation from idle waiting. Implementations should define a tolerance threshold per dimension; discrepancies within tolerance should not generate `resource_discrepancy` trail entries.

**Confidence as coordinator input.** The `confidence` field is metadata for the coordinator, not for the runtime. Implementations should surface confidence levels in whatever interface the coordinator uses to monitor workspaces — a dashboard, a query API, or a signal handler. A coordinator that reads `confidence: low` from multiple workspaces may decide to slow down, increase review coverage, or escalate. The protocol does not prescribe coordinator behavior based on confidence — it ensures the data is available.

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
