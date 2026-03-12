# WACP: Workspace Tree

## Metadata

```yaml
title: "WACP: Workspace Tree"
id: wacp-spec-tree
type: constituent-spec
tier: abstract
category: topology
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §6.5 (workspace tree)
  - §4.8 (originator, coordinator model)
depends_on:
  - wacp-spec-workspace
  - wacp-spec-signal
  - wacp-spec-identity
  - wacp-spec-user
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, topology, tree, structural-tree, causal-tree, operations, invariants]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [The Structural Tree](#2-the-structural-tree)
3. [The Causal Tree](#3-the-causal-tree)
4. [Tree Operations](#4-tree-operations)
5. [Subtree as a Unit](#5-subtree-as-a-unit)
6. [Tree Invariants](#6-tree-invariants)
7. [Trail Events](#7-trail-events)
8. [Conformance Requirements](#8-conformance-requirements)
9. [Implementation Notes](#9-implementation-notes)
10. [References](#10-references)

---

## 1. Purpose

The protocol uses the word "tree" throughout — workspace tree, subtree, tree root, tree propagation, tree containment. Workspace.md §5 defines the tree as a workspace concern: "workspaces form a strict tree rooted at the coordinator's workspace." Signal.md defines propagation as an upward tree traversal. The user spec defines the originator chain as a causal tree overlaying the structural tree. But no spec defines the tree as a formal topological structure — what kind of tree, what properties it has, what operations are valid on it, and how the structural tree and causal tree relate.

This spec fills that gap. It defines:

- The **structural tree** — the parent → child relationship between workspaces, its properties, and its invariants.
- The **causal tree** — the originator → workspace relationship, its properties, and how it overlays the structural tree.
- **Tree operations** — the formal set of operations the protocol performs on the tree (traverse, cascade, reparent) and the derived effect of pruning.
- **Two roots** — the structural root (coordinator's workspace) and the causal roots (humans and the system), and how they relate.
- **Subtree as a unit** — what it means for a subtree to function as a unit of work, containment, failure, and attribution.

The workspace spec defines *what* a tree node is (a workspace with nine components, a lifecycle, a state machine). This spec defines *how* nodes relate to each other and *what operations* the protocol performs on those relationships.

## 2. The Structural Tree

The workspace tree is a rooted, ordered tree. Every workspace except the root has exactly one parent. The root is the coordinator's workspace, created by the runtime at initialization (PROTOCOL §5.1).

### 2.1 Formal Properties

| Property | Value |
|----------|-------|
| Type | Rooted tree (connected, acyclic, directed graph with a designated root) |
| Root | The coordinator's workspace |
| Edge direction | Parent → child (creation direction); signals propagate child → parent (reverse direction) |
| Node type | Workspace |
| Edge semantics | "created by" — the parent (or its delegate) created the child |
| Maximum depth | Unbounded (protocol imposes no depth limit; budget containment is the practical bound) |
| Branching factor | Unbounded (a workspace may create any number of children, subject to budget and admission control) |
| Ordering | Children are ordered by creation timestamp |
| Mutability | Edges are immutable once formed, with one exception: reparenting on cross-ownership failure (§4.4) |

### 2.2 Node Lifecycle and Tree Membership

A workspace joins the tree at creation and never leaves. Terminal workspaces (`closed`, `failed`) remain in the tree as immutable records. The tree grows monotonically — nodes are added but never removed. This is consistent with the protocol's "workspaces are never destroyed" invariant (Workspace spec §4).

**Consequence:** The tree at any point in time is a prefix of every future tree. Any historical tree query returns a subset of the current tree.

### 2.3 The Root

The root workspace is unique:

- Created by the runtime, not by another workspace. It has no parent (`parent: null`).
- Holds the coordinator role — the only workspace that does.
- If it fails, all workspaces fail regardless of ownership (PROTOCOL §6.5). The root is the single point of structural failure.
- It is the structural root of every signal propagation path.
- It is the only workspace that can observe all other workspaces via the global trail.

The root always carries `originator: "system"` — the coordinator is the system (PROTOCOL §4.8).

## 3. The Causal Tree

The structural tree answers "who created this workspace?" The causal tree answers "what caused this workspace?" The causal tree is defined by the `originator` field (User spec §5, PROTOCOL §4.8). This section introduces the causal tree as a tree-level concept — how it overlays the structural tree and gives rise to the "two roots" property. The full formalization — the causal forest, causal boundaries, and causal operations — is in the Causation spec (causation.md).

### 3.1 Formal Properties

| Property | Value |
|----------|-------|
| Type | Forest (a set of trees, one per distinct originator value) |
| Roots | One per distinct `originator` value — each `user_id` or `"system"` is the causal root of its subtrees |
| Edge direction | Same as the structural tree — parent → child (originator propagates downward) |
| Edge semantics | "caused by the same origin" — parent and child share the same originator |
| Overlay | The causal tree is an overlay on the structural tree — same nodes, same edges, but partitioned by originator value |

### 3.2 Overlay Relationship

The causal tree uses the same nodes and edges as the structural tree. It does not introduce new edges. The difference is in how the tree is partitioned:

- In the structural tree, every node belongs to one tree rooted at the coordinator.
- In the causal tree, nodes are partitioned into forests by originator value. All workspaces with `originator: X` form one causal subtree. All workspaces with `originator: "system"` form another.

The causal forest is a partition of the structural tree's node set. Every structural node belongs to exactly one causal subtree. Causal subtrees interleave within the structural tree, but only in one direction: human-originated subtrees appear within system-originated subtrees (at injection points), never the reverse. Because originator inheritance is unconditional (invariant T-6), a human-originated workspace's children are always human-originated — no system-originated workspace can appear below a human-originated one. The full formalization of causal boundaries and the causal forest is in the Causation spec (causation.md).

### 3.3 Causal Boundaries

A causal boundary exists where a parent and child have different originator values. Because originator inheritance is unconditional, boundaries are unidirectional: they occur only where a system-originated workspace (typically the coordinator) creates a child workspace on behalf of a human injection — the child carries the human's `user_id` as originator, while the parent carries `"system"`. The reverse cannot happen.

**Example — mixed tree:**

```
ws-root (coordinator, originator: "system")
 ├── ws-recovery-01 (worker, originator: "system")
 ├── ws-human-task (worker, originator: user-A)       ← causal boundary
 │    ├── ws-subtask-01 (worker, originator: user-A)
 │    └── ws-subtask-02 (worker, originator: user-A)
 └── ws-maintenance (worker, originator: "system")
