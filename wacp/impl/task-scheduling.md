# WACP Implementation: Task Scheduling

```yaml
id: wacp-impl-task-scheduling
type: implementation-spec
status: complete
created: 2026-03-23
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4.6 (task)
  - §7 (integration and checkpoints)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-topology
  - wacp-spec-task
  - wacp-spec-workspace
  - wacp-spec-signal
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, task, scheduling, dispatch, gate, retry, resource-allocation]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Task Lifecycle State Machine](#2-task-lifecycle-state-machine)
3. [Gate Enforcement](#3-gate-enforcement)
4. [Dispatch Policy](#4-dispatch-policy)
5. [Resource Allocation](#5-resource-allocation)
6. [Context Assembly](#6-context-assembly)
7. [Retry and Cancellation](#7-retry-and-cancellation)
8. [Progressive Decomposition](#8-progressive-decomposition)
9. [Task-Workspace Coupling](#9-task-workspace-coupling)
10. [Trail Events](#10-trail-events)
11. [References](#11-references)

## 1. Purpose

This spec defines how the coordinator decides what to do with tasks — when to dispatch them, where to dispatch them, how much budget to allocate, and what to do when they fail. It answers "what is the coordinator's scheduling logic" — not "how are tasks stored" (that's the topology spec, §3) or "how are checkpoints merged" (that's the integration spec).

The topology spec (§3) defines the task graph data structure: `TaskGraph`, `TaskNode`, adjacency lists, readiness counters, and the core operations (`insert`, `mark_completed`, `mark_failed`, `dispatchable`, `bind_task_to_workspace`). This spec builds on those data structures. It does not redefine them — it defines the *policy* that uses them.

**Scope.** The task lifecycle state machine — all eight states, their transitions, and the triggers that drive them. Gate enforcement for the `draft → pending` transition (the protocol's only human approval gate on the task lifecycle). Dispatch policy — how the coordinator selects which ready tasks to dispatch and where to place them in the workspace tree. Resource allocation — how workspace budgets are derived from task estimates. Context assembly — what dependency outputs are included in the new workspace's context. Retry and cancellation — the coordinator's failure response. Progressive decomposition — adding subtasks to the graph during execution. Task-workspace coupling — how task state mirrors workspace state.

**Not in scope.** Task graph data structures (topology spec, §3). Integration logic — what happens after a task reaches `completed` (integration spec). Workspace lifecycle internals (runtime spec, §4). Envelope delivery or signal propagation (runtime spec, §9–10). How agents execute tasks (sdk-agent spec).

**Design constraint.** The task lifecycle is driven by workspace state. Every task status transition (except `draft → pending` and coordinator-initiated cancellation/retry) is triggered by a workspace state change. The coordinator observes workspace signals and updates task status accordingly — it does not poll. Task scheduling decisions are policy, not protocol — the protocol defines what transitions are valid; the coordinator decides when and why to trigger them.

---

## 2. Task Lifecycle State Machine

The task lifecycle has eight states, matching the `TaskStatus` protobuf enum (protocol-interface spec, §3). Transitions are unidirectional — no backward movement. Two states are terminal (`integrated`, `cancelled`); one (`failed`) is non-terminal because the coordinator may retry.

### 2.1 State Diagram

```
draft ──► pending ──► assigned ──► in_progress ──► completed ──► integrated
  │                                     │                │
  │                                     │                └──► failed ──► pending (retry)
  │                                     │                              │
  │                                     └──► failed ──► pending (retry)│
  │                                                    │               │
  └──► cancelled                                       └──► cancelled ◄┘
```

### 2.2 States

| State | Meaning | Workspace state | Entry trigger |
|-------|---------|-----------------|---------------|
| `Draft` | Created, awaiting human approval | None | Task inserted into graph |
| `Pending` | Approved, awaiting dispatch | None | Gate approval or timeout auto-approve |
| `Assigned` | Workspace created and bound | `Idle` | Coordinator dispatches task |
| `InProgress` | Agent working | `Active` / `Blocked` | Workspace emits `started` signal |
| `Completed` | Agent finished, awaiting integration | `Integrating` | Workspace emits `complete` signal |
| `Failed` | Workspace failed (retryable) | `Failed` | Workspace reaches `Failed` state |
| `Integrated` | Terminal — output merged into parent | `Closed` | Integration succeeds |
| `Cancelled` | Terminal — coordinator withdrew task | Any / None | Coordinator decision |

### 2.3 Transitions

The task FSM is implemented as an instantiation of the generic state machine engine (runtime spec, §4). Every transition follows the four-step sequence: permission check → precondition check → trail write → state update.

```rust
impl StateMachine for TaskLifecycle {
    type State = TaskStatus;
    type Trigger = TaskTrigger;
    type Context = TaskTransitionContext;

