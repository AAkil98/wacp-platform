# WACP: Visibility Graph

## Metadata

```yaml
title: "WACP: Visibility Graph"
id: wacp-spec-visibility
type: constituent-spec
tier: abstract
category: topology
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §6.8 (visibility and authority)
  - §3.4 (context is scoped, not shared)
depends_on:
  - wacp-spec-tree
  - wacp-spec-workspace
  - wacp-spec-roles
  - wacp-spec-identity
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, topology, visibility, information-flow, access-control, authority, directed-graph]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [The Visibility Graph](#2-the-visibility-graph)
3. [Default Visibility and the Structural Tree](#3-default-visibility-and-the-structural-tree)
4. [Dynamic Visibility Grants](#4-dynamic-visibility-grants)
5. [Write-Access Scope (Authority)](#5-write-access-scope-authority)
6. [Visibility Graph and Other Topologies](#6-visibility-graph-and-other-topologies)
7. [Information Flow](#7-information-flow)
8. [Invariants](#8-invariants)
9. [Trail Events](#9-trail-events)
10. [Conformance Requirements](#10-conformance-requirements)
11. [Implementation Notes](#11-implementation-notes)
12. [References](#12-references)

---

## 1. Purpose

The port rights graph (Channels spec) governs who can *send* to whom. The visibility graph governs who can *see* whom. These are different dimensions of access: port rights control write-path communication (sending envelopes), visibility controls read-path access (reading working memory, checkpoints, local trails).

The workspace spec (§8) defines visibility rules — set at creation, additive only, scoped to the grantor's visibility. This spec defines the visibility graph as a topological structure: its formal properties, its relationship to the structural tree and the port rights graph, and how it evolves through dynamic grants.

The visibility graph also subsumes authority — the write-access counterpart of visibility. Authority is simpler (frozen at creation, no dynamic grants), but it uses the same directed-graph structure. Both are defined here.

## 2. The Visibility Graph

### 2.1 Formal Definition

The visibility graph is a directed graph where nodes are workspaces and edges represent read access.

**Formally:** Given a set of workspaces W and visibility sets V (one per workspace), the visibility graph G_vis = (W, E) where:
- E = {(A, B) | B ∈ A.visibility_set} — "A can see B"
- Every workspace has a self-loop: A can always see A

### 2.2 Formal Properties

| Property | Value |
|----------|-------|
| Type | Directed graph (not necessarily acyclic, not necessarily connected) |
| Nodes | Workspaces |
| Edge semantics | `A → B` means "workspace A can read workspace B's working memory, checkpoint register, and local trail" |
| Self-loops | Every workspace has a self-loop (implicit: a workspace can always see itself) |
| Cycles | Allowed — A can see B and B can see A (the coordinator sees all workers; a worker with cross-visibility sees a peer) |
| Mutability | **Additive only.** Edges are added but never removed. The graph grows monotonically. |
| Relationship to structural tree | Correlated but distinct — default visibility flows downward in the tree, but explicit grants create cross-tree edges |

### 2.3 Additive-Only Property

The visibility graph's defining topological property: edges are added but never removed. This is unique among the protocol's topological structures:

- The port rights graph is fully mutable (create, transfer, revoke, consume).
- The ownership partition is mutable (transfer moves nodes between domains).
- The structural tree is stable (no removal) but reparenting mutates edges.
- The visibility graph only grows.

**Why additive-only.** The Workspace spec (§8) explains: removing visibility creates a class of race conditions where an agent reads a resource, acts on it, then loses access to the resource. The agent's decisions would be based on context it can no longer verify. Additive-only prevents this: once you can see something, you can always see it.

**Consequence:** The visibility graph at any point in time is a subgraph of every future visibility graph. Any visibility query is a lower bound — the workspace may gain more visibility but never less.

## 3. Default Visibility and the Structural Tree

### 3.1 Default Downward Visibility

The structural tree determines the initial visibility graph. The default rule (Tree spec §2, Workspace spec §5, invariant 2): a parent can see all its descendants. A child sees only what was explicitly granted.

**For the coordinator (root):** Visibility is total — the coordinator can see every workspace. In graph terms, the root has edges to every other node.

**For a delegate:** Visibility includes the delegate's own workspace and all descendants in its subtree. The delegate sees its workers' workspaces by default.

**For a worker:** Visibility includes only its own workspace — plus whatever the coordinator explicitly grants at creation or through dynamic grants.

### 3.2 Initial Visibility Graph

For a simple tree (coordinator + N workers with no cross-visibility):

```
Coordinator → Worker-1 (default: parent sees child)
Coordinator → Worker-2
...
Coordinator → Worker-N
Worker-1 → Worker-1 (self-loop)
Worker-2 → Worker-2 (self-loop)
...
Worker-N → Worker-N (self-loop)
```

The initial graph is a star from the coordinator to all workers, plus self-loops. No worker-to-worker edges. No upward edges (children do not see their parent by default).

### 3.3 Containment Invariant

A child's visibility set must be a subset of its parent's visibility set (Tree spec §6, invariant T-7; Workspace spec §5, invariant 3). In graph terms: if A is a child of B, then the set of nodes reachable from A via visibility edges is a subset of the set reachable from B.

This is the containment principle applied to information flow: a child can never see more than its parent. The coordinator sees everything. Each level of delegation narrows the visible scope.

## 4. Dynamic Visibility Grants

### 4.1 Grant Mechanism

The coordinator (or a delegate within its subtree) may grant additional visibility to a workspace in `active` or `blocked` state (Workspace spec §8):

```yaml
visibility_grant:
  workspace: workspace_id     # the receiving workspace
  target: workspace_id        # the workspace being made visible
  reason: string              # why the grant is needed (recorded in trail)
  timestamp: timestamp
