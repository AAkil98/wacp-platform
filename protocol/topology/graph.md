# WACP: Task Graph

## Metadata

```yaml
title: "WACP: Task Graph"
id: wacp-spec-graph
type: constituent-spec
tier: abstract
category: topology
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4.6 (task primitive, DAG, task-workspace binding)
depends_on:
  - wacp-spec-task
  - wacp-spec-workspace
  - wacp-spec-tree
  - wacp-spec-identity
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, topology, graph, dag, decomposition-tree, task-workspace-mapping, dependencies]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [The Dependency Graph](#2-the-dependency-graph)
3. [Graph Operations](#3-graph-operations)
4. [The Decomposition Tree](#4-the-decomposition-tree)
5. [Task Graph to Workspace Tree Mapping](#5-task-graph-to-workspace-tree-mapping)
6. [Graph Invariants](#6-graph-invariants)
7. [Trail Events](#7-trail-events)
8. [Conformance Requirements](#8-conformance-requirements)
9. [Implementation Notes](#9-implementation-notes)
10. [References](#10-references)

---

## 1. Purpose

The workspace tree (Tree spec) is the protocol's primary topological structure — it organizes execution containers into a hierarchy. The task graph is the protocol's second topological structure — it organizes work into a dependency plan.

These two structures are built from different primitives (workspaces vs. tasks), have different shapes (tree vs. DAG), and serve different purposes (containment vs. scheduling). But they are not independent — the coordinator maps tasks to workspaces, connecting the plan to its execution. The task spec (§3, §5, §7) defines the graph's schema, lifecycle, and binding rules. This spec defines the graph as a topological structure: its formal properties, its operations, its internal sub-structures (the decomposition tree within the DAG), and its relationship to the workspace tree.

The protocol uses two distinct graphs built from tasks:

- The **dependency graph** — a DAG defined by `depends_on` edges. Answers: "what must finish before this task can start?"
- The **decomposition tree** — a tree defined by `parent_task` edges. Answers: "what was this task broken down from?"

These structures are orthogonal. A task may depend on tasks that are not its siblings in the decomposition tree. A task may be decomposed from a parent that is not in its dependency chain. The DAG is about scheduling order; the tree is about planning provenance.

## 2. The Dependency Graph

### 2.1 Formal Properties

| Property | Value |
|----------|-------|
| Type | Directed acyclic graph (DAG) |
| Nodes | Tasks |
| Edge semantics | `A → B` means "B depends on A" — A must complete before B can start |
| Edge source | The `depends_on` field on each task |
| Cycles | Forbidden — validated at construction and insertion (Task spec §3, invariant 1) |
| Connected | Not required — a graph may have disconnected components (independent task groups with no inter-dependencies) |
| Rooted | Not required — a DAG may have multiple source nodes (tasks with no dependencies) |
| Scope | Intra-graph only — dependencies cannot cross graph boundaries (Task spec §3, invariant 3) |
| Mutability | Edges are immutable after task creation. Nodes may be added (progressive decomposition) but never removed. |

### 2.2 Distinguished Nodes

**Sources.** Tasks with no incoming edges (`depends_on` is empty). These are immediately ready — they can be dispatched as soon as they reach `pending` status. A graph with multiple sources has multiple independent starting points.

**Sinks.** Tasks with no outgoing edges (no other task depends on them). These are the graph's final products. A graph with one sink has a clear deliverable; a graph with multiple sinks produces multiple outputs.

**The root task.** The graph's `root_task` field identifies the top-level goal. The root task is typically a source in the dependency graph (no `depends_on` edges). When the coordinator decomposes the root into subtasks, the completion relationship is tracked through the decomposition tree (§4, `parent_task` field), not through dependency edges — `depends_on` is immutable (G-4). The coordinator decides when the root task is satisfied (Task spec §7, rule 3). The root task is a planning concept (this is the goal), not a topological requirement (this must be a source).

### 2.3 Graph Shapes

The DAG's shape determines the available parallelism:

**Linear chain.** `A → B → C → D`. No parallelism — each task depends on the previous. This is the degenerate case: the DAG is a path. Total duration equals the sum of all task durations.

**Wide parallel.** Multiple sources, one sink. Tasks execute in parallel; the sink collects results. Maximum parallelism. Duration equals the longest task plus the sink's processing time.

**Diamond.** `A → B, A → C, B → D, C → D`. Fan-out then fan-in. B and C execute in parallel after A; D waits for both. The protocol's most common pattern: decompose, parallelize, recompose.

**General DAG.** Arbitrary dependencies with partial ordering. Some tasks are parallelizable, others are serialized. The critical path (§3.2) determines the minimum duration.

The protocol imposes no constraint on graph shape. Any acyclic directed graph over tasks within a single graph is valid.

## 3. Graph Operations

### 3.1 Readiness and Dispatchability

**Readiness (graph property).** A task is ready when all tasks in its `depends_on` list have reached `completed` or `integrated` status. Readiness is a property of the task's position in the DAG — it says "this task's preconditions are satisfied," regardless of the task's own status.

**Dispatchability (operational property).** A task is dispatchable when it is ready AND its status is `pending`. Dispatchability is the DAG's primary operational query — it answers "what can be dispatched now?" The runtime computes dispatchability from the graph structure and task statuses. When a task completes, its dependents are re-evaluated.

**Algorithm (dispatchability):**

```
for each task T in the graph:
  T.dispatchable = (T.status == pending) AND
                   for all D in T.depends_on: D.status in {completed, integrated}
