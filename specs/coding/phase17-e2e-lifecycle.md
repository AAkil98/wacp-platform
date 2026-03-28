# Task 17.1: E2E Full Lifecycle Tests

## Scope

Add missing integration/conflict CoordinatorCommands to the workspace actor (prerequisite), then write E2E integration tests that exercise the full coordinator lifecycle with spawned workspace actors.

## Prerequisite: Integration CoordinatorCommands

The FSM has triggers for IntegrationSucceeded/IntegrationFailed/ConflictDetected/ConflictResolved/ConflictUnresolvable but the workspace actor has no commands to trigger them. Add:

```rust
pub enum CoordinatorCommand {
    // ... existing ...
    IntegrationSucceeded,
    IntegrationFailed,
    ConflictDetected,
    ConflictResolved,
    ConflictUnresolvable,
}
```

Each maps directly to its FSM trigger in `handle_coordinator_cmd`.

## Crate

`wacp-workspace` (command additions), `wacp-coordinator` (E2E tests)

## Test Infrastructure

Helper to drive the coordinator event loop in tests:

```rust
async fn drain_events(
    coordinator: &mut Coordinator,
    event_rx: &mut mpsc::Receiver<WorkspaceEvent>,
    max: usize,
) -> Vec<WorkspaceEvent> { ... }
```

## Tests

| Test | Verifies |
|------|----------|
| `e2e_single_worker_lifecycle` | Dispatch → directive delivery → Active → checkpoint → Complete → Integrating → IntegrationSucceeded → Closed. Tree status matches at each step. |
| `e2e_multi_worker_parallel` | Two tasks dispatched concurrently, both complete independently, both close. |
| `e2e_delegation_subtask` | Parent dispatches, creates subtask in graph. Subtask workspace dispatched, completes, integrates. Parent workspace still active after subtask completes. |
| `e2e_workspace_survives_full_cycle` | After Closed, workspace handle removed, tree node persists with terminal status. |

## Acceptance Criteria

- 5 new CoordinatorCommand variants trigger their corresponding FSM transitions.
- Single worker E2E: full path from Idle to Closed with checkpoint creation.
- Multi-worker E2E: parallel dispatch and completion.
- All existing tests pass.
