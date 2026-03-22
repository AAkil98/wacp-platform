# WACP: Integration

## Metadata

```yaml
title: "WACP: Integration"
id: wacp-spec-integration
type: constituent-spec
tier: abstract
category: mechanisms
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §7.4–§7.9
depends_on:
  - wacp-spec-workspace
  - wacp-spec-signal
  - wacp-spec-checkpoint
  - wacp-spec-user
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, integration, merge, conflict, salvage, coordination]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Integration Process](#2-integration-process)
3. [Merge Strategies](#3-merge-strategies)
4. [Integration Ordering](#4-integration-ordering)
5. [Conflict Detection](#5-conflict-detection)
6. [Conflict Resolution](#6-conflict-resolution)
7. [Salvage Integration](#7-salvage-integration)
8. [Trail Events](#8-trail-events)
9. [Conformance Requirements](#9-conformance-requirements)
10. [Implementation Notes](#10-implementation-notes)

---

## 1. Purpose

Integration is the protocol's assembly operation. Where checkpoints capture individual progress, integration assembles that progress into a coherent whole. It is the act of merging a workspace's final checkpoint into the parent workspace — taking what an agent produced and making it part of the larger result.

Only the coordinator performs integration. It is a deliberate, evaluated act — not an automatic consequence of completion. When a worker emits `complete`, the workspace enters `integrating` state, but the merge does not happen until the coordinator decides to accept, revise, or reject the work. This separation of completion from integration gives the coordinator full control over quality and ordering. Successful integration also drives the task's terminal transition — from `completed` to `integrated` (Task spec, §4) — marking the unit of work as fully delivered.

Integration handles three concerns that checkpoints alone cannot:
- **Assembly** — combining artifacts from multiple workspaces into a single coherent output, using a merge strategy appropriate to the work.
- **Conflict detection** — identifying when two workspaces' outputs are incompatible (overlapping content, contradictory conclusions, broken dependencies, constraint violations).
- **Conflict resolution** — resolving incompatibilities through coordinator judgment, human escalation, or agent rework.

The protocol also defines salvage integration — a path for recovering useful work from failed workspaces. When a workspace fails (budget exhaustion, unrecoverable error) but has produced checkpoints, the coordinator may extract and integrate those partial artifacts under stricter guardrails.

## 2. Integration Process

Integration begins when a worker emits `complete` and the coordinator initiates the merge. The coordinator reads the workspace's final checkpoint (Checkpoint spec, §4, rule 2) and makes one of three decisions.

```
Agent emits `complete` signal
        │
        ▼
Workspace enters `integrating` state
        │
        ▼
Coordinator reads final checkpoint
        │
        ▼
Coordinator evaluates
        │
        ├── accept ──► merge artifacts into parent (§3)
        │                    │
        │                    ├── no conflicts ──► workspace → closed
        │                    │
        │                    └── conflicts ─────► workspace → conflicted (§5, §6)
        │
        ├── revise ──► workspace → failed (reason: revision_required)
        │                    │
        │                    ▼
        │              coordinator creates new workspace
        │              with feedback directive referencing
        │              the failed workspace's trail
        │
        └── reject ──► workspace → failed (reason: rejected)
                             │
                             ▼
                       coordinator decides: retry
                       in new workspace or abort task