```

The grant adds an edge to the visibility graph: `(workspace, target)`. The edge is permanent — it cannot be revoked.

### 4.2 Grant Rules

1. **Additive only.** Grants add edges. Nothing removes edges.
2. **Scoped to the grantor's visibility.** The grantor can only grant visibility to workspaces within its own visibility set. The coordinator (visibility: all) can grant anything. A delegate can only grant visibility to workspaces it can see — which is its subtree (§3.1) plus any grants it has received.
3. **No state restriction on target.** The target workspace may be in any state, including terminal. Granting visibility to a closed workspace lets the agent read its final state and checkpoints.
4. **Containment preserved.** The grant must not violate the containment invariant. Since the grantor can only grant within its own visibility, and the grantee is a descendant (or within the grantor's subtree), containment is preserved by construction.

### 4.3 Grant Patterns

**Cross-peer visibility.** The coordinator grants Worker-A visibility into Worker-B. Worker-A can now read Worker-B's checkpoints and working memory. This enables information sharing between parallel workers without envelope communication.

**Dependency context.** When binding a task to a workspace, the coordinator grants visibility to the workspaces that completed dependency tasks. The worker can read the predecessors' outputs directly.

**Observer scope expansion.** The coordinator grants an observer visibility into additional workspaces during execution — expanding the observer's monitoring scope.

## 5. Write-Access Scope (Authority)

### 5.1 Nature of Authority

Authority is the write counterpart of visibility, but it is NOT a workspace-to-workspace graph in the same sense. The protocol defines authority as "the set of resources a workspace may modify" (PROTOCOL §12.6) — it is role-derived and resource-scoped (`Set[resource_ref]`), not a set of workspace IDs like visibility.

The visibility set contains workspace IDs: "I can read workspace B." The authority set contains resource references and role-derived action capabilities: "I can modify my own working memory," "I can create child workspaces," "I can abort workspaces in my subtree."

### 5.2 Role-Derived Write Scope

While authority is not a workspace-to-workspace graph, each role implies a write scope over workspaces:

| Role | Write scope | Derived from |
|------|-------------|-------------|
| **Coordinator** | All workspaces (abort, visibility grants, state transitions via runtime operations) | Root role |
| **Delegate** | Own workspace + subtree workspaces (create children, send directives, integrate checkpoints) | Delegation capability |
| **Worker** | Own workspace only (modify working memory, produce checkpoints) | Base worker role |
| **Observer** | Own workspace only (produce observation checkpoints) | Base observer role |

### 5.3 Properties

| Property | Value |
|----------|-------|
| Mutability | **Frozen at creation.** Authority is set at creation and cannot be modified after the workspace leaves `idle` (Workspace spec §4, §8). No dynamic grants. No revocation. |
| Asymmetry | Deliberate — granting read access mid-task is safe; granting write access mid-task breaks isolation. |
| Write scope ⊆ read scope | A workspace's role-derived write scope is always within its visibility scope. The coordinator can modify all workspaces and can see all workspaces. A delegate can modify its subtree and can see its subtree. A worker can modify itself and can see itself. Write access never exceeds read access. |
| Containment | A child's authority set must be a subset of its parent's (Workspace spec §5, invariant 3). Authority narrows as you descend the tree. |

## 6. Visibility Graph and Other Topologies

| Topology | Relationship to visibility |
|----------|---------------------------|
| **Structural tree** (Tree spec) | Default visibility follows the tree downward. Parent sees children; children do not see parent by default. Containment invariant derived from tree structure. |
| **Port rights graph** (Channels spec) | Independent dimension. Port rights control sending; visibility controls reading. A workspace may see another workspace (visibility) but not be able to send to it (no send right). Conversely, a workspace may hold a send right to a workspace it cannot see (unusual but valid — the coordinator grants the right without granting visibility). |
| **Causal forest** (Causation spec) | No direct relationship. Visibility does not follow originator. A system-originated workspace can see a human-originated workspace and vice versa. Visibility is granted based on need, not causation. |
| **Ownership domains** (Ownership spec) | No direct relationship. A user's ownership domain does not determine what their workspaces can see. Visibility is per-workspace, not per-user. |

## 7. Information Flow

The visibility graph is the protocol's information flow graph. Information flows from visible workspaces to viewing workspaces:

- A worker reads a peer's checkpoint (granted cross-visibility) → information flows from peer to worker.
- The coordinator reads all workspaces → information flows from everywhere to the coordinator.
- An observer reads designated workspaces → information flows from those workspaces to the observer.

**Information flow direction.** In the visibility graph, edge direction is `viewer → target`. Information flows in the opposite direction: from target to viewer. The visibility edge says "A can see B" — meaning information about B reaches A.

**No reverse channel.** Visibility is one-way. A can read B, but B does not know that A is reading. There is no notification, no access log visible to B. The trail records visibility grants, but the target workspace has no programmatic awareness of who is watching.

## 8. Invariants

| # | Invariant | Enforced by |
|---|-----------|-------------|
| VI-1 | **Self-visibility.** Every workspace can see itself. | Implicit |
| VI-2 | **Additive only.** Visibility edges are never removed. The graph grows monotonically. | Runtime, enforced by rejecting revocation |
| VI-3 | **Containment.** A child's visibility set is a subset of its parent's visibility set. | Runtime, at creation and grant |
| VI-4 | **Grantor-scoped grants.** A grantor can only grant visibility to workspaces within its own visibility set. | Runtime, at grant |
| VI-5 | **Authority frozen.** The authority set does not change after the workspace leaves `idle`. | Runtime |
| VI-6 | **Write scope within read scope.** A workspace's role-derived write scope is always within its visibility scope. | Runtime, at creation |

## 9. Trail Events

Visibility topology changes produce events defined in the workspace spec:

| Operation | Trail event | Defined in |
|-----------|------------|------------|
| Visibility set at creation | `workspace_created` (includes visibility set) | Workspace spec §13 |
| Dynamic visibility grant | `visibility_granted` | Workspace spec §13 |
| Authority set at creation | `workspace_created` (includes authority set) | Workspace spec §13 |

The visibility graph's full history is reconstructable from these events: start with the `workspace_created` events (initial visibility sets), apply `visibility_granted` events in timestamp order.

## 10. Conformance Requirements

| # | Requirement | Level |
|---|-------------|-------|
| VI-C1 | Every workspace MUST have implicit visibility into itself. | Core |
| VI-C2 | Visibility MUST be additive only. The runtime MUST reject attempts to revoke visibility. | Core |
| VI-C3 | The containment invariant MUST be enforced: a child's visibility set MUST be a subset of its parent's. | Core |
| VI-C4 | Authority MUST be frozen after the workspace leaves `idle`. The runtime MUST reject authority modifications after `idle`. | Core |
| VI-C5 | Dynamic visibility grants MUST be scoped to the grantor's own visibility set. | Standard |
| VI-C6 | `visibility_granted` trail events MUST be recorded for every dynamic grant. | Standard |
| VI-C7 | The runtime MUST support visibility queries — enumerating what a workspace can see (outbound edges) and who can see a workspace (inbound edges). | Standard |

**7 requirements.** Core ensures the graph's fundamental properties (self-visibility, additive-only, containment, authority frozen). Standard ensures grant semantics and trail recording.

## 11. Implementation Notes

*This section is non-normative.*

**Visibility as a set per workspace.** The simplest implementation: each workspace carries a `Set[workspace_id]` for its visibility. Edge addition is set insertion. Containment check at creation is a subset test. Dynamic grants are set insertions with a grantor-scope check.

**Reverse index.** To answer "who can see workspace X?" maintain a reverse index: `Map[workspace_id → Set[workspace_id]]`. Updated on creation and grant. This enables efficient "who is watching?" queries without scanning all workspaces.

**Authority as a frozen set.** Authority is a `Set[workspace_id]` populated at creation and never modified. Since most workspaces have authority only over themselves, the common case is a singleton set. Implementations can optimize: if authority == {self}, store a flag rather than a set.

**Visibility graph vs. port rights graph.** Both are directed graphs over workspaces. An implementation may use a single graph store with edge types (visibility, authority, send_right, send_once_right) rather than separate structures. Operations differ — visibility edges are additive-only, port right edges are fully mutable — but the storage model can be shared.

## 12. References

| Spec | Sections referenced | Relationship |
|------|-------------------|--------------|
| [Workspace](../primitives/workspace.md) | §2 (visibility set, authority set components), §4 (creation), §5 (containment invariant), §8 (visibility and authority rules, dynamic grants) | Workspaces carry visibility and authority sets |
| [Workspace Tree](tree.md) | §2.2 (node lifecycle), §5.2 (unit of containment), §6 (invariant T-7: containment) | Default visibility follows the structural tree; containment derived from tree structure |
| [Communication Topology](channels.md) | §2 (port rights graph) | Port rights and visibility are independent access dimensions |
| [Roles](../foundations/roles.md) | §3 (permission matrix), §6 (access column) | Roles determine initial visibility scope |
| [PROTOCOL.md](../PROTOCOL.md) | §3.4 (context is scoped), §6.8 (visibility and authority) | Canonical visibility model |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
