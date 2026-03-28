# Task 16.3: Unbind/Bind + Rollback

## Scope

Implement migration-aware bind validation in the coordinator handler, token lifecycle management for agent replacement, and the rollback path that transitions workspaces to Failed on migration failure. This task wires the migration coordinator into the request handler's bind flow and the orchestrator's event processing.

## Dependencies

- Task 16.1 (MigrationCoordinator)
- Task 16.2 (MigrationSnapshot, updated CoordinatorCommand)

## Crates

- `wacp-coordinator` — handler changes, orchestrator integration

## Handler Changes

### Migration-Aware Bind

The `RequestHandler` gains a reference to `MigrationCoordinator` for bind-time validation:

```rust
pub struct RequestHandler<'a> {
    pub topology: &'a mut TopologySet,
    pub task_graph: &'a mut TaskGraph,
    pub gate_controller: &'a mut GateController,
    pub integration_queue: &'a IntegrationQueue,
    pub timeout_tracker: &'a TimeoutTracker,
    pub liveness_monitor: &'a LivenessMonitor,
    pub migration: &'a MigrationCoordinator,   // NEW
    next_envelope_id: &'a mut u64,
    next_checkpoint_id: &'a mut u64,
}
```

### `handle_bind` — Updated

```rust
pub fn handle_bind(
    &self,
    workspace_id: &str,
    agent_identity: Option<&str>,
) -> Result<BindResult, HandlerError> {
    let node = self.topology.tree.get(workspace_id)
        .ok_or(HandlerError::WorkspaceNotFound(workspace_id.to_string()))?;

    // Migration identity check: if workspace is migrating,
    // only the expected new agent may bind.
    if node.status == WorkspaceState::Migrating {
        if let Some(expected) = self.migration.expected_agent(workspace_id) {
            match agent_identity {
                Some(identity) if identity == expected.agent_type => {
                    // Correct agent — proceed with bind
                }
                _ => {
                    return Err(HandlerError::PermissionDenied(
                        "migration: agent identity mismatch".to_string(),
                    ));
                }
            }
        }
    }

    Ok(BindResult {
        workspace_id: node.id.clone(),
        state: node.status,
        role: String::new(),
        owner: node.owner.clone(),
        originator: node.originator.clone(),
        parent: node.parent.clone(),
        task_id: node.task_id.clone(),
    })
}
```

### New Error Variant

```rust
pub enum HandlerError {
    WorkspaceNotFound(String),
    NoSendRight(String),
    PermissionDenied(String),   // NEW — migration identity mismatch
}
```

## Orchestrator Integration

### Event Processing — MigrationSnapshot

```rust
WorkspaceEvent::MigrationSnapshot { workspace_id, snapshot } => {
    let _ = self.migration.set_snapshot(workspace_id.as_ref(), snapshot);
}
```

### Bind Processing — Migration Completion

When the handler accepts a bind for a migrating workspace, the orchestrator sends `MigrationComplete` to the workspace actor:

```rust
// In the agent request processing loop:
AgentRequest::Bind { request, reply } => {
    let result = handler.handle_bind(&request.workspace_id, Some(&request.agent_identity));
    if result.is_ok() && self.migration.is_migrating(&request.workspace_id) {
        // Migration bind succeeded — send MigrationComplete to workspace actor
        if let Some(ctx) = self.migration.get(&request.workspace_id) {
            let restore_blocked = ctx.pre_migration_state == WorkspaceState::Blocked;
            if let Some(handle) = self.workspace_handles.get(request.workspace_id.as_ref()) {
                let _ = handle.coordinator_tx.send(
                    CoordinatorCommand::MigrationComplete { restore_blocked }
                ).await;
            }
        }
    }
    let _ = reply.send(result);
}
```

### Rollback — `fail_migration` helper on Coordinator

```rust
impl Coordinator {
    pub async fn fail_migration(&mut self, workspace_id: &WorkspaceId, error: String, step: u32) -> Option<MigrationContext> {
        let ctx = self.migration.fail(workspace_id.as_ref(), error, step).ok()?;
        // Transition workspace to Failed
        if let Some(handle) = self.workspace_handles.get(workspace_id.as_ref()) {
            let _ = handle.coordinator_tx.send(CoordinatorCommand::Abort).await;
        }
        Some(ctx)
    }
}
```

