# WACP Implementation: Topology Operations

```yaml
id: wacp-impl-topology
type: implementation-spec
status: complete
created: 2026-03-22
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §6.3 (workspace tree)
  - §6.8 (visibility and authority)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-storage
  - wacp-spec-workspace
  - wacp-spec-trail
  - wacp-spec-signal
  - wacp-spec-identity
  - wacp-spec-user
  - wacp-spec-roles
  - wacp-topo-tree
  - wacp-topo-graph
  - wacp-topo-visibility
  - wacp-topo-ownership
  - wacp-topo-causation
  - wacp-topo-channels
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, topology, tree, graph, visibility, ownership, causation, channels, port-rights]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Workspace Tree](#2-workspace-tree)
3. [Task Graph](#3-task-graph)
4. [Visibility Graph](#4-visibility-graph)
5. [Ownership Domains](#5-ownership-domains)
6. [Causation](#6-causation)
7. [Communication Topology](#7-communication-topology)
8. [Compound Operations](#8-compound-operations)
9. [Consistency and Recovery](#9-consistency-and-recovery)
10. [References](#10-references)

## 1. Purpose

This spec defines how the six topology structures of the WACP protocol become data structures and algorithms in the Rust runtime. It answers "how are relationships between workspaces represented, queried, and mutated" — not "what the relationships mean" (that's the six topology specs) or "how actors process messages" (that's the runtime spec).

The protocol defines six overlapping but independent topologies over the same node set (workspaces and tasks):

| Topology | Spec | Structure | Mutability | Owner |
|----------|------|-----------|------------|-------|
| Workspace tree | `topology/tree.md` | Rooted tree (parent pointers) | Monotonically growing; edges stable except reparenting | Coordinator actor |
| Task graph | `topology/graph.md` | DAG (dependency edges) + forest (decomposition edges) | Monotonically growing; edges immutable after creation | Coordinator actor |
| Visibility graph | `topology/visibility.md` | Directed graph (visibility set per node) | Additive only — edges added, never removed | Coordinator actor (grants); workspace actor (reads) |
| Ownership domains | `topology/ownership.md` | Partition of nodes by `owner` field | Mutable via transfer | Coordinator actor |
| Causal forest | `topology/causation.md` | Partition of structural tree by `originator` field | Immutable — originator set at creation, never changed | Set at creation; read-only thereafter |
| Port rights graph | `topology/channels.md` | Directed multigraph (send/receive/send-once rights) | Highly mutable — created, transferred, consumed, revoked | Coordinator actor (creation, revocation); workspace actor (transfer via envelope) |

These six structures share nodes but have independent edge sets, independent mutation rules, and independent invariants. The runtime must maintain all six simultaneously, update them atomically when compound operations span multiple topologies, and recover them from the trail after a crash.

**Scope.** Data representation for each topology. Indexing strategies for efficient queries. Mutation operations and their atomicity requirements. Traversal algorithms (upward, downward, lateral, causal). Failure cascade algorithm with ownership-bounded propagation. Reparenting mechanics. Task readiness computation. Visibility grant insertion with containment enforcement. Port rights lifecycle management. Compound operations that span multiple topologies (workspace creation, failure cascade, ownership transfer). Recovery of topology state from the trail.

**Not in scope.** Actor message processing (runtime spec, §3). Permission engine evaluation (runtime spec, §5). Workspace internal state (runtime spec, §8). Concurrency model (runtime spec, §14). Trail write mechanics (runtime spec, §6). Storage backends (storage spec). Integration logic — how the coordinator uses the task graph to decide what to integrate (that's the integration spec's job). Task scheduling — how the coordinator decides which ready task to dispatch next (that's the task-scheduling spec's job). This spec provides the data structures and operations those specs build on.

**Design constraint.** All topology state is owned by the coordinator actor. Workspace actors hold read-only projections (their own visibility set, authority set, originator, owner). The coordinator is the single writer for all topology mutations — there are no concurrent writers and no locking. Topology state is derived from the trail — every mutation produces a trail entry, and recovery reconstructs topology state by replaying those entries. The topology indices are acceleration structures, not sources of truth.

---

## 2. Workspace Tree

The workspace tree is the containment hierarchy — every workspace except the root has exactly one parent, and the root is the coordinator's workspace. The tree determines signal propagation paths, failure cascade scope, budget containment, and default visibility. This section defines the data representation, traversal algorithms, failure cascade, and reparenting.

### 2.1 Data Representation

The tree is stored as a flat table in the coordinator actor's state, not as a recursive data structure. Each entry holds the workspace id, its parent, and metadata needed for traversal and cascade decisions.

```rust
pub struct WorkspaceTree {
    nodes: HashMap<WorkspaceId, TreeNode>,
    root: WorkspaceId,
    children_index: HashMap<WorkspaceId, Vec<WorkspaceId>>,
    originator_index: HashMap<OriginatorValue, Vec<WorkspaceId>>,
    owner_index: HashMap<UserId, Vec<WorkspaceId>>,
}

pub struct TreeNode {
    pub id: WorkspaceId,
    pub parent: Option<WorkspaceId>,       // None only for root
    pub owner: UserId,
    pub originator: OriginatorValue,
    pub status: WorkspaceStatus,
    pub created_at: Timestamp,
}

pub enum OriginatorValue {
    System,
    User(UserId),
}
```

**Why a flat table.** A recursive tree structure (`children: Vec<TreeNode>`) makes traversal natural but mutation expensive — inserting a node requires finding the parent, reparenting requires moving subtrees, and ownership queries require full scans. A flat table with three indices (`children_index`, `originator_index`, `owner_index`) makes all operations O(1) lookup + O(k) traversal where k is the result set size.

**`children_index`.** Maps parent id to its children. Updated on workspace creation (append child) and reparenting (remove from old parent, append to new). This is the primary downward traversal index.

**`originator_index`.** Maps originator value to all workspaces with that originator. Used for causal traversal (§6) and causal impact queries. Updated only on workspace creation — originator is immutable.

**`owner_index`.** Maps user id to all workspaces owned by that user. Used for ownership domain queries (§5) and escalation routing. Updated on workspace creation and ownership transfer.

### 2.2 Traversals

Four traversal directions, each with a defined algorithm and use case.

**Upward (child → root).** Follow parent pointers from a starting node to the root. Used for signal propagation — signals travel up the tree from the emitting workspace to the coordinator.

```rust
impl WorkspaceTree {
    pub fn ancestors(&self, id: &WorkspaceId) -> Vec<WorkspaceId> {
        let mut path = Vec::new();
        let mut current = id.clone();
        while let Some(node) = self.nodes.get(&current) {
            if let Some(parent) = &node.parent {
                path.push(parent.clone());
                current = parent.clone();
            } else {
                break; // reached root
            }
        }
        path
    }
}
```

Complexity: O(d) where d is the depth of the starting node. The tree is typically shallow — the coordinator dispatches tasks to workers at depth 1, delegates create subtrees at depth 2–3. Depths beyond 5 are unusual.

**Downward (parent → leaves).** BFS from a starting node using the `children_index`. Used for failure cascade, budget containment checks, and subtree enumeration.

```rust
pub fn descendants(&self, id: &WorkspaceId) -> Vec<WorkspaceId> {
    let mut result = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(id.clone());
    while let Some(current) = queue.pop_front() {
        if let Some(children) = self.children_index.get(&current) {
            for child in children {
                result.push(child.clone());
                queue.push_back(child.clone());
            }
        }
    }
    result
}
```

Complexity: O(n) where n is the subtree size. This is the most expensive traversal — failure cascade on a delegate with many children visits all of them.

**Lateral (sibling enumeration).** Siblings share a parent. Look up the parent, then look up the parent's children in `children_index`, excluding the querying node. Used for integration ordering — the coordinator may integrate sibling workspaces in a defined order.

```rust
pub fn siblings(&self, id: &WorkspaceId) -> Vec<WorkspaceId> {
    let node = match self.nodes.get(id) { Some(n) => n, None => return vec![] };
    let parent = match &node.parent { Some(p) => p, None => return vec![] };
    self.children_index.get(parent)
        .map(|children| children.iter().filter(|c| *c != id).cloned().collect())
        .unwrap_or_default()
}
```

Complexity: O(k) where k is the number of siblings.

**Causal (filter by originator).** Enumerate all workspaces with a given originator that are also descendants of a given node. This is an intersection: `originator_index[originator] ∩ descendants(node)`. Used for causal impact queries — "if user X's state changes, which workspaces in this subtree are affected?"

For small trees, this is computed by filtering `descendants()` by originator. For large trees with frequent causal queries, a combined index (`(parent, originator) → children`) could be added, but the initial implementation uses the simple intersection — the tree is expected to be shallow and narrow in typical deployments.

### 2.3 Failure Cascade

When a workspace transitions to `failed`, its descendants may need to fail too. The cascade algorithm respects ownership boundaries (tree spec, invariant T-8): same-owner children are failed, cross-owner children are reparented to the coordinator.

**Algorithm.** BFS from the failed workspace, partitioning children at each level:

```rust
pub struct CascadeResult {
    pub failed: Vec<WorkspaceId>,
    pub reparented: Vec<WorkspaceId>,
}