```

**Three decisions, no backward transitions.**

**Accept** — the coordinator judges the work satisfactory and proceeds to merge. The merge strategy (§3) determines how artifacts are combined. If no conflicts arise, the workspace transitions to `closed`. If conflicts arise, it transitions to `conflicted` (§5).

**Revise** — the coordinator judges the work insufficient but recoverable. The current workspace is failed with `reason: revision_required`. The coordinator creates a new workspace with a feedback directive that references the failed workspace's trail — giving the new agent full context of what was attempted and why it was insufficient. The original workspace's trail is preserved as a complete, immutable record of the first attempt.

**Reject** — the coordinator judges the work unusable. The workspace is failed with `reason: rejected`. The coordinator decides whether to retry in a new workspace or abort the task entirely. No new workspace is automatically created — rejection may be final.

**Task state follows the decision.** When the workspace executes a task (Task spec, §5), the integration decision drives the task's lifecycle: `accept` leading to successful merge transitions the task from `completed` to `integrated` (terminal). `revise` and `reject` transition the task to `failed` — the coordinator may then retry the task in a new workspace or cancel it (Task spec, §4).

**Revise does not reopen the workspace.** Consistent with the workspace lifecycle (Workspace spec, §3), no backward transitions exist. A workspace that has entered `integrating` cannot return to `active`. When revision is needed, a new workspace is created. This preserves the original workspace's trail and avoids the complexity of resuming a completed agent.

**The coordinator may insert an evaluation step.** Before deciding accept/revise/reject, the coordinator may dispatch a derived role (e.g., a reviewer) to evaluate the work. The evaluator reads the worker's checkpoint and produces its own checkpoint containing the evaluation. The coordinator reads both — the worker's output and the evaluator's assessment — before deciding. This is a workflow-level configuration, not a protocol requirement. The protocol supports both direct evaluation (coordinator decides alone) and gated evaluation (coordinator decides after receiving an evaluation).

## 3. Merge Strategies

The coordinator selects a merge strategy at integration time. Three strategies are defined, each with different cost and safety characteristics.

| Strategy | Mechanism | Cost | Safety |
|----------|-----------|------|--------|
| `direct` | Artifacts copied into parent as-is | Low | Low — no conflict detection |
| `layered` | Artifacts applied on top of existing parent state | Medium | Medium — overlap detected |
| `evaluated` | Coordinator reads both states and synthesizes a result | High | High — full context merge |

**Direct.** The workspace's artifacts are copied into the parent without transformation. No comparison with existing parent state. Used when the workspace had full authority over its output and no conflicts are possible — typically when the coordinator assigned non-overlapping work to each workspace. Direct merge is fast and cheap but blind: if two workspaces modified the same resource, the second direct merge silently overwrites the first.

**Layered.** The workspace's artifacts are applied on top of the existing parent state. If a resource exists in both, the workspace version wins. The runtime detects overlap (a resource modified by both the workspace and a prior integration) and records it. Unlike direct merge, layered merge knows when overlap occurs — it just resolves it by preferring the incoming workspace. Used when directives are designed to produce non-overlapping work but minor overlap is tolerable.

**Evaluated.** The coordinator reads both the existing parent state and the workspace's artifacts, then produces a new checkpoint in the parent workspace that synthesizes both. This is the most expensive strategy — it requires the coordinator to actively reason about the merge — but the safest. The coordinator acts as the merge brain, resolving conflicts with full context. Used for high-stakes work, overlapping directives, or any situation where blind merging is unacceptable.

**Strategy selection is the coordinator's decision.** The protocol does not prescribe which strategy to use for which situation. The coordinator selects based on its knowledge of the work: how the directives were scoped, whether overlap is expected, and how critical correctness is. Workflow configurations may recommend or require specific strategies for specific stages.

**Strategy and integration mode.** The `mode` field on the integration record distinguishes normal integration from salvage integration (§7). Strategy availability depends on mode:

| Mode | Direct | Layered | Evaluated |
|------|--------|---------|-----------|
| `normal` | available | available | available |
| `salvage` | not available | not available | mandatory |

Salvage integration mandates the `evaluated` strategy — partial work from a failed workspace cannot be blindly merged.

## 4. Integration Ordering

When multiple workspaces complete concurrently, the coordinator integrates them sequentially — one at a time, in the order it chooses. There is no parallel merge. Each integration sees the result of all prior integrations.

**Why sequential.** Parallel merges introduce a class of conflicts that require a coordination protocol of their own — two simultaneous merges may both succeed individually but produce an inconsistent combined result. Sequential integration is simpler, predictable, and sufficient. The coordinator is a single agent and can only evaluate one merge at a time. The constraint is structural, not a limitation to be lifted later.

**Ordering is the coordinator's decision.** The protocol does not prescribe integration order. The coordinator selects the order based on its knowledge of the work:
- **Foundational work first** — integrate outputs that other outputs depend on before integrating the dependents.
- **Priority-based** — integrate `urgent` workspaces before `normal` ones (Workspace spec, §4).
- **Confidence-based** — integrate high-confidence outputs first to establish a solid base, then integrate lower-confidence outputs against that base.
- **Dependency-aware** — when directive A's output is used by directive B, integrate A before B. The task graph (Task spec, §3) provides the natural dependency structure — integrating tasks in topological order of the DAG ensures that each integration sees its dependencies' results.

The coordinator may combine these heuristics. The protocol's only requirement is that integration is sequential and that each integration sees the cumulative result of all prior integrations.

**Integration order affects conflict detection.** The order in which workspaces are integrated determines which conflicts are detected. If workspace A and workspace B both modify the same resource, integrating A first means B's integration detects the conflict — not A's. The coordinator should account for this when choosing order: integrating the more authoritative or more complete workspace first reduces the likelihood that its work is flagged as conflicting.

**The `integrate` signal.** When the coordinator initiates integration, the runtime emits an `integrate` signal (Signal spec, §2). This signal marks the start of the merge operation in the trail. The signal is emitted once per integration, not once per workspace — it is a coordinator action, not a workspace action.

## 5. Conflict Detection

When the coordinator merges a workspace's artifacts into the parent, the merge may reveal incompatibilities with the existing parent state. A detected conflict transitions the workspace from `integrating` to `conflicted` (Workspace spec, §3). This is a deliberate pause — the system has identified that two pieces of work do not fit together, and a resolution decision is required before the merge can complete.

**Four conflict types.** The protocol defines four conflict types. Like signal types, conflict types are a closed set — the protocol's behavior depends on understanding every type.

| Type | Meaning | Example |
|------|---------|---------|
| `content_overlap` | Two workspaces modified the same resource | Both agents edited the same file |
| `semantic_contradiction` | Two workspaces reached incompatible conclusions | Agent A says "use approach X," agent B says "use approach Y" |
| `dependency_violation` | A workspace's output invalidates an assumption from a prior integration | Agent A's integration changed an API that agent B's code depends on |
| `constraint_breach` | Integration would violate a workflow-level constraint | Merged output exceeds a size limit, breaks a validation rule, or fails a test |

**Content overlap** is structural — the runtime can detect it mechanically by comparing modified resources. **Semantic contradiction** requires judgment — the coordinator or an evaluator must recognize that two outputs are logically incompatible. **Dependency violation** is causal — it emerges when integration order creates a mismatch between assumptions and reality. **Constraint breach** is rule-based — it is detected by evaluating the merged result against predefined constraints.

A single integration may produce multiple conflicts of different types. Each conflict is recorded individually in the integration record's `conflicts` list.

**Detection depends on merge strategy.** Not all strategies detect all conflict types:

| Strategy | content_overlap | semantic_contradiction | dependency_violation | constraint_breach |
|----------|----------------|----------------------|---------------------|-------------------|
| `direct` | not detected | not detected | not detected | not detected |
| `layered` | detected | not detected | not detected | not detected |
| `evaluated` | detected | detected | detected | detected |

Direct merge detects nothing — it copies blindly. Layered merge detects resource-level overlap but cannot reason about semantics or dependencies. Only evaluated merge provides full conflict detection, because the coordinator actively reads and reasons about both states. This is the fundamental tradeoff between merge strategies: cheaper strategies miss conflicts; the expensive strategy catches them.

**The `conflicted` state.** A workspace in `conflicted` is subject to timeout (Workspace spec, §6). If neither the coordinator, a human, nor any resolution strategy resolves the conflict within the workspace's remaining timeout, the workspace transitions to `failed` with `reason: conflict_timeout`.

## 6. Conflict Resolution

When the coordinator detects a conflict (§5), it selects a resolution strategy based on the conflict type, the workflow configuration, and the stakes involved. Three strategies are defined.

| Strategy | Behavior | Outcome |
|----------|----------|---------|
| `coordinator_resolve` | Coordinator resolves unilaterally and completes the merge | workspace → `closed` |
| `human_escalate` | Conflict routed to the human highway for resolution | workspace → `closed` or `failed` |
| `agent_rework` | Workspace failed, new workspace created with conflict context | workspace → `failed` |

**Coordinator resolve.** The coordinator examines the conflicting artifacts, makes a judgment, and completes the merge. The workspace transitions from `conflicted` to `closed`. The resolution is recorded in the integration record's `resolution` field. Used for conflicts where the coordinator has full context — typically `content_overlap` and `dependency_violation` where the fix is mechanical or clearly inferrable.

**Human escalate.** The conflict is routed to the human highway (PROTOCOL §8). The human reviews the conflicting artifacts and decides: approve a resolution, modify the merge, or reject. If the human approves, the workspace transitions from `conflicted` to `closed`. If the human rejects, the workspace transitions to `failed`. If the escalation times out without human response, the fallback behavior defined in the workflow configuration executes. Used for `semantic_contradiction`, high-stakes `constraint_breach`, or any conflict exceeding the coordinator's authority.

**Escalation routing.** When a conflict is escalated to the highway, it routes to the workspace's owner by default (User spec, §4.1; Human Highway spec, §2.4). The `approve_integration_own` capability (User spec, §6.2) authorizes the owner to approve or reject the resolution for workspaces in their ownership tree. A user with `approve_integration_any` may resolve conflicts for any workspace. The capability check occurs at resolution time, not at escalation time — the conflict is presented to the owner, but the resolution is enforced.

**Agent rework.** The coordinator fails the current workspace and creates a new workspace with a feedback directive that includes the conflict context — what conflicted, with what, and why the merge could not proceed. The new workspace gives an agent the chance to revise the work with full awareness of the conflict. Used when the conflict is best resolved by the agent who produced the work, or when the coordinator cannot resolve it unilaterally and human escalation is not configured.

**Resolution process:**

```
workspace enters conflicted
        │
        ▼
