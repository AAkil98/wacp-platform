# WACP: Trail

## Metadata

```yaml
title: "WACP: Trail"
id: wacp-spec-trail
type: constituent-spec
tier: abstract
category: primitives
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §9
depends_on:
  - wacp-spec-clock
  - wacp-spec-identity
  - wacp-spec-roles
  - wacp-spec-workspace
  - wacp-spec-signal
  - wacp-spec-envelope
  - wacp-spec-checkpoint
  - wacp-spec-task
  - wacp-spec-integration
  - wacp-spec-human-highway
  - wacp-spec-user
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, trail, audit, observability, history, integrity, storage]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Trail Entry Schema](#2-trail-entry-schema)
3. [Scopes](#3-scopes)
4. [Integrity Invariants](#4-integrity-invariants)
5. [Querying](#5-querying)
6. [Observability Views](#6-observability-views)
7. [Storage Model](#7-storage-model)
8. [Compaction and Retention](#8-compaction-and-retention)
9. [Event Registry](#9-event-registry)
10. [Conformance Requirements](#10-conformance-requirements)
11. [Implementation Notes](#11-implementation-notes)

---

## 1. Purpose

The trail is the protocol's memory. AI agents have no persistent state after a run ends — their context windows close, their working memory is released, their reasoning is gone. What survives is the trail: the complete, ordered, immutable record of everything that happened. The trail is not a log. Logs are operational afterthoughts — write-behind, best-effort, grepped when something breaks. The trail is first-class: append-only, queryable, and authoritative. If an event is not in the trail, it did not happen. If it is in the trail, it is immutable.

Every other Protocol primitive produces trail entries. Workspaces record state transitions. Signals record emissions and deliveries. Envelopes record creation, delivery, rejection. Checkpoints record production and validation. Tasks record lifecycle changes. Integration records decisions. The Human Highway records every human action. This spec does not define those events — each primitive's spec defines its own trail entries (Workspace spec §13, Signal spec §8, Envelope spec §10, Checkpoint spec §7, Task spec §8, Integration spec §8, Human Highway spec §8). This spec defines the container: the entry format, the scoping model, the integrity guarantees, the query interface, the storage lifecycle, and the consolidated event registry.

Three systems depend on the trail as their foundation. Recovery replays trail entries to reconstruct system state after failure — the trail is the single source of truth from which any workspace can be restored (PROTOCOL §10.2). Observability reads the trail to produce views — run overviews, workspace timelines, message flows, workflow comparisons. Security audits the trail to verify that every action was authorized and every actor is accountable — the hash chain makes tampering detectable (PROTOCOL §11.5). The trail serves all three because it captures the same thing: what happened, in what order, by whom.

## 2. Trail Entry Schema

Every protocol event produces exactly one trail entry. The entry is the atomic unit of the trail — indivisible, immutable once written, and self-describing. All entries share a common header; each event type adds a type-specific body.

```yaml
trail_entry:
  id: string              # globally unique (Identity spec, rule 1)
  timestamp: datetime     # runtime-assigned (Clock spec, invariant 1)
  workspace: workspace_id # the workspace this event belongs to (null for system-level events)
  actor: string           # who caused this event (see actor semantics below)
  event_type: string      # the event type from the registry (§9)
  body: object            # type-specific payload (defined by each primitive's spec)
  prev_hash: string       # hash of the previous entry in the same scope (§4)