pub fn failure_cascade(
    &self,
    failed_id: &WorkspaceId,
    failed_owner: &UserId,
) -> CascadeResult {
    let mut result = CascadeResult { failed: vec![], reparented: vec![] };
    let mut queue = VecDeque::new();
    queue.push_back(failed_id.clone());

    while let Some(current) = queue.pop_front() {
        let children = match self.children_index.get(&current) {
            Some(c) => c.clone(),
            None => continue,
        };

        for child_id in children {
            let child = match self.nodes.get(&child_id) {
                Some(n) => n,
                None => continue,
            };

            // Skip children already in terminal states
            if child.status.is_terminal() {
                continue;
            }

            if &child.owner == failed_owner {
                // Same owner: cascade failure, recurse into subtree
                result.failed.push(child_id.clone());
                queue.push_back(child_id);
            } else {
                // Different owner: reparent to root, do NOT recurse
                result.reparented.push(child_id.clone());
                // Reparented subtree is not cascaded — it survives
            }
        }
    }

    result
}
```

**Cascade ordering.** The BFS visits children in creation order (the order they appear in `children_index`). Each failed child produces a `workspace_state_changed` trail entry with `trigger: parent_failed` before the cascade continues to its children. This means trail entries for the cascade are written top-down — a parent's failure entry precedes its children's failure entries.

**Root failure.** When the root workspace fails, the cascade is total — all workspaces fail regardless of ownership (tree spec, invariant T-9). The algorithm skips the ownership check for root failure:

```rust
if failed_id == &self.root {
    // Root failure: everything fails, no reparenting
    for child_id in children {
        if !child.status.is_terminal() {
            result.failed.push(child_id.clone());
            queue.push_back(child_id);
        }
    }
}
```

**Cascade execution.** The cascade algorithm computes the set of workspaces to fail and reparent. The coordinator actor then executes the cascade sequentially — sending abort commands to each workspace actor in the `failed` set, and reparent commands for the `reparented` set. Each abort produces a trail entry. The cascade is not atomic in the database sense — it is a sequence of individual state transitions, each independently trail-recorded. If the runtime crashes mid-cascade, recovery detects the incomplete cascade (parent is `failed`, children are not) and resumes it.

### 2.4 Reparenting

Reparenting moves a workspace from its current parent to the root coordinator. It occurs only during failure cascade for cross-owner children (§2.3). The protocol does not support arbitrary reparenting — only coordinator-as-new-parent.

```rust
pub fn reparent_to_root(&mut self, child_id: &WorkspaceId) -> Result<(), TreeError> {
    let node = self.nodes.get_mut(child_id)
        .ok_or(TreeError::NotFound)?;
    let old_parent = node.parent.clone()
        .ok_or(TreeError::CannotReparentRoot)?;

    // Remove from old parent's children
    if let Some(siblings) = self.children_index.get_mut(&old_parent) {
        siblings.retain(|id| id != child_id);
    }

    // Add to root's children
    self.children_index.entry(self.root.clone())
        .or_default()
        .push(child_id.clone());

    // Update parent pointer
    node.parent = Some(self.root.clone());

    Ok(())
}
```

**What reparenting preserves.** The workspace's id, owner, originator, visibility set, authority set, and all nine internal components are unchanged (tree spec, invariant T-10). Only the `parent` field changes. The workspace continues executing — its agent does not notice the reparent. Signal propagation changes — signals now travel to the root coordinator instead of to the old (now failed) parent.

**Trail event.** A `workspace_reparented` trail entry is written with the old parent, new parent (always root), and reason (`parent_failed`).

### 2.5 Tree Insertion

Workspace creation adds a node to the tree. The coordinator actor calls `insert` when creating a new workspace.

```rust
pub fn insert(&mut self, node: TreeNode) -> Result<(), TreeError> {
    let parent = node.parent.as_ref()
        .ok_or(TreeError::MissingParent)?;

    // Parent must exist and be in a non-terminal state
    let parent_node = self.nodes.get(parent)
        .ok_or(TreeError::ParentNotFound)?;
    if parent_node.status.is_terminal() {
        return Err(TreeError::ParentTerminal);
    }

    let id = node.id.clone();
    let owner = node.owner.clone();
    let originator = node.originator.clone();

    // Update indices
    self.children_index.entry(parent.clone())
        .or_default()
        .push(id.clone());
    self.originator_index.entry(originator)
        .or_default()
        .push(id.clone());
    self.owner_index.entry(owner)
        .or_default()
        .push(id.clone());

    // Insert node
    self.nodes.insert(id, node);

    Ok(())
}
```

**Monotonic growth.** Nodes are never removed from the tree (tree spec, invariant T-4). Terminal workspaces remain in the `nodes` map as archived records. The tree only grows. This simplifies recovery — replaying `workspace_created` events in order reconstructs the tree exactly.

### 2.6 Invariant Enforcement

The tree enforces 10 invariants (tree spec). Most are structural guarantees maintained by construction:

| Invariant | How enforced |
|-----------|-------------|
| T-1: Single parent | `insert` sets parent once; `reparent_to_root` changes it atomically |
| T-2: Acyclic | Insert only as child of existing node — no upward edges, no self-loops |
| T-3: Connected | Every node has a parent (except root) — insert requires parent existence |
| T-4: Monotonic growth | No `remove` method exists |
| T-5: Stable edges | Only `reparent_to_root` modifies edges, and only during failure cascade |
| T-6: Originator inheritance | Set at creation per inheritance rules (§6); immutable field, no setter |
| T-7: Containment | Enforced by permission engine (runtime spec, §5) at query time, not by tree structure |
| T-8: Ownership-bounded cascade | `failure_cascade` algorithm partitions by owner |
| T-9: Root failure total | `failure_cascade` skips ownership check when `failed_id == root` |
| T-10: Reparenting preserves identity | `reparent_to_root` modifies only the `parent` field |

---

## 3. Task Graph

The task graph is a DAG of dependencies between tasks — "task B cannot start until task A completes." Layered on top of this is a decomposition tree — "task A was broken down into subtasks B, C, D." These are two independent structures over the same node set. The DAG drives scheduling (which task is ready?); the decomposition tree provides provenance (where did this task come from?).

The task graph is distinct from the workspace tree. A task may be assigned to any workspace, and the dependency edges between tasks do not correspond to parent-child edges in the workspace tree. The coordinator uses the task graph to decide *what* to dispatch; it uses the workspace tree to decide *where* to dispatch it.

### 3.1 Data Representation

```rust
pub struct TaskGraph {
    tasks: HashMap<TaskId, TaskNode>,
    /// Forward edges: task → tasks that depend on it (dependents)
    forward: HashMap<TaskId, Vec<TaskId>>,
    /// Reverse edges: task → tasks it depends on (dependencies)
    reverse: HashMap<TaskId, Vec<TaskId>>,
    /// Decomposition: parent_task → subtasks
    subtasks: HashMap<TaskId, Vec<TaskId>>,
    /// Task → workspace binding
    task_workspace: HashMap<TaskId, WorkspaceId>,
    /// Workspace → task reverse index
    workspace_task: HashMap<WorkspaceId, TaskId>,
}

