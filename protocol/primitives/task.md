# WACP: Task

## Metadata

```yaml
title: "WACP: Task"
id: wacp-spec-task
type: constituent-spec
tier: abstract
category: primitives
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4.6 (task primitive, lifecycle, DAG, task-workspace binding)
depends_on:
  - wacp-spec-workspace
  - wacp-spec-signal
  - wacp-spec-checkpoint
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, task, decomposition, dag, allocation, planning, work-unit]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Task Schema](#2-task-schema)
3. [Task Graph (DAG)](#3-task-graph-dag)
4. [Task Lifecycle](#4-task-lifecycle)
5. [Task-Workspace Binding](#5-task-workspace-binding)
6. [Resource Estimation and Allocation](#6-resource-estimation-and-allocation)
7. [Decomposition](#7-decomposition)
8. [Trail Events](#8-trail-events)
9. [Conformance Requirements](#9-conformance-requirements)
10. [Implementation Notes](#10-implementation-notes)

---

## 1. Purpose

Tasks are the protocol's work primitive. Where workspaces provide execution containers and envelopes carry communication, tasks define what needs to be done. They are structured units of work with explicit dependencies, resource estimates, and lifecycle state — giving the protocol visibility into the work itself, not just the machinery that executes it.

Without tasks, the protocol is blind to work structure. The coordinator receives a goal, decomposes it internally, and dispatches directives as opaque payloads. The protocol cannot answer: how many units of work exist? Which depend on which? How far along is the overall goal? Is the current allocation optimal? These questions are invisible because work decomposition lives in the coordinator's head, not in the protocol.

Tasks make decomposition a first-class protocol concept. A goal becomes a task graph — a directed acyclic graph where nodes are units of work and edges are dependencies. The coordinator constructs the graph; the protocol maintains it. From the graph, the protocol knows which tasks are ready (all dependencies met), which are blocked (waiting on predecessors), and which are complete. Workspace allocation follows naturally: spawn a workspace for each ready task, not blindly.

The task-workspace relationship is one-to-many across time. A task is assigned to a workspace for execution. If the workspace fails, the coordinator may retry — creating a new workspace for the same task. The protocol links all attempts: workspace A was the first attempt at task T, workspace B was the second. The trail records this history; the coordinator uses it to make smarter decisions on retry.

Tasks serve four protocol-level purposes:
- **Decomposition** — structured breakdown of a goal into executable units with explicit dependencies.
- **Allocation** — dependency-aware distribution of work across workspaces, enabling parallelism where the graph permits and sequencing where it requires.
- **Progress** — measurable advancement toward the goal, expressed as task completion rather than workspace count.
- **Resource planning** — per-task estimates that enable the coordinator to distribute budgets proportionally, reserving more for expensive tasks and less for cheap ones.

## 2. Task Schema

Every task carries the same structure. The schema captures what the work is, what it depends on, what it costs, and where it stands.

```yaml
task:
  id: string                      # runtime-assigned, globally unique (Identity spec, rule 1)
  name: string                    # human-readable label for the unit of work
  description: string             # what needs to be done, in sufficient detail for directive construction
  depends_on: [task_id]           # tasks that must complete before this one can start (DAG edges)
  parent_task: task_id            # the task this was decomposed from (null for root tasks)
  priority: enum
    - normal
    - elevated
    - urgent
  resource_estimate:              # optional — coordinator's prediction of cost
    tokens: integer               # expected token consumption
    wall_time: duration           # expected elapsed time
    cost: number                  # expected monetary cost (implementation-defined unit)
  status: enum
    - draft                       # defined by coordinator, awaiting human approval
    - pending                     # approved, waiting for dependencies or dispatch
    - assigned                    # bound to a workspace, workspace in idle
    - in_progress                 # workspace is active
    - completed                   # workspace completed, checkpoint available
    - failed                      # workspace failed (may be retried)
    - integrated                  # output merged into parent (terminal)
    - cancelled                   # coordinator withdrew the task (terminal)
  workspace_ref: workspace_id    # current workspace executing this task (null if pending or terminal)
  workspace_history: [workspace_id]  # all workspaces that have attempted this task, in order
  checkpoint_ref: checkpoint_id  # deliverable — the final checkpoint from the completing workspace
  graph_ref: task_graph_id       # the task graph this task belongs to
  timestamp: datetime            # runtime-assigned at creation (Clock spec, invariant 1)