```

Here, `ws-root → ws-human-task` is a causal boundary: the structural parent has `originator: "system"` and the child has `originator: user-A`. Within the human-rooted subtree, all children inherit `originator: user-A` unconditionally.

### 3.4 Two Roots

The structural tree has one root — the coordinator's workspace (`originator: "system"`). The causal tree has as many roots as there are distinct originator values. In a deployment with three active users, the causal tree has four roots: `"system"`, `user-A`, `user-B`, `user-C`.

| State | Structural roots | Causal roots |
|-------|-----------------|--------------|
| No human injections | 1 (coordinator) | 1 (`"system"`) |
| One user, one injection | 1 (coordinator) | 2 (`"system"` + one `user_id`) |
| N users, M injections | 1 (coordinator) | 1 + N (`"system"` + N distinct users) |

The coordinator's root workspace always has `originator: "system"`. Human injections create causal boundaries — the injected workspace carries the human's `user_id`, overriding inheritance at that point (PROTOCOL §4.8, rule 4). The causal tree diverges from the structural tree at every injection point.

## 4. Tree Operations

The protocol performs three operations on the workspace tree — traverse, cascade, and reparent — each with defined semantics, preconditions, and trail consequences. A fourth concept, pruning, is a derived effect of cascade reaching terminal states, not an independent operation.

### 4.1 Traverse

**Definition.** Walk the tree from a starting node in a specified direction.

**Directions:**

| Direction | Semantics | Used by |
|-----------|-----------|---------|
| **Upward** (child → root) | Follow `parent` edges toward the root | Signal propagation (Signal spec §3), escalation routing (Human-Highway spec §2.4) |
| **Downward** (parent → leaves) | Follow child edges toward the leaves | Failure cascade (§4.3), budget containment (Workspace spec §6), visibility inheritance |
| **Lateral** (sibling enumeration) | Enumerate children of the same parent | Integration ordering (Integration spec), coordinator dispatch |
| **Causal** (originator filter) | Traverse the structural tree, filtered to nodes with a specific originator | User-state impact evaluation (User spec §3.5), audit queries |

**Upward traversal terminates at the root.** The coordinator's workspace has no parent — upward traversal halts there. The global trail captures signals that reach the root.

**Downward traversal terminates at leaves.** A leaf workspace has no children. In practice, most downward traversals terminate early — failure cascade stops at ownership boundaries (§4.3), visibility checks stop at the first denied node.

**Causal traversal is a filtered structural traversal.** The causal tree uses the same edges as the structural tree. "Show me all workspaces originated by user X" is a downward traversal from the root, filtering for `originator == X`.

### 4.2 Cascade

**Definition.** Propagate an effect downward through a subtree.

Cascade is downward traversal with side effects. When a node transitions, the effect may propagate to its descendants. The protocol defines three cascade types:

**Failure cascade.** When a workspace transitions to `failed`, its children within the same ownership boundary are also failed, recursively (PROTOCOL §6.5, Workspace spec §5, invariant 1). Children with a different owner are reparented (§4.4), not failed.

```
Failure cascade rule:
  for each child of failed workspace:
    if child.owner == failed.owner:
      child → failed (reason: parent_failed)
      recurse into child's children
    else:
      reparent child to coordinator (§4.4)