pub struct TaskNode {
    pub id: TaskId,
    pub name: String,
    pub description: String,
    pub parent_task: Option<TaskId>,
    pub status: TaskStatus,
    pub workspace_ref: Option<WorkspaceId>,
    pub remaining_dependencies: u32,
    pub created_at: Timestamp,
}
```

**Two adjacency lists.** `forward` maps a task to the tasks that depend on it (its dependents). `reverse` maps a task to the tasks it depends on (its dependencies). Both are built at insertion time and never modified — dependency edges are immutable after creation (graph spec, invariant G-4). Having both directions enables:
- Readiness update: when task A completes, iterate `forward[A]` to decrement dependents' counters.
- Dependency query: `reverse[B]` gives all of B's prerequisites.

**`remaining_dependencies` counter.** Each task tracks how many of its dependencies are not yet complete. When a dependency completes, the coordinator decrements this counter. When it reaches zero, the task is ready. This is the key optimization — readiness is O(1) per completion event instead of O(k) to re-scan all dependencies.

**Task-workspace binding.** `task_workspace` maps a task to the workspace executing it. `workspace_task` provides the reverse lookup. A task has at most one workspace at a time (it may be reassigned if the workspace fails). A workspace executes at most one task.

### 3.2 Task Insertion

Tasks are inserted by the coordinator when decomposing work. Insertion validates acyclicity and sets up the dependency counter.

```rust
impl TaskGraph {
    pub fn insert(
        &mut self,
        id: TaskId,
        name: String,
        description: String,
        parent_task: Option<TaskId>,
        depends_on: Vec<TaskId>,
    ) -> Result<(), GraphError> {
        // Validate: all dependencies must exist
        for dep in &depends_on {
            if !self.tasks.contains_key(dep) {
                return Err(GraphError::DependencyNotFound(dep.clone()));
            }
        }

        // Validate: no cycles — the new task's dependencies must not
        // transitively depend on the new task. Since the task doesn't
        // exist yet, this is trivially satisfied for a new insertion.
        // Cycle risk only exists if depends_on contains a task that
        // is already a dependent of a task that depends_on the new task —
        // but since the new task has no dependents yet, no cycle is possible.
        // Cycles are structurally prevented by insert-time validation:
        // edges point only to existing tasks, and existing tasks cannot
        // gain new dependencies after creation.

        // Validate: parent_task must exist if provided
        if let Some(ref parent) = parent_task {
            if !self.tasks.contains_key(parent) {
                return Err(GraphError::ParentTaskNotFound(parent.clone()));
            }
        }

        let remaining = depends_on.len() as u32;

        // Build adjacency entries
        for dep in &depends_on {
            self.forward.entry(dep.clone())
                .or_default()
                .push(id.clone());
            self.reverse.entry(id.clone())
                .or_default()
                .push(dep.clone());
        }

        // Build decomposition entry
        if let Some(ref parent) = parent_task {
            self.subtasks.entry(parent.clone())
                .or_default()
                .push(id.clone());
        }

        self.tasks.insert(id.clone(), TaskNode {
            id,
            name,
            description,
            parent_task,
            status: TaskStatus::Draft,
            workspace_ref: None,
            remaining_dependencies: remaining,
            created_at: Timestamp::now(),
        });

        Ok(())
    }
}
```

**Cycle prevention by construction.** Dependency edges point only from a new task to existing tasks. Existing tasks cannot gain new dependencies after creation (graph spec, invariant G-4). Since a new task has no dependents when inserted, and its dependencies are all pre-existing nodes, a cycle cannot form. This structural guarantee eliminates the need for a runtime cycle detection algorithm — acyclicity is maintained as a construction invariant.

### 3.3 Readiness and Dispatchability

A task is **ready** when all its dependencies have completed (or been integrated). A task is **dispatchable** when it is ready AND its status is `pending` (approved by a gate, if gates are configured).

```rust
impl TaskGraph {
    /// Called when a task completes. Decrements dependent counters
    /// and returns newly ready tasks.
    pub fn mark_completed(&mut self, id: &TaskId) -> Vec<TaskId> {
        let task = match self.tasks.get_mut(id) {
            Some(t) => t,
            None => return vec![],
        };
        task.status = TaskStatus::Completed;

        let mut newly_ready = Vec::new();

        let dependents = match self.forward.get(id) {
            Some(deps) => deps.clone(),
            None => return newly_ready,
        };

        for dependent_id in dependents {
            if let Some(dependent) = self.tasks.get_mut(&dependent_id) {
                dependent.remaining_dependencies =
                    dependent.remaining_dependencies.saturating_sub(1);
                if dependent.remaining_dependencies == 0 {
                    newly_ready.push(dependent_id);
                }
            }
        }

        newly_ready
    }

    /// Returns all tasks that are ready and dispatchable.
    pub fn dispatchable(&self) -> Vec<&TaskId> {
        self.tasks.values()
            .filter(|t| t.status == TaskStatus::Pending && t.remaining_dependencies == 0)
            .map(|t| &t.id)
            .collect()
    }

    /// Returns all tasks with no dependencies (sources / immediately ready).
    pub fn sources(&self) -> Vec<&TaskId> {
        self.tasks.values()
            .filter(|t| {
                self.reverse.get(&t.id)
                    .map_or(true, |deps| deps.is_empty())
            })
            .map(|t| &t.id)
            .collect()
    }
}
```

**Counter-based readiness.** When task A completes, the coordinator calls `mark_completed("A")`. The method iterates A's forward edges (dependents), decrementing each dependent's `remaining_dependencies` counter. Any dependent whose counter reaches zero is returned as newly ready. The coordinator then decides whether to dispatch each newly ready task (this decision is the task-scheduling spec's concern — the topology spec only provides the readiness signal).

**Complexity.** `mark_completed` is O(k) where k is the number of dependents of the completed task — typically 1–3 in practice. `dispatchable` is O(n) over all tasks — called infrequently (when the coordinator needs to fill idle workspaces).

### 3.4 Task Failure

When a workspace fails, its task may need special handling. The task itself transitions to `failed`, but its dependents are not automatically failed — the coordinator decides whether to retry (create a new workspace for the same task), reassign, or cascade the failure.

```rust
pub fn mark_failed(&mut self, id: &TaskId) -> Vec<TaskId> {
    let task = match self.tasks.get_mut(id) {
        Some(t) => t,
        None => return vec![],
    };
    task.status = TaskStatus::Failed;
    task.workspace_ref = None;

    // Return dependents that are now blocked — their dependency
    // will never complete unless the coordinator retries this task.
    self.forward.get(id)
        .cloned()
        .unwrap_or_default()
}
```

The returned dependent list is advisory — the coordinator uses it to decide whether to cancel dependents, retry the failed task, or escalate to the human. The task graph does not make policy decisions.

### 3.5 Task-Workspace Binding

When the coordinator dispatches a task to a workspace, the binding is recorded in both directions:

```rust
pub fn bind_task_to_workspace(
    &mut self,
    task_id: &TaskId,
    workspace_id: &WorkspaceId,
) -> Result<(), GraphError> {
    let task = self.tasks.get_mut(task_id)
        .ok_or(GraphError::TaskNotFound)?;
    task.workspace_ref = Some(workspace_id.clone());
    task.status = TaskStatus::Assigned;

    self.task_workspace.insert(task_id.clone(), workspace_id.clone());
    self.workspace_task.insert(workspace_id.clone(), task_id.clone());
    Ok(())
}

pub fn unbind_task(&mut self, task_id: &TaskId) {
    if let Some(task) = self.tasks.get_mut(task_id) {
        if let Some(ws) = task.workspace_ref.take() {
            self.workspace_task.remove(&ws);
        }
        self.task_workspace.remove(task_id);
    }
}
```

Unbinding occurs when a workspace fails (the task may be retried on a new workspace) or when migration completes (the task remains bound to the same workspace id — the agent changed, not the workspace).

### 3.6 Decomposition

The decomposition tree tracks provenance — "task X was decomposed into subtasks Y and Z." This is recorded via the `parent_task` field on each task and the `subtasks` index.

**Progressive decomposition.** A delegate workspace may decompose its task into subtasks mid-execution. The coordinator inserts subtasks with `parent_task` set to the delegate's task. The subtasks may have dependencies among themselves (forming a sub-DAG within the larger DAG) or may be independent.

```rust
pub fn subtasks_of(&self, task_id: &TaskId) -> Vec<&TaskId> {
    self.subtasks.get(task_id)
        .map(|ids| ids.iter().collect())
        .unwrap_or_default()
}

