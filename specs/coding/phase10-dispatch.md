# Task 10.3: Dispatch + Resource Allocation

## Scope

Add `Dispatcher` to `wacp-coordinator` — selects dispatchable tasks, allocates budgets, creates workspaces via `TopologySet`, binds tasks. Includes capacity check and budget derivation from task descriptions.

**Does NOT produce:** Context assembly from dependency outputs (10.4). Actual workspace actor spawning (that's the orchestrator's concern). tokio timers for capacity polling.

## Dependencies

- Task 10.1 (task graph with `dispatchable`, `bind`)
- Task 9.5 (`TopologySet` with `create_workspace`)
- `wacp-types` (`ResourceBudget`, `WorkspaceId`, `TaskId`, `Originator`)

## Types

### New: `DispatchConfig`

```rust
pub struct DispatchConfig {
    pub max_concurrent_workspaces: Option<usize>,
    pub default_budget: ResourceBudget,
    pub budget_margin: f32,  // e.g., 0.2 for 20% margin over estimate
}
```

### New: `DispatchAction`

What the coordinator should do after dispatch planning:

```rust
pub struct DispatchAction {
    pub task_id: TaskId,
    pub workspace_id: WorkspaceId,
    pub parent: WorkspaceId,
    pub budget: ResourceBudget,
}
```

### New: `Dispatcher`

```rust
pub struct Dispatcher {
    config: DispatchConfig,
    next_ws_id: u64,
}
```

## Functions

### `Dispatcher::new(config: DispatchConfig) -> Self`

### `try_dispatch(&mut self, graph: &mut TaskGraph, topo: &mut TopologySet) -> Vec<DispatchAction>`

Main entry: collect dispatchable tasks, prioritize (creation order), check capacity, create workspace + bind for each. Returns the actions taken (for trail recording and actor spawning by the orchestrator).

### `select_parent(graph: &TaskGraph, task: &Task, root: &WorkspaceId) -> WorkspaceId`

Subtask → parent workspace. Root-level task → coordinator root.

### `allocate_budget(&self, default: &ResourceBudget) -> ResourceBudget`

Apply margin to the default budget. Returns a budget for the new workspace.

### `has_capacity(topo: &TopologySet, max: Option<usize>) -> bool`

Check active workspace count against configured maximum.

### `generate_workspace_id(&mut self) -> WorkspaceId`

Monotonic id generation.

## Tests

| Test | Verifies |
|------|----------|
| `dispatch_single_task` | One dispatchable task → one DispatchAction with workspace created and task bound |
| `dispatch_multiple_tasks` | Multiple dispatchable → all dispatched in order |
| `dispatch_respects_capacity` | With max_concurrent=1, only one task dispatched even if two are dispatchable |
| `dispatch_skips_non_dispatchable` | Draft and InProgress tasks are not dispatched |
| `dispatch_binds_task_to_workspace` | After dispatch, task_for_workspace returns the task id |
| `dispatch_creates_workspace_in_topology` | After dispatch, workspace exists in tree with correct parent |
| `select_parent_root_task` | Root-level task → coordinator root workspace |
| `select_parent_subtask` | Subtask → workspace of parent task |
| `allocate_budget_applies_margin` | Budget with 20% margin scales limits correctly |
| `allocate_budget_unlimited_stays_unlimited` | None limits remain None after margin |
| `has_capacity_unlimited` | No max → always true |
| `has_capacity_at_limit` | At max → returns false |

## Acceptance Criteria

- Dispatcher collects dispatchable tasks and creates workspaces.
- Capacity limit enforced.
- Budget allocation with configurable margin.
- Task bound to workspace after dispatch.
- All 12 tests pass.
- `cargo clippy` clean.