```

**Field semantics:**

- **`id`** — globally unique, runtime-assigned, never reused (Identity spec, rules 1, 6). The entry ID is the trail's primary key — every reference to a trail event uses this ID.
- **`timestamp`** — assigned by the runtime at the moment the event occurs, not when the entry is written. Monotonic within a workspace (Clock spec, invariant 1). Across workspaces, timestamps provide partial ordering only (Clock spec, invariant 2) — two entries in different workspaces with the same timestamp are concurrent.
- **`workspace`** — the workspace this event is associated with. Most events belong to exactly one workspace. System-level events (system lifecycle, capacity exhaustion) have `workspace: null` — they belong to the global trail but no specific workspace.
- **`actor`** — identifies who caused the event. Three categories:

| Actor value | Meaning | Example events |
|-------------|---------|----------------|
| Role name (`coordinator`, `worker`, or derived) | An agent in that role caused the event | `signal_emitted`, `checkpoint_created`, `envelope_created` |
| `protocol` | The runtime or protocol rules caused the event | `workspace_state_changed`, `budget_exceeded`, `envelope_rejected` |
| `user_id` | A specific human identified by their `user_id` (User spec, §2) | `gate_resolved`, `human_injection`, `escalation_resolved` |

The actor field serves two purposes: it classifies the source (agent, runtime, or human) and, for human actions, identifies the specific person. Agent actors are recorded by role, not by agent ID — agent identity is tracked through the workspace binding. Human actors are recorded by `user_id` — the protocol carries the specific human identity because the trail must answer "what did user X do?" without application-layer joins. The system tokens `"fallback"` and `"protocol"` are reserved — any actor value that is not a role name or a system token is a `user_id`.

- **`event_type`** — a string from the event registry (§9). The event type determines the schema of the `body` field. Unknown event types are rejected — the registry is closed within a protocol version.
- **`body`** — the type-specific payload. Each primitive's spec defines the body schema for its event types. The trail spec does not constrain body contents beyond requiring that they are structured objects (not opaque blobs). Bodies must be serializable and round-trip without loss.
- **`prev_hash`** — cryptographic hash of the previous entry in the same scope (local trail or global trail). Forms the hash chain that makes the trail tamper-evident (§4). The first entry in a scope has `prev_hash: null` — it is the chain's anchor.

**One event, one entry.** Every protocol event produces exactly one trail entry. No event produces zero entries (the no-gaps invariant, §4). No event produces multiple entries — if an event has compound effects (e.g., a workspace state change that also triggers a budget check), each effect is a separate event with its own entry. The trail is a sequence of atomic facts, not a sequence of compound narratives.

**Write-ahead.** The trail entry is written before the event takes effect. A workspace state transition is recorded in the trail before the state machine is updated. An envelope delivery is recorded before the envelope is placed in the inbox. This ensures that if the runtime crashes mid-operation, the trail reflects the intent — recovery can determine whether the operation completed or needs to be replayed (PROTOCOL §10.2).

## 3. Scopes

The trail exists at two scopes: local and global. The local trail is a workspace's own history — component 7 of the internal model (Workspace spec, §2). The global trail is the system's complete history — the union of all local trails, ordered by timestamp. The global trail is the master record; local trails are projections of it.

**Local trail.** Every workspace has a local trail — the ordered sequence of trail entries where `workspace` equals that workspace's ID. The local trail captures everything that happened to and within the workspace: state transitions, envelope deliveries, signal emissions, checkpoint creations, budget events, migration events, suspension events, visibility grants. The agent can read its own local trail to understand its history (Workspace spec, §2, component 7). The local trail is strictly ordered — entries are monotonic by timestamp with no duplicates (Clock spec, invariant 1). This total ordering within a workspace enables deterministic replay.

**Global trail.** The global trail is the union of all local trails plus system-level entries (`workspace: null`). It is ordered by timestamp, but the ordering is partial — entries from different workspaces with the same timestamp are concurrent (Clock spec, invariant 2). The global trail is not a separate data structure that duplicates local trails. It is the master: local trails are projections derived by filtering on `workspace`. Implementations may store the global trail as a single sequence and derive local views at query time, or maintain local trails independently and reconstruct the global view by merging — both are valid as long as the projection relationship holds.

**Access rules.** Trail access follows the role hierarchy:

| Role | Local trail access | Global trail access |
|------|--------------------|---------------------|
| Worker | Own workspace's local trail only | No access |
| Observer | Own workspace's local trail, plus observed workspaces' local trails (per visibility set) | No access |
| Coordinator | All local trails in the workspace tree | Full global trail |

The coordinator reads the global trail to understand the system's state. Workers read their own local trail to understand their history — they cannot read other workspaces' trails unless granted visibility (Workspace spec, §8). Observers read the trails of workspaces in their visibility set. These access rules are enforced by the runtime; unauthorized trail reads are rejected and recorded as trail entries themselves (a `trail_access_denied` event).

**System-level entries.** Some events do not belong to any workspace: system initialization, forced shutdown, capacity exhaustion at the system level. These entries have `workspace: null` and appear only in the global trail — they have no local trail to belong to. System-level entries use `actor: protocol` because the runtime, not an agent, caused them.

**The projection invariant.** For any workspace W, the local trail of W equals the global trail filtered to entries where `workspace == W.id`, preserving order. This is not an optimization — it is a correctness requirement. If the local trail and the global trail disagree about what happened to a workspace, the system is in an inconsistent state. Recovery validates this invariant (PROTOCOL §10.2).

## 4. Integrity Invariants

The trail's value depends on trust. If entries can be deleted, the trail cannot prove what happened. If entries can be modified, the trail cannot prove what was recorded. If events can occur without producing entries, the trail cannot prove what didn't happen. Three invariants protect these properties.

**Invariant 1: Append-only.** No trail entry may be deleted within the retention window (§8). The trail only grows. Entries move between storage tiers (hot → warm → cold) but are never removed until the retention policy for their tier permits it. Even then, deletion is of the physical storage — a `trail_compacted` summary entry proves the original entries existed. The append-only property is what makes the trail a reliable audit record. If deletion were permitted, recovery could not trust the trail to be complete, and security audits could not trust the trail to be honest.

**Invariant 2: Immutable.** No trail entry may be modified after creation. The `id`, `timestamp`, `workspace`, `actor`, `event_type`, `body`, and `prev_hash` are fixed at write time. There is no update operation — only append. If a previous entry was incorrect (e.g., a resource meter discrepancy discovered later), the correction is recorded as a new entry that references the original, not as a modification of the original. The trail is a court record, not a wiki.

**Invariant 3: No gaps.** Every protocol event produces exactly one trail entry. If an event occurs and no entry is recorded, the system is in violation. The no-gaps invariant is enforced by the write-ahead rule (§2): the trail entry is written before the event takes effect. If the write fails, the event does not proceed — the runtime must either retry the write or fail the operation. A gap in the trail means either the runtime has a bug or the storage layer has lost data — both are system failures that trigger recovery (PROTOCOL §10.2).

**Hash chain.** Each trail entry includes a `prev_hash` field — the cryptographic hash of the previous entry in the same scope. Local trails have their own hash chains (each entry hashes its predecessor within that workspace). The global trail has a separate chain (each entry hashes its global predecessor). The first entry in each scope has `prev_hash: null`.

The hash chain provides tamper evidence: modifying or removing any entry breaks the chain from that point forward. Verification is a linear scan — compute the hash of each entry and compare it to the next entry's `prev_hash`. A mismatch at entry N means entries at or before N have been tampered with. The hash chain does not prevent tampering — it makes tampering detectable.

**Hash algorithm.** The protocol does not mandate a specific hash algorithm. The runtime selects an algorithm at initialization and records it in the first trail entry's body. All entries in a run use the same algorithm. Implementations should use a cryptographically strong hash (SHA-256 or better). The choice is recorded, not prescribed, so that the protocol can outlive any single algorithm.

**Cross-scope anchoring.** Each local trail's chain is independent, but the global trail's chain interleaves entries from all workspaces. This means the global chain implicitly orders and anchors local entries — if a local entry appears in the global chain at position N, it is anchored to the global chain's integrity. Tampering with a local entry would break both the local chain and the global chain.

## 5. Querying

The trail is queryable. Every trail entry is a structured record with indexed fields — the trail is a database, not a file. Queries are read-only: they project, filter, and aggregate trail entries without modifying them.

**Query dimensions.** A trail query filters entries along one or more dimensions:

| Dimension | Field | Example |
|-----------|-------|---------|
| Workspace | `workspace` | All entries for `ws-task-03` |
| Actor | `actor` | All entries by user X (`actor == user_id`) or all agent entries (`actor == coordinator`) |
| Event type | `event_type` | All `checkpoint_created` entries |
| Time range | `timestamp` | All entries between T1 and T2 |
| Body field | `body.*` | All `workspace_state_changed` where `to_state == failed` |

**Compound queries.** Dimensions combine with AND logic. "All `envelope_delivered` events for workspace `ws-task-03` between T1 and T2" filters on event type, workspace, and time range simultaneously. OR logic is expressed as multiple queries — the protocol does not define a query language, only the dimensions that must be filterable.

**Aggregate queries.** Beyond filtering, the trail supports aggregation for analytical use:

- **Count** — how many entries match a filter. "How many checkpoints did workspace W produce?" "How many envelopes were rejected?"
- **Group** — partition matching entries by a dimension. "Count entries by event type for workspace W." "Count failures by workspace for the entire run."
- **Sum** — aggregate numeric body fields. "Total tokens consumed across all workspaces in group G" (summing `resource_usage.tokens` from `checkpoint_created` entries).
- **Timeline** — entries ordered by timestamp for a filter. This is the default — filtering without aggregation returns a timeline.

Aggregate queries are the foundation of comparative analysis. Comparing two workflow configurations means running the same aggregate queries against two runs' trails and comparing the results: total cost, total time, failure count, checkpoint count, human intervention count. The trail makes workflows measurable.

**Scope-aware querying.** Queries respect the access rules (§3). A worker querying its own local trail sees all entries. A worker querying a peer's trail sees nothing (unless visibility is granted). The coordinator queries the global trail and sees everything. The runtime enforces scope restrictions at query time — a query that requests entries outside the caller's access scope returns an empty result, not an error. This is consistent with visibility semantics elsewhere in the protocol: you simply do not see what you do not have access to.

**Query interface.** The protocol defines what must be queryable, not how queries are expressed. Implementations may use SQL, a custom query API, in-memory filtering, or any mechanism that supports the dimensions and aggregations above. The protocol requires only that the trail store can answer these queries efficiently enough to support real-time observability — the coordinator must be able to query the trail during a run, not only after it.

## 6. Observability Views

Views are structured projections of the trail designed for specific audiences and questions. They are not separate data structures — each view is a query pattern applied to the trail. The protocol defines five standard views that implementations should support.

**6.1 Run Overview.** A high-level snapshot of the entire run. Shows every workspace's current state, grouped by state category (productive, suspended, resolution, terminal). Answers: "What is happening right now?" and "How far along is the run?"

Constructed from: `workspace_created` entries (to enumerate workspaces), the latest `workspace_state_changed` entry per workspace (to determine current state), and `task_status_changed` entries (to show task progress as X of Y completed).

The run overview is the coordinator's dashboard — the first thing read when assessing system health. It is also the human highway's primary visibility surface (Human Highway spec, §2.1).

**6.2 Workspace Timeline.** A chronological view of a single workspace's activity. Shows every trail entry for that workspace in timestamp order — state transitions, envelope deliveries, signal emissions, checkpoint creations, budget events. Answers: "What happened in this workspace, and in what order?"

Constructed from: the workspace's local trail, unfiltered and unmodified. The workspace timeline is the simplest view — it is the local trail itself, presented chronologically. It is also the recovery target: replaying a workspace timeline reconstructs the workspace's state (PROTOCOL §10.2).

**6.3 Message Flow.** A directed view of communication between workspaces. Shows envelopes and signals as edges between workspace nodes. Answers: "Who is talking to whom, and what are they saying?"

Constructed from: `envelope_created` and `envelope_delivered` entries (showing message flow), `signal_emitted` and `signal_delivered` entries (showing notification flow). Each edge carries the envelope type or signal type as a label. The message flow view reveals the communication topology — which workspaces are active collaborators, which are isolated, where bottlenecks form.

**6.4 Comparison View.** A side-by-side analysis of two or more runs. Shows the same aggregate metrics for each run, enabling direct comparison. Answers: "Which workflow configuration produced better results?"

Constructed from: aggregate queries (§5) applied to each run's trail. Standard comparison metrics include: total cost, total wall time, workspace count, failure count, retry count, checkpoint count, human intervention count, and per-task completion time. The comparison view is the protocol's core instrument for evaluating workflows — the finding that "review adds ~80% time but catches silent failures" was produced by this view.

Comparison requires that runs use the same task graph (or comparable graphs) so that metrics are meaningful. Comparing a 3-task run against a 30-task run tells you nothing about workflow quality. The protocol does not enforce comparability — it provides the instrument. The coordinator (or human) decides what to compare.

**6.5 Highway Activity View.** All human interactions extracted from the trail. Shows gate events (triggered, resolved, timed out), injections, and escalations in chronological order. Answers: "What did humans do, and when?"

Constructed from: all trail entries where `actor` is a `user_id` (not a role name or system token), plus gate events (`gate_triggered`, `gate_resolved`, `gate_timeout`), injection events (`human_injection`), and escalation events (`escalation_received`, `escalation_resolved`, `escalation_timeout`). The highway activity view gives the human supervisor a record of their own interventions — and in multi-user deployments, can be filtered by `user_id` to show a single user's actions.

## 7. Storage Model

The trail grows monotonically. Every protocol event adds an entry, and entries are never deleted within the retention window. Without a storage model, the trail would consume unbounded resources. The three-tier storage model manages this growth by matching access latency to access frequency — recent entries are fast to read, older entries are progressively cheaper to store.

**Three tiers:**

| Tier | Contents | Latency | Durability |
|------|----------|---------|------------|
| Hot | Entries for active workspaces (non-terminal state) | Sub-millisecond | Must survive runtime restart |
| Warm | Entries for closed or failed workspaces within the retention window | Milliseconds to seconds | Must survive runtime restart |
| Cold | Entries archived beyond the retention window or compacted from warm | Seconds to minutes | Must survive storage failure (replicated or backed up) |

**Hot tier.** Entries for workspaces in `idle`, `active`, `blocked`, `migrating`, `suspended`, `integrating`, or `conflicted` state. These entries are actively read — by the coordinator for observability, by agents for their local trail, by the runtime for state reconstruction. Sub-millisecond read latency is required because trail queries are in the critical path: the coordinator reads the trail to make dispatch decisions, and the runtime reads the trail to enforce the write-ahead rule. When a workspace reaches a terminal state (`closed` or `failed`), its entries are candidates for migration to warm.

**Warm tier.** Entries for terminal workspaces that are still within the retention window. These entries are read less frequently — during post-run analysis, comparison queries, or recovery after a delayed failure. Millisecond-to-second latency is acceptable. The warm tier is the trail's primary storage — most entries live here for most of their lifetime, because most workspaces are terminal by the time the run completes.

**Cold tier.** Entries that have been archived beyond the retention window or compacted from warm storage. These entries are rarely read — only for historical analysis, compliance audits, or long-term comparisons across runs. Seconds-to-minutes latency is acceptable. Cold storage may use compressed formats, object storage, or offline media. The protocol requires only that cold entries remain queryable through the standard query interface (§5), even if queries are slow.

**Tier migration.** Entries move from hot to warm when their workspace reaches a terminal state. Entries move from warm to cold when compaction runs (§8) or when the retention window expires. Entries never move backward — cold entries do not return to warm, warm entries do not return to hot. Tier migration is a storage optimization; it does not change the entries themselves. An entry in cold storage has the same `id`, `timestamp`, `body`, and `prev_hash` as when it was in hot storage.

**Durability requirements.** Hot and warm tiers must survive a runtime restart — if the runtime crashes and recovers, the trail must still be complete. This means hot and warm storage must be durable (written to persistent storage, not only held in memory). Cold storage must survive storage failure — replication or backup is required. The trail is the protocol's single source of truth; losing trail data is a system failure.

## 8. Compaction and Retention

The trail is append-only but not infinite. Compaction reduces the storage footprint of the warm tier by archiving non-essential entries to cold storage while preserving essential entries in place. Retention policies govern how long entries persist in each tier before they may be deleted entirely.

**Compaction.** Compaction operates on warm-tier entries — entries for terminal workspaces. It separates essential entries from archivable entries:

**Essential entries** (preserved in warm, never compacted):

| Category | Event types |
|----------|-------------|
| State transitions | `workspace_created`, `workspace_state_changed` (terminal) |
| Checkpoints | `checkpoint_created` |
| Integration decisions | `integration_completed`, `integration_aborted` |
| Conflicts | `conflict_detected`, `conflict_resolved` |
| Human decisions | `gate_resolved`, `human_injection`, `escalation_resolved` |
| Task milestones | `task_created`, `task_completed`, `task_failed` |
| Failures | Any entry where `to_state == failed` or `reason` indicates error |

**Archivable entries** (moved to cold during compaction):

| Category | Event types |
|----------|-------------|
| Intermediate state changes | `workspace_state_changed` (non-terminal transitions) |
| Routine deliveries | `envelope_delivered`, `signal_delivered` |
| Budget monitoring | `budget_warning`, `liveness_warning` |
| Operational events | `priority_changed`, `visibility_granted` |

The distinction is recoverability: essential entries are sufficient to reconstruct the workspace's outcome — what was produced, whether it succeeded, what decisions were made. Archivable entries capture the journey — how the workspace got there. Both are valuable; essential entries are more valuable per byte.

**Compaction summary.** When compaction runs, it produces a `trail_compacted` entry in the warm tier:

```yaml
trail_compacted:
  workspace: workspace_id
  entries_archived: integer        # count of entries moved to cold
  entries_preserved: integer       # count of entries kept in warm
  archive_range:                   # timestamp range of archived entries
    from: datetime
    to: datetime
  archive_hash: string             # hash of the archived entries (integrity proof)