pub fn is_leaf_task(&self, task_id: &TaskId) -> bool {
    self.subtasks.get(task_id)
        .map_or(true, |s| s.is_empty())
}
```

**Decomposition containment (invariant G-6).** Subtask dependencies must be within the same graph — a subtask cannot depend on a task from a different decomposition hierarchy. This is enforced at insertion time: `insert` validates that all `depends_on` entries exist in the same `TaskGraph`. Since there is one global `TaskGraph`, this invariant is trivially satisfied. If multi-graph support is added in the future, this check would need strengthening.

### 3.7 Invariant Enforcement

| Invariant | How enforced |
|-----------|-------------|
| G-1: Acyclic | Edges only from new tasks to existing tasks; no post-creation edge addition |
| G-2: Single-graph membership | One global `TaskGraph` — no mechanism for multiple graphs |
| G-3: Intra-graph dependencies | All dependencies validated against the single graph at insertion |
| G-4: Immutable edges | No `add_dependency` method after insertion; `forward`/`reverse` only written at `insert` |
| G-5: Monotonic growth | No `remove_task` method; failed/cancelled tasks remain in the graph |
| G-6: Decomposition containment | Single graph — trivially satisfied |
| G-7: Readiness derives from graph | `remaining_dependencies` counter computed from `depends_on` edges |

---

## 4. Visibility Graph

The visibility graph defines who can read whose state — "workspace A can read workspace B's working memory, checkpoints, and local trail." Visibility is a directed graph: A → B means A can see B, but B cannot necessarily see A. Edges are additive only — once granted, visibility is never revoked.

### 4.1 Data Representation

Visibility is stored as a set per workspace in the coordinator, with a reverse index for "who can see me?" queries.

```rust
pub struct VisibilityGraph {
    /// Forward: workspace → set of workspaces it can see
    can_see: HashMap<WorkspaceId, HashSet<WorkspaceId>>,
    /// Reverse: workspace → set of workspaces that can see it
    seen_by: HashMap<WorkspaceId, HashSet<WorkspaceId>>,
}
```

**Why `HashSet`.** Visibility checks are hot-path operations — every `ReadResource` and `QueryTrail` RPC validates that the caller has visibility to the target. `HashSet` gives O(1) membership checks. The set is small per workspace (typically 1–10 entries for workers, up to n for the coordinator), so memory overhead is minimal.

**Self-visibility is implicit.** Every workspace can see itself (visibility spec, invariant VI-1). This is not stored in the graph — it is checked in the query path: `can_see(A, B)` returns true if `A == B` or `B ∈ can_see[A]`.

### 4.2 Default Visibility at Creation

When a workspace is created, its initial visibility set is determined by its role:

```rust
impl VisibilityGraph {
    pub fn initialize_for_workspace(
        &mut self,
        id: &WorkspaceId,
        role: &RoleName,
        parent: &WorkspaceId,
        tree: &WorkspaceTree,
    ) {
        let mut visible = HashSet::new();

        match role.base_role() {
            BaseRole::Coordinator => {
                // Coordinator sees all workspaces
                for ws_id in tree.all_workspace_ids() {
                    visible.insert(ws_id.clone());
                }
            }
            BaseRole::Worker => {
                // Worker sees only itself (implicit) — empty set
            }
            BaseRole::Observer => {
                // Observer sees only itself (implicit) — empty set
            }
        }

        // Delegates (derived from coordinator) see self + subtree
        if role.is_delegate() {
            for descendant in tree.descendants(parent) {
                visible.insert(descendant);
            }
        }

        // Update reverse index
        for target in &visible {
            self.seen_by.entry(target.clone())
                .or_default()
                .insert(id.clone());
        }

        self.can_see.insert(id.clone(), visible);
    }
}
```

**Coordinator total visibility.** The coordinator's visibility set contains every workspace. When a new workspace is created, it is automatically added to the coordinator's visibility set — `grant(coordinator_id, new_workspace_id)` is called as part of workspace creation.

### 4.3 Dynamic Grants

The coordinator may grant additional visibility at runtime. Grants are additive only — there is no revoke operation.

```rust
pub fn grant(
    &mut self,
    viewer: &WorkspaceId,
    target: &WorkspaceId,
    grantor_visibility: &HashSet<WorkspaceId>,
) -> Result<bool, VisibilityError> {
    // Invariant VI-4: grantor can only grant visibility to targets
    // within its own visibility scope
    if !grantor_visibility.contains(target) {
        return Err(VisibilityError::GrantorCannotSee(target.clone()));
    }

    let set = self.can_see.entry(viewer.clone()).or_default();

    // Already visible — grant is idempotent
    if set.contains(target) {
        return Ok(false);
    }

    set.insert(target.clone());

    self.seen_by.entry(target.clone())
        .or_default()
        .insert(viewer.clone());

    Ok(true) // newly granted
}
```

**Grantor scoping (invariant VI-4).** The grantor can only grant visibility to resources within its own visibility scope. For the root coordinator (visibility: all), this is trivially satisfied. For delegates, this enforces containment — a delegate cannot grant visibility to workspaces outside its subtree.

**Containment check (invariant VI-3).** `child.visibility ⊆ parent.visibility`. This is maintained by construction:
- At creation, default visibility is a subset of the parent's (workers start empty, delegates see their subtree which is a subtree of the parent's scope).
- Dynamic grants pass through the grantor's visibility check — the grantor is the parent or coordinator, whose visibility is a superset.

No explicit containment check is needed at grant time — the grantor scoping rule ensures containment transitively.

### 4.4 Visibility Queries

```rust
impl VisibilityGraph {
    /// Can viewer see target?
    pub fn can_see(&self, viewer: &WorkspaceId, target: &WorkspaceId) -> bool {
        // Self-visibility is implicit (VI-1)
        if viewer == target {
            return true;
        }
        self.can_see.get(viewer)
            .map_or(false, |set| set.contains(target))
    }

    /// What can this workspace see?
    pub fn visible_to(&self, viewer: &WorkspaceId) -> &HashSet<WorkspaceId> {
        static EMPTY: HashSet<WorkspaceId> = HashSet::new();
        self.can_see.get(viewer).unwrap_or(&EMPTY)
    }

    /// Who can see this workspace?
    pub fn who_can_see(&self, target: &WorkspaceId) -> &HashSet<WorkspaceId> {
        static EMPTY: HashSet<WorkspaceId> = HashSet::new();
        self.seen_by.get(target).unwrap_or(&EMPTY)
    }
}
```

**Where visibility is enforced.** The workspace actor checks visibility before serving `ReadResource` and `QueryTrail` requests. The workspace actor holds a copy of its visibility set (shared via `Arc<HashSet<WorkspaceId>>` from the coordinator). When a grant occurs, the coordinator sends a `VisibilityGrant` command to the workspace actor, which adds the new target to its local copy. This avoids checking the coordinator's graph on every read — the workspace actor has a local, always-current view of its own visibility.

### 4.5 Authority

Authority (write scope) is separate from visibility. It is frozen at creation and never modified (visibility spec, invariant VI-5).

```rust
pub struct AuthoritySet {
    /// Resources this workspace can write to (role-derived + resource-scoped)
    writable: HashSet<ResourceId>,
}
```

Authority is not stored in the `VisibilityGraph` — it is a per-workspace field in the `WorkspaceState` struct (runtime spec, §8). It is set at creation and enforced by the permission engine. The topology spec does not manage authority mutations because there are none — authority is immutable.

**Invariant VI-6: write scope within read scope.** `authority ⊆ visibility`. This is checked at workspace creation — the coordinator validates that the authority set is a subset of the initial visibility set. Since visibility only grows (additive grants) and authority never changes, the invariant holds for the lifetime of the workspace.

### 4.6 Invariant Enforcement

| Invariant | How enforced |
|-----------|-------------|
| VI-1: Self-visibility | `can_see()` returns true for `viewer == target` without consulting the set |
| VI-2: Additive only | No `revoke` method exists; `grant` is the only mutation |
| VI-3: Containment | Grantor scoping (VI-4) ensures child visibility is a subset of grantor visibility |
| VI-4: Grantor-scoped | `grant()` checks `grantor_visibility.contains(target)` |
| VI-5: Authority frozen | `AuthoritySet` has no mutation methods after construction |
| VI-6: Write ⊆ read | Checked at workspace creation; visibility grows, authority doesn't |

---

## 5. Ownership Domains

Ownership partitions workspaces by the `owner` field — every workspace has an owner (a `user_id`), and ownership determines escalation routing and failure cascade boundaries. Unlike originator (§6), ownership is mutable — the coordinator can transfer a workspace to a different owner.

### 5.1 Data Representation

Ownership is tracked in two places: the `owner` field on each `TreeNode` (§2.1), and a dedicated index for domain queries.

```rust
// The owner_index in WorkspaceTree (§2.1) serves as the primary structure:
// owner_index: HashMap<UserId, Vec<WorkspaceId>>
//
// Additional escalation routing table:
pub struct EscalationRouter {
    /// workspace → owner (redundant with TreeNode.owner, but avoids
    /// tree lookup on every escalation)
    routing: HashMap<WorkspaceId, UserId>,
}
```

The `owner_index` in `WorkspaceTree` is the authoritative ownership structure. The `EscalationRouter` is a derived index optimized for the hot path — when a workspace emits an `escalation` signal, the coordinator needs the owner's identity immediately to route it to the highway.

### 5.2 Ownership at Creation

The workspace's owner is set at creation following the inheritance rules from the workspace spec:

1. **Default inheritance.** The new workspace inherits the parent's owner.
2. **Explicit override.** The coordinator may set a different owner at creation (e.g., when creating a workspace on behalf of a specific user).

```rust
pub fn resolve_owner(
    parent: &TreeNode,
    explicit_owner: Option<&UserId>,
) -> UserId {
    explicit_owner
        .cloned()
        .unwrap_or_else(|| parent.owner.clone())
}
```

### 5.3 Ownership Transfer

Transfer changes a workspace's owner. It is a per-workspace operation — children are not affected (ownership spec, invariant OW-3).

```rust
impl WorkspaceTree {
    pub fn transfer_ownership(
        &mut self,
        workspace_id: &WorkspaceId,
        new_owner: &UserId,
    ) -> Result<UserId, TreeError> {
        let node = self.nodes.get_mut(workspace_id)
            .ok_or(TreeError::NotFound)?;
        let old_owner = node.owner.clone();

        if &old_owner == new_owner {
            return Ok(old_owner); // no-op
        }

        // Update node
        node.owner = new_owner.clone();

        // Update owner_index: remove from old, add to new
        if let Some(list) = self.owner_index.get_mut(&old_owner) {
            list.retain(|id| id != workspace_id);
        }
        self.owner_index.entry(new_owner.clone())
            .or_default()
            .push(workspace_id.clone());

        Ok(old_owner)
    }
}