```

**Field semantics:**

- **`id`** — globally unique, runtime-assigned (Identity spec, rule 1). Never reused (rule 6).
- **`name`** — a concise label. Not unique — two tasks may share a name (e.g., "write tests" appears twice for different modules). The `id` distinguishes them.
- **`description`** — the work specification. The coordinator uses this to construct the directive envelope when assigning the task to a workspace. The description is the bridge between the task (what) and the directive (how).
- **`depends_on`** — a list of task IDs that must reach `completed` or `integrated` status before this task can be assigned. An empty list means the task has no dependencies and is immediately ready. This field defines the DAG edges (§3).
- **`parent_task`** — links this task to the task it was decomposed from (§7). Null for root-level tasks. Forms the decomposition tree, which is orthogonal to the dependency DAG.
- **`priority`** — inherited by the workspace when the task is assigned. `urgent` tasks are assigned before `normal` tasks when multiple tasks are ready simultaneously.
- **`resource_estimate`** — the coordinator's prediction of how much this task will cost. Optional — the coordinator may not know in advance. When present, enables DAG-aware budget distribution (§6). The three dimensions match the workspace resource meter (Workspace spec, §6) and checkpoint resource tracking (Checkpoint spec, §6).
- **`status`** — tracks the task's position in its lifecycle (§4). Tasks begin as `draft` and require human approval through the human highway before becoming `pending`. Subsequent transitions are managed by the runtime based on workspace state transitions. The coordinator sets `cancelled` directly.
- **`workspace_ref`** — the workspace currently executing this task. Null when the task is `pending` (not yet assigned) or terminal (`integrated`, `cancelled`). Updated when a new workspace is assigned (including retries).
- **`workspace_history`** — ordered list of all workspaces that have attempted this task. The first entry is the first attempt; subsequent entries are retries. This field makes retry history a first-class protocol record, not an inference from the trail.
- **`checkpoint_ref`** — set when the task reaches `completed` status. Points to the final checkpoint from the completing workspace. This is the task's deliverable — what the coordinator reads at integration time.
- **`graph_ref`** — the task graph this task belongs to (§3). Every task belongs to exactly one graph.
- **`timestamp`** — assigned by the runtime at creation. Monotonic within the graph.

## 3. Task Graph (DAG)

Tasks are organized into a directed acyclic graph through their `depends_on` fields. The graph is the coordinator's work plan — a structured representation of what needs doing and in what order.

```yaml
task_graph:
  id: string                      # runtime-assigned, globally unique
  root_task: task_id              # the top-level goal this graph decomposes
  tasks: [task_id]                # all tasks in the graph
  timestamp: datetime             # runtime-assigned at creation
```

**Structure.** Each task's `depends_on` list defines edges: if task B depends on task A, there is an edge from A to B. The graph is acyclic — circular dependencies are rejected at validation. A task with an empty `depends_on` list is a leaf in the dependency sense — it has no predecessors and is immediately ready.

```
    A ─────► C ─────► E
    │                 ▲
    └──► B ──► D ─────┘