    fn transition(
        state: Self::State,
        trigger: Self::Trigger,
        ctx: &Self::Context,
    ) -> Result<Self::State, TransitionError> {
        match (state, trigger) {
            // Gate approval
            (TaskStatus::Draft, TaskTrigger::Approve) =>
                Ok(TaskStatus::Pending),
            (TaskStatus::Draft, TaskTrigger::AutoApprove) =>
                Ok(TaskStatus::Pending),

            // Dispatch
            (TaskStatus::Pending, TaskTrigger::Assign) =>
                Ok(TaskStatus::Assigned),

            // Workspace signals
            (TaskStatus::Assigned, TaskTrigger::WorkspaceStarted) =>
                Ok(TaskStatus::InProgress),
            (TaskStatus::InProgress, TaskTrigger::WorkspaceCompleted) =>
                Ok(TaskStatus::Completed),

            // Failure (from assigned or in_progress)
            (TaskStatus::Assigned, TaskTrigger::WorkspaceFailed) =>
                Ok(TaskStatus::Failed),
            (TaskStatus::InProgress, TaskTrigger::WorkspaceFailed) =>
                Ok(TaskStatus::Failed),

            // Integration outcome
            (TaskStatus::Completed, TaskTrigger::IntegrationSucceeded) =>
                Ok(TaskStatus::Integrated),
            (TaskStatus::Completed, TaskTrigger::IntegrationFailed) =>
                Ok(TaskStatus::Failed),

            // Retry
            (TaskStatus::Failed, TaskTrigger::Retry) =>
                Ok(TaskStatus::Pending),

            // Cancellation (from any non-terminal state)
            (TaskStatus::Draft, TaskTrigger::Cancel) =>
                Ok(TaskStatus::Cancelled),
            (TaskStatus::Pending, TaskTrigger::Cancel) =>
                Ok(TaskStatus::Cancelled),
            (TaskStatus::Assigned, TaskTrigger::Cancel) =>
                Ok(TaskStatus::Cancelled),
            (TaskStatus::InProgress, TaskTrigger::Cancel) =>
                Ok(TaskStatus::Cancelled),
            (TaskStatus::Failed, TaskTrigger::Cancel) =>
                Ok(TaskStatus::Cancelled),

            // Terminal states reject all triggers
            (TaskStatus::Integrated, _) =>
                Err(TransitionError::TerminalState),
            (TaskStatus::Cancelled, _) =>
                Err(TransitionError::TerminalState),

            // All other combinations are illegal
            _ => Err(TransitionError::IllegalTransition),
        }
    }
}

pub enum TaskTrigger {
    Approve,
    AutoApprove,
    Assign,
    WorkspaceStarted,
    WorkspaceCompleted,
    WorkspaceFailed,
    IntegrationSucceeded,
    IntegrationFailed,
    Retry,
    Cancel,
}
```

**Exhaustive matching.** Rust's match exhaustiveness guarantees every `(state, trigger)` combination is handled. Adding a state or trigger forces every match arm to be updated — an unhandled transition is a compile error.

### 2.4 Transition Ownership

Not all transitions are triggered by the same actor:

| Transition | Triggered by | Mechanism |
|-----------|-------------|-----------|
| `Draft → Pending` | Human (via highway) or timeout | Gate response or timer expiry (§3) |
| `Pending → Assigned` | Coordinator | Dispatch decision (§4) |
| `Assigned → InProgress` | Workspace actor | `started` signal propagated to coordinator |
| `InProgress → Completed` | Workspace actor | `complete` signal propagated to coordinator |
| `* → Failed` | Workspace actor or coordinator | `failed` signal, abort, timeout, or budget exceeded |
| `Completed → Integrated` | Coordinator | Integration logic (integration spec) |
| `Completed → Failed` | Coordinator | Integration failure |
| `Failed → Pending` | Coordinator | Retry decision (§7) |
| `* → Cancelled` | Coordinator | Cancellation decision (§7) |

The coordinator is the sole writer of task state — workspace signals arrive as messages on the coordinator's channel, and the coordinator applies the transition. No workspace actor directly mutates `TaskNode.status`. This preserves single-writer serialization (runtime spec, §14, invariant 1).

---

## 3. Gate Enforcement

The `draft → pending` transition is the protocol's human approval gate on the task lifecycle. A task in `draft` cannot be dispatched until a human approves it (via the highway) or a timeout policy takes effect.

### 3.1 Gate Event

When a task enters `draft`, the coordinator creates a gate event and routes it to the highway:

```rust
pub struct TaskGateEvent {
    pub gate_id: GateId,
    pub gate_type: GateType::TaskApproval,
    pub task_id: TaskId,
    pub task_name: String,
    pub task_description: String,
    pub depends_on: Vec<TaskId>,
    pub resource_estimate: Option<ResourceEstimate>,
    pub timeout_ms: u64,
    pub fallback_action: GateFallback,
    pub created_at: Timestamp,
}