impl EscalationRouter {
    pub fn update_owner(
        &mut self,
        workspace_id: &WorkspaceId,
        new_owner: &UserId,
    ) {
        self.routing.insert(workspace_id.clone(), new_owner.clone());
    }
}
```

**What transfer changes.** Escalation routing changes immediately — escalations for this workspace now route to the new owner. Failure cascade scope changes — if this workspace's parent fails, the cascade algorithm (§2.3) compares the workspace's owner against the parent's owner; the new owner is used.

**What transfer does not change.** The originator field (immutable — ownership spec, invariant OW-4). The workspace's children (non-cascading — invariant OW-3). The workspace's visibility set, authority set, or any other internal component.

**Trail event.** A `workspace_ownership_transferred` trail entry is written with the old owner, new owner, and workspace id.

### 5.4 Domain Queries

```rust
impl WorkspaceTree {
    /// All workspaces owned by a user
    pub fn domain(&self, owner: &UserId) -> &[WorkspaceId] {
        self.owner_index.get(owner)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// The owner of a workspace
    pub fn owner_of(&self, workspace_id: &WorkspaceId) -> Option<&UserId> {
        self.nodes.get(workspace_id).map(|n| &n.owner)
    }
}

impl EscalationRouter {
    /// Where should escalations for this workspace go?
    pub fn route(&self, workspace_id: &WorkspaceId) -> Option<&UserId> {
        self.routing.get(workspace_id)
    }
}
```

### 5.5 Invariant Enforcement

| Invariant | How enforced |
|-----------|-------------|
| OW-1: Every workspace has owner | `TreeNode.owner` is required (not `Option`) |
| OW-2: Owner is `user_id` | Type system — `owner: UserId`, not `OriginatorValue` |
| OW-3: Transfer is per-workspace | `transfer_ownership` modifies one node; no child iteration |
| OW-4: Transfer doesn't change originator | `transfer_ownership` does not touch `node.originator` |
| OW-5: Ownership boundaries delimit cascade | `failure_cascade` (§2.3) partitions by owner |
| OW-6: Escalation routing follows ownership | `EscalationRouter` updated on creation and transfer |

---

## 6. Causation

Causation tracks the origin of work — "who caused this workspace to exist?" Every workspace has an immutable `originator` field (`user_id | "system"`) set at creation. The causal forest partitions the structural tree by originator, producing a system sub-forest and one sub-forest per human user who injected work.

### 6.1 Originator Resolution

The originator is set at creation and never changed. The resolution rules:

```rust
pub fn resolve_originator(
    parent: &TreeNode,
    is_injection: bool,
    injector: Option<&UserId>,
) -> OriginatorValue {
    if is_injection {
        // Human injection: originator is the injector (creates a causal boundary)
        OriginatorValue::User(
            injector.expect("injection must have injector").clone()
        )
    } else {
        // Delegation: inherit parent's originator
        parent.originator.clone()
    }
}
```

**Three cases:**
1. **Root creation.** Always `System`. The root workspace is the system's bootstrapping point.
2. **System action (delegation).** Inherits parent's originator. When the coordinator decomposes a task and dispatches subtasks, the subtasks inherit the originator chain from their parent.
3. **Human injection.** The injector's `user_id` becomes the originator. This creates a causal boundary — a point where the causal chain transitions from system to user (or from one user to another, though this is rare).

### 6.2 Causal Index and Queries

The `originator_index` in `WorkspaceTree` (§2.1) provides the primary causal query:

```rust
impl WorkspaceTree {
    /// All workspaces caused by this originator
    pub fn causal_domain(&self, originator: &OriginatorValue) -> &[WorkspaceId] {
        self.originator_index.get(originator)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Causal subtree: workspaces with this originator that are
    /// descendants of the given root
    pub fn causal_subtree(
        &self,
        originator: &OriginatorValue,
        subtree_root: &WorkspaceId,
    ) -> Vec<WorkspaceId> {
        let descendants: HashSet<_> =
            self.descendants(subtree_root).into_iter().collect();
        self.causal_domain(originator)
            .iter()
            .filter(|id| descendants.contains(id))
            .cloned()
            .collect()
    }

    /// Causal impact: if user X's state changes, which active
    /// workspaces are in their causal domain?
    pub fn causal_impact(&self, user_id: &UserId) -> Vec<WorkspaceId> {
        let originator = OriginatorValue::User(user_id.clone());
        self.causal_domain(&originator)
            .iter()
            .filter(|id| {
                self.nodes.get(id)
                    .map_or(false, |n| !n.status.is_terminal())
            })
            .cloned()
            .collect()
    }
}
```

**Causal impact query.** When a human's state changes (e.g., permissions revoked, user deactivated), the coordinator needs to identify all active workspaces that exist because of that human. The `causal_impact` query returns non-terminal workspaces with the user's originator. The coordinator then decides what to do — suspend, abort, or continue depending on the nature of the state change. This is a coordinator policy decision, not a topology operation.

### 6.3 Causal Boundaries

A causal boundary is a point in the tree where the originator changes — always from `System` to `User(id)` (causation spec, invariant CA-6: boundaries are unidirectional). Boundaries are not stored explicitly — they are detectable by comparing a node's originator to its parent's originator.

```rust
pub fn is_causal_boundary(&self, workspace_id: &WorkspaceId) -> bool {
    let node = match self.nodes.get(workspace_id) {
        Some(n) => n,
        None => return false,
    };
    let parent = match &node.parent {
        Some(p) => match self.nodes.get(p) {
            Some(n) => n,
            None => return false,
        },
        None => return false, // root has no parent
    };
    node.originator != parent.originator
}
```

Boundary detection is O(1) — two lookups and a comparison. It is used for diagnostics and trail queries ("show me all injection points"), not for hot-path protocol operations.

### 6.4 Invariant Enforcement

| Invariant | How enforced |
|-----------|-------------|
| CA-1: Required | `TreeNode.originator` is required (not `Option`) |
| CA-2: Typed | `OriginatorValue` enum: `System` or `User(UserId)` |
| CA-3: Immutable | `resolve_originator` called at creation; no setter on `TreeNode.originator` |
| CA-4: Unconditional inheritance | `resolve_originator` copies parent's originator unless injection |
| CA-5: Downward closure | Inheritance ensures subtrees share originator unless an injection boundary intervenes |
| CA-6: Unidirectional boundaries | `System → User` only; `resolve_originator` never produces `System` from a `User` parent |
| CA-7: Root is system | Root node hardcoded with `OriginatorValue::System` |

---

## 7. Communication Topology

The communication topology is the port rights graph — a directed multigraph of active send, receive, and send-once rights between workspaces. This graph is independent of the workspace tree. It determines who can send envelopes to whom and is the most mutable of the six topologies.

### 7.1 Data Representation

```rust
pub struct PortRightsGraph {
    /// All active rights, indexed by holder
    by_holder: HashMap<WorkspaceId, Vec<PortRight>>,
    /// Reverse index: target → holders who can send to it
    by_target: HashMap<WorkspaceId, Vec<WorkspaceId>>,
    /// Right lookup by id (for transfer and revocation)
    by_id: HashMap<PortRightId, PortRight>,
}

pub struct PortRight {
    pub id: PortRightId,
    pub holder: WorkspaceId,
    pub target: WorkspaceId,
    pub kind: PortRightKind,
    pub status: PortRightStatus,
}

pub enum PortRightKind {
    Send,
    SendOnce,
    Receive,
}

pub enum PortRightStatus {
    Active,
    Consumed,   // send-once, used
    Revoked,    // coordinator revoked
    Expired,    // holder or target reached terminal state
}
```

**Three indices.** `by_holder` for "what can this workspace send to?" queries (used on every envelope send). `by_target` for "who can send to this workspace?" queries (used for diagnostics). `by_id` for direct right lookup (used for transfer and revocation).

### 7.2 Initial Rights at Creation

When a workspace is created, the coordinator creates initial port rights based on the permission matrix (runtime spec, §5):

```rust
impl PortRightsGraph {
    pub fn initialize_for_workspace(
        &mut self,
        workspace_id: &WorkspaceId,
        role: &RoleName,
        parent: &WorkspaceId,
    ) {
        // Coordinator → workspace: send right (for directives, feedback)
        self.create_right(parent.clone(), workspace_id.clone(), PortRightKind::Send);

        // Workspace → coordinator: send right (for queries)
        self.create_right(workspace_id.clone(), parent.clone(), PortRightKind::Send);

        // Workspace: receive right (always granted)
        self.create_right(workspace_id.clone(), workspace_id.clone(), PortRightKind::Receive);
    }