```

**Properties:**
- Readiness is monotonic under normal forward-only transitions: once dependencies are satisfied, they remain satisfied. However, if the coordinator cancels a `completed` dependency (task spec §4 permits cancellation of any non-terminal state), readiness can be revoked — the cancelled dependency no longer satisfies the `{completed, integrated}` requirement.
- Multiple tasks may become ready simultaneously when a shared dependency completes.
- Sources are dispatchable immediately upon reaching `pending` status.

### 3.2 Critical Path

**Definition.** The longest path through the DAG, weighted by task duration (actual or estimated). The critical path determines the minimum total duration — no amount of parallelism can reduce it below the sum of critical-path task durations.

The protocol does not compute the critical path. The graph structure makes it computable. The critical path is a coordinator concern — it informs resource allocation strategy (Task spec §6, critical-path weighted allocation) but is not a protocol operation.

**Properties:**
- The critical path may change during execution: if a task on the critical path completes faster than estimated, or if a non-critical task takes longer, the critical path shifts.
- In a linear chain, the critical path is the entire chain.
- In a wide parallel graph, the critical path is the longest parallel branch plus the sink.

### 3.3 Topological Sort

**Definition.** A linear ordering of all tasks such that for every dependency edge `A → B`, A appears before B.

The topological sort is not explicitly performed by the protocol, but it underlies readiness computation and integration ordering. Any valid dispatch order is a topological sort of the ready tasks. The Integration spec's sequential merge order (Integration spec §7.6 in PROTOCOL) respects the DAG's topological ordering.

**Properties:**
- A DAG always admits at least one topological sort (this is equivalent to the acyclicity invariant).
- Multiple valid orderings may exist — the coordinator chooses among them based on priority, resource availability, and critical path analysis.

### 3.4 Cycle Detection

**Definition.** Verify that the graph contains no directed cycles.

Cycle detection runs at two points:
1. **Graph construction** — when the coordinator submits a new graph, the runtime validates acyclicity.
2. **Task insertion** — when a task is added via progressive decomposition, the runtime validates that the new edges do not introduce a cycle.

**Algorithm:** Standard DFS-based cycle detection or topological sort (if the sort fails to include all nodes, a cycle exists). The specific algorithm is implementation-defined; the invariant is protocol-required (Task spec §3, invariant 1).

### 3.5 Dependency Resolution

**Definition.** Given a task, enumerate all tasks it transitively depends on (its ancestors in the DAG).

Dependency resolution is the downward closure of the `depends_on` relation. It answers: "what must complete before this task can start, including indirect dependencies?"

**Use cases:**
- **Context assembly.** When binding a task to a workspace, the coordinator may include checkpoints from resolved dependencies in the workspace's context (Task spec §5, rule 4).
- **Failure impact.** When a task fails, dependency resolution identifies all tasks that transitively depend on it — these cannot proceed unless the failed task is retried or the graph is restructured.
- **Progress computation.** "What percentage of the work is done?" requires knowing the full dependency set, not just direct predecessors.

## 4. The Decomposition Tree

### 4.1 Formal Properties

| Property | Value |
|----------|-------|
| Type | Rooted forest (a set of rooted trees) |
| Nodes | Tasks |
| Edge semantics | `parent_task → child_task` means "this task was decomposed into that subtask" |
| Edge source | The `parent_task` field on each task |
| Root(s) | Tasks with `parent_task: null` — the original goals |
| Depth | Unbounded (no protocol limit on decomposition depth) |
| Mutability | Edges are immutable. New nodes (subtasks) may be added; existing nodes cannot be removed or reparented. |

### 4.2 Relationship to the Dependency Graph

The decomposition tree and the dependency graph are orthogonal structures over the same node set:

| | Dependency graph | Decomposition tree |
|---|---|---|
| **Edge meaning** | "Must complete before" | "Was broken down from" |
| **Structure** | DAG (acyclicity enforced at construction and insertion) | Forest (always acyclic by construction) |
| **Governs** | Execution order, readiness, parallelism | Planning provenance, scope tracking |
| **Mutability** | Immutable edges | Immutable edges |
| **Cross-graph** | Forbidden (Task spec §3, invariant 3) | N/A — `parent_task` is within the same graph (Rule 2, Task spec §7) |

**Orthogonality example:**

```
Decomposition tree:          Dependency graph:
       Goal                     A ──► C ──► E
      / | \                     │           ▲
     A  B  C                    └──► B ──► D ┘
        |
       D  E