The workspace actor receives Abort, transitions from Migrating → Failed (FSM: `(Migrating, MigrationFailed) → Failed`). This fires the standard `StateChanged` event and then `Terminated` event, which the orchestrator processes normally.

Wait — `CoordinatorAbort` on Migrating is not defined in the FSM. Let me check...

Looking at the FSM:
```
(Migrating, MigrationFailed) => Ok(Failed)
```

There's no `(Migrating, CoordinatorAbort) → Failed`. The abort trigger needs to work in Migrating state. Two options:
1. Add `(Migrating, CoordinatorAbort) → Failed` to the FSM
2. Use `MigrationFailed` trigger instead of `Abort`

**Decision:** Add a new `CoordinatorCommand::MigrationFailed` command that triggers `WorkspaceTrigger::MigrationFailed`. The Abort command is for external abort (parent failure, etc.) — migration failure is a distinct concept.

### New CoordinatorCommand

```rust
pub enum CoordinatorCommand {
    // ... existing ...
    MigrationFailed,   // NEW — triggers MigrationFailed transition (Migrating → Failed)
}
```

### Updated Rollback

```rust
pub async fn fail_migration(&mut self, workspace_id: &WorkspaceId, error: String, step: u32) -> Option<MigrationContext> {
    let ctx = self.migration.fail(workspace_id.as_ref(), error, step).ok()?;
    if let Some(handle) = self.workspace_handles.get(workspace_id.as_ref()) {
        let _ = handle.coordinator_tx.send(CoordinatorCommand::MigrationFailed).await;
    }
    Some(ctx)
}
```

### FSM Addition for Abort During Migration

Also add abort support during migration (§9.2 — abort takes priority):

```rust
(Migrating, CoordinatorAbort) => Ok(Failed),
```

This handles the case where a parent workspace fails during migration, triggering cascade abort.

## Token Lifecycle

Token management is an integration point — the coordinator calls the authenticator during migration:

### Unbind (Step 5)

```rust
// authenticator.revoke_agent(&workspace_id) — called by coordinator
// No new types needed — PskAuthenticator.revoke_agent already exists
```

### Bind (Step 6)

```rust
// authenticator.register_agent(&workspace_id, &role) -> new_token
// Token passed to new agent through launch mechanism (out of scope)
```

The coordinator holds an `Arc<dyn Authenticator>` or receives auth operations through a trait. For this task, the migration coordinator exposes the token lifecycle as method calls that the orchestrator dispatches to the authenticator.

## Tests

| Test | Verifies |
|------|----------|
| `bind_normal_workspace` | Bind to non-migrating workspace succeeds without identity check |
| `bind_migrating_correct_agent` | Bind to migrating workspace with matching agent_type succeeds |
| `bind_migrating_wrong_agent` | Bind to migrating workspace with wrong identity returns PermissionDenied |
| `bind_migrating_no_identity` | Bind to migrating workspace with no identity returns PermissionDenied |
| `fail_migration_sends_migration_failed` | fail_migration sends MigrationFailed command to workspace actor |
| `fail_migration_removes_context` | After fail_migration, is_migrating returns false |
| `fail_migration_not_migrating` | fail_migration for non-migrating workspace returns None |
| `migration_snapshot_event_stored` | MigrationSnapshot event → migration.set_snapshot() called |
| `bind_triggers_migration_complete` | Successful bind during migration → MigrationComplete sent to actor |
| `bind_restores_active` | Migration from Active → MigrationComplete { restore_blocked: false } |
| `bind_restores_blocked` | Migration from Blocked → MigrationComplete { restore_blocked: true } |
| `fsm_migrating_abort` | `(Migrating, CoordinatorAbort) → Failed` |
| `fsm_migrating_migration_failed` | `(Migrating, MigrationFailed) → Failed` (existing, verify) |
| `abort_during_migration_cancels` | Abort during migration → Failed, migration context removed |

## Acceptance Criteria

- Handler validates agent identity on bind to migrating workspace.
- Only the expected new agent can bind to a migrating workspace.
- Successful bind to migrating workspace triggers MigrationComplete command to workspace actor.
- `fail_migration` sends MigrationFailed to workspace actor and removes migration context.
- Abort during migration works (cascade abort from parent failure).
- Token lifecycle methods are called at the right points (revoke on unbind, register on bind).
- All existing handler and orchestrator tests pass (updated for new handler constructor).
- `cargo clippy` clean.