```

The compaction summary proves that the archived entries existed — it records the count, time range, and hash. The no-gaps invariant is preserved: the entries still exist in cold storage, and the summary in warm storage proves their existence. Compaction does not delete — it relocates.

**Retention policies.** Retention governs how long entries persist before they may be deleted:

| Tier | Retention | Deletion permitted? |
|------|-----------|---------------------|
| Hot | As long as workspace is active | Never — entries move to warm, not deleted |
| Warm | Configurable per run (recommended: run duration + analysis window) | After retention expires, entries may be moved to cold |
| Cold | Configurable per deployment (recommended: months to years) | After retention expires, entries may be permanently deleted |

**Hot tier entries are never deleted.** They move to warm when the workspace terminates. Deleting hot entries would violate the write-ahead rule — the runtime needs hot entries for in-flight operations.

**Warm tier retention** should be long enough for post-run analysis. A run that completes in 10 minutes may need warm retention of hours (for immediate analysis) or days (for batch comparison across runs). The protocol recommends warm retention of at least the run duration plus an implementation-defined analysis window.

**Cold tier retention** is a deployment decision. Compliance requirements may demand years of retention. Cost-sensitive deployments may delete after weeks. The protocol requires only that cold entries remain queryable until their retention expires — once expired, deletion is permitted (not required).

**Snapshots.** A snapshot is a point-in-time immutable copy of the trail — all entries up to a given timestamp, frozen as a single artifact. Snapshots serve two purposes: archival (a snapshot is the boundary between "retainable" and "deletable") and replay (a snapshot is a known-good starting point for recovery). Snapshots are taken at the warm tier — they capture the post-compaction state of a completed run. A snapshot's integrity is verifiable through the hash chain — the chain from the first entry to the snapshot boundary must be unbroken.

## 9. Event Registry

The event registry is the consolidated index of every trail event type defined across the Protocol tier. Each primitive's spec defines its own trail events — this registry collects them into a single reference. The registry is closed within a protocol version: only event types listed here are valid. Unknown event types are rejected by the runtime.

**Workspace events** (Workspace spec, §13) — 22 events:

| Event | Category |
|-------|----------|
| `workspace_created` | Lifecycle |
| `workspace_state_changed` | Lifecycle |
| `workspace_rejected` | Lifecycle |
| `budget_warning` | Resource |
| `budget_exceeded` | Resource |
| `budget_modified` | Resource |
| `liveness_warning` | Resource |
| `priority_changed` | Resource |
| `visibility_granted` | Visibility |
| `batch_abort` | Group |
| `batch_priority_changed` | Group |
| `migration_started` | Migration |
| `migration_completed` | Migration |
| `migration_failed` | Migration |
| `suspension_started` | Suspension |
| `suspension_resumed` | Suspension |
| `graceful_termination_initiated` | Graceful termination |
| `graceful_termination_expired` | Graceful termination |
| `conflict_detected` | Integration |
| `conflict_resolved` | Integration |
| `workspace_ownership_transferred` | Ownership |
| `workspace_reparented` | Ownership |

**User events** (User spec, §8) — 12 events:

| Event | Category |
|-------|----------|
| `user_created` | User lifecycle |
| `authentication_succeeded` | Authentication |
| `authentication_failed` | Authentication |
| `user_suspended` | User lifecycle |
| `user_resumed` | User lifecycle |
| `user_blocked` | User lifecycle |
| `user_unblocked` | User lifecycle |
| `user_deactivated` | User lifecycle |
| `user_reactivated` | User lifecycle |
| `capability_granted` | Capability lifecycle |
| `capability_revoked` | Capability lifecycle |
| `capability_denied` | Capability enforcement |

**Signal events** (Signal spec, §8) — 2 events:

| Event | Category |
|-------|----------|
| `signal_emitted` | Signal lifecycle |
| `signal_delivered` | Signal lifecycle |

**Envelope events** (Envelope spec, §10) — 9 events:

| Event | Category |
|-------|----------|
| `envelope_created` | Envelope lifecycle |
| `envelope_delivered` | Envelope lifecycle |
| `envelope_rejected` | Envelope lifecycle |
| `envelope_undeliverable` | Envelope lifecycle |
| `envelope_redelivered` | Envelope lifecycle |
| `port_right_created` | Port right lifecycle |
| `port_right_transferred` | Port right lifecycle |
| `port_right_revoked` | Port right lifecycle |
| `port_right_consumed` | Port right lifecycle |

**Checkpoint events** (Checkpoint spec, §7) — 3 events:

| Event | Category |
|-------|----------|
| `checkpoint_created` | Checkpoint lifecycle |
| `checkpoint_rejected` | Checkpoint lifecycle |
| `resource_discrepancy` | Checkpoint lifecycle |

**Task events** (Task spec, §8) — 7 events:

| Event | Category |
|-------|----------|
| `task_created` | Task lifecycle |
| `task_approved` | Task lifecycle |
| `task_assigned` | Task lifecycle |
| `task_status_changed` | Task lifecycle |
| `task_completed` | Task lifecycle |
| `task_failed` | Task lifecycle |
| `graph_created` | Task lifecycle |

**Integration events** (Integration spec, §8) — 3 events:

| Event | Category |
|-------|----------|
| `integration_started` | Integration lifecycle |
| `integration_completed` | Integration lifecycle |
| `integration_aborted` | Integration lifecycle |

**Human Highway events** (Human Highway spec, §8) — 8 events:

| Event | Category |
|-------|----------|
| `gate_triggered` | Gate |
| `gate_resolved` | Gate |
| `gate_timeout` | Gate |
| `gate_reentry_blocked` | Gate |
| `human_injection` | Injection |
| `escalation_received` | Escalation |
| `escalation_resolved` | Escalation |
| `escalation_timeout` | Escalation |

**Recovery events** (Recovery spec, §4) — 2 events:

| Event | Category |
|-------|----------|
| `system_degraded` | System failure |
| `recovery_completed` | Recovery lifecycle |

**Security events** (Security spec, §9) — 1 event:

| Event | Category |
|-------|----------|
| `integrity_violation` | Integrity |

Note: `authentication_failed` is listed under User events (above) since credential verification is the authentication boundary's concern. Its canonical schema is defined in the Security spec §9, which covers all entity types (agents, coordinators, and users).

**Trail-own events** — 3 events:

| Event | Category |
|-------|----------|
| `trail_compacted` | Storage lifecycle |
| `trail_access_denied` | Access control |
| `trail_snapshot_created` | Storage lifecycle |

**Total: 72 unique event types** across 11 sources (8 primitive specs + 2 mechanism specs + trail-own). The `conflict_detected` and `conflict_resolved` events appear in both Workspace and Integration specs — they are the same events, defined in Workspace §13 (where the state transition occurs) and referenced in Integration §8 (where the cause originates). The `workspace_ownership_transferred` and `workspace_reparented` events appear in both Workspace §13 and User §8 — they are workspace events triggered by ownership operations; listed under Workspace, not duplicated under User. The `authentication_failed` event appears in both User §8 and Security §9 — the User spec lists it; the Security spec defines the canonical schema. The port right events are defined in Envelope spec §10 alongside envelope events. The registry counts each once.

**Registry growth.** New event types are added only through new protocol versions or new specs. The Infrastructure, System, and Runtime tiers will add their own trail events when those specs are drafted — the registry will grow accordingly. Within a protocol version, the registry is frozen.

## 10. Conformance Requirements

| Requirement | Level | Description |
|-------------|-------|-------------|
| Trail entry schema | Core | Runtime MUST produce trail entries conforming to the schema defined in §2 for every protocol event. |
| One event one entry | Core | Every protocol event MUST produce exactly one trail entry. No event may produce zero or multiple entries. |
| Write-ahead | Core | Trail entries MUST be written before the corresponding event takes effect. If the write fails, the event MUST NOT proceed. |
| Immutability | Core | Trail entries MUST NOT be modified after creation. No update operation exists. |
| Append-only | Core | Trail entries MUST NOT be deleted within the retention window. |
| Hash chain | Core | Every trail entry MUST include a `prev_hash` linking to the previous entry in its scope. The runtime MUST validate chain integrity. |
| Local trail projection | Core | For any workspace W, the local trail MUST equal the global trail filtered to entries where `workspace == W.id`, preserving order. |
| Scope-aware access | Core | Runtime MUST enforce trail access rules (§3). Workers read own local trail only. Observers read per visibility set. Coordinator reads global trail. |
| Global trail ordering | Core | The global trail MUST order entries by timestamp. Entries with equal timestamps from different workspaces are concurrent — no artificial ordering imposed. |
| Query dimensions | Standard | Trail store MUST support filtering by workspace, actor, event type, time range, and body fields as defined in §5. |
| Compound queries | Standard | Trail store MUST support AND-combination of multiple query dimensions. |
| Aggregate queries | Standard | Trail store MUST support count, group, and sum aggregations over query results. |
| Real-time queryability | Standard | Trail store MUST be queryable during a run, not only after completion. Hot-tier entries MUST be available for query within the same run. |
| Three-tier storage | Standard | Runtime MUST implement hot, warm, and cold storage tiers as defined in §7 with the specified latency and durability requirements. |
| Tier migration | Standard | Runtime MUST migrate entries from hot to warm when workspaces reach terminal state. Entries MUST NOT move backward between tiers. |
| Hot and warm durability | Standard | Hot and warm tier entries MUST survive runtime restart. |
| Compaction | Standard | Runtime MUST support compaction of warm-tier entries, preserving essential entries and producing `trail_compacted` summary entries. |
| Retention policies | Standard | Runtime MUST support configurable retention windows for warm and cold tiers. |
| Observability views | Full | Runtime SHOULD support the five standard observability views defined in §6. |
| Snapshots | Full | Runtime SHOULD support point-in-time trail snapshots with hash-chain-verifiable integrity. |
| Cold tier queryability | Full | Cold-tier entries SHOULD remain queryable through the standard query interface. |
| User attribution on highway events | Standard | Human-originated trail entries MUST carry the acting human's `user_id` in the `actor` field (User spec, §2). Trail queries MUST support filtering by specific `user_id`. |
| Cross-run comparison | Full | Trail store SHOULD support aggregate queries across multiple runs for workflow comparison. |

## 11. Implementation Notes

These notes are non-normative. They capture practical guidance for implementers.

**Trail store as a database.** The simplest conformant implementation uses an append-only table in SQLite or PostgreSQL. Each trail entry is a row. The `workspace`, `actor`, `event_type`, and `timestamp` columns are indexed for query performance. The `body` column stores JSON. The hash chain is a computed column or maintained by a trigger on insert. SQLite is sufficient for single-runtime deployments; PostgreSQL or a distributed database is needed for multi-runtime federation.

**Hot tier as in-memory with write-through.** Keep hot-tier entries in an in-memory data structure (ordered map keyed by entry ID) and write-through to persistent storage on every append. This satisfies both the sub-millisecond latency requirement and the durability requirement. The in-memory structure is the read path; the persistent store is the durability path. On restart, reload hot-tier entries from persistent storage.

**Hash chain implementation.** Hash the concatenation of the entry's fields (excluding `prev_hash`) using the selected algorithm. The hash input should be a canonical serialization — implementations must define a deterministic byte representation to avoid hash mismatches from field reordering or whitespace differences. JSON with sorted keys and no whitespace is a simple canonical form.

**Write-ahead with transactions.** If the trail store and the state machine share a database, use a single transaction: write the trail entry and update the state in one atomic operation. This simplifies the write-ahead guarantee — either both succeed or neither does. If the trail store is separate from the state machine, write the trail entry first, then update the state. If the state update fails after the trail write, the trail contains an entry for an event that did not complete — recovery detects this (the trail says "transitioning to X" but the state machine is still at the previous state) and replays the transition.

**Compaction as a background job.** Run compaction periodically (e.g., after each workspace termination, or on a timer) as a background process. Compaction reads warm-tier entries, classifies them as essential or archivable, moves archivable entries to cold storage, and writes the `trail_compacted` summary. Compaction should not block hot-tier writes — it operates on a different tier.

**Observability views as materialized queries.** The five standard views can be implemented as cached query results that refresh on a configurable interval. The run overview refreshes on every `workspace_state_changed` event. The workspace timeline is a live view of the local trail — no caching needed. The message flow and highway activity views refresh on relevant event types. The comparison view is computed on demand (it queries completed runs, not live data).

**Scope enforcement at the query layer.** Implement trail access control in the query layer, not the storage layer. The storage layer stores all entries uniformly. The query layer checks the caller's workspace ID and role, computes the accessible scope (own local trail, visibility set trails, or global), and filters results accordingly. This keeps the storage layer simple and the access logic centralized.

**Cold storage as compressed archives.** Cold-tier entries can be stored as compressed files (gzip, zstd) grouped by workspace or by time range. Each archive file includes a manifest listing entry IDs and timestamps for fast lookup without decompression. Querying cold storage decompresses the relevant archive, applies the query filter, and returns results. This trades latency for storage cost — acceptable for the cold tier's seconds-to-minutes latency requirement.

**Event registry validation.** At startup, load the event registry and build a lookup table from event type string to body schema. On every trail write, validate the `event_type` against the registry and the `body` against the expected schema. Reject unknown event types immediately — they indicate a protocol version mismatch or a bug. Schema validation can be strict (reject malformed bodies) or lenient (log a warning but accept) depending on the deployment's tolerance for schema drift.

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