```

Task B is a child of Goal in the decomposition tree (Goal was broken into A, B, C). Task D is a child of B (B was further decomposed). In the dependency graph, D depends on B (must complete after B). E depends on C and D. Task A has no decomposition children but is a dependency of both B and C.

The decomposition tree tells you "where did this task come from in the planning process?" The dependency graph tells you "what must happen before this task can execute?"

### 4.3 Scope Tracking

The decomposition tree tracks scope evolution. When the coordinator progressively decomposes a task (Task spec §7), new subtasks appear as children of the decomposed task. The tree records the full history of planning decisions:

- **Initial scope.** Root tasks with `parent_task: null` — the original plan.
- **Scope expansion.** Tasks with non-null `parent_task` added after the initial graph — discovered during execution.
- **Scope depth.** The depth of the decomposition tree indicates how many levels of refinement the plan underwent.

The human highway's `task_approval` gate (Human-Highway spec §4) fires for every new task — including decomposition products. The decomposition tree gives the human context: "this new task came from decomposing task X, which itself came from Goal Y."

## 5. Task Graph to Workspace Tree Mapping

The task graph and the workspace tree are different structures on different objects. The task-workspace binding (Task spec §5) connects them. This section formalizes the mapping.

### 5.1 The Mapping

When the coordinator dispatches a task, it creates a workspace and binds the task to it. This creates a mapping: `task → workspace`. The mapping has the following properties:

| Property | Value |
|----------|-------|
| Direction | Task → workspace (a task knows its workspace; the workspace knows its task via `task_ref` in the directive) |
| Cardinality (point-in-time) | 1:1 — one task maps to at most one workspace at any moment (Task spec §5, rule 1) |
| Cardinality (over time) | 1:N — a task may be bound to multiple workspaces across retries (Task spec §5, `workspace_history`) |
| Coverage | Partial — not every workspace has a task (system bootstrap, recovery workspaces). Not every task has a workspace (pending, cancelled tasks). |
| Structural alignment | Not required — a task's position in the DAG does not determine its workspace's position in the tree |

### 5.2 Structural Non-Alignment

The task graph and workspace tree are structurally independent. A task dependency `A → B` does NOT mean workspace-A is the parent of workspace-B. Workspace parentage is determined by who created the workspace (the coordinator or a delegate), not by task dependencies.

**Typical case:** The coordinator dispatches tasks A, B, and C. All three workspaces are children of the coordinator's workspace — siblings in the tree. But in the task graph, B depends on A and C depends on B. The dependency chain (A → B → C) is encoded in the DAG; the structural relationship (all children of coordinator) is encoded in the tree.

**Delegate case:** When a delegate decomposes a task into subtasks, the subtask workspaces ARE children of the delegate's workspace in the tree. Here, the decomposition tree and the workspace tree align: `parent_task` corresponds to the parent workspace. But the dependency edges within those subtasks do not correspond to workspace tree edges — subtask workspaces are siblings, even if their tasks have dependencies.

### 5.3 Two Ordering Principles

The workspace tree orders execution by containment — a child's resources are carved from its parent's budget, and a child's failure cascades to its parent (within ownership).

The task graph orders execution by dependency — a task cannot start until its predecessors complete.

These orderings are complementary, not competing. The workspace tree governs *where* work happens and *how much* it can consume. The task graph governs *when* work happens and *what must precede it*. The coordinator consults both when dispatching: the graph says "task B is ready" (dependencies met), and the tree says "workspace W has budget remaining" (containment permits it).

## 6. Graph Invariants

These invariants hold at all times. Violation of any invariant is a protocol error.

| # | Invariant | Enforced by | Source |
|---|-----------|-------------|--------|
| G-1 | **Acyclic.** The dependency graph contains no directed cycles. | Runtime, at construction and insertion | Task spec §3, invariant 1 |
| G-2 | **Single-graph membership.** Every task belongs to exactly one graph. | Runtime, at creation | Task spec §3, invariant 2 |
| G-3 | **Intra-graph dependencies.** Dependencies reference tasks within the same graph only. | Runtime, at creation | Task spec §3, invariant 3 |
| G-4 | **Immutable edges.** Once a task's `depends_on` is set, it cannot change. | Runtime | Task spec §3 |
| G-5 | **Monotonic growth.** Tasks may be added to a graph; tasks are never removed. Cancelled tasks remain as cancelled nodes. | Protocol invariant | Task spec §3 |
| G-6 | **Decomposition containment.** Subtasks belong to the same graph as their parent task. | Runtime, at insertion | Task spec §7, rule 2 |
| G-7 | **Readiness derives from the graph.** A task is ready if and only if all its `depends_on` tasks have reached `completed` or `integrated`. A task is dispatchable if and only if it is ready and its status is `pending`. | Runtime, readiness computation | Task spec §3 |

## 7. Trail Events

The task graph does not introduce new trail event types beyond those defined in the task spec (§8). Graph operations produce events defined there:

| Operation | Trail event | Defined in |
|-----------|------------|------------|
| Graph construction | `graph_created` | Task spec §8 |
| Task addition | `task_created` (with `parent_task` if decomposed) | Task spec §8 |
| Readiness-driven dispatch | `task_assigned` | Task spec §8 |
| Completion and dependency resolution | `task_completed`, `task_status_changed` | Task spec §8 |

## 8. Conformance Requirements

| # | Requirement | Level |
|---|-------------|-------|
| G-C1 | The dependency graph MUST be validated as acyclic at construction and at every task insertion. | Core |
| G-C2 | Readiness MUST be computed from the graph structure: a task is ready when all `depends_on` tasks have reached `completed` or `integrated`. | Core |
| G-C3 | The runtime MUST re-evaluate dependents when a task completes or is integrated. | Standard |
| G-C4 | The decomposition tree MUST be maintained: `parent_task` is set at creation and immutable. | Standard |
| G-C5 | Progressive decomposition MUST add tasks to the existing graph, not create a new graph. | Standard |
| G-C6 | The runtime MUST support dependency resolution — enumerating transitive dependencies of a task. | Standard |
| G-C7 | Cancelled tasks MUST remain in the graph as cancelled nodes. The graph MUST NOT shrink. | Core |

**7 requirements.** Core ensures acyclicity and correct readiness computation. Standard ensures decomposition tracking, dependency resolution, and proper progressive decomposition.

## 9. Implementation Notes

*This section is non-normative.*

**Adjacency list representation.** The dependency graph is naturally an adjacency list: `Map[task_id → Set[task_id]]` for forward edges (dependents) and a reverse map for backward edges (dependencies). Both are needed: the reverse map answers "what does this task depend on?" (readiness check), the forward map answers "what depends on this task?" (propagation on completion).

**Incremental readiness.** Rather than recomputing readiness for all tasks, maintain a counter per task: `remaining_dependencies = len(depends_on) - completed_dependencies`. When a dependency completes, decrement the counter. When the counter reaches zero, the task is ready. This is O(1) per completion event.

**Decomposition tree as a flat field.** The decomposition tree does not require a tree data structure. The `parent_task` field on each task is sufficient. "Show me the decomposition history" is a recursive query upward through `parent_task`. "Show me all subtasks" is a filter on `parent_task == this_task_id`.

**Graph-tree mapping index.** Maintain a bidirectional index between task IDs and workspace IDs: `task_id → workspace_id` (current binding) and `workspace_id → task_id` (reverse lookup). This enables efficient cross-queries: "which task is this workspace executing?" and "which workspace is executing this task?"

**Critical path computation.** Longest-path in a DAG: topologically sort the tasks, then relax edges in topological order, tracking the longest path to each node. O(V + E) where V is tasks and E is dependency edges. Recompute when estimates change or actual durations become available.

## 10. References

| Spec | Sections referenced | Relationship |
|------|-------------------|--------------|
| [Task](../primitives/task.md) | §2 (schema), §3 (DAG, three invariants), §4 (lifecycle), §5 (task-workspace binding), §6 (resource estimation), §7 (decomposition), §8 (trail events) | The graph's nodes are tasks. Task spec defines node properties; this spec defines node relationships and the graph's topology. |
| [Workspace Tree](tree.md) | §2 (structural tree), §4 (operations), §5 (subtree as a unit) | The task graph maps to the workspace tree through task-workspace binding. The two structures are structurally independent but operationally connected. |
| [Workspace](../primitives/workspace.md) | §4 (creation, directive), §5 (workspace tree), §6 (resource management) | Workspaces are the execution containers for tasks. The task-workspace mapping connects the plan to its execution. |
| [Identity](../foundations/identity.md) | §2 (identifier rules) | Task IDs, graph IDs, workspace IDs referenced in graph operations. |
| [PROTOCOL.md](../PROTOCOL.md) | §4.6 (task primitive, DAG) | Canonical task graph definition. |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