```

In this graph, A has no dependencies (ready immediately). B depends on A. C depends on A. D depends on B. E depends on C and D. The coordinator can execute A first, then B and C in parallel, then D (after B), then E (after both C and D).

**Three invariants:**

**Invariant 1: The graph is acyclic.** The runtime validates the graph at construction and rejects cycles. A cycle means two tasks each waiting for the other — deadlock by definition. Cycle detection runs when tasks are added or when `depends_on` fields are set. A task cannot depend on itself.

**Invariant 2: Every task belongs to exactly one graph.** A task's `graph_ref` is set at creation and immutable. Tasks cannot be shared across graphs. If the coordinator needs the same work in two contexts, it creates two tasks — one per graph.

**Invariant 3: Dependencies reference tasks within the same graph.** A task's `depends_on` list may only contain task IDs from the same graph. Cross-graph dependencies do not exist at the protocol level. If the coordinator needs to sequence work across graphs, it manages that ordering itself — the protocol does not provide inter-graph dependency tracking.

**Readiness.** A task is ready when all of its dependencies have reached `completed` or `integrated` status. The runtime computes readiness from the graph — no coordinator query is needed. When a task completes, the runtime re-evaluates its dependents and marks newly ready tasks. This is the protocol's scheduling primitive: the coordinator asks "what is ready?" and the runtime answers from the graph.

**Critical path.** The longest chain of dependent tasks through the graph determines the minimum time to complete all work — the critical path. The protocol does not compute the critical path, but the graph structure makes it computable. Implementations may surface critical path analysis to the coordinator for smarter resource allocation — assigning more budget and higher priority to tasks on the critical path.

**Graph mutability.** The graph grows but does not shrink. Tasks may be added to the graph after construction (§7, decomposition). Tasks may be cancelled (`status: cancelled`) but are not removed from the graph — they remain as cancelled nodes. Dependencies may not be modified after a task is created — the `depends_on` list is immutable. This ensures the graph structure is stable once constructed, preventing mid-execution rewiring that would invalidate scheduling decisions.

## 4. Task Lifecycle

A task moves through a lifecycle that begins with human oversight and then mirrors the workspace lifecycle. The first transition — `draft` to `pending` — requires human approval through the human highway. Subsequent transitions derive from workspace state: when the workspace transitions, the task transitions with it.

```
draft ──► pending ──► assigned ──► in_progress ──► completed ──► integrated
  │                       │              │              │
  │                       │              │              └──► (salvage integration)
  │                       │              │
  │                       │              └──► failed ──► pending (retry)
  │                       │                      │
  │                       │                      └──► cancelled
  │                       │
  │                       └──► cancelled
  │
  └──► cancelled