```

**Budget cascade.** A child's budget is carved from its parent's remaining budget (Roles spec §4, Workspace spec §6). Budget consumption in a child reduces the parent's available budget. Budget exhaustion cascades upward: a child's budget failure reduces the parent's remaining capacity but does not directly fail the parent.

**Coordinator notification cascade.** When a user's state changes (User spec §3.5), the coordinator is notified and may evaluate the originator tree for downstream action. This is not an automatic cascade — the protocol defines the signal, the coordinator decides the response.

### 4.3 Failure Cascade — Detailed

Failure cascade is the most consequential tree operation. It determines the blast radius of a workspace failure.

**Algorithm:**

1. Workspace W transitions to `failed`.
2. For each child C of W:
   a. If `C.owner == W.owner`: transition C to `failed` with `reason: parent_failed`. Record `workspace_state_changed` trail event (with `trigger: parent_failed`). Recurse into C's children.
   b. If `C.owner != W.owner`: reparent C to the coordinator (§4.4). Do NOT fail C.
3. If W is the root (coordinator): fail ALL workspaces regardless of owner. There is no parent to reparent to.

**Properties:**

- **Ownership-bounded.** The cascade never crosses an ownership boundary except at the root. This prevents one user's failure from destroying another user's work.
- **Recursive.** The cascade recurses to arbitrary depth within the ownership boundary.
- **Immediate.** Failed children are transitioned in the same logical operation as the parent's failure. There is no window where a child is active but its parent is failed.
- **Trail-complete.** Every workspace failed by cascade produces its own `workspace_state_changed` trail entry with `trigger: parent_failed`. The trail reconstructs the exact cascade path.

**Causal tree interaction.** Failure cascade follows the structural tree, not the causal tree. A workspace with `originator: user-A` that is structurally a child of a workspace with `originator: "system"` is affected by the system workspace's failure (if same owner), not by user-A's state changes.

### 4.4 Reparent

**Definition.** Move a workspace from one parent to another in the structural tree.

Reparenting is the tree's only structural mutation. It occurs in exactly one circumstance: a parent workspace fails and a child workspace has a different owner. The child is reparented to the coordinator's workspace — the protocol's equivalent of Unix orphan reparenting to init.

**Preconditions:**
- The old parent is in a terminal state (`failed`).
- The child has a different `owner` than the old parent.

**Effects:**
- The child's `parent` field changes to the coordinator's workspace ID.
- A `workspace_reparented` trail event is recorded (Workspace spec §5, User spec §4.4).
- The child's `originator` does NOT change — reparenting is a structural change, not a causal one.
- The child's `owner` does NOT change — the coordinator inherits structural responsibility, not ownership.
- The child's state is unaffected — it continues in whatever state it was in.

**The coordinator decides what happens next.** The protocol reparents the orphan; the coordinator decides its fate: reassign to a new parent in the owner's scope, leave under coordinator management, consult the owning user through the highway, or abort.

**Reparenting is not recursive.** Only the immediate children of the failed parent are reparented. Grandchildren remain structurally attached to their reparented parent — the subtree moves as a unit.

**No voluntary reparenting.** The protocol does not support moving a workspace to a different parent under normal operation. Reparenting is exclusively a failure-recovery mechanism. The structural tree is stable once formed, with this single exception.

### 4.5 Prune (Derived Effect)

**Definition.** A subtree transitions out of active consideration without being destroyed.

Pruning is not a protocol operation — it is the observable effect of cascade reaching terminal states. No separate "prune" command exists. Workspaces are never destroyed. "Pruning" means the subtree has reached terminal states and become an immutable record. The tree retains the pruned nodes for trail integrity and audit.

**What triggers pruning:**
- A workspace transitions to `closed` or `failed`. Its subtree is either also terminal (via cascade) or reparented (§4.4).
- The entire tree is pruned when the root fails.

**What pruning preserves:**
- All trail entries from the pruned subtree.
- All checkpoints from the pruned subtree.
- The structural relationships (parent → child edges) — queryable for post-mortem analysis.

**What pruning removes:**
- Active processing — no more envelope delivery, signal emission, or checkpoint creation.
- Resource consumption — the resource meter stops.

**Prune is cascade + terminal.** Pruning is not a separate operation — it is the consequence of failure cascade (§4.3) reaching terminal states. The tree operation is cascade; the tree effect is pruning.

## 5. Subtree as a Unit

A subtree is not just a collection of workspaces — it is a unit of work, containment, failure, and attribution. This section defines what "unit" means in each context.

### 5.1 Unit of Work

A subtree rooted at a delegate workspace is a unit of work: the delegate received a directive, decomposed it into sub-directives, and created child workspaces to execute them. The subtree's work is complete when the delegate's workspace reaches `integrating` — all children have completed, and the delegate is assembling their results.

**Task graph alignment.** A subtree of the workspace tree typically corresponds to a subgraph of the task graph. The delegate's task decomposes into the children's tasks. But the correspondence is not always 1:1 — a task may be retried in a new workspace (different structural position, same task graph node), and a workspace may be created without a task reference (system bootstrap, recovery).

### 5.2 Unit of Containment

A subtree is a containment unit: a child's visibility, authority, and budget are subsets of its parent's (Workspace spec §5, invariant 3). The containment invariant holds transitively — a grandchild's visibility is a subset of its parent's, which is a subset of the grandparent's. No workspace in the subtree can exceed the root of the subtree's boundaries.

**Budget is the hard boundary.** A delegate's children draw from the delegate's remaining budget. The delegate's budget is drawn from its parent's remaining budget. Budget is the tightest containment constraint — visibility and authority are set-based (can be granted up to the parent's set), but budget is quantity-based (strictly decreasing with depth).

### 5.3 Unit of Failure

A subtree is a failure unit within an ownership boundary. When the subtree root fails, all descendants with the same owner fail recursively (§4.3). The ownership boundary limits the blast radius — cross-owned workspaces survive via reparenting.

**The root's subtree is the entire tree.** When the coordinator fails, the blast radius is the entire system — there is no parent to reparent to.

### 5.4 Unit of Attribution

A subtree is an attribution unit when all nodes share the same originator. "Show me everything user X caused" returns a set of causal subtrees — every workspace with `originator: X`, which by the propagation invariant (User spec §5.2, rule 4) forms connected subtrees within the structural tree.

**Cross-boundary subtrees.** A causal subtree may span multiple structural subtrees. If user X injects two separate directives, each creates a separate structural subtree under the coordinator. Both belong to user X's causal tree, but they are not structurally connected. The causal tree is a forest of structural subtrees.

## 6. Tree Invariants

These invariants hold at all times. Violation of any invariant is a protocol error.

| # | Invariant | Enforced by |
|---|-----------|-------------|
| T-1 | **Single parent.** Every workspace except the root has exactly one parent. | Runtime, at creation |
| T-2 | **Acyclic.** The tree contains no cycles. A workspace cannot be its own ancestor. | Runtime, at creation (parent must exist and must not be a descendant) |
| T-3 | **Connected.** Every workspace is reachable from the root via parent edges. | Follows from T-1 + single root |
| T-4 | **Monotonic growth.** Nodes are added but never removed. The tree only grows. | Protocol invariant: workspaces are never destroyed |
| T-5 | **Stable edges.** Parent → child edges are immutable once formed, except for reparenting on cross-ownership failure (§4.4). | Runtime, structural tree management |
| T-6 | **Originator inheritance.** A child's originator equals its parent's originator, except at injection points where the injecting human's `user_id` overrides inheritance (Causation spec CA-4, CA-6; PROTOCOL §4.8, rules 4–5). | Runtime, at creation (User spec §5.2, rules 4–5) |
| T-7 | **Containment.** A child's visibility, authority, and budget are subsets of its parent's. | Runtime, at creation (Workspace spec §5, invariant 3) |
| T-8 | **Ownership-bounded cascade.** Failure cascade propagates only within the same ownership boundary, except at the root (Ownership spec OW-5; §4.3). | Runtime, during failure cascade (§4.3) |
| T-9 | **Root failure is total.** If the root fails, all workspaces fail regardless of ownership or originator. | Runtime, during failure cascade (§4.3) |
| T-10 | **Reparenting preserves identity.** Reparenting changes the structural parent but not the originator, owner, or any other workspace field. | Runtime, during reparenting (§4.4) |

## 7. Trail Events

The tree itself does not introduce new trail event types. Tree operations produce events defined by other specs:

| Operation | Trail event | Defined in |
|-----------|------------|------------|
| Node creation | `workspace_created` | Workspace spec §13 |
| Failure cascade | `workspace_state_changed` (with `trigger: parent_failed`) | Workspace spec §13 |
| Reparenting | `workspace_reparented` | Workspace spec §13, User spec §4.4 |
| Signal propagation | `signal_emitted`, `signal_delivered` | Signal spec §8 |

The tree spec does not duplicate these events. It defines the operations that produce them.

## 8. Conformance Requirements

| # | Requirement | Level |
|---|-------------|-------|
| T-C1 | The workspace tree MUST be a rooted tree satisfying invariants T-1 through T-3. | Core |
| T-C2 | The runtime MUST enforce single-parent assignment at creation. A workspace creation request naming a nonexistent or cyclic parent MUST be rejected. | Core |
| T-C3 | Signal propagation MUST follow upward traversal (§4.1) from emitting workspace to root. | Core |
| T-C4 | Failure cascade MUST follow the ownership-bounded algorithm in §4.3. Cross-owned children MUST be reparented, not failed. | Core |
| T-C5 | Root failure MUST fail all workspaces regardless of ownership or originator. | Core |
| T-C6 | Reparenting MUST preserve originator, owner, and workspace state (§4.4). | Standard |
| T-C7 | The runtime MUST support causal traversal — filtering the structural tree by originator value (§4.1). | Standard |
| T-C8 | Containment (T-7) MUST be enforced at creation. A child workspace MUST NOT be granted visibility, authority, or budget exceeding its parent's. | Core |
| T-C9 | Originator inheritance (T-6) MUST be enforced at creation. A child's originator MUST equal its parent's originator, except at injection points where the injecting human's `user_id` overrides inheritance (Causation spec CA-C3; PROTOCOL §4.8, rule 4). | Core |

**9 requirements.** Core ensures the tree is well-formed, signals propagate correctly, failure cascades are ownership-bounded, and containment is enforced. Standard adds reparenting semantics and causal traversal support.

## 9. Implementation Notes

*This section is non-normative.*

**Tree as a flat table.** The workspace tree does not require a tree data structure. A flat table of workspaces with a `parent` column is sufficient. Upward traversal is a recursive query (`WITH RECURSIVE` in SQL, or a simple loop following parent pointers). Downward traversal is a filter on `parent`. Most tree operations are O(depth) or O(subtree size), not O(tree size).

**Precomputed ancestry.** For large trees, precomputing the ancestry path (root → ... → node) at creation avoids repeated upward traversals. This is a space-time tradeoff — O(depth) storage per node, O(1) ancestry checks.

**Failure cascade as a breadth-first walk.** The failure cascade algorithm (§4.3) is naturally a breadth-first traversal from the failed node. At each level, partition children into same-owner (fail) and cross-owner (reparent). Continue BFS into the same-owner children. The cross-owner children are reparented and their subtrees are untouched.

**Causal tree as an index.** The causal tree is an index on the structural tree, keyed by `originator`. A hash map from `originator → Set[workspace_id]` supports efficient causal traversal without walking the full tree.

**Reparenting and signal routing.** After reparenting, signals from the reparented workspace propagate to the coordinator (the new parent), not to the old parent (which is terminal). Implementations should update signal routing tables when reparenting occurs.

## 10. References

| Spec | Sections referenced | Relationship |
|------|-------------------|--------------|
| [Workspace](../primitives/workspace.md) | §4 (creation, originator field), §5 (tree structure, three invariants), §6 (resource management), §13 (trail events) | The tree's nodes are workspaces. Workspace spec defines node properties; this spec defines node relationships. |
| [Signal](../primitives/signal.md) | §3 (propagation rules), §8 (trail events) | Signal propagation is upward tree traversal |
| [User](../primitives/user.md) | §3.5 (coordinator notification on state change), §4 (ownership, reparenting), §5 (originator propagation) | Ownership boundaries determine cascade scope. Originator defines the causal tree. |
| [Identity](../foundations/identity.md) | §2 (identifier rules) | Workspace IDs, user IDs referenced in tree operations |
| [Causation](causation.md) | §3 (causal forest), §4 (coordinator model as topology), §7 (invariants CA-4, CA-5, CA-6) | Full formalization of the causal tree introduced in §3 of this spec |
| [PROTOCOL.md](../PROTOCOL.md) | §4.8 (originator, coordinator model), §6.5 (workspace tree) | Canonical tree definition and coordinator model declaration |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