Coordinator classifies conflict(s)
        │
        ▼
Coordinator selects resolution strategy
        │
        ├── coordinator_resolve ──► resolve, complete merge ──► workspace → closed
        │
        ├── human_escalate ────────► human highway activated
        │                                  │
        │                                  ├── human approves ──► workspace → closed
        │                                  ├── human rejects ───► workspace → failed
        │                                  └── timeout ─────────► fallback executes
        │
        └── agent_rework ─────────► workspace → failed
                                          │
                                          ▼
                                    new workspace created with
                                    conflict context in directive
```

**Multiple conflicts, one strategy per conflict.** When an integration produces multiple conflicts, the coordinator selects a resolution strategy for each conflict independently. A single integration might resolve one `content_overlap` via `coordinator_resolve` and escalate a `semantic_contradiction` via `human_escalate`. All conflicts must be resolved before the workspace can transition from `conflicted` to `closed`. If any single conflict results in `agent_rework`, the entire workspace is failed — partial resolution is not possible when the workspace needs rework.

**Conflict timeout.** A workspace in `conflicted` state is subject to the workspace's remaining timeout (Workspace spec, §6). If resolution does not complete within this window, the workspace transitions to `failed` with `reason: conflict_timeout`. This prevents conflicts from stalling the system indefinitely — particularly relevant when `human_escalate` is waiting for a human who may not respond promptly.

## 7. Salvage Integration

When a workspace fails — particularly due to budget exhaustion — it may contain useful work in its checkpoint chain. The agent may have produced one or more provisional checkpoints, or even a final checkpoint, before the failure occurred. Normal integration is unavailable because the workspace never emitted `complete` and never entered `integrating` state. Salvage integration provides a protocol-blessed path for the coordinator to recover and merge these partial artifacts.

**When salvage applies.** Salvage integration is available for any workspace in `failed` state that has at least one checkpoint. The most common trigger is `reason: budget_exceeded`, but salvage is not restricted to budget failures — an agent that failed due to an unrecoverable error may also have produced partial work worth preserving. The coordinator decides whether to salvage; the protocol does not automatically salvage failed workspaces.

**Checkpoint selection.** In normal integration, the protocol uses the most recent checkpoint with `status: final` (Checkpoint spec, §4, rule 2). In salvage integration, the agent never got to finalize its work, so the selection rules differ:

1. If a checkpoint with `status: final` exists, the coordinator uses it. The agent declared this work complete before the failure — it is the strongest candidate.
2. If only `provisional` checkpoints exist, the coordinator selects the most recent one by default. The coordinator may select an earlier checkpoint if it judges the most recent to be incomplete or corrupted (e.g., the agent was mid-revision when budget ran out and the earlier checkpoint was more coherent).
3. The coordinator may select no checkpoint and decline to salvage. Salvage is always optional.

The selected checkpoint is recorded in the integration record's `checkpoint_ref` field. The `mode` field is set to `salvage`.

**Three mandatory guardrails.** Salvaged artifacts carry inherent uncertainty — the agent did not complete its work, did not self-review, and may have been mid-thought. The protocol imposes three guardrails that do not apply to normal integration:

**Guardrail 1: Evaluated strategy only.** Salvage integration must use the `evaluated` merge strategy (§3). The `direct` and `layered` strategies are not permitted — they would merge artifacts without the coordinator assessing their completeness. The coordinator must actively read the salvaged checkpoint, evaluate whether it is usable, and produce a synthesized result.

**Guardrail 2: Confidence is treated as low.** Regardless of the checkpoint's self-reported `confidence` field, the coordinator treats salvaged checkpoints as `confidence: low` for decision-making purposes. The agent's confidence assessment was made before the failure, potentially before the agent finished its intended work. A checkpoint marked `confidence: high` by an agent that was about to revise it is not high-confidence — the agent was interrupted before it could act on its own assessment.

**Guardrail 3: Trail transparency.** The integration record's `mode: salvage` field ensures that every downstream consumer — evaluators, analysis, humans — can distinguish salvaged integrations from normal ones. The trail makes salvage visible; nothing is silently merged from a failed workspace.

**Salvage integration flow:**

```
Workspace W fails (e.g., budget_exceeded)
        │
        ▼