```

**Eight states:**

**`draft`** — The coordinator has defined the task and placed it in the graph, but no work may begin. The task is a proposal — the coordinator's plan for how to decompose and execute the goal. The human reviews the task through the human highway (PROTOCOL §8) and decides: approve, modify, or cancel. Approval transitions the task to `pending`. The `draft` → `pending` gate is subject to timeout — if the human does not respond within the configured window, the fallback behavior executes (auto-approve, escalate, or cancel, per workflow configuration).

**`pending`** — The task has been approved by a human and is waiting for execution. Either its dependencies are not yet met (blocked by the DAG) or the coordinator has not yet dispatched it (ready but waiting). A task also returns to `pending` when the coordinator retries after failure — the previous workspace is recorded in `workspace_history`, and the task awaits a new assignment.

**`assigned`** — The coordinator has created a workspace for this task and bound them together. The workspace is in `idle` state, waiting for its directive. The task's `workspace_ref` points to the new workspace. The directive envelope references the task ID, linking execution to the work plan.

**`in_progress`** — The workspace has emitted `started`. The agent is actively working. The task remains in this state through `blocked`, `migrating`, and `suspended` workspace states — these are workspace-level conditions that do not change the task's status. The task is still in progress; the workspace is temporarily unable to advance.

**`completed`** — The workspace has emitted `complete` and a final checkpoint is available. The task's `checkpoint_ref` is set to the final checkpoint. The task is done from the agent's perspective — but not yet integrated. The coordinator reads the checkpoint and decides: integrate, revise, or reject.

**`failed`** — The workspace has reached `failed` state. The task records the failure but is not terminal — the coordinator may retry by creating a new workspace for the same task (§5). The failed workspace is appended to `workspace_history`. If the coordinator retries, the task returns to `pending`. If the coordinator does not retry, the task remains `failed` or is explicitly cancelled.

**`integrated`** — Terminal state. The task's checkpoint has been merged into the parent workspace through integration (Integration spec, §2). The task is complete in every sense — work done, output merged, value delivered.

**`cancelled`** — Terminal state. The coordinator withdrew the task before it could complete. Cancellation is explicit — the coordinator sets `status: cancelled`. A cancelled task's workspace (if any) is aborted. Dependent tasks that relied solely on this task become unresolvable — the coordinator must cancel them too or restructure the graph.

**State derivation:**

| Event | Task transition |
|-------|-----------------|
| Human approves via highway | `draft` → `pending` |
| Approval timeout (auto-approve) | `draft` → `pending` |
| Approval timeout (cancel) | `draft` → `cancelled` |
| Workspace created and bound | `pending` → `assigned` |
| `started` signal | `assigned` → `in_progress` |
| `complete` signal | `in_progress` → `completed` |
| `failed` signal | `in_progress` → `failed` (or `assigned` → `failed`) |
| Integration completes | `completed` → `integrated` |
| Coordinator cancels | any non-terminal → `cancelled` |
| Coordinator retries | `failed` → `pending` |

**No task-specific signals.** Task state is derived from workspace state and human highway actions — there are no separate signal types for tasks. The existing eleven signals (Signal spec, §2) are sufficient. The runtime maps workspace transitions to task transitions automatically. This keeps the signal set closed and avoids introducing a parallel notification system.

**The human gate is the protocol's planning checkpoint.** Before any workspace is spawned, before any tokens are consumed, a human sees the plan. The `draft` state ensures that the coordinator's decomposition is reviewed — not rubber-stamped after the fact. This is where the human highway meets the task primitive: tasks propose, humans approve, workspaces execute.

## 5. Task-Workspace Binding

A task describes what needs to be done. A workspace provides where it gets done. Binding connects the two — the coordinator assigns a task to a workspace, and the protocol tracks the relationship through execution, completion, and potentially across retries.

**Binding is a three-step protocol operation:**

1. **Directive construction.** The coordinator constructs a directive envelope from the task's `description` field. The directive's payload carries the work specification; its metadata references the task ID (`task_ref: task_id`). This reference is the link — it lets the runtime map workspace events back to the task that caused them.

2. **Workspace creation.** The coordinator creates a workspace (Workspace spec, §4), passing the directive envelope as the `directive` parameter. The workspace enters `idle` state. Its internal model is populated: the directive is frozen, the context may include references to checkpoints from dependency tasks, and the budget may reflect the task's `resource_estimate`.

3. **Binding registration.** The runtime sets the task's `workspace_ref` to the new workspace ID, appends the workspace ID to `workspace_history`, and transitions the task from `pending` to `assigned`. From this point, workspace state transitions drive task state transitions (§4).

**Five rules:**

**Rule 1: One task, one workspace at a time.** A task's `workspace_ref` points to at most one workspace. There is no parallel execution of the same task — if the coordinator wants parallel attempts, it creates separate tasks. This prevents conflicting outputs for the same unit of work.

**Rule 2: One workspace, one task.** A workspace executes exactly one task. The workspace's directive is immutable (Workspace spec, §2) — once created for a task, it cannot be repurposed. If the workspace completes or fails, its task binding is permanent. The workspace-to-task mapping is injective: no two tasks share a workspace.

**Rule 3: Binding requires `pending` status.** A task can only be bound to a workspace when it is in `pending` status — approved by a human and ready for execution. The runtime rejects binding attempts for tasks in any other state. This enforces the lifecycle: `draft` tasks have not been approved, `assigned` or `in_progress` tasks already have a workspace, and terminal tasks are done.

**Rule 4: Dependency context flows through binding.** When binding a task to a workspace, the coordinator may include checkpoint references from completed dependency tasks in the workspace's `context` parameter. This is how the graph's information flows — task B depends on task A, so when B is bound to a workspace, the coordinator passes A's `checkpoint_ref` in the context. The protocol does not automate this — the coordinator decides what context to include. The graph provides the information; the coordinator curates it.

**Rule 5: Unbinding is implicit at terminal states.** When a workspace reaches a terminal state (`closed` or `failed`), the task-workspace binding ends. For `closed`: the task transitions to `completed` (workspace emitted `complete` and integration succeeded) — `workspace_ref` remains set for reference, but no further workspace activity occurs. For `failed`: the task transitions to `failed` — `workspace_ref` remains set to the failed workspace, and the coordinator may retry (creating a new binding).

**Retry binding.** When a task fails and the coordinator decides to retry, the sequence repeats: the coordinator constructs a new directive (possibly amended with feedback from the failure), creates a new workspace, and the runtime binds the task to it. The task's `workspace_ref` updates to the new workspace. The previous workspace remains in `workspace_history` — the protocol preserves the full attempt record. The new directive may reference the failed workspace's trail or checkpoints, giving the new agent visibility into what went wrong.

The protocol does not limit the number of retries. The coordinator decides when to stop retrying and cancel the task. Implementations may impose retry limits as a safety measure, but the protocol itself treats retry as a coordinator decision, not a protocol constraint.

**Binding and the internal model.** The binding connects task-level concepts to workspace-level components:

| Task field | Workspace component | Relationship |
|------------|-------------------|--------------|
| `description` | Directive | Task description → directive envelope payload |
| `resource_estimate` | Resource Meter (budget) | Estimate informs budget allocation |
| `priority` | Priority (creation parameter) | Inherited directly |
| `checkpoint_ref` | Checkpoint Register (final entry) | Set when workspace completes |
| `depends_on` (resolved) | Context | Dependency checkpoints passed as context |

## 6. Resource Estimation and Allocation

Tasks carry resource estimates — the coordinator's prediction of what each unit of work will cost. These estimates serve the protocol at two levels: individual workspace budgets (how much to give this workspace) and graph-level distribution (how to divide a total budget across all tasks proportionally).

**Resource dimensions.** The task's `resource_estimate` carries three dimensions, matching the workspace resource meter (Workspace spec, §6) and checkpoint resource tracking (Checkpoint spec, §6):

| Dimension | Unit | Meaning |
|-----------|------|---------|
| `tokens` | integer | Expected token consumption (input + output) |
| `wall_time` | duration | Expected elapsed time from `assigned` to `completed` |
| `cost` | number | Expected monetary cost (implementation-defined unit) |

All three are optional. The coordinator may estimate one, two, or all three — or none. A task with no `resource_estimate` can still be assigned; the workspace simply receives no estimate-informed budget. The coordinator sets the workspace budget directly.

**Estimates are predictions, not limits.** The `resource_estimate` is the coordinator's best guess at creation time. It does not constrain the workspace — the workspace budget (set at creation) is the constraint. The estimate informs the budget; it does not become the budget. The coordinator may allocate more than the estimate (safety margin) or less (cost pressure). The runtime enforces workspace budgets (Workspace spec, §6), not task estimates.

**Graph-level allocation.** When all tasks in a graph carry resource estimates, the coordinator can distribute a total budget proportionally. Given a total token budget of 100,000 and five tasks with estimates of 10k, 20k, 30k, 15k, and 25k, the coordinator can allocate proportionally — each task receives its estimated share of the total. This is a coordinator strategy, not a protocol mechanism — the protocol provides the estimates, the coordinator does the math.

**Three allocation strategies** (coordinator-chosen, not protocol-enforced):

**Proportional.** Each task receives a budget proportional to its estimate relative to the total. Simple, predictable, but does not account for uncertainty — a task that runs over budget starves.

**Proportional with reserve.** The coordinator withholds a percentage (e.g., 20%) as a reserve pool. Tasks receive proportional shares of the remaining 80%. When a task exhausts its budget, the coordinator may allocate additional budget from the reserve. This absorbs estimation error without starving other tasks.

**Critical-path weighted.** Tasks on the critical path (§3) receive larger allocations than their estimates suggest — because their completion determines the graph's overall duration. Non-critical tasks receive tighter budgets. This prioritizes throughput on the longest dependency chain.

The protocol does not prescribe an allocation strategy. It provides the inputs — per-task estimates and the graph structure — and the coordinator selects the strategy. Implementations may offer built-in allocation strategies as coordinator utilities.

**Estimation accuracy improves with history.** When the coordinator retries a task or decomposes a task into subtasks, earlier attempts provide actual resource consumption data (from the workspace resource meter). The coordinator can use this data to refine estimates for subsequent tasks. The protocol records actuals in checkpoint resource tracking (Checkpoint spec, §6) and workspace resource meters — the data is available. Using it to improve estimates is a coordinator concern, not a protocol requirement.

**No runtime enforcement of estimates.** The runtime does not compare task estimates to actual consumption. It enforces workspace budgets — which are the binding constraint. If a workspace exceeds its budget, the runtime acts (warning, then fail). Whether the workspace budget was derived from a task estimate or set directly by the coordinator is invisible to the runtime. This separation keeps the task primitive clean: estimates inform, budgets enforce.

## 7. Decomposition

Decomposition is how a task becomes a graph. The coordinator takes a high-level goal and breaks it into smaller, executable units — each one a task with its own description, dependencies, and resource estimate. The result is a task graph (§3) where the root task represents the original goal and child tasks represent the work required to achieve it.

**Decomposition is a coordinator action, not an automatic process.** The protocol provides the structure — the `parent_task` field, the graph container, the DAG invariants — but the coordinator decides how to decompose. The protocol does not analyze goals, infer subtasks, or suggest breakdowns. It accepts the coordinator's decomposition and validates the structure.

**Two decomposition patterns:**

**Upfront decomposition.** The coordinator analyzes the goal and constructs the full task graph before any work begins. All tasks are created in `draft` status, the graph is validated (acyclic, single-graph membership, intra-graph dependencies), and the entire plan is submitted for human approval through the highway. The human sees the full scope — every task, every dependency, every estimate — before a single workspace is spawned.

**Progressive decomposition.** The coordinator creates an initial graph with coarse-grained tasks, then refines them during execution. When a task completes and the coordinator reads its checkpoint, it may discover that downstream work needs to be broken down further. The coordinator adds new tasks to the graph — each with `parent_task` pointing to the task being decomposed, and `depends_on` reflecting the new dependency structure. These new tasks enter as `draft` and require human approval before becoming `pending`.

Both patterns may be combined. The coordinator may construct an initial graph upfront and refine it progressively as execution reveals more information. The protocol supports both because the graph grows but does not shrink (§3) — adding tasks is always valid, removing them is not.

**Decomposition rules:**

**Rule 1: Child tasks reference their parent.** When a task is decomposed into subtasks, each subtask's `parent_task` field is set to the decomposed task's ID. This forms a decomposition tree — orthogonal to the dependency DAG. The tree tracks where tasks came from; the DAG tracks what they depend on. A task may have a parent (decomposition origin) that is not in its `depends_on` list, and dependencies that are not its parent.

**Rule 2: Child tasks belong to the same graph.** Subtasks created through decomposition are added to the parent task's graph (`graph_ref`). No new graph is created — the existing graph grows. This maintains invariant 2 (every task belongs to exactly one graph) and invariant 3 (dependencies are intra-graph).

**Rule 3: Decomposed tasks are not automatically completed.** When a task is decomposed into subtasks, the parent task does not automatically complete. The coordinator decides when the parent is satisfied — typically when all child tasks reach `completed` or `integrated`. The protocol does not enforce this relationship. The coordinator may consider a parent task complete when a subset of children finish, or it may add more children later. The decomposition tree is informational; the dependency DAG is structural.

**Rule 4: New tasks enter as `draft`.** Every task added through progressive decomposition enters the graph in `draft` status. The human gate applies uniformly — whether the task was part of the initial plan or discovered mid-execution. This prevents scope creep without human awareness. The coordinator cannot silently expand the work plan.

**Decomposition depth.** The protocol does not limit decomposition depth — a task may be decomposed into subtasks, which may themselves be decomposed. The `parent_task` chain tracks the full lineage. Implementations may impose depth limits as a safety measure to prevent runaway decomposition, but the protocol itself places no ceiling.

## 8. Trail Events

Every task lifecycle transition and structural change produces a trail entry. The trail is the protocol's memory of what happened to each task — when it was created, how it moved through its lifecycle, which workspaces attempted it, and how the graph evolved.

**Seven event types:**

| Event | Trigger | Key fields |
|-------|---------|------------|
| `task_created` | Task added to graph | `task_id`, `graph_id`, `parent_task` (if decomposed), `name`, `depends_on`, `priority` |
| `task_approved` | Human approves via highway | `task_id`, `approval_source` (human or timeout-auto-approve) |
| `task_assigned` | Workspace bound to task | `task_id`, `workspace_id`, `attempt_number` |
| `task_status_changed` | Any status transition | `task_id`, `from_status`, `to_status`, `workspace_id` (if applicable) |
| `task_completed` | Workspace emits `complete` | `task_id`, `workspace_id`, `checkpoint_id` |
| `task_failed` | Workspace reaches `failed` | `task_id`, `workspace_id`, `attempt_number`, `failure_reason` |
| `graph_created` | Task graph constructed | `graph_id`, `root_task_id`, `task_count` |

**Event semantics:**

- **`task_created`** — recorded when a task is added to a graph, whether at initial construction or through progressive decomposition (§7). The `parent_task` field distinguishes root tasks (null) from decomposed subtasks. The `depends_on` snapshot captures the dependency list at creation — since dependencies are immutable (§3), this is the permanent record.

- **`task_approved`** — recorded when a task transitions from `draft` to `pending`. The `approval_source` distinguishes explicit human approval from timeout-triggered auto-approval. This is the protocol's record that the human gate was passed — or bypassed by timeout policy.

- **`task_assigned`** — recorded each time a task is bound to a workspace (§5). The `attempt_number` is the position in `workspace_history` — 1 for the first attempt, 2 for the first retry, and so on. Multiple `task_assigned` entries for the same task indicate retries.

- **`task_status_changed`** — the general transition event. Recorded for every status change in the lifecycle (§4). This is the catch-all — it captures transitions not covered by the specific events above (`pending` → `assigned`, `in_progress` → `completed`, etc.). The `workspace_id` is included when the transition involves a workspace (null for `draft` → `pending` or `draft` → `cancelled`).

- **`task_completed`** — recorded when a task reaches `completed` status. The `checkpoint_id` references the task's deliverable — the final checkpoint from the completing workspace. This entry links the task to its output in the trail.

- **`task_failed`** — recorded when a task reaches `failed` status. The `failure_reason` is inherited from the workspace's failure (workspace `failed` signal reason). The `attempt_number` indicates which attempt failed, enabling failure pattern analysis across retries.

- **`graph_created`** — recorded once per graph at construction. The `task_count` is the initial count — tasks added through progressive decomposition produce their own `task_created` entries but do not update this event.

**Trail entry format.** All task trail entries follow the standard trail entry format (Trail spec). Each entry carries a timestamp (Clock spec, invariant 1), an actor (the coordinator for most task events, the runtime for derived transitions), and the event-specific fields listed above. Task trail entries are recorded in the global trail and projected into the coordinator's local trail.

**No payload in trail entries.** Task trail entries record structural facts — IDs, statuses, references. The task's `description`, `resource_estimate`, and other content fields are accessible through the task itself. The trail records what happened, not what the task contains.

## 9. Conformance Requirements

Three tiers: **Core** (minimum viable task support), **Standard** (full lifecycle and graph management), **Full** (resource estimation, decomposition, and history tracking).

**Core (7 requirements):**

| # | Requirement |
|---|-------------|
| C-1 | Tasks carry the schema defined in §2. All fields present; `id` and `timestamp` runtime-assigned. |
| C-2 | Task status follows the eight-state lifecycle (§4). No transitions exist outside those defined in the state derivation table. |
| C-3 | Tasks begin in `draft` status. The `draft` → `pending` transition requires human approval or timeout fallback. |
| C-4 | The runtime validates the task graph is acyclic at construction and when tasks are added (invariant 1, §3). Cycles are rejected. |
| C-5 | Every task belongs to exactly one graph. `graph_ref` is set at creation and immutable (invariant 2, §3). |
| C-6 | Dependencies reference tasks within the same graph only (invariant 3, §3). Cross-graph dependencies are rejected. |
| C-7 | `depends_on` is immutable after task creation (§3). The runtime rejects attempts to modify dependencies. |

**Standard (8 requirements):**

| # | Requirement |
|---|-------------|
| S-1 | Binding enforces one task per workspace and one workspace per task at any time (§5, rules 1–2). |
| S-2 | Binding requires `pending` status (§5, rule 3). The runtime rejects binding for tasks in any other state. |
| S-3 | `workspace_ref` is updated on binding and preserved at terminal states (§5, rule 5). |
| S-4 | `workspace_history` is appended on every binding, including retries (§5). Order reflects attempt sequence. |
| S-5 | `checkpoint_ref` is set when the task reaches `completed` status, referencing the final checkpoint from the completing workspace (§2). |
| S-6 | Readiness is computed from the graph: a task is ready when all `depends_on` tasks have reached `completed` or `integrated` (§3). |
| S-7 | Cancellation of a task aborts its workspace (if any) and is recorded in the trail (§4, §8). |
| S-8 | All seven trail event types (§8) are recorded for their respective triggers. |

**Full (5 requirements):**

| # | Requirement |
|---|-------------|
| F-1 | `resource_estimate` fields are validated when present: `tokens` ≥ 0, `wall_time` > 0, `cost` ≥ 0 (§6). |
| F-2 | Progressive decomposition is supported: tasks may be added to an existing graph after construction (§7). New tasks enter as `draft`. |
| F-3 | `parent_task` is set for decomposed tasks and immutable after creation (§7, rule 1). |
| F-4 | The `task_approved` trail entry distinguishes explicit human approval from timeout-auto-approve (§8). |
| F-5 | `attempt_number` in `task_assigned` and `task_failed` trail entries accurately reflects the task's position in `workspace_history` (§8). |

**20 requirements total.** Core ensures structural integrity and the human gate. Standard ensures correct task-workspace binding and trail recording. Full ensures resource validation, decomposition support, and approval provenance.

## 10. Implementation Notes

*This section is non-normative. It captures practical guidance for implementers — approaches that have proven useful but are not protocol requirements.*

**Graph storage.** The task graph is a small data structure — typically tens to hundreds of nodes. An adjacency list (task ID → list of dependent task IDs) is sufficient for readiness computation and cycle detection. Implementations need not use a graph database. A dictionary keyed by task ID, with each entry holding the task schema and a precomputed `ready` flag, handles the common case. Cycle detection at insertion time (topological sort or DFS-based) prevents invalid graphs from forming.

**Readiness propagation.** When a task completes, the runtime must re-evaluate its dependents. Two approaches: eager propagation (on completion, walk the dependent list and mark newly ready tasks) or lazy evaluation (compute readiness on demand when the coordinator asks "what is ready?"). Eager propagation is simpler and avoids redundant graph walks. For small graphs, the difference is negligible.

**Retry limits.** The protocol does not limit retries, but implementations should. A reasonable default: 3 attempts per task. After the limit, the task remains in `failed` status and the coordinator must explicitly cancel or override the limit. The retry limit is a configuration parameter, not a protocol constant.

**Estimate calibration.** Implementations that track actual resource consumption alongside estimates can compute estimation accuracy over time. A simple metric: `actual / estimated` ratio per task. A coordinator that consistently overestimates (ratio < 0.5) or underestimates (ratio > 2.0) can adjust future estimates. This is a coordinator-side optimization — the protocol provides the data (workspace resource meters, checkpoint resource tracking), the implementation provides the analysis.

**Graph visualization.** The task graph is naturally visual — nodes are tasks, edges are dependencies, color indicates status. Implementations that surface graph visualization to the human highway give humans a powerful overview: what is done, what is in progress, what is blocked, what is ready. The graph structure (§3) and task statuses (§4) provide all the data needed for rendering.

**Batch approval.** When the coordinator submits a full graph for human approval (upfront decomposition, §7), the human may want to approve all tasks at once rather than individually. Implementations may support batch approval — transitioning all `draft` tasks in a graph to `pending` in a single action. This is a human highway convenience, not a protocol feature. Each task still receives its own `task_approved` trail entry.

**Task identity across retries.** The task ID is stable across retries — workspace IDs change, but the task ID does not. This makes task-level queries straightforward: "show me everything that happened for task T" returns all attempts, all workspaces, all failures, and the final result. The `workspace_history` field is the index; the trail entries are the detail.

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
