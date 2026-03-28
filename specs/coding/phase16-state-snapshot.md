# Task 16.2: State Snapshot + Restore

## Scope

Add `MigrationSnapshot` type and capture/restore methods on `WorkspaceState`. Update the workspace FSM with a `MigrationSucceededBlocked` trigger for returning to Blocked state after migration. Update `CoordinatorCommand` and `WorkspaceActor` to support the migration command flow: `MigrateBegin` triggers snapshot capture and emission, `MigrationComplete` restores the pre-migration state. Guard agent messages in Migrating state.

## Dependencies

- serde (Serialize, Deserialize on MigrationSnapshot and its components)

## Crates

- `wacp-workspace` — MigrationSnapshot type, capture/restore, actor changes
- `wacp-fsm` — new trigger + transition

## Types

### `MigrationSnapshot` (wacp-workspace)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSnapshot {
    pub inbox: Vec<Envelope>,
    pub working_memory: Vec<u8>,
    pub checkpoint_register: Vec<Checkpoint>,
    pub resource_meter: ResourceMeter,
    pub trail_sequence: u64,
    pub delivered_envelope_ids: HashSet<String>,
}
```

All contained types already derive Serialize + Deserialize (Envelope, Checkpoint, ResourceMeter, ResourceUsage, ResourceBudget in wacp-types). If any lack derives, add them.

## FSM Changes (wacp-fsm)

### New trigger

```rust
pub enum WorkspaceTrigger {
    // ... existing triggers ...
    MigrationSucceededBlocked,  // NEW — return to Blocked after migration
}
```

### New transition

```rust
// Existing:
(Migrating, MigrationSucceeded) => Ok(Active),
(Migrating, MigrationFailed) => Ok(Failed),