Coordinator reads W's checkpoint chain from trail
        │
        ├── No checkpoints exist ──► salvage not possible
        │                            coordinator creates new workspace
        │
        └── Checkpoints exist ──► coordinator selects checkpoint
                │
                ▼
        Coordinator decides: salvage or discard?
                │
                ├── Discard ──► coordinator creates new workspace
                │               with failed trail as context
                │
                └── Salvage ──► coordinator initiates integration
                                mode: salvage, strategy: evaluated
                        │
                        ▼
                Coordinator evaluates checkpoint
                        │
                        ├── Usable ──► merge into parent
                        │               (normal conflict detection applies)
                        │
                        └── Not usable ──► integration aborted
                                           coordinator creates new workspace
```

**Workspace state.** The source workspace remains in `failed` state throughout. Salvage integration does not resurrect the workspace — it extracts artifacts from the wreckage. The workspace is still terminal, still immutable, still fully preserved in the trail. The integration occurs in the parent workspace, not the failed workspace.

**Interaction with conflict detection.** Normal conflict detection (§5) applies to salvage integration. A salvaged checkpoint may conflict with previously integrated work. All four conflict types are possible. The coordinator resolves conflicts using the same strategies (§6) — but should be more willing to use `agent_rework` than `coordinator_resolve`, given the lower confidence in salvaged artifacts.

## 8. Trail Events

Integration produces trail entries at each stage — initiation, completion, conflict detection, and conflict resolution. Three event types are integration-specific (`integration_started`, `integration_completed`, `integration_aborted`). Two additional event types (`conflict_detected`, `conflict_resolved`) are defined in the Workspace spec (§13) — they record workspace state transitions caused by integration conflicts. All five are listed here for completeness.

| Event | When | Body |
|-------|------|------|
| `integration_started` | Coordinator initiates integration | `source` (workspace), `target` (parent workspace), `owner` (user_id of source workspace), `mode`, `strategy`, `checkpoint_ref`, `timestamp` |
| `integration_completed` | Merge completes successfully | `source`, `target`, `mode`, `strategy`, `result` (`success` or `conflict_resolved`), `timestamp` |
| `integration_aborted` | Coordinator aborts integration (salvage deemed unusable, or rejection) | `source`, `target`, `mode`, `reason`, `timestamp` |
| `conflict_detected` | Workspace enters `conflicted` state | `workspace_id`, `conflict_type`, `resources` (affected), `description` (Workspace spec §13) |
| `conflict_resolved` | Workspace exits `conflicted` state | `workspace_id`, `conflict_type`, `resolution_strategy`, `resolution` (description), `outcome` (`closed` or `failed`) (Workspace spec §13) |

**Two entries per successful integration.** A conflict-free integration produces `integration_started` and `integration_completed`. These are separate events because they happen at different moments — the coordinator may spend significant time evaluating the checkpoint between start and completion, especially with the `evaluated` strategy.

**Four entries for conflicted integration.** A conflicted integration produces `integration_started`, one or more `conflict_detected` entries (one per conflict), corresponding `conflict_resolved` entries, and finally `integration_completed` or `integration_aborted`. Each conflict is recorded individually — a single integration with two conflicts produces two detection entries and two resolution entries.

**Salvage transparency.** Salvage integrations carry `mode: salvage` in every trail entry, from `integration_started` through completion or abortion. Trail queries can filter on mode to distinguish salvaged integrations from normal ones. This fulfills guardrail 3 (§7) — salvage is visible at every stage.

**The `integrate` signal and trail entries are complementary.** The `integrate` signal (Signal spec, §2) propagates upward through the workspace tree, notifying the parent that a merge has started. The `integration_started` trail entry records the same event with full detail (source, target, mode, strategy, checkpoint). The signal is for real-time coordination; the trail entry is for audit and recovery.

**Conflict entries are paired.** Every `conflict_detected` entry is eventually followed by a `conflict_resolved` entry for the same conflict — unless the workspace times out, in which case the conflict is resolved implicitly by the workspace's transition to `failed` with `reason: conflict_timeout`. The resolution entry records this: `outcome: failed`, `resolution_strategy: timeout`.

## 9. Conformance Requirements

| Requirement | Level | Description |
|-------------|-------|-------------|
| Three integration decisions | Core | Runtime MUST support `accept`, `revise`, and `reject` as coordinator decisions on completed workspaces. |
| Three merge strategies | Core | Runtime MUST implement `direct`, `layered`, and `evaluated` merge strategies. |
| Sequential integration | Core | Integrations MUST be performed sequentially. Each integration MUST see the cumulative result of all prior integrations. No parallel merges. |
| Final checkpoint only | Core | Normal integration MUST use the most recent checkpoint with `status: final`. |
| No backward transitions | Core | Integration MUST NOT reopen a workspace. `revise` and `reject` MUST transition the workspace to `failed`. |
| Four conflict types | Core | Runtime MUST recognize `content_overlap`, `semantic_contradiction`, `dependency_violation`, and `constraint_breach`. The set is closed. |
| Conflicted state transition | Core | Detected conflicts MUST transition the workspace from `integrating` to `conflicted`. |
| Three resolution strategies | Core | Runtime MUST support `coordinator_resolve`, `human_escalate`, and `agent_rework` as conflict resolution strategies. |
| Trail recording | Core | Every integration stage MUST produce the corresponding trail entry defined in §8. Conflict entries MUST be paired (detected → resolved). |
| Integrate signal | Core | Runtime MUST emit an `integrate` signal when the coordinator initiates integration. |
| Salvage mode restriction | Standard | Salvage integration MUST use the `evaluated` strategy. `direct` and `layered` MUST NOT be available for salvage. |
| Salvage confidence override | Standard | Salvaged checkpoints MUST be treated as `confidence: low` regardless of the checkpoint's self-reported confidence. |
| Salvage trail transparency | Standard | Salvage integrations MUST carry `mode: salvage` in all trail entries. |
| Salvage checkpoint selection | Standard | In salvage mode, coordinator MUST be able to select any checkpoint in the chain, not only the most recent `final`. |
| Conflict timeout | Standard | Workspaces in `conflicted` state MUST be subject to timeout. Unresolved conflicts MUST transition the workspace to `failed` with `reason: conflict_timeout`. |
| Strategy-conflict detection | Full | `direct` merge SHOULD NOT claim to detect conflicts. `layered` SHOULD detect `content_overlap`. `evaluated` SHOULD detect all four types. |
| Integration ordering transparency | Full | The coordinator's chosen integration order SHOULD be recorded in the trail for audit and analysis. |

## 10. Implementation Notes

These notes are non-normative. They capture practical guidance for implementers.

**Integration as a coordinator operation.** Integration runs in the coordinator's workspace, not the source workspace. The coordinator reads the source workspace's checkpoint (via the checkpoint register or trail), reads the parent workspace's current state, and produces a merged result. Implementations should provide the coordinator with read access to the source workspace's checkpoint chain without granting write access — the source workspace is in `integrating` state and its inbox is sealed (Envelope spec, §4).

**Merge strategy as a pluggable function.** The three merge strategies share a common signature: `(parent_state, source_checkpoint) → merged_state | conflicts`. Implementations should represent strategies as pluggable functions that the coordinator selects at integration time. The `direct` strategy is trivial (copy), `layered` requires resource-level comparison, and `evaluated` requires the coordinator to reason about both inputs. For `evaluated`, the coordinator itself is the merge function — implementations should provide it with both states and let it produce the synthesis.

**Conflict detection granularity.** `content_overlap` can be detected mechanically by comparing `payload.files_changed` across the source checkpoint and prior integrations. If two checkpoints list the same file path, overlap exists. `semantic_contradiction` and `dependency_violation` require deeper analysis — comparing the content of artifacts, not just their paths. Implementations may provide conflict detection as a pipeline: structural checks first (fast, mechanical), semantic checks second (expensive, coordinator-driven).

**Conflict resolution state machine.** The `conflicted` state is a sub-state machine within the workspace lifecycle. Implementations should track each conflict independently — its type, its assigned resolution strategy, and its outcome. The workspace exits `conflicted` only when all conflicts are resolved. A simple implementation maintains a conflict list and removes entries as they are resolved; the workspace transitions when the list is empty.

**Salvage checkpoint access.** Salvage integration requires reading the checkpoint chain of a `failed` workspace. The workspace is terminal — its checkpoint register is frozen. Implementations should ensure that terminal workspaces' checkpoint registers remain readable. The trail is the fallback: if the register is unavailable, the checkpoint chain can be reconstructed from `checkpoint_created` trail entries.

**Sequential integration queue.** When multiple workspaces complete concurrently, implementations should maintain an integration queue. The coordinator processes one integration at a time, in the order it selects. The queue is the coordinator's input — it reads `complete` signals (or `checkpoint` signals for salvage candidates) and enqueues workspaces for integration. The queue ordering is the coordinator's decision; the runtime provides the queue mechanism.

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