pub enum GateFallback {
    AutoApprove,
    Cancel,
}
```

The gate event is delivered to all connected highway clients via the `StreamGates` RPC (protocol-interface spec, §5). The human sees the task's name, description, dependencies, and estimated resource cost. They approve, reject (cancel), or modify the task.

### 3.2 Gate Resolution

Three outcomes:

**Human approves.** The highway client sends a `GateResponse` with `decision: APPROVE`. The coordinator transitions the task from `draft` to `pending` with trigger `Approve`. A `task_approved` trail entry is written with `approval_source: "human"`.

**Human rejects.** The highway client sends a `GateResponse` with `decision: REJECT`. The coordinator transitions the task from `draft` to `cancelled` with trigger `Cancel`.

**Timeout.** No highway response arrives within `timeout_ms`. The coordinator applies the `fallback_action`:
- `AutoApprove`: transition to `pending` with trigger `AutoApprove`. Trail entry records `approval_source: "timeout_auto_approve"`.
- `Cancel`: transition to `cancelled`.

```rust
impl CoordinatorActor {
    fn handle_gate_response(&mut self, response: GateResponse) {
        let gate = match self.pending_gates.remove(&response.gate_id) {
            Some(g) => g,
            None => return, // gate already resolved (timeout or other user)
        };

        match response.decision {
            GateDecision::Approve => {
                self.transition_task(&gate.task_id, TaskTrigger::Approve);
                self.try_dispatch();
            }
            GateDecision::Reject => {
                self.transition_task(&gate.task_id, TaskTrigger::Cancel);
            }
            GateDecision::Modify => {
                // Modification: update task description or estimates,
                // then approve with the modified values
                self.apply_task_modifications(&gate.task_id, &response.modifications);
                self.transition_task(&gate.task_id, TaskTrigger::Approve);
                self.try_dispatch();
            }
        }
    }

