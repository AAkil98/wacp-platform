# Task 10.1: Task Graph Enhancement

## Scope

Add readiness counters, forward adjacency list, task-workspace bidirectional binding, `mark_completed`/`mark_failed` methods, `dispatchable` query, and workspace-to-task status propagation. Replace O(n*k) `ready_tasks` scan with O(1) counter-based readiness.

**Does NOT produce:** Gate enforcement (10.2), dispatch policy (10.3), context assembly (10.4).

## Dependencies

- `wacp-types` (`Task`, `TaskId`, `WorkspaceId`, `TaskStatus`)
- `wacp-fsm` (`TaskFsm`, `TaskTrigger`)
- Phase 9 complete

## Types

### Modified: `TaskGraph`

Add fields:

```rust
pub struct TaskGraph {
    tasks: HashMap<String, Task>,
    remaining_deps: HashMap<String, u32>,            // NEW — readiness counter
    forward: HashMap<String, Vec<TaskId>>,            // NEW — task → dependents
    task_workspace: HashMap<String, WorkspaceId>,     // NEW — task → workspace
    workspace_task: HashMap<String, TaskId>,           // NEW — workspace → task
}
```

### New: `GraphError` variants

```rust
pub enum GraphError {
    // existing...
    TaskNotFound(String),
    WorkspaceAlreadyBound(WorkspaceId),
    TaskAlreadyBound(TaskId),
}
```

## Functions

### New: `mark_completed(&mut self, id: &TaskId) -> Vec<TaskId>`

Decrement `remaining_deps` for all dependents (via `forward[id]`). Return task ids whose counter reached zero (newly ready). Does NOT change the completed task's status — caller does that via `transition`.

### New: `mark_failed(&mut self, id: &TaskId) -> Vec<TaskId>`

Return dependents of the failed task (they are now blocked). Advisory — caller decides whether to cancel, retry, or escalate.

### New: `bind(&mut self, task_id: &TaskId, workspace_id: &WorkspaceId) -> Result<(), GraphError>`

Set `task.workspace_ref`, add to `workspace_history`, record in both binding maps. Error if task or workspace already bound.

### New: `unbind(&mut self, task_id: &TaskId)`

Clear `task.workspace_ref`, remove from both binding maps. Called on workspace failure before retry.

### New: `task_for_workspace(&self, workspace_id: &WorkspaceId) -> Option<&TaskId>`

Reverse lookup: workspace → task.

### New: `dispatchable(&self) -> Vec<&TaskId>`

Tasks in `Pending` status with `remaining_deps == 0`. O(n) scan but simple — no dependency checking needed.

### Modified: `add_task`

On insertion, compute `remaining_deps` from dep count (counting only non-terminal deps). Build `forward` edges from each dependency to the new task.

### Modified: `ready_tasks`

Replace O(n*k) scan with counter-based: `Pending` + `remaining_deps == 0`. (Equivalent to `dispatchable` — keep both for backward compatibility, `ready_tasks` delegates to `dispatchable`.)

## Tests

| Test | Verifies |
|------|----------|
| `remaining_deps_set_on_insert` | Task with 2 deps has remaining_deps == 2 |
| `remaining_deps_zero_for_no_deps` | Task with no deps has remaining_deps == 0 |
| `remaining_deps_skips_terminal` | Task depending on an Integrated task starts with remaining_deps == 0 for that dep |
| `mark_completed_decrements` | After dep completes, dependent's counter decremented |
| `mark_completed_returns_newly_ready` | Dependent with counter reaching 0 is returned |
| `mark_completed_not_ready_if_other_deps` | Dependent with 2 deps, only 1 completed, not returned |
| `mark_failed_returns_dependents` | Returns list of dependents that are now blocked |
| `bind_sets_both_maps` | After bind, both task_workspace and workspace_task populated |
| `bind_duplicate_task_rejected` | Binding an already-bound task returns error |
| `bind_duplicate_workspace_rejected` | Binding to an already-bound workspace returns error |
| `unbind_clears_both` | After unbind, both maps cleared, workspace_ref is None |
| `task_for_workspace_lookup` | Reverse lookup works after bind |
| `dispatchable_uses_counter` | Pending task with remaining_deps == 0 is dispatchable |
| `dispatchable_excludes_non_pending` | Draft task with remaining_deps == 0 is NOT dispatchable |
| `forward_edges_built_on_insert` | After inserting t2 depending on t1, forward[t1] contains t2 |

## Acceptance Criteria

- Readiness is O(1) per completion event (counter decrement), not O(n*k) scan.
- Task-workspace binding is bidirectional and enforced (no double-binding).
- `mark_completed` returns newly ready tasks.
- All 15 new tests pass.
- All existing task graph tests pass.
- `cargo clippy` clean.
