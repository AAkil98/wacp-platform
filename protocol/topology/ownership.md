# WACP: Ownership Domains

## Metadata

```yaml
title: "WACP: Ownership Domains"
id: wacp-spec-ownership
type: constituent-spec
tier: abstract
category: topology
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4.8 (ownership rules)
  - §6.5 (workspace tree, ownership-bounded cascade)
depends_on:
  - wacp-spec-tree
  - wacp-spec-workspace
  - wacp-spec-user
  - wacp-spec-identity
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, topology, ownership, domains, partition, failure-boundary, transfer, escalation-routing]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Ownership as a Partition](#2-ownership-as-a-partition)
3. [Domain Structure](#3-domain-structure)
4. [Transfer](#4-transfer)
5. [Failure Cascade and Ownership](#5-failure-cascade-and-ownership)
6. [Escalation Routing](#6-escalation-routing)
7. [Invariants](#7-invariants)
8. [Trail Events](#8-trail-events)
9. [Conformance Requirements](#9-conformance-requirements)
10. [Implementation Notes](#10-implementation-notes)
11. [References](#11-references)

---

## 1. Purpose

Causation answers "what caused this workspace?" Ownership answers "who is responsible for this workspace?" Both partition the workspace tree's nodes, but they differ in a fundamental way: causation is immutable (originator never changes), ownership is mutable (transfer changes it).

The User spec (§4) defines ownership rules — assignment, inheritance, override, transfer. The Tree spec (§4.3) defines ownership-bounded failure cascade. This spec defines ownership domains as a topological structure: how the `owner` field partitions the workspace tree into domains, what properties those domains have, how transfer reshapes them, and how they interact with failure cascade and escalation routing.

## 2. Ownership as a Partition

### 2.1 Definition

An ownership domain is the set of all workspaces owned by a single user. The `owner` field on each workspace determines which domain it belongs to.

**Formally:** Given a workspace tree T with nodes W, the ownership partition P is a set of domains {D₁, D₂, ...}, one per distinct `owner` value, where:
- Dᵢ contains all nodes w ∈ W where w.owner == vᵢ
- Every node belongs to exactly one domain
- The union of all domains equals W

### 2.2 Formal Properties

| Property | Value |
|----------|-------|
| Type | Partition of the workspace tree's node set |
| Partition key | `owner: user_id` (root workspace owner is implementation-defined — User spec §4.2, rule 3) |
| Mutability | **Mutable.** Transfer moves a workspace from one domain to another. |
| Connectivity | Not required — a domain may contain disconnected workspaces scattered across the tree |
| Contiguity | Not required — a user's workspaces need not be structurally adjacent |
| Inheritance | Default: a child inherits its parent's owner. Override: the coordinator may set a different owner at creation. |

### 2.3 Contrast with Causation

| | Ownership | Causation |
|---|---|---|
| Field | `owner` | `originator` |
| Type | `user_id` (root workspace: implementation-defined) | `user_id \| "system"` |
| Mutable | Yes (transfer) | No |
| Inheritance | Default, with coordinator override | Unconditional, no override |
| Domains are contiguous | No | Yes (causal subtrees are downward-closed) |
| Boundary direction | Any (coordinator can set any owner on any workspace) | Unidirectional (`"system"` → `user_id` only) |

Ownership domains are less structured than causal subtrees. A causal subtree is always a contiguous, downward-closed subtree of the structural tree. An ownership domain can be scattered — a user may own workspaces at any depth, in any subtree, with non-owned workspaces in between.

## 3. Domain Structure

### 3.1 Domain Shapes

**Contiguous subtree.** The simplest case: a user owns a workspace and all its descendants. This arises from inheritance — the coordinator creates a workspace with `owner: X`, and all children inherit the owner. The domain is a complete subtree of the structural tree.

**Scattered set.** The general case: a user owns workspaces at different points in the tree. This arises from explicit override (the coordinator sets `owner: X` on workspaces that are not structurally related) or from transfer (ownership moved from one user to another on individual workspaces).

**Single workspace.** A domain may contain a single workspace — a user who owns exactly one unit of work.

**Empty domain.** A domain becomes empty when all its workspaces reach terminal states. The user's `user_id` still exists (user spec §3.6 — deactivation is not deletion), but they own no active workspaces. An empty domain is a valid state — the user may own work again if new workspaces are created with them as owner.

### 3.2 Ownership Boundaries

An ownership boundary is a structural edge where the parent and child have different owners. Unlike causal boundaries (which are unidirectional), ownership boundaries can point in any direction — any user can own a workspace whose parent is owned by a different user.

**Ownership boundaries determine failure cascade scope.** The failure cascade algorithm (Tree spec §4.3) stops at ownership boundaries: a parent's failure cascades to same-owner children but reparents cross-owner children to the coordinator. The ownership boundary is the blast radius delimiter.

**Example:**

```
ws-root (coordinator, owner: impl-defined)
 ├── ws-A (worker, owner: alice)
 │    ├── ws-A1 (worker, owner: alice)
 │    └── ws-A2 (worker, owner: bob)    ← ownership boundary
 │         └── ws-A2a (worker, owner: bob)
 └── ws-B (worker, owner: bob)
      └── ws-B1 (worker, owner: bob)
