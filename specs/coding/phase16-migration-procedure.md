# Task 16.1: Migration Procedure

## Scope

Implement `MigrationCoordinator` in wacp-coordinator — the state tracker and orchestrator for agent migration. This module owns the `active_migrations` map, validates preconditions, tracks migration timeouts, and provides start/complete/fail lifecycle methods. Pure state — no async, no tokio dependency. The coordinator orchestrator integrates it.

## Dependencies

- wacp-types (WorkspaceState, WorkspaceId, UserId)
- wacp-workspace (MigrationSnapshot — defined in task 16.2)
- serde (Serialize, Deserialize for snapshot types in MigrationContext)

## Crate

`wacp-coordinator` — new module `migration.rs`

## Types

### `AgentRef`

```rust
/// Identifies a replacement agent. Opaque to the runtime.
#[derive(Debug, Clone)]
pub struct AgentRef {
    pub agent_type: String,
    pub config: Option<Vec<u8>>,
}
```

### `MigrationRequest`

```rust
/// Coordinator-internal migration request.
#[derive(Debug)]
pub struct MigrationRequest {
    pub workspace_id: WorkspaceId,
    pub new_agent: AgentRef,
    pub reason: String,
}
```

### `MigrationContext`

```rust
/// Tracks a single in-progress migration.
#[derive(Debug)]
pub struct MigrationContext {
    pub workspace_id: WorkspaceId,
    pub pre_migration_state: WorkspaceState,
    pub old_agent: String,
    pub new_agent: AgentRef,
    pub reason: String,
    pub snapshot: Option<MigrationSnapshot>,
    pub started_at_ms: u64,
}
```

### `MigrationError`

```rust
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(String),

    #[error("invalid state for migration: {0:?} (must be Active or Blocked)")]
    InvalidState(WorkspaceState),

    #[error("workspace already migrating: {0}")]
    AlreadyMigrating(String),

    #[error("workspace not migrating: {0}")]
    NotMigrating(String),

    #[error("snapshot already set for: {0}")]
    SnapshotAlreadySet(String),
}
```

### `MigrationCoordinator`

```rust
pub struct MigrationCoordinator {
    active: HashMap<String, MigrationContext>,
    timeout_ms: u64,
}
```

## Methods

### `MigrationCoordinator`

| Method | Signature | Behavior |
|--------|-----------|----------|
| `new` | `(timeout_ms: u64) -> Self` | Default timeout 60_000ms |
| `start` | `(&mut self, request: MigrationRequest, current_state: WorkspaceState, old_agent: String, now_ms: u64) -> Result<&MigrationContext, MigrationError>` | Validates: workspace not already migrating, state is Active or Blocked. Creates MigrationContext with `started_at_ms = now_ms`, inserts into map, returns reference |
| `get` | `(&self, workspace_id: &str) -> Option<&MigrationContext>` | Lookup |
| `get_mut` | `(&mut self, workspace_id: &str) -> Option<&mut MigrationContext>` | Mutable lookup |
| `set_snapshot` | `(&mut self, workspace_id: &str, snapshot: MigrationSnapshot) -> Result<(), MigrationError>` | Sets snapshot on context. Errors: NotMigrating, SnapshotAlreadySet |
| `complete` | `(&mut self, workspace_id: &str) -> Result<MigrationContext, MigrationError>` | Removes and returns context. Error: NotMigrating |
| `fail` | `(&mut self, workspace_id: &str, error: String, step: u32) -> Result<MigrationContext, MigrationError>` | Removes and returns context (error + step stored on returned context for trail event). Error: NotMigrating |
| `check_timeouts` | `(&self, now_ms: u64) -> Vec<WorkspaceId>` | Returns workspace IDs where `now_ms - started_at_ms >= timeout_ms` |
| `is_migrating` | `(&self, workspace_id: &str) -> bool` | Map contains key |
| `active_count` | `(&self) -> usize` | Map length |
| `expected_agent` | `(&self, workspace_id: &str) -> Option<&AgentRef>` | Returns the expected new agent for identity verification during bind |

## Orchestrator Integration

Add `migration: MigrationCoordinator` field to `Coordinator`. Initialize with configurable timeout. The orchestrator will use it in the event loop to:
- Call `start()` when migration is requested
- Call `set_snapshot()` when MigrationSnapshot event arrives from workspace actor
- Call `complete()` after sending MigrationComplete command and receiving state change confirmation
- Call `fail()` on timeout or error
- Call `check_timeouts()` periodically

## Tests

| Test | Verifies |
|------|----------|
| `start_active_workspace` | start() succeeds for Active state, context stored |
| `start_blocked_workspace` | start() succeeds for Blocked state, pre_migration_state preserved |
| `start_invalid_state_idle` | start() rejects Idle with InvalidState |
| `start_invalid_state_suspended` | start() rejects Suspended with InvalidState |
| `start_invalid_state_migrating` | start() rejects Migrating with InvalidState |
| `start_invalid_state_terminal` | start() rejects Closed/Failed with InvalidState |
| `start_already_migrating` | Second start() for same workspace returns AlreadyMigrating |
| `set_snapshot` | set_snapshot() stores snapshot on context |
| `set_snapshot_not_migrating` | set_snapshot() for non-migrating workspace returns NotMigrating |
| `set_snapshot_already_set` | set_snapshot() when snapshot exists returns SnapshotAlreadySet |
| `complete_returns_context` | complete() removes and returns context with all fields |
| `complete_not_migrating` | complete() for non-migrating workspace returns NotMigrating |
| `fail_returns_context` | fail() removes and returns context |
| `check_timeouts_expired` | Workspace past timeout_ms appears in check_timeouts() |
| `check_timeouts_not_expired` | Workspace within timeout_ms does not appear |
| `parallel_migrations` | Two different workspaces can migrate concurrently |
| `expected_agent` | Returns correct AgentRef for migrating workspace, None for non-migrating |

## Acceptance Criteria

- `MigrationCoordinator` is a pure state tracker — no async, no IO.
- Only Active and Blocked states accepted for migration start.
- One migration per workspace enforced.
- Parallel migrations of different workspaces permitted.
- `complete()` and `fail()` consume the migration context (remove from map).
- Timeout check is a pure query — does not mutate state.
- All existing coordinator tests pass.
- `cargo clippy` clean.