    fn create_right(
        &mut self,
        holder: WorkspaceId,
        target: WorkspaceId,
        kind: PortRightKind,
    ) -> PortRightId {
        let id = PortRightId::generate();
        let right = PortRight {
            id: id.clone(),
            holder: holder.clone(),
            target: target.clone(),
            kind,
            status: PortRightStatus::Active,
        };

        self.by_holder.entry(holder.clone())
            .or_default()
            .push(right.clone());
        self.by_target.entry(target)
            .or_default()
            .push(holder);
        self.by_id.insert(id.clone(), right);

        id
    }
}
```

This creates a star topology with the coordinator at the center and bidirectional send rights to each worker. The initial topology mirrors the workspace tree's parent-child relationships.

### 7.3 Right Lifecycle

Rights pass through a defined lifecycle. Each transition produces a trail entry.

**Transfer.** A workspace may transfer a send right to another workspace via an envelope's `rights` field. The sender loses the right; the receiver gains it.

```rust
pub fn transfer(
    &mut self,
    right_id: &PortRightId,
    new_holder: &WorkspaceId,
) -> Result<(), PortRightError> {
    let right = self.by_id.get_mut(right_id)
        .ok_or(PortRightError::NotFound)?;

    if right.status != PortRightStatus::Active {
        return Err(PortRightError::NotActive);
    }
    if right.kind == PortRightKind::Receive {
        return Err(PortRightError::ReceiveNotTransferable);
    }

    let old_holder = right.holder.clone();
    right.holder = new_holder.clone();

    // Update holder index: remove from old, add to new
    if let Some(rights) = self.by_holder.get_mut(&old_holder) {
        rights.retain(|r| &r.id != right_id);
    }
    self.by_holder.entry(new_holder.clone())
        .or_default()
        .push(right.clone());

    // Update target index
    if let Some(holders) = self.by_target.get_mut(&right.target) {
        holders.retain(|h| h != &old_holder);
        holders.push(new_holder.clone());
    }

    Ok(())
}
```

**Consumption.** A `send_once` right is consumed (destroyed) after a single envelope delivery.

```rust
pub fn consume(&mut self, right_id: &PortRightId) -> Result<(), PortRightError> {
    let right = self.by_id.get_mut(right_id)
        .ok_or(PortRightError::NotFound)?;

    if right.kind != PortRightKind::SendOnce {
        return Err(PortRightError::NotSendOnce);
    }
    if right.status != PortRightStatus::Active {
        return Err(PortRightError::NotActive);
    }

    right.status = PortRightStatus::Consumed;
    Ok(())
}
```

**Revocation.** The coordinator may revoke any right. In-flight envelopes using the right are still delivered (channels spec, invariant CH-4: revocation is immediate for *future* sends, not retroactive).

```rust
pub fn revoke(&mut self, right_id: &PortRightId) -> Result<(), PortRightError> {
    let right = self.by_id.get_mut(right_id)
        .ok_or(PortRightError::NotFound)?;

    if right.status != PortRightStatus::Active {
        return Err(PortRightError::NotActive);
    }

    right.status = PortRightStatus::Revoked;
    Ok(())
}
```

**Expiration.** When a workspace reaches a terminal state, all rights held by or targeting that workspace expire. The coordinator iterates the workspace's rights and marks them `Expired`.

```rust
pub fn expire_workspace(&mut self, workspace_id: &WorkspaceId) {
    // Expire rights held by this workspace
    if let Some(rights) = self.by_holder.get_mut(workspace_id) {
        for right in rights.iter_mut() {
            if right.status == PortRightStatus::Active {
                right.status = PortRightStatus::Expired;
            }
        }
    }

    // Expire rights targeting this workspace
    // (holders can no longer send to a terminal workspace)
    for (_, rights) in self.by_holder.iter_mut() {
        for right in rights.iter_mut() {
            if &right.target == workspace_id
                && right.status == PortRightStatus::Active
            {
                right.status = PortRightStatus::Expired;
            }
        }
    }
}
```

### 7.4 Envelope Send Validation

Before delivering an envelope, the delivery pipeline checks the port rights graph:

```rust
pub fn validate_send(
    &self,
    sender: &WorkspaceId,
    target: &WorkspaceId,
) -> Result<&PortRight, PortRightError> {
    let rights = self.by_holder.get(sender)
        .ok_or(PortRightError::NoRights)?;

    rights.iter()
        .find(|r| {
            &r.target == target
                && r.status == PortRightStatus::Active
                && matches!(r.kind, PortRightKind::Send | PortRightKind::SendOnce)
        })
        .ok_or(PortRightError::NoRightToTarget(target.clone()))
}
```

This is called by the permission engine (runtime spec, §5) after the permission matrix check. The matrix says the role *could* send; the port rights graph says the workspace *currently can* send. Both must pass.

### 7.5 Channel Ordering

A channel is an ordered pair (sender, receiver). Envelopes within a channel are delivered in creation order (channels spec, invariant CH-2). Channel ordering is not maintained by the port rights graph — it is maintained by the delivery pipeline (runtime spec, §9). The port rights graph only answers "is this send permitted?" The delivery pipeline ensures FIFO ordering per channel by processing envelopes sequentially per sender-receiver pair.

### 7.6 Invariant Enforcement

| Invariant | How enforced |
|-----------|-------------|
| CH-1: No right, no delivery | `validate_send` called before every delivery |
| CH-2: Channel ordering | Delivery pipeline processes per-channel FIFO (runtime spec, §9) |
| CH-3: Send-once consumed | `consume` transitions to `Consumed` after delivery |
| CH-4: Revocation immediate | `revoke` transitions to `Revoked`; future `validate_send` rejects |
| CH-5: Receive non-transferable | `transfer` rejects `PortRightKind::Receive` |
| CH-6: All lifecycle events recorded | Each operation produces a trail entry (`port_right_*`) |

---

## 8. Compound Operations

The six topologies are independent data structures, but protocol operations often span several of them simultaneously. A workspace creation touches the tree, task graph, visibility graph, ownership index, causation index, and port rights graph. A failure cascade touches the tree, ownership index, port rights graph, and escalation router. This section defines the compound operations and their sequencing.

### 8.1 Workspace Creation

Creating a workspace is the most topology-intensive operation. It updates all six structures in a single coordinator message handler.

```rust
impl CoordinatorActor {
    fn create_workspace(
        &mut self,
        parent_id: &WorkspaceId,
        task_id: &TaskId,
        role: &RoleName,
        explicit_owner: Option<&UserId>,
        is_injection: bool,
        injector: Option<&UserId>,
        initial_visibility: Vec<WorkspaceId>,
    ) -> Result<WorkspaceId, CreateError> {
        let parent = self.tree.node(parent_id)?;
        let workspace_id = WorkspaceId::generate();

        let owner = resolve_owner(parent, explicit_owner);
        let originator = resolve_originator(parent, is_injection, injector);

        // 1. Tree: insert node
        self.tree.insert(TreeNode {
            id: workspace_id.clone(),
            parent: Some(parent_id.clone()),
            owner: owner.clone(),
            originator: originator.clone(),
            status: WorkspaceStatus::Idle,
            created_at: self.clock.now(),
        })?;

        // 2. Task graph: bind task to workspace
        self.task_graph.bind_task_to_workspace(task_id, &workspace_id)?;

        // 3. Visibility: initialize default + explicit grants
        self.visibility.initialize_for_workspace(
            &workspace_id, role, parent_id, &self.tree,
        );
        for target in &initial_visibility {
            let grantor_vis = self.visibility.visible_to(parent_id);
            let _ = self.visibility.grant(&workspace_id, target, grantor_vis);
        }

        // 4. Ownership: escalation router
        self.escalation_router.update_owner(&workspace_id, &owner);

        // 5. Causation: handled by tree.insert (originator_index updated)

        // 6. Port rights: initial rights from permission matrix
        self.port_rights.initialize_for_workspace(
            &workspace_id, role, parent_id,
        );

        // 7. Coordinator visibility: coordinator sees the new workspace
        let coord_vis = self.visibility.visible_to(&self.tree.root).clone();
        let _ = self.visibility.grant(&self.tree.root, &workspace_id, &coord_vis);

        Ok(workspace_id)
    }
}
```

**Ordering.** Tree insertion first — all other operations reference the node. Task binding second — associates the workspace with its purpose. Visibility and port rights last — they depend on the node existing in the tree. The ordering within the coordinator's single-threaded message handler is sequential — no concurrency concerns.

**Trail events.** A single `workspace_created` trail entry captures all topology state: parent, owner, originator, role, initial visibility set, initial authority set. Recovery reconstructs all six topology structures from this one event type.

### 8.2 Workspace Termination

When a workspace reaches a terminal state (`closed` or `failed`), several topologies need cleanup.

```rust
fn terminate_workspace(
    &mut self,
    workspace_id: &WorkspaceId,
    new_status: WorkspaceStatus,
) {
    // 1. Tree: update status (node remains, marked terminal)
    if let Some(node) = self.tree.nodes.get_mut(workspace_id) {
        node.status = new_status;
    }

    // 2. Task graph: update task status, unbind
    if let Some(task_id) = self.task_graph.workspace_task.get(workspace_id).cloned() {
        if new_status == WorkspaceStatus::Closed {
            let newly_ready = self.task_graph.mark_completed(&task_id);
            self.handle_newly_ready_tasks(newly_ready);
        } else {
            let blocked = self.task_graph.mark_failed(&task_id);
            self.handle_blocked_dependents(blocked);
        }
    }

    // 3. Port rights: expire all rights involving this workspace
    self.port_rights.expire_workspace(workspace_id);

    // 4. Visibility: no cleanup needed — terminal workspaces remain
    //    visible for trail queries and checkpoint reads

    // 5. Failure cascade (if failed, not closed)
    if new_status == WorkspaceStatus::Failed {
        let cascade = self.tree.failure_cascade(
            workspace_id,
            &self.tree.nodes[workspace_id].owner,
        );
        self.execute_cascade(cascade);
    }
}
```

**Cascade execution.** The `execute_cascade` method processes the `CascadeResult` (§2.3):

```rust
fn execute_cascade(&mut self, cascade: CascadeResult) {
    // Fail same-owner children
    for child_id in &cascade.failed {
        // Send abort to workspace actor (high-priority channel)
        let _ = self.workspace_channels[child_id]
            .coordinator_tx
            .send(CoordinatorCommand::Abort {
                reason: "parent_failed".into(),
            });
        // Recursive: terminate_workspace will cascade further
    }

    // Reparent cross-owner children
    for child_id in &cascade.reparented {
        self.tree.reparent_to_root(child_id).ok();
        self.escalation_router.update_owner(
            child_id,
            self.tree.nodes[child_id].owner.clone().into(),
        );
        // Trail: workspace_reparented entry
    }
}
```

The cascade is recursive through `terminate_workspace` — when a cascaded child reaches `failed`, its own children are cascaded. The recursion terminates because the tree is finite and acyclic, and terminal nodes are skipped.

### 8.3 Ownership Transfer

Transfer updates the ownership index and escalation router. It does not affect the tree structure, visibility, causation, or port rights.

```rust
fn transfer_ownership(
    &mut self,
    workspace_id: &WorkspaceId,
    new_owner: &UserId,
) -> Result<(), TransferError> {
    // 1. Tree: update owner field and owner_index
    let old_owner = self.tree.transfer_ownership(workspace_id, new_owner)?;

    // 2. Escalation router: update routing
    self.escalation_router.update_owner(workspace_id, new_owner);

    // 3. Trail: workspace_ownership_transferred
    //    (old_owner, new_owner, workspace_id)

    Ok(())
}
```

**What does NOT change.** The workspace's position in the tree (parent, children). Its originator. Its visibility set. Its authority set. Its port rights. Its task binding. Transfer is a metadata change, not a structural change.

### 8.4 Envelope Delivery

Envelope delivery touches the port rights graph (validation) and potentially the port rights graph again (transfer, consumption). It does not modify the tree, task graph, visibility, ownership, or causation.

```rust
fn validate_and_deliver_envelope(
    &mut self,
    sender: &WorkspaceId,
    target: &WorkspaceId,
    envelope: &Envelope,
) -> Result<(), DeliveryError> {
    // 1. Permission matrix check (runtime spec, §5)
    self.permission_engine.check_send(sender, target, &envelope.r#type)?;

    // 2. Port rights check
    let right = self.port_rights.validate_send(sender, target)?;
    let right_id = right.id.clone();
    let right_kind = right.kind.clone();

    // 3. Deliver (runtime spec, §9 — delivery pipeline)
    // ... trail write, inbox append, acknowledgment ...

    // 4. If send_once, consume the right
    if right_kind == PortRightKind::SendOnce {
        self.port_rights.consume(&right_id)?;
        // Trail: port_right_consumed
    }

    // 5. If envelope carries rights to transfer
    if let Some(transfer_rights) = &envelope.rights {
        for right_id in transfer_rights {
            self.port_rights.transfer(right_id, target)?;
            // Trail: port_right_transferred
        }
    }

    Ok(())
}
```

### 8.5 Visibility Grant

A dynamic visibility grant touches only the visibility graph. The coordinator validates the grant against its own visibility, then updates the viewer's set and notifies the workspace actor.

```rust
fn grant_visibility(
    &mut self,
    viewer: &WorkspaceId,
    target: &WorkspaceId,
) -> Result<(), VisibilityError> {
    // The coordinator is always the grantor
    let grantor_vis = self.visibility.visible_to(&self.tree.root).clone();

    let newly_granted = self.visibility.grant(viewer, target, &grantor_vis)?;

    if newly_granted {
        // Notify the workspace actor so it updates its local copy
        if let Some(channels) = self.workspace_channels.get(viewer) {
            let _ = channels.coordinator_tx.send(
                CoordinatorCommand::VisibilityGrant {
                    resource_ids: vec![target.clone()],
                }
            );
        }
        // Trail: visibility_granted
    }

    Ok(())
}
```

### 8.6 Operation-Topology Matrix

Which topologies each operation reads (R) or writes (W):

| Operation | Tree | Task Graph | Visibility | Ownership | Causation | Port Rights |
|-----------|------|-----------|------------|-----------|-----------|-------------|
| Workspace creation | W | W | W | W | W | W |
| Workspace termination | W | W | — | — | — | W |
| Failure cascade | R/W | R | — | R | — | W |
| Reparenting | W | — | — | — | — | — |
| Task completion | — | W | — | — | — | — |
| Envelope delivery | — | — | — | — | — | R/W |
| Visibility grant | — | — | W | — | — | — |
| Ownership transfer | W | — | — | W | — | — |
| Escalation routing | R | — | — | R | — | — |
| Causal impact query | R | — | — | — | R | — |

Workspace creation is the only operation that writes all six topologies. Most operations touch 1–2 topologies. This sparsity is why the six structures are independent — coupling them would add overhead to operations that don't need the coupling.

---

## 9. Consistency and Recovery

All topology state is derived from the trail. The topology structures in the coordinator's memory are acceleration indices — if they were lost, they could be rebuilt by replaying trail events. This section defines the recovery procedure for topology state and the consistency guarantees.

### 9.1 Trail-Driven Consistency

Every topology mutation produces a trail entry *before* the mutation takes effect (runtime spec, §6 — write-ahead rule). The trail entry is the commit point. If the runtime crashes between the trail write and the index update, recovery replays the trail entry and applies the index update. The index is never ahead of the trail — it may be behind (briefly, during the gap between trail write and index update within the same message handler), but this gap is invisible because the coordinator processes messages sequentially.

**No separate topology persistence.** The topology structures are not written to disk independently. They exist only in the coordinator actor's memory. On restart, they are rebuilt from the trail. This is simpler than maintaining a separate persistence layer for topology state and eliminates the risk of topology-trail divergence.

### 9.2 Recovery Procedure

Recovery reconstructs all six topologies by replaying trail events in global sequence order (runtime spec, §13). The relevant event types:

| Trail event | Topologies updated |
|------------|-------------------|
| `workspace_created` | Tree (insert), visibility (initialize), ownership (index + router), causation (originator index), port rights (initial rights) |
| `workspace_state_changed` | Tree (status update); if terminal: port rights (expire), task graph (complete/fail) |
| `workspace_reparented` | Tree (reparent) |
| `workspace_ownership_transferred` | Tree (owner + owner_index), escalation router |
| `task_created` | Task graph (insert) |
| `task_assigned` | Task graph (bind to workspace) |
| `task_status_changed` | Task graph (status update, readiness counters) |
| `port_right_created` | Port rights (create) |
| `port_right_transferred` | Port rights (transfer) |
| `port_right_consumed` | Port rights (consume) |
| `port_right_revoked` | Port rights (revoke) |
| `visibility_granted` | Visibility (grant) |

```rust
fn recover_topology(
    &mut self,
    entries: impl Iterator<Item = TrailEntry>,
) {
    for entry in entries {
        match entry.event_type.as_str() {
            "workspace_created" => {
                let body: WorkspaceCreatedBody = deserialize(&entry.body);
                self.tree.insert(TreeNode {
                    id: body.workspace_id,
                    parent: Some(body.parent),
                    owner: body.owner,
                    originator: body.originator,
                    status: WorkspaceStatus::Idle,
                    created_at: entry.timestamp,
                });
                self.visibility.initialize_for_workspace(
                    &body.workspace_id, &body.role, &body.parent, &self.tree,
                );
                self.escalation_router.update_owner(
                    &body.workspace_id, &body.owner,
                );
                self.port_rights.initialize_for_workspace(
                    &body.workspace_id, &body.role, &body.parent,
                );
            }
            "workspace_state_changed" => {
                let body: StateChangedBody = deserialize(&entry.body);
                if let Some(node) = self.tree.nodes.get_mut(&body.workspace_id) {
                    node.status = body.new_state;
                }
                if body.new_state.is_terminal() {
                    self.port_rights.expire_workspace(&body.workspace_id);
                }
            }
            "workspace_reparented" => {
                let body: ReparentedBody = deserialize(&entry.body);
                self.tree.reparent_to_root(&body.workspace_id).ok();
            }
            "workspace_ownership_transferred" => {
                let body: TransferBody = deserialize(&entry.body);
                self.tree.transfer_ownership(
                    &body.workspace_id, &body.new_owner,
                ).ok();
                self.escalation_router.update_owner(
                    &body.workspace_id, &body.new_owner,
                );
            }
            "task_created" => {
                let body: TaskCreatedBody = deserialize(&entry.body);
                self.task_graph.insert(
                    body.task_id, body.name, body.description,
                    body.parent_task, body.depends_on,
                ).ok();
            }
            "task_assigned" => {
                let body: TaskAssignedBody = deserialize(&entry.body);
                self.task_graph.bind_task_to_workspace(
                    &body.task_id, &body.workspace_id,
                ).ok();
            }
            "task_status_changed" => {
                let body: TaskStatusBody = deserialize(&entry.body);
                match body.new_status {
                    TaskStatus::Completed | TaskStatus::Integrated => {
                        self.task_graph.mark_completed(&body.task_id);
                    }
                    TaskStatus::Failed | TaskStatus::Cancelled => {
                        self.task_graph.mark_failed(&body.task_id);
                    }
                    _ => {
                        if let Some(task) = self.task_graph.tasks.get_mut(&body.task_id) {
                            task.status = body.new_status;
                        }
                    }
                }
            }
            "port_right_created" => {
                let body: PortRightCreatedBody = deserialize(&entry.body);
                self.port_rights.create_right(body.holder, body.target, body.kind);
            }
            "port_right_transferred" => {
                let body: PortRightTransferBody = deserialize(&entry.body);
                self.port_rights.transfer(&body.right_id, &body.new_holder).ok();
            }
            "port_right_consumed" => {
                let body: PortRightConsumedBody = deserialize(&entry.body);
                self.port_rights.consume(&body.right_id).ok();
            }
            "port_right_revoked" => {
                let body: PortRightRevokedBody = deserialize(&entry.body);
                self.port_rights.revoke(&body.right_id).ok();
            }
            "visibility_granted" => {
                let body: VisibilityGrantBody = deserialize(&entry.body);
                let grantor_vis = self.visibility.visible_to(&self.tree.root).clone();
                self.visibility.grant(
                    &body.viewer, &body.target, &grantor_vis,
                ).ok();
            }
            _ => {} // events not relevant to topology
        }
    }
}
```

**Idempotency.** Replaying the same event twice produces the same result. Insertions into sets are deduplicated. Counter decrements are guarded by `saturating_sub`. Status updates are overwrites. This ensures recovery is safe to run multiple times (runtime spec, §13 — idempotent recovery invariant).

### 9.3 Snapshot Acceleration

System snapshots (storage spec, §7) include the coordinator's topology state. On recovery, the snapshot provides a starting point — the recovery engine loads the snapshot and replays only trail events after the snapshot's sequence number.

The snapshot format serializes all six topology structures:

| Structure | Snapshot content |
|-----------|-----------------|
| Workspace tree | `nodes` map (all `TreeNode` entries), `root` id |
| Task graph | `tasks` map, `forward`/`reverse` adjacency lists, `subtasks`, bindings |
| Visibility graph | `can_see` map (forward sets only — reverse rebuilt from forward) |
| Ownership | Included in tree nodes (`owner` field) + `escalation_router.routing` |
| Causation | Included in tree nodes (`originator` field) — indices rebuilt from nodes |
| Port rights | `by_id` map (all `PortRight` entries) — holder/target indices rebuilt |

The reverse indices (`seen_by`, `by_holder`, `by_target`, `children_index`, `originator_index`, `owner_index`) are not serialized — they are rebuilt from the forward data after snapshot load. This halves the snapshot size and eliminates the risk of index-data inconsistency in the snapshot.

---

## 10. References

### Protocol Topology Specs

| Spec | Referenced in | Topic |
|------|--------------|-------|
| `topology/tree.md` | §2 | Workspace tree — 10 invariants (T-1 through T-10), traversals, cascade, reparenting |
| `topology/graph.md` | §3 | Task graph — DAG + decomposition tree, 7 invariants (G-1 through G-7), readiness |
| `topology/visibility.md` | §4 | Visibility graph — additive-only, 6 invariants (VI-1 through VI-6), authority |
| `topology/ownership.md` | §5 | Ownership domains — transfer, cascade boundaries, 6 invariants (OW-1 through OW-6) |
| `topology/causation.md` | §6 | Causal forest — originator, boundaries, 7 invariants (CA-1 through CA-7) |
| `topology/channels.md` | §7 | Port rights graph — lifecycle, channels, 6 invariants (CH-1 through CH-6) |

### Protocol Sections

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §6.3 (workspace tree) | §2 | Tree structure, state transitions, cascade rules |
| §6.8 (visibility and authority) | §4 | Dynamic grants, authority frozen at creation |
| §4.6 (task) | §3 | Task lifecycle, dependency edges, decomposition |

### Runtime Spec (`impl/runtime.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §3 (process model) | §8 | Coordinator actor owns all topology state |
| §5 (permission engine) | §4.4, §7.4 | Permission matrix + port rights for envelope validation |
| §6 (trail write-ahead) | §9.1 | Write-ahead rule — trail entry before mutation |
| §8 (workspace isolation) | §4.5 | Authority set as workspace field, visibility set via `Arc` |
| §9 (envelope delivery) | §7.5, §8.4 | Delivery pipeline, channel FIFO ordering |
| §12 (resource enforcement) | §8.2 | Timeout/budget cascade on workspace failure |
| §13 (recovery engine) | §9.2 | Trail replay, state reconstruction, idempotency |

### Storage Spec (`impl/storage.md`)

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §7 (snapshots) | §9.3 | System snapshot includes coordinator topology state |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../protocol/PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../protocol/TAXONOMY.md)*