    fn handle_gate_timeout(&mut self, gate_id: &GateId) {
        let gate = match self.pending_gates.remove(gate_id) {
            Some(g) => g,
            None => return,
        };

        match gate.fallback_action {
            GateFallback::AutoApprove => {
                self.transition_task(&gate.task_id, TaskTrigger::AutoApprove);
                self.try_dispatch();
            }
            GateFallback::Cancel => {
                self.transition_task(&gate.task_id, TaskTrigger::Cancel);
            }
        }
    }
}
```

### 3.3 Gate Timeout Management

Gate timeouts are managed using `tokio::time::sleep_until` futures in the coordinator's `FuturesUnordered`, the same mechanism as workspace timeouts (runtime spec, §12). When a gate is created, the coordinator inserts a timeout future. When a human responds, the coordinator removes the gate from `pending_gates` — if the timeout fires after the response, the `pending_gates.remove` returns `None` and the timeout is a no-op.

### 3.4 First Response Wins

Multiple highway clients may be connected. The first response to a gate wins — subsequent responses receive `applied: false` in the `GateResponseAck` (protocol-interface spec, §5). This is enforced by the `pending_gates.remove` pattern: the first response removes the entry, and subsequent responses find nothing to remove.

---

## 4. Dispatch Policy

When a task reaches `pending` and its dependencies are satisfied (readiness counter at zero — topology spec, §3.3), the coordinator may dispatch it. This section defines how the coordinator selects tasks and places them.

### 4.1 Dispatch Loop

The coordinator runs dispatch whenever the ready set changes — after a gate approval, after a task completion (which may unblock dependents), or after a retry. The loop is not periodic — it is event-driven.

```rust
impl CoordinatorActor {
    fn try_dispatch(&mut self) {
        let dispatchable: Vec<TaskId> = self.task_graph.dispatchable()
            .into_iter().cloned().collect();

        if dispatchable.is_empty() {
            return;
        }

        // Sort by priority (policy decision)
        let ordered = self.prioritize(dispatchable);

        for task_id in ordered {
            // Check if we have capacity to create another workspace
            if !self.has_dispatch_capacity() {
                break;
            }

            self.dispatch_task(&task_id);
        }
    }
}
```

### 4.2 Prioritization

The coordinator orders dispatchable tasks before dispatching. The ordering is a policy decision — the protocol does not prescribe a scheduling algorithm. The initial implementation uses a two-level sort:

1. **Priority class.** `urgent` tasks before `normal` tasks. Priority is a task-level field set at creation.
2. **Creation order.** Within the same priority class, tasks created earlier are dispatched first (FIFO). This provides fairness and predictability.

```rust
fn prioritize(&self, tasks: Vec<TaskId>) -> Vec<TaskId> {
    let mut with_priority: Vec<_> = tasks.into_iter()
        .filter_map(|id| {
            self.task_graph.tasks.get(&id)
                .map(|t| (id, t.priority, t.created_at))
        })
        .collect();

    with_priority.sort_by(|a, b| {
        b.1.cmp(&a.1)                    // urgent before normal (descending)
            .then(a.2.cmp(&b.2))          // earlier before later (ascending)
    });

    with_priority.into_iter().map(|(id, _, _)| id).collect()
}
```

**Critical path scheduling.** An optional enhancement: weight tasks by their position on the critical path (longest dependency chain to a sink). Critical-path tasks receive higher priority because delaying them delays the entire run. This requires computing the critical path on the DAG — a topological-sort-based O(V + E) algorithm. Not included in the initial implementation; the FIFO policy is sufficient for typical workloads where the human structures the task graph with explicit priorities.

### 4.3 Capacity Check

The coordinator does not dispatch unboundedly. `has_dispatch_capacity` checks:

1. **Active workspace count.** A configurable maximum number of concurrent active workspaces (default: no limit). This prevents resource exhaustion on the host.
2. **Remaining global budget.** If a global budget ceiling is configured, the coordinator checks whether allocating a new workspace would exceed it.

```rust
fn has_dispatch_capacity(&self) -> bool {
    let active = self.tree.nodes.values()
        .filter(|n| !n.status.is_terminal() && n.status != WorkspaceStatus::Idle)
        .count();

    if let Some(max) = self.config.max_concurrent_workspaces {
        if active >= max {
            return false;
        }
    }

    true
}
```

### 4.4 Dispatch Execution

Dispatching a task creates a workspace, binds the task, and delivers the directive:

```rust
fn dispatch_task(&mut self, task_id: &TaskId) {
    let task = match self.task_graph.tasks.get(task_id) {
        Some(t) => t.clone(),
        None => return,
    };

    // 1. Allocate budget (§5)
    let budget = self.allocate_budget(&task);

    // 2. Assemble context from dependencies (§6)
    let context = self.assemble_context(task_id);

    // 3. Determine tree placement
    let parent = self.select_parent(&task);

    // 4. Determine role
    let role = self.select_role(&task);

    // 5. Create workspace (topology spec, §8.1 — compound operation)
    let workspace_id = self.create_workspace(
        &parent, task_id, &role,
        None,   // owner: inherit from parent
        false,  // not an injection
        None,   // no injector
        vec![], // initial visibility: default
    ).expect("workspace creation should succeed");

    // 6. Apply budget to workspace
    self.apply_workspace_budget(&workspace_id, &budget);

    // 7. Transition task: pending → assigned
    self.transition_task(task_id, TaskTrigger::Assign);

    // 8. Deliver directive envelope to workspace
    let directive = self.build_directive(&task, &context);
    self.deliver_directive(&workspace_id, directive);

    // 9. Trail: task_assigned
}
```

### 4.5 Tree Placement

The coordinator decides where in the workspace tree to place the new workspace. The default strategy places all task workspaces as direct children of the root coordinator workspace. This produces a flat, star-shaped tree — simple, with the coordinator as the direct parent of every worker.

For delegate-based decomposition, the subtask workspaces are placed as children of the delegate workspace. The delegate's task id determines the parent workspace:

```rust
fn select_parent(&self, task: &TaskNode) -> WorkspaceId {
    match &task.parent_task {
        Some(parent_task_id) => {
            // Subtask: place under the delegate workspace executing the parent task
            self.task_graph.task_workspace.get(parent_task_id)
                .cloned()
                .unwrap_or_else(|| self.tree.root.clone())
        }
        None => {
            // Root-level task: place under coordinator
            self.tree.root.clone()
        }
    }
}
```

---

## 5. Resource Allocation

When dispatching a task, the coordinator must decide how much budget to allocate to the new workspace. The task's `resource_estimate` is advisory — the workspace budget is a hard limit enforced by the runtime (runtime spec, §12).

### 5.1 Resource Estimate

Tasks may carry an optional estimate of resource consumption:

```rust
pub struct ResourceEstimate {
    pub tokens: Option<u64>,        // expected LLM token consumption
    pub wall_time_ms: Option<u64>,  // expected elapsed time
    pub cost_micros: Option<u64>,   // expected monetary cost
}
```

Estimates are set at task creation by the coordinator or by the human during gate approval (modification). They are advisory — the runtime does not enforce them. The coordinator uses them to compute workspace budgets.

### 5.2 Allocation Strategies

The coordinator selects a budget allocation strategy. The initial implementation provides three:

**Direct.** Set workspace budget directly from the task estimate with a safety margin. Simple, predictable.

```rust
fn allocate_direct(estimate: &ResourceEstimate, margin: f32) -> ResourceBudget {
    let scale = 1.0 + margin; // e.g., 1.2 for 20% margin
    ResourceBudget {
        max_tokens: estimate.tokens.map(|t| (t as f32 * scale) as u64).unwrap_or(0),
        max_wall_time_ms: estimate.wall_time_ms.map(|t| (t as f32 * scale) as u64).unwrap_or(0),
        max_cost_micros: estimate.cost_micros.map(|c| (c as f32 * scale) as u64).unwrap_or(0),
        ..Default::default()
    }
}
```

**Proportional.** Distribute the remaining global budget proportionally across all dispatchable tasks, weighted by their estimates. Tasks with larger estimates receive larger budgets.

```rust
fn allocate_proportional(
    estimate: &ResourceEstimate,
    total_remaining: &ResourceBudget,
    all_estimates: &[ResourceEstimate],
) -> ResourceBudget {
    let total_tokens: u64 = all_estimates.iter()
        .filter_map(|e| e.tokens)
        .sum();
    let fraction = estimate.tokens
        .map(|t| t as f64 / total_tokens.max(1) as f64)
        .unwrap_or(1.0 / all_estimates.len() as f64);

    ResourceBudget {
        max_tokens: (total_remaining.max_tokens as f64 * fraction) as u64,
        max_wall_time_ms: (total_remaining.max_wall_time_ms as f64 * fraction) as u64,
        max_cost_micros: (total_remaining.max_cost_micros as f64 * fraction) as u64,
        ..Default::default()
    }
}
```

**No estimate.** When a task has no resource estimate, the coordinator falls back to deployment-configured defaults (`resources.default_budget` from deployment spec, §2.7). If defaults are also zero (unlimited), the workspace runs without budget limits.

### 5.3 Budget Application

The computed budget is applied to the workspace via the workspace actor's resource meter:

```rust
fn apply_workspace_budget(&mut self, workspace_id: &WorkspaceId, budget: &ResourceBudget) {
    if let Some(channels) = self.workspace_channels.get(workspace_id) {
        let _ = channels.coordinator_tx.send(
            CoordinatorCommand::BudgetUpdate {
                new_budget: budget.clone(),
            }
        );
    }
}
```

The budget is included in the `BindResponse` when the agent connects (protocol-interface spec, §4) — the agent knows its limits from the start.

---

## 6. Context Assembly

When dispatching a task, the coordinator assembles the workspace's context from the outputs of the task's dependencies. The context is read-only information the agent receives at bind time.

### 6.1 Dependency Resolution

The coordinator resolves the task's `depends_on` list to completed dependency outputs:

```rust
fn assemble_context(&self, task_id: &TaskId) -> TaskContext {
    let deps = self.task_graph.reverse.get(task_id)
        .cloned()
        .unwrap_or_default();

    let mut dependency_outputs = Vec::new();

    for dep_id in &deps {
        let dep_task = match self.task_graph.tasks.get(dep_id) {
            Some(t) => t,
            None => continue,
        };

        // The dependency must be completed or integrated
        if !matches!(dep_task.status, TaskStatus::Completed | TaskStatus::Integrated) {
            continue; // should not happen if readiness is correct
        }

        // Collect the dependency's final checkpoint reference
        if let Some(ref checkpoint_ref) = dep_task.checkpoint_ref {
            dependency_outputs.push(DependencyOutput {
                task_id: dep_id.clone(),
                task_name: dep_task.name.clone(),
                checkpoint_ref: checkpoint_ref.clone(),
            });
        }
    }

    TaskContext { dependency_outputs }
}