// New:
(Migrating, MigrationSucceededBlocked) => Ok(Blocked),
```

The coordinator picks the trigger based on `MigrationContext.pre_migration_state`:
- Active → `MigrationSucceeded`
- Blocked → `MigrationSucceededBlocked`

## WorkspaceState Methods (wacp-workspace)

### `capture_snapshot`

```rust
pub fn capture_snapshot(&self) -> MigrationSnapshot {
    MigrationSnapshot {
        inbox: self.inbox.iter().cloned().collect(),
        working_memory: self.working_memory.clone(),
        checkpoint_register: self.checkpoint_register.clone(),
        resource_meter: self.resource_meter.clone(),
        trail_sequence: self.trail_sequence,
        delivered_envelope_ids: self.delivered_envelope_ids.clone(),
    }
}
```

Captures the five mutable components. The four immutable components (directive, context, visibility, authority) are unchanged by migration and do not need capture.

### `restore_from_snapshot`

```rust
pub fn restore_from_snapshot(
    config: WorkspaceConfig,
    snapshot: MigrationSnapshot,
    restore_status: wacp_types::WorkspaceState,
) -> Self {
    Self {
        id: config.id,
        status: restore_status,
        role: config.role,
        base_role: config.base_role,
        parent: config.parent,
        owner: config.owner,
        originator: config.originator,
        delegate: config.delegate,
        directive: config.directive,
        inbox: snapshot.inbox.into(),
        context: config.context,
        working_memory: snapshot.working_memory,
        checkpoint_register: snapshot.checkpoint_register,
        resource_meter: snapshot.resource_meter,
        trail_sequence: snapshot.trail_sequence,
        visibility_set: config.visibility,
        authority_set: config.authority,
        delivered_envelope_ids: snapshot.delivered_envelope_ids,
    }
}
```

Used for crash recovery reconstruction. During normal migration, the workspace actor persists — no replacement needed. `restore_from_snapshot` is the recovery path.

## CoordinatorCommand Changes

```rust
pub enum CoordinatorCommand {
    Abort,
    Suspend,
    Resume,
    MigrateBegin,                                    // renamed from Migrate
    MigrationComplete { restore_blocked: bool },     // NEW
    GrantVisibility(Vec<String>),
    UpdateBudget(ResourceBudget),
    DeliverEnvelope(Envelope),
    GracefulTermination { grace_period_ms: u64 },
}
```

- `MigrateBegin` replaces `Migrate` — triggers `CoordinatorMigrate`, then captures and emits snapshot.
- `MigrationComplete { restore_blocked }` — triggers `MigrationSucceeded` (if false) or `MigrationSucceededBlocked` (if true).

## WorkspaceEvent Changes

```rust
pub enum WorkspaceEvent {
    Signal(Signal),
    StateChanged { workspace_id: WorkspaceId, from: wacp_types::WorkspaceState, to: wacp_types::WorkspaceState },
    Terminated(Box<ArchivedWorkspace>),
    CheckpointCreated(Checkpoint),
    MigrationSnapshot { workspace_id: WorkspaceId, snapshot: MigrationSnapshot },  // NEW
    Error { workspace_id: WorkspaceId, message: String },
}
```

## WorkspaceActor Changes

### `handle_coordinator_cmd` — MigrateBegin

```rust
CoordinatorCommand::MigrateBegin => {
    self.transition(WorkspaceTrigger::CoordinatorMigrate).await;
    // Only capture if transition succeeded
    if self.state.status == wacp_types::WorkspaceState::Migrating {
        let snapshot = self.state.capture_snapshot();
        let _ = self.event_tx.send(WorkspaceEvent::MigrationSnapshot {
            workspace_id: self.state.id.clone(),
            snapshot,
        }).await;
    }
}
```

### `handle_coordinator_cmd` — MigrationComplete

```rust
CoordinatorCommand::MigrationComplete { restore_blocked } => {
    let trigger = if restore_blocked {
        WorkspaceTrigger::MigrationSucceededBlocked
    } else {
        WorkspaceTrigger::MigrationSucceeded
    };
    self.transition(trigger).await;
}
```

### `handle_agent_msg` — Migrating guard

```rust
async fn handle_agent_msg(&mut self, msg: AgentMessage) {
    // Reject agent messages in Migrating state (migration spec §4.3)
    if self.state.status == wacp_types::WorkspaceState::Migrating {
        return;
    }
    // ... existing match on msg ...
}
```

## Tests

| Test | Verifies |
|------|----------|
| `snapshot_capture_empty_workspace` | New workspace snapshot has empty inbox, empty checkpoints, zero resource usage |
| `snapshot_capture_with_state` | Workspace with inbox entries, checkpoints, working memory, resource usage captures all |
| `snapshot_capture_preserves_inbox_order` | Inbox order (priority + FIFO) preserved in snapshot |
| `snapshot_capture_preserves_dedup_set` | delivered_envelope_ids captured for dedup continuity |
| `restore_from_snapshot` | Restored state matches: inbox, working memory, checkpoints, resource meter, trail sequence, dedup set |
| `restore_status_active` | restore_from_snapshot with Active status produces Active workspace |
| `restore_status_blocked` | restore_from_snapshot with Blocked status produces Blocked workspace |
| `restore_immutable_from_config` | Restored state uses config's directive, context, visibility, authority — not snapshot |
| `fsm_migrating_to_blocked` | `(Migrating, MigrationSucceededBlocked) → Blocked` |
| `actor_migrate_begin_emits_snapshot` | MigrateBegin command → Migrating state + MigrationSnapshot event emitted |
| `actor_migrate_begin_invalid_state` | MigrateBegin from Idle → Error event, no snapshot emitted |
| `actor_migration_complete_to_active` | MigrationComplete { restore_blocked: false } → Active |
| `actor_migration_complete_to_blocked` | MigrationComplete { restore_blocked: true } → Blocked |
| `actor_agent_msg_rejected_in_migrating` | EmitSignal/CreateCheckpoint from agent while Migrating → silently dropped |
| `actor_envelopes_accepted_in_migrating` | DeliverEnvelope from coordinator still appends to inbox in Migrating state |
| `snapshot_serde_roundtrip` | Serialize → deserialize MigrationSnapshot produces identical data |

## Acceptance Criteria

- `MigrationSnapshot` captures all 5 mutable workspace components + dedup set.
- `capture_snapshot()` is a pure read — does not mutate workspace state.
- `restore_from_snapshot()` produces a workspace state with snapshot's mutable data and config's immutable data.
- FSM: `Migrating → Blocked` via `MigrationSucceededBlocked` works alongside existing `Migrating → Active`.
- Workspace actor emits `MigrationSnapshot` event immediately after successful transition to Migrating.
- Agent messages silently dropped in Migrating state.
- Coordinator envelope delivery to inbox still works in Migrating state (envelopes buffered for new agent).
- All existing workspace + FSM tests pass.
- `cargo clippy` clean.
