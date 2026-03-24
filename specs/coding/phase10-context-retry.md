# Task 10.4: Context Assembly + Retry + Decomposition

## Scope

Add context assembly (dependency output collection for workspace context), retry policy (failure response with configurable limits), cancellation cascade (cancel unreachable dependents), and progressive decomposition support (subtask insertion mid-execution).

**Does NOT produce:** Actual workspace actor communication or signal handling (Phase 13).

## Dependencies

- Task 10.1 (task graph with `bind`/`unbind`, `mark_completed`/`mark_failed`, forward edges)
- `wacp-types` (`Task`, `TaskId`, `CheckpointId`, `TaskStatus`)
- `wacp-fsm` (`TaskTrigger`)

## Types

### New: `DependencyOutput`

```rust
pub struct DependencyOutput {
    pub task_id: TaskId,
    pub task_name: String,
    pub checkpoint_ref: CheckpointId,
}
```

### New: `TaskContext`

```rust
pub struct TaskContext {
    pub dependency_outputs: Vec<DependencyOutput>,
}
```

### New: `RetryPolicy`

```rust
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub retry_on_timeout: bool,
    pub retry_on_budget: bool,
    pub retry_on_agent_failure: bool,
}
```

### New: `SchedulingOps`

A stateless operations struct providing scheduling methods on `TaskGraph`:

```rust
pub struct SchedulingOps;
```

## Functions

### `SchedulingOps::assemble_context(graph: &TaskGraph, task_id: &TaskId) -> TaskContext`

Collect completed dependency outputs. For each dep in `task.depends_on`: if the dep has a `checkpoint_ref`, include it as a `DependencyOutput`. Returns empty context if no deps or no checkpoint refs.

### `SchedulingOps::should_retry(policy: &RetryPolicy, task: &Task, reason: &str) -> bool`

Check retry eligibility: attempts < max_attempts, failure reason permitted by policy flags.

### `SchedulingOps::prepare_retry(graph: &mut TaskGraph, task_id: &TaskId) -> Result<(), GraphError>`

Unbind task from workspace, transition `Failed → Pending` (via existing FSM — uses `Assign` since current FSM has `Failed → Assigned`). The task becomes dispatchable again.

Note: the current FSM supports `Failed → Assigned` not `Failed → Pending`. We'll use unbind + transition to Assigned as the retry path until the FSM is extended.

### `SchedulingOps::cancel_task(graph: &mut TaskGraph, task_id: &TaskId) -> Vec<TaskId>`

Transition task to Cancelled. Return list of dependents that were also cancelled (cascade). Cascade only cancels Draft/Pending dependents — in-progress tasks are left running.

### `SchedulingOps::add_subtasks(graph: &mut TaskGraph, parent_task_id: &TaskId, subtasks: Vec<Task>) -> Result<Vec<TaskId>, GraphError>`

Insert subtasks with `parent_task` set. Validates parent exists. Returns the inserted task ids. Each subtask enters the graph in its given status (typically Draft).

## Tests

| Test | Verifies |
|------|----------|
| `assemble_context_collects_deps` | Context contains dependency outputs with checkpoint refs |
| `assemble_context_skips_no_checkpoint` | Deps without checkpoint_ref are excluded |
| `assemble_context_empty_no_deps` | Task with no deps gets empty context |
| `should_retry_within_limit` | First failure → retry allowed |
| `should_retry_exceeds_limit` | Too many attempts → retry denied |
| `should_retry_timeout_denied` | Timeout failure with retry_on_timeout=false → denied |
| `should_retry_agent_allowed` | Agent failure with retry_on_agent_failure=true → allowed |
| `prepare_retry_unbinds` | After retry, task has no workspace_ref |
| `cancel_task_transitions` | Task status becomes Cancelled |
| `cancel_cascades_to_pending_dependents` | Pending dependents of cancelled task are also cancelled |
| `cancel_does_not_cascade_to_in_progress` | InProgress dependents are left running |
| `add_subtasks_sets_parent` | Subtasks have parent_task set correctly |
| `add_subtasks_validates_parent` | Nonexistent parent → error |
| `add_subtasks_in_graph` | Subtasks appear in graph and are retrievable |

## Acceptance Criteria

- Context assembly collects dependency checkpoint refs.
- Retry policy limits attempts and respects failure reason.
- Cancellation cascades to unreachable Draft/Pending dependents only.
- Subtask insertion sets parent_task and validates parent existence.
- All 14 tests pass.
- `cargo clippy` clean.