```

If ws-A fails:
- ws-A1 fails (same owner: alice)
- ws-A2 is reparented to ws-root (different owner: bob)
  - ws-A2a survives as part of ws-A2's subtree (reparenting moves the subtree as a unit)

## 4. Transfer

Transfer is the ownership domain's topological mutation. When ownership of a workspace is transferred from user X to user Y, the workspace moves from domain X to domain Y.

### 4.1 Transfer Properties

| Property | Value |
|----------|-------|
| Scope | Single workspace (User spec §4.5). Transfer does not cascade to children. |
| Atomicity | Atomic — the workspace is in domain X or domain Y, never in transition. |
| Trail record | `workspace_ownership_transferred` event (User spec §4.5). |
| Effect on children | None — children retain their current owner. If they inherited the old owner, they keep it. |
| Effect on originator | None — transfer changes owner, not originator. |
| Effect on agents | None — transfer is transparent to agents. They are unaware of ownership. |
| Effect on escalation routing | Immediate — escalations from the workspace now route to the new owner. |
| Effect on failure cascade | Immediate — the workspace is now in the new owner's domain for cascade purposes. |

### 4.2 Transfer and Domain Topology

Transfer can create, grow, shrink, fragment, or merge ownership domains:

**Create.** Transfer a workspace to a user who currently owns no workspaces. A new domain appears.

**Grow.** Transfer a workspace to a user who already owns other workspaces. The receiving domain grows by one node.

**Shrink.** Transfer a workspace away from a user. The donor domain shrinks by one node. If it was the last workspace, the domain becomes empty.

**Fragment.** Transfer a workspace that was connecting two parts of a domain. The donor domain may split into disconnected components. (Domains can already be scattered — fragmentation increases the number of components.)

**Merge.** Transfer a workspace to a user whose domain now becomes contiguous with another workspace they own. The receiving domain's components merge. (This is unlikely in practice but topologically possible.)

### 4.3 Non-Cascading Transfer

Transfer applies to a single workspace (User spec §4.5). It does NOT propagate to children. If a user transfers ownership of workspace W, W's children retain their current owners — which may still be the old owner (via inheritance at creation time).

**Consequence:** Transfer can create an ownership "island" — a workspace owned by user Y, whose parent is owned by user X and whose children are owned by user X. The island is structurally surrounded by a different owner's domain.

**Why non-cascading.** Automatic cascade would be dangerous — transferring a high-level workspace would silently reassign all descendant work. The protocol requires explicit per-workspace transfer to prevent accidental mass reassignment (User spec §4.5).

## 5. Failure Cascade and Ownership

The ownership partition is the failure cascade's primary structural input. The cascade algorithm (Tree spec §4.3) uses ownership boundaries to determine blast radius.

### 5.1 Same-Owner Cascade

When a workspace W fails, the cascade walks the structural tree downward from W. At each node, if the child has `owner == W.owner`, the child transitions to `failed` with `reason: parent_failed` and the cascade recurses into that child's children. If the child has a different owner, the cascade does not enter that child — it is reparented instead (§5.2). The cascade recurses to arbitrary depth within the ownership boundary, skipping (and reparenting) any cross-owner node along the way.

### 5.2 Cross-Owner Reparenting

When a workspace W fails and a child C has `owner != W.owner`, the child is reparented to the coordinator rather than failed. The child's subtree (regardless of ownership) moves as a unit to the coordinator.

### 5.3 Root Failure

When the root workspace fails, all workspaces fail regardless of ownership. There is no parent to reparent to — the coordinator IS the parent. Root failure is total (Tree spec §4.3, invariant T-9).

### 5.4 Ownership, Cascade, and Causation

Failure cascade follows the structural tree, filtered by ownership. It does NOT follow causation. A workspace with `originator: user-A` and `owner: user-B` is in user-B's ownership domain for cascade purposes — if user-B's parent workspace fails, this workspace is cascaded based on user-B's ownership, not user-A's causation.

The three partitions (structural, causal, ownership) are independent. Failure cascade uses structural + ownership. Causal impact uses structural + causal. Escalation routing uses ownership (§6).

## 6. Escalation Routing

Escalation signals route to the workspace's owner by default (User spec §4.1, Human-Highway spec §2.4). The ownership domain determines the human who receives escalations.

### 6.1 Default Routing

When an agent emits an `escalation` signal, the highway routes it to the workspace's `owner`. The target user's state determines delivery (Human-Highway spec §2.4): if `active`, the escalation is delivered immediately; if `suspended` or `blocked`, the escalation is queued until the user returns; if `deactivated`, the escalation is rejected and the coordinator is notified to reroute.

### 6.2 Routing After Transfer

If a workspace's ownership is transferred from user X to user Y, subsequent escalations route to user Y. The transfer takes immediate effect on escalation routing — no explicit rerouting step is needed.

### 6.3 Routing and Causation

Escalation routing follows ownership, not causation. A workspace with `originator: user-A` and `owner: user-B` sends escalations to user-B. User-A caused the workspace; user-B is responsible for it. Escalations go to the responsible party.

## 7. Invariants

| # | Invariant | Enforced by |
|---|-----------|-------------|
| OW-1 | **Every workspace has an owner.** The `owner` field is required. | Runtime, at creation |
| OW-2 | **Owner is a `user_id`.** Unlike originator, owner cannot be `"system"`. Workspaces exist on behalf of a human. The root workspace is the single exception: its owner is implementation-defined (User spec §4.2, rule 3) because the coordinator is the system, not a user. | Runtime, at creation |
| OW-3 | **Transfer is per-workspace.** Transfer does not cascade to children. | Runtime, at transfer |
| OW-4 | **Transfer does not change originator.** Owner and originator are independent fields. | Runtime, at transfer |
| OW-5 | **Ownership boundaries delimit failure cascade.** Failure cascade propagates within same-owner subtrees; cross-owner children are reparented. | Runtime, during cascade |
| OW-6 | **Escalation routing follows ownership.** The owner of a workspace receives its escalations. | Highway, at escalation delivery |

## 8. Trail Events

Ownership operations produce events defined in other specs:

| Operation | Trail event | Defined in |
|-----------|------------|------------|
| Workspace creation with owner | `workspace_created` (includes `owner` field) | Workspace spec §13 |
| Ownership transfer | `workspace_ownership_transferred` | User spec §4.5 |
| Failure cascade (same-owner) | `workspace_state_changed` (with `trigger: parent_failed`) | Workspace spec §13 |
| Reparenting (cross-owner) | `workspace_reparented` | Workspace spec §13 |

## 9. Conformance Requirements

| # | Requirement | Level |
|---|-------------|-------|
| OW-C1 | Every workspace MUST have an `owner` field. The field MUST be a `user_id`. | Core |
| OW-C2 | Default ownership inheritance MUST be applied when no explicit owner is specified at creation. | Core |
| OW-C3 | Failure cascade MUST respect ownership boundaries: same-owner descendants are failed, cross-owner descendants are reparented. | Core |
| OW-C4 | Ownership transfer MUST apply to a single workspace. Transfer MUST NOT cascade to children. | Standard |
| OW-C5 | Transfer MUST NOT modify the workspace's originator. | Standard |
| OW-C6 | Escalation routing MUST follow the current owner. Transfer MUST take immediate effect on routing. | Standard |
| OW-C7 | The runtime MUST support ownership domain queries — enumerating all workspaces owned by a given user. | Standard |

**7 requirements.** Core ensures every workspace is owned, inheritance works, and cascade respects ownership. Standard ensures transfer semantics and escalation routing correctness.

## 10. Implementation Notes

*This section is non-normative.*

**Ownership as an indexed column.** Like the originator index (Causation spec §10), maintain `Map[user_id → Set[workspace_id]]` for the ownership partition. Domain queries are index lookups. Transfer is an index move: remove from old user's set, add to new user's set.

**Cascade traversal.** The failure cascade algorithm (Tree spec §4.3) needs the parent's owner and each child's owner. If workspaces are stored in a flat table, this is a join on `parent` with a filter on `owner`. Pre-indexing children by parent enables O(children) cascade per level.

**Escalation routing table.** Maintain a routing table: `Map[workspace_id → user_id]` for escalation routing. Initialized from the `owner` field at creation. Updated on transfer. The highway consults this table, not the workspace record, for routing decisions — this allows routing to be updated atomically with transfer.

**Ownership history.** The `workspace_ownership_transferred` trail event records transfers. To reconstruct ownership at a historical point in time, replay transfer events up to the target timestamp. Implementations may cache ownership snapshots at regular intervals for efficient historical queries.

## 11. References

| Spec | Sections referenced | Relationship |
|------|-------------------|--------------|
| [Workspace Tree](tree.md) | §4.3 (failure cascade), §4.4 (reparenting), §6 (invariants T-5, T-8, T-10) | Cascade and reparenting use ownership boundaries |
| [Causation](causation.md) | §2 (originator field), §3 (causal forest), §7 (invariants CA-3, CA-4) | Ownership and causation are orthogonal partitions; transfer does not affect originator |
| [Workspace](../primitives/workspace.md) | §4 (creation, owner field), §5 (tree, ownership-bounded cascade), §13 (trail events) | Workspaces carry the owner field |
| [User](../primitives/user.md) | §3.1 (user states — affects escalation delivery), §4 (ownership rules, reparenting, transfer), §4.5 (transfer constraints and trail event) | User spec defines ownership rules; this spec defines ownership topology |
| [Human-Highway](../mechanisms/human-highway.md) | §2.4 (escalation routing) | Escalation routing follows ownership |
| [PROTOCOL.md](../PROTOCOL.md) | §4.8 (ownership rules), §6.5 (ownership-bounded cascade) | Canonical ownership definition |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