pub struct TaskContext {
    pub dependency_outputs: Vec<DependencyOutput>,
}

pub struct DependencyOutput {
    pub task_id: TaskId,
    pub task_name: String,
    pub checkpoint_ref: String,    // checkpoint id — payload readable via ReadResource
}
```

### 6.2 Context Delivery

The context is serialized into the workspace's `context` field — component 3 of the nine internal components (runtime spec, §8). It is immutable after creation. The agent reads it at bind time via the `BindResponse.context` field and uses it to understand what its dependencies produced.

The agent can read the full checkpoint payloads via `ReadResource` RPCs — the context carries references (checkpoint ids), not the payloads themselves. This keeps the `BindResponse` small even when dependency outputs are large.

---

## 7. Retry and Cancellation

When a task fails, the coordinator decides whether to retry or cancel. This is a policy decision — the protocol permits unlimited retries, but practical implementations enforce limits.

### 7.1 Retry

Retrying a task transitions it from `failed` back to `pending`. The coordinator creates a new workspace for the retry — the failed workspace remains as an archived record. The task's `workspace_history` field records all workspaces that have attempted the task.

```rust
fn retry_task(&mut self, task_id: &TaskId) {
    let task = match self.task_graph.tasks.get_mut(task_id) {
        Some(t) => t,
        None => return,
    };

    // Record the failed workspace in history
    if let Some(ref ws) = task.workspace_ref {
        task.workspace_history.push(ws.clone());
    }

    // Unbind from failed workspace
    self.task_graph.unbind_task(task_id);

    // Transition: failed → pending
    self.transition_task(task_id, TaskTrigger::Retry);

    // Task is now pending again — try_dispatch will pick it up
    // if its dependencies are still satisfied
    self.try_dispatch();
}
```

### 7.2 Retry Policy

The coordinator applies a configurable retry policy:

```rust
pub struct RetryPolicy {
    pub max_attempts: u32,            // 0 = no retry, 1 = one retry, etc.
    pub retry_on_timeout: bool,       // retry if workspace failed due to timeout
    pub retry_on_budget: bool,        // retry if workspace failed due to budget exceeded
    pub retry_on_agent_failure: bool,  // retry if agent emitted failed signal
}
```

**Default policy.** `max_attempts: 1` (one retry), `retry_on_timeout: false`, `retry_on_budget: false`, `retry_on_agent_failure: true`. Timeout and budget failures suggest the task is fundamentally too large — retrying with the same budget will fail again. Agent failures may be transient (model error, tool failure) and worth retrying.

The retry count is tracked by the length of `workspace_history`:

```rust
fn should_retry(&self, task: &TaskNode, failure_reason: &str) -> bool {
    let attempts = task.workspace_history.len() + 1; // +1 for current attempt
    if attempts > self.retry_policy.max_attempts as usize {
        return false;
    }

    match failure_reason {
        "timeout" => self.retry_policy.retry_on_timeout,
        "budget_exceeded" => self.retry_policy.retry_on_budget,
        _ => self.retry_policy.retry_on_agent_failure,
    }
}
```

### 7.3 Cancellation

Cancelling a task transitions it to `cancelled` (terminal). If the task has a bound workspace, the coordinator aborts the workspace.

```rust
fn cancel_task(&mut self, task_id: &TaskId) {
    let task = match self.task_graph.tasks.get(task_id) {
        Some(t) => t,
        None => return,
    };

    // Abort workspace if bound
    if let Some(ref ws_id) = task.workspace_ref {
        if let Some(channels) = self.workspace_channels.get(ws_id) {
            let _ = channels.coordinator_tx.send(
                CoordinatorCommand::Abort { reason: "task_cancelled".into() }
            );
        }
    }

    // Transition: * → cancelled
    self.transition_task(task_id, TaskTrigger::Cancel);

    // Cancel dependents that can no longer complete
    self.cascade_cancellation(task_id);
}
```

### 7.4 Cancellation Cascade

When a task is cancelled, its dependents may become unreachable — they depend on a task that will never complete. The coordinator identifies affected dependents and cancels them:

```rust
fn cascade_cancellation(&mut self, cancelled_id: &TaskId) {
    let dependents = self.task_graph.forward.get(cancelled_id)
        .cloned()
        .unwrap_or_default();

    for dependent_id in dependents {
        let dependent = match self.task_graph.tasks.get(&dependent_id) {
            Some(t) => t,
            None => continue,
        };

        // Only cancel if the dependent has no alternative path to completion
        // (all dependencies must be completed/integrated; this one won't be)
        if dependent.status == TaskStatus::Draft || dependent.status == TaskStatus::Pending {
            self.cancel_task(&dependent_id);
        }
        // Tasks already in_progress or assigned continue — the coordinator
        // may still salvage them or they may complete independently
    }
}
```

The cascade is conservative: it only cancels tasks that haven't started. Tasks already in progress are left running — the coordinator may decide to salvage their output even if an upstream dependency was cancelled.

---

## 8. Progressive Decomposition

A delegate workspace may decompose its task into subtasks during execution. The subtasks are added to the task graph as new `draft` nodes with `parent_task` set to the delegate's task.

### 8.1 Decomposition Flow

1. The delegate agent sends a structured envelope to the coordinator describing the subtasks it wants to create.
2. The coordinator validates the subtask definitions: names, descriptions, inter-subtask dependencies, resource estimates.
3. The coordinator inserts each subtask into the task graph via `task_graph.insert()` (topology spec, §3.2) with `parent_task` set.
4. Each subtask enters `draft` and flows through the normal gate → dispatch → execution pipeline.
5. The delegate workspace enters `blocked` while waiting for subtask completion. It emits a `blocked` signal with reason "awaiting subtasks."
6. As subtasks complete and are integrated, the delegate receives feedback envelopes with the results.
7. When all subtasks are complete, the delegate resumes (emits `started`) and produces its own final checkpoint incorporating the subtask outputs.

### 8.2 Subtask Dependencies

Subtasks may depend on each other, forming a sub-DAG within the larger task graph. The topology spec's acyclicity guarantee (invariant G-1) holds — subtasks can only depend on pre-existing tasks, never on tasks created later.

Subtasks may also depend on tasks outside the decomposition — sibling tasks of the delegate's parent task. This enables cross-branch dependencies where one delegate's subtask needs output from another delegate's work.

### 8.3 Decomposition and Integration

Subtask integration is nested: each subtask produces checkpoints that are integrated into the delegate's context. The delegate then produces its own checkpoint that is integrated into the delegate's parent. The integration spec defines the merge mechanics — this spec only notes that decomposition creates a two-level integration hierarchy.

---

## 9. Task-Workspace Coupling

Task state mirrors workspace state — every workspace lifecycle event produces a corresponding task state transition. This section defines the coupling.

### 9.1 Signal-to-Task Mapping

The coordinator observes workspace signals and maps them to task triggers:

```rust
impl CoordinatorActor {
    fn handle_workspace_signal(&mut self, signal: Signal) {
        let task_id = match self.task_graph.workspace_task.get(&signal.workspace_id) {
            Some(id) => id.clone(),
            None => return, // workspace has no bound task
        };

        match signal.r#type {
            SignalType::Started => {
                self.transition_task(&task_id, TaskTrigger::WorkspaceStarted);
            }
            SignalType::Complete => {
                // Set checkpoint_ref before transitioning
                if let Some(task) = self.task_graph.tasks.get_mut(&task_id) {
                    task.checkpoint_ref = self.find_final_checkpoint(&signal.workspace_id);
                }
                self.transition_task(&task_id, TaskTrigger::WorkspaceCompleted);
                // Integration begins (integration spec)
            }
            SignalType::Failed => {
                let reason = signal.reason.clone();
                self.transition_task(&task_id, TaskTrigger::WorkspaceFailed);
                // Retry or cancel (§7)
                if self.should_retry(
                    self.task_graph.tasks.get(&task_id).unwrap(),
                    &reason,
                ) {
                    self.retry_task(&task_id);
                } else {
                    self.cancel_task(&task_id);
                }
            }
            SignalType::Blocked => {
                // Task remains InProgress — blocked is a workspace state,
                // not a task state. The task is still "in progress" even
                // when the workspace is waiting for input.
            }
            _ => {} // checkpoint, acknowledged, escalation, suspend, migrate
                    // do not affect task status
        }
    }
}
```

### 9.2 State Correspondence

| Workspace state | Task status | Notes |
|----------------|-------------|-------|
| `Idle` | `Assigned` | Workspace created, agent not yet connected |
| `Active` | `InProgress` | Agent working |
| `Blocked` | `InProgress` | Agent waiting for input — task is still in progress |
| `Suspended` | `InProgress` | Workspace paused — task unchanged |
| `Migrating` | `InProgress` | Agent being replaced — task unchanged |
| `Integrating` | `Completed` | Merge in progress |
| `Conflicted` | `Completed` | Conflict resolution in progress |
| `Closed` | `Integrated` | Integration succeeded |
| `Failed` | `Failed` | Workspace failed — task may retry |

**`Blocked` does not affect task status.** A blocked workspace is waiting for input — the agent emitted `blocked` because it needs feedback, context, or a dependency result. The task is still in progress. The coordinator may respond by sending feedback, granting visibility, or waiting for a dependency to complete. The task only transitions when the workspace reaches a terminal state or emits `complete`.

### 9.3 Workspace History

Each task tracks all workspaces that have attempted it:

```rust
pub struct TaskNode {
    // ... other fields (topology spec, §3.1)
    pub workspace_history: Vec<WorkspaceId>,
    pub checkpoint_ref: Option<String>,
}
```

`workspace_history` is appended on every dispatch (§4.4) and retry (§7.1). It provides an audit trail: "this task was attempted by workspace A (failed), then workspace B (succeeded)." The history is recorded in `task_assigned` trail entries with an `attempt_number` field.

---

## 10. Trail Events

Seven trail event types for the task lifecycle. All are written by the coordinator actor.

### 10.1 Event Definitions

| Event | When | Key fields |
|-------|------|------------|
| `graph_created` | Task graph initialized for a run | `graph_id`, `root_task_id`, `task_count` |
| `task_created` | Task inserted into graph | `task_id`, `graph_id`, `name`, `parent_task`, `depends_on`, `priority`, `resource_estimate` |
| `task_approved` | Gate resolved (`draft → pending`) | `task_id`, `approval_source` (`"human"` or `"timeout_auto_approve"`) |
| `task_assigned` | Workspace bound to task | `task_id`, `workspace_id`, `attempt_number` |
| `task_status_changed` | Any status transition | `task_id`, `from_status`, `to_status`, `workspace_id` (if applicable), `trigger` |
| `task_completed` | Workspace emits `complete` | `task_id`, `workspace_id`, `checkpoint_ref` |
| `task_failed` | Task reaches `failed` | `task_id`, `workspace_id`, `attempt_number`, `failure_reason` |

### 10.2 Recovery

Task state is fully recoverable from the trail. The recovery procedure replays task-related trail events through the topology's `recover_topology` method (topology spec, §9.2), which reconstructs the `TaskGraph` from `task_created`, `task_assigned`, and `task_status_changed` events.

The scheduling-specific state — pending gates, retry counters, active dispatch decisions — is transient. On recovery:
- Tasks in `draft` re-emit gate events to the highway.
- Tasks in `pending` are re-evaluated for dispatch.
- Tasks in `assigned` wait for the workspace to reconnect.
- Tasks in `completed` trigger re-evaluation for integration.
- Tasks in `failed` are re-evaluated for retry.

The coordinator's `try_dispatch` runs after recovery completes, picking up where the pre-crash coordinator left off.

---

## 11. References

### Protocol Specs

| Spec | Referenced in | Topic |
|------|--------------|-------|
| `primitives/task.md` | §2, §3, §8, §9, §10 | Task lifecycle (8 states), gate, decomposition, workspace binding, trail events |
| `mechanisms/integration.md` | §2.3, §8.3 | Integration triggers on task completion, merge strategies |
| PROTOCOL.md §4.6 | §1 | Task as core primitive, dependency model |
| PROTOCOL.md §7 | §6 | Integration and checkpoints, context assembly |
| PROTOCOL.md §8 | §3 | Human highway gate mechanics |

### Implementation Specs

| Spec | Referenced in | Topic |
|------|--------------|-------|
| `impl/topology.md` §3 | §1, §4.1, §6.1, §7.1, §9.1 | TaskGraph data structure, readiness counters, `dispatchable()`, `mark_completed()` |
| `impl/topology.md` §8.1 | §4.4 | Compound workspace creation operation |
| `impl/runtime.md` §4 | §2.3 | Generic state machine engine, `StateMachine` trait |
| `impl/runtime.md` §12 | §5 | Resource enforcement, workspace budgets, warning thresholds |
| `impl/runtime.md` §14 | §2.4 | Single-writer serialization, coordinator as sole task state writer |
| `impl/protocol-interface.md` §4 | §5.3 | `BindResponse` carries budget, `ResourceBudget` message |
| `impl/protocol-interface.md` §5 | §3.1 | `StreamGates` RPC, `GateResponse`, gate mechanics |
| `impl/deployment.md` §2.7 | §5.2 | Default resource budget configuration |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/PROTOCOL.md) | Taxonomy: [TAXONOMY.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/TAXONOMY.md)*
