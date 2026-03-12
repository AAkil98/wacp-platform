# WACP: Causation

## Metadata

```yaml
title: "WACP: Causation"
id: wacp-spec-causation
type: constituent-spec
tier: abstract
category: topology
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4.8 (originator, coordinator model)
  - §6.5 (workspace tree, two roots)
depends_on:
  - wacp-spec-tree
  - wacp-spec-workspace
  - wacp-spec-user
  - wacp-spec-identity
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, topology, causation, originator, coordinator-model, causal-tree, causal-forest, mixed-tree]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [The Originator Field](#2-the-originator-field)
3. [The Causal Forest](#3-the-causal-forest)
4. [The Coordinator Model as Topology](#4-the-coordinator-model-as-topology)
5. [Causal Tree Operations](#5-causal-tree-operations)
6. [Relationship to Other Topology Specs](#6-relationship-to-other-topology-specs)
7. [Invariants](#7-invariants)
8. [Trail Events](#8-trail-events)
9. [Conformance Requirements](#9-conformance-requirements)
10. [Implementation Notes](#10-implementation-notes)
11. [References](#11-references)

---

## 1. Purpose

Every workspace in the protocol has two addresses: a structural address (its position in the workspace tree, defined by `parent`) and a causal address (its origin, defined by `originator`). The Tree spec defines the structural tree and mentions the causal tree as an overlay. The User spec defines the originator field and its propagation rules. Neither formalizes what the causal tree IS as a topological structure, how it partitions the workspace tree, or how it gives rise to the coordinator model.

This spec fills three gaps:

- **The causal tree as topology.** Formal definition of the causal forest — the set of trees formed by partitioning the workspace tree by originator value.
- **The originator as discriminator.** The required `originator` field (`user_id | "system"`) as the mechanism that partitions every workspace into a causal subtree. No workspace is unaddressed.
- **The coordinator model as topology.** The coordinator is the system. The root workspace is always system-originated. The causal forest's structure — system sub-forest plus human subtrees — is an observable property of the tree, not a configuration choice.

## 2. The Originator Field

### 2.1 Type and Requirement

Every workspace carries an `originator` field (PROTOCOL §4.8, User spec §5). The field is:

- **Required.** Never absent. Every workspace declares its causal origin.
- **Typed.** `user_id | "system"`. Two value types, two meanings:
  - `user_id` — a human caused this workspace. The value names the specific human.
  - `"system"` — the system caused this workspace. No human is in the causal chain.
- **Immutable.** Set at creation, never changed. Ownership transfer, reparenting, and state transitions do not affect the originator.
- **Inherited.** A child workspace inherits its parent's originator when created via delegation (PROTOCOL §4.8, rule 5). At injection points, the injecting human's `user_id` overrides inheritance (§2.2).

### 2.2 Setting the Originator

The originator is set at one of three points:

**Root creation.** The root workspace (coordinator's workspace) always carries `originator: "system"`. The coordinator is the system (PROTOCOL §4.8, rule 2).

**System action.** When the coordinator creates a workspace autonomously — bootstrap, recovery, maintenance, or any action not traceable to a human injection — the workspace carries `originator: "system"`. Inheritance applies: the coordinator's originator propagates to its children.

**Human injection.** When a human injects a directive through the highway (Human-Highway spec §2.3), the resulting workspace carries `originator: <user_id>` of the injecting human. **This overrides inheritance at the injection point** — the injected workspace has a different originator than its structural parent (PROTOCOL §4.8, rule 4). This is the only mechanism that creates causal boundaries.

**Rule precedence.** Injection (rule 4) overrides inheritance (rule 5) at injection points. Inheritance applies everywhere else. After the initial setting, inheritance governs: every child of a human-originated workspace is human-originated (same `user_id`). Every child of a system-originated workspace is system-originated (`"system"`). The causal origin propagates downward through the entire subtree, unconditionally.

### 2.3 The Root Workspace

The root workspace always carries `originator: "system"`. There is no deployment in which the coordinator is a user. The coordinator is categorically different from users — not a privileged user, but a different kind of entity.

## 3. The Causal Forest

### 3.1 Formal Definition

The causal forest is the partition of the workspace tree's nodes by originator value. Each partition forms a set of connected subtrees within the structural tree.

**Formally:** Given a workspace tree T with nodes W and edges E (parent → child), the causal forest F is a set of sub-forests {F₁, F₂, ...}, one per distinct originator value, where:
- Fᵢ contains all nodes w ∈ W where w.originator == vᵢ
- The edges in Fᵢ are the subset of E connecting nodes in Fᵢ

**Properties:**

| Property | Value |
|----------|-------|
| Type | Forest — a set of rooted trees |
| Partition | The causal forest is a partition of the structural tree's node set. Every node belongs to exactly one causal sub-forest. |
| Roots | Each causal sub-forest has one or more roots — the topmost nodes with that originator value in the structural tree. |
| Connected components | Each causal sub-forest may have multiple connected components (disconnected subtrees sharing the same originator). |
| Edge inheritance | Edges within a causal sub-forest are a subset of the structural tree's edges. No new edges are introduced. |

### 3.2 Causal Subtrees

A causal subtree is a connected component within a causal sub-forest. It is a maximal connected set of nodes sharing the same originator value, within the structural tree.

**Key property:** Because originator inheritance is unconditional (User spec §5.2, rule 4), a causal subtree is always a complete subtree of the structural tree from its root downward. If a node has originator X, then every descendant of that node also has originator X. There are no "holes" — a causal subtree cannot skip a level.

**Consequence:** Causal subtrees are downward-closed within the structural tree. If you know the root of a causal subtree (the topmost node with that originator), you know the entire subtree: it is the structural subtree rooted at that node.

### 3.3 Causal Boundaries

A causal boundary is a structural edge where the parent and child have different originator values. This occurs only at **injection points** — where a human injects into a system-rooted context, creating a workspace with a different originator than its structural parent.

**Boundary direction:** Causal boundaries always point from `"system"` → `user_id`. The system creates a workspace on behalf of a human injection. The reverse — a human-originated workspace creating a system-originated child — does not occur because inheritance is unconditional. A human-originated workspace's children always inherit the human's originator.

**Implication:** Causal boundaries are one-directional. Once a subtree is human-originated, it stays human-originated. Once a subtree is system-originated, it stays system-originated (unless a boundary introduces a human-originated child).

### 3.4 Causal Boundary Count

| State | Number of causal boundaries |
|-------|---------------------------|
| No human injections | 0 — all workspaces share `originator: "system"` |
| Single user, one injection | 1 — one boundary where the human's subtree begins |
| N injections from M users | N — one boundary per injection point |

The number of causal boundaries equals the number of distinct human injection points. Each injection creates one boundary edge in the structural tree.

## 4. The Coordinator Model as Topology

The coordinator is the system. This is not a deployment choice — it is how the protocol works. The topological consequence is that the root workspace always carries `originator: "system"`, and the causal forest always has a system sub-forest rooted at the coordinator.

### 4.1 The System Sub-Forest

**Topological signature:** All workspaces created by the coordinator autonomously (bootstrap, recovery, maintenance) belong to the `"system"` sub-forest. The root workspace is in this sub-forest. The system sub-forest is always connected — it includes the root, and all system-originated workspaces trace to the root through system-originated parents. It has "holes" where human-originated subtrees branch off, but it remains one connected component.

A fresh tree (no human injections) consists entirely of the system sub-forest. This is the starting state.

### 4.2 Human Sub-Forests

Each distinct `user_id` that has injected into the system has its own sub-forest. Each sub-forest contains one or more connected subtrees — one per injection by that user. If user A injects twice, user A's sub-forest has two connected components (two separate structural subtrees under the coordinator, both carrying `originator: user-A`).

Human sub-forests may be disconnected. The system sub-forest is always connected.

### 4.3 Mixed Trees

Every tree with at least one human injection is a mixed tree — it contains both the system sub-forest and one or more human sub-forests. This is the normal operating state. The distinction is not a model choice — it is a consequence of whether humans have injected. A fresh tree (no injections) has one sub-forest (`"system"`). The first injection makes it mixed.

**Topological signature:** The causal forest has 1 + N sub-forests (`"system"` + N distinct users). The structural tree has one root (coordinator). The causal forest has 1 + N roots. The two-roots distinction (Tree spec §3.4) is always meaningful once humans interact.

## 5. Causal Tree Operations

### 5.1 Causal Traversal

**Definition.** Walk the structural tree, filtered by originator value.

"Show me all workspaces originated by user X" is a causal traversal: visit every node in the structural tree where `originator == X`. Because causal subtrees are downward-closed (§3.2), this is equivalent to finding the roots of user X's causal subtrees and enumerating their structural descendants.

### 5.2 Causal Impact

**Definition.** Given a user state change, identify all workspaces in that user's causal sub-forest.

When a user's state changes (User spec §3.4) — active → suspended, active → blocked, or any state → deactivated — the coordinator is notified (User spec §3.5). The coordinator may evaluate the causal tree to determine downstream impact: which workspaces were caused by this user?

Causal impact is a read operation — it does not modify the tree. The protocol defines the signal (user state changed); the coordinator decides the response (User spec §3.5). The causal tree provides the information needed for that decision.

### 5.3 Causal Attribution

**Definition.** Given a workspace, determine its causal origin.

This is a constant-time operation — read the `originator` field. No traversal needed. The originator field is the causal tree's index.

"Who caused this workspace?" → read `originator`.
"Who is responsible for this workspace?" → read `owner`.
"Who created this workspace?" → read `parent` (structural tree edge).

Three questions, three fields, three different answers.

## 6. Relationship to Other Topology Specs

| Topology spec | Relationship to causation |
|--------------|--------------------------|
| **Tree** (tree.md) | The structural tree is the substrate. The causal forest is a partition of the structural tree's nodes. Causal traversal uses structural edges. |
| **Graph** (graph.md) | The task graph is independent of causation. Tasks do not carry an originator — workspaces do. The mapping is indirect: task → workspace → originator. |
| **Ownership** (ownership.md) | Ownership and causation are orthogonal. Owner can change (transfer); originator cannot. Owner determines failure cascade scope; originator determines causal attribution. Both partition the workspace tree's nodes, but differently. |
| **Channels** (channels.md) | Port rights and conversation threads operate on workspaces regardless of originator. A system-originated workspace can communicate with a human-originated workspace. Causation does not gate communication. |
| **Visibility** (visibility.md) | Visibility grants operate on workspaces regardless of originator. Causation does not gate visibility — a human-originated workspace can see a system-originated workspace if granted access. |

## 7. Invariants

| # | Invariant | Enforced by |
|---|-----------|-------------|
| CA-1 | **Required.** Every workspace has an originator. The field is never absent. | Runtime, at creation |
| CA-2 | **Typed.** Originator is `user_id \| "system"`. No other values are valid. | Runtime, at creation |
| CA-3 | **Immutable.** Originator never changes after creation. | Runtime |
| CA-4 | **Unconditional inheritance (delegation).** A child's originator equals its parent's originator when created via delegation. At injection points, the injecting human's `user_id` overrides inheritance (PROTOCOL §4.8, rules 4–5). | Runtime, at creation |
| CA-5 | **Downward closure (within causal subtrees).** Within a causal subtree (no injection points), if a node has originator X, every descendant has originator X. Injection creates new subtrees with different originators. (Follows from CA-4 and §2.2.) | Structural property |
| CA-6 | **Causal boundaries are unidirectional.** Boundaries exist only from `"system"` → `user_id`, never `user_id` → `"system"` or `user_id_A` → `user_id_B`. | Follows from CA-4 and §2.2 |
| CA-7 | **Root is system-originated.** The root workspace always carries `originator: "system"`. The coordinator is the system. | Runtime, at initialization |

## 8. Trail Events

Causation does not introduce new trail event types. The originator field is recorded in existing events:

| Trail event | Originator field | Defined in |
|------------|-----------------|------------|
| `workspace_created` | `originator` captured at creation | Workspace spec §13 |
| `human_injection` | `actor: user_id` — the injecting human, who becomes the originator of the resulting workspace | Human-Highway spec §8 |

Causal attribution queries — "show me everything user X caused" — are trail queries filtering on `originator == X` across `workspace_created` events.

## 9. Conformance Requirements

| # | Requirement | Level |
|---|-------------|-------|
| CA-C1 | Every workspace MUST carry an `originator` field. The field MUST be `user_id` or `"system"`. | Core |
| CA-C2 | The originator MUST be immutable after creation. | Core |
| CA-C3 | A child workspace's originator MUST equal its parent's originator. The runtime MUST reject creation requests that violate this. | Core |
| CA-C4 | The root workspace's originator MUST be `"system"`, set by the runtime at initialization. | Core |
| CA-C5 | The runtime MUST support causal traversal — enumerating all workspaces with a given originator value. | Standard |
| CA-C6 | The runtime MUST support causal impact queries — given a user, identify all workspaces in that user's causal sub-forest. | Standard |

**6 requirements.** Core ensures every workspace is causally addressed and inheritance is enforced. Standard ensures the causal tree is queryable.

## 10. Implementation Notes

*This section is non-normative.*

**Originator as an indexed column.** The simplest implementation of the causal forest is an index on the originator field: `Map[originator_value → Set[workspace_id]]`. Causal traversal is an index lookup. Causal impact is an index lookup followed by state enumeration. No graph traversal needed — the originator field makes the causal tree directly queryable.

**Tree state detection.** To determine whether the tree is mixed, check if any workspace carries an originator other than `"system"`. A fresh tree (no human injections) is pure system. A tree with at least one `user_id` originator is mixed. An implementation can track this as a running count of distinct originator values.

**Causal boundary tracking.** To enumerate causal boundaries, scan `workspace_created` events where `child.originator != parent.originator`. In practice, this only occurs at injection points. A dedicated boundary index may be useful for systems with high injection rates.

**Multi-user attribution reports.** For systems with multiple users, the causal forest enables per-user reports: total workspaces caused, total resources consumed (sum of resource meters across causal subtree), success rate (completed vs. failed workspaces). These are aggregate queries over the originator index — no graph traversal needed.

## 11. References

| Spec | Sections referenced | Relationship |
|------|-------------------|--------------|
| [Workspace Tree](tree.md) | §2 (structural tree), §3 (causal tree overview), §4 (operations) | The structural tree is the substrate for the causal forest |
| [Workspace](../primitives/workspace.md) | §4 (creation, originator field), §5 (tree, two roots) | Workspaces carry the originator field; creation sets it |
| [User](../primitives/user.md) | §3.4 (state transitions), §3.5 (coordinator notification), §5 (originator propagation rules), §7 (coordinator model — user-side implications) | User spec defines originator rules; this spec defines causation topology |
| [Human-Highway](../mechanisms/human-highway.md) | §2.3 (injection), §2.4 (escalation routing) | Human injection is the mechanism that sets originator to `user_id` |
| [Identity](../foundations/identity.md) | §2 (identifier rules) | `user_id` follows identity rules |
| [PROTOCOL.md](../PROTOCOL.md) | §4.8 (originator, coordinator model) | Canonical definition of originator and coordinator model |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
