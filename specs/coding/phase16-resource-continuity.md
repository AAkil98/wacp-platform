# Task 16.4: Resource Continuity

## Scope

Verify and complete resource enforcement continuity across migration. Ensure timeout timer pauses during Migrating state, liveness monitor resets on migration completion, budget meter transfers via snapshot, and inbox is preserved. Add the three migration trail event types. Wire migration timeout checking into the coordinator's periodic check cycle.

## Dependencies

- Task 16.1 (MigrationCoordinator — timeout checking)
- Task 16.2 (MigrationSnapshot — budget/inbox transfer)
- Task 16.3 (Rollback — timeout triggers fail_migration)

## Crates

- `wacp-coordinator` — resource module integration, trail event types, timeout wiring

## TimeoutTracker — Migrating State

The existing `on_state_change` already handles Migrating correctly: Migrating is not in the set {Active, Blocked, Conflicted}, so the timer pauses. When the workspace returns to Active or Blocked after migration, the timer resumes. **No code change needed** — just verification tests.

Timer behavior across migration:
1. Workspace Active (timer running) → Migrating (timer pauses, elapsed accumulated)
2. Migration in progress (timer paused — no wall time consumed)
3. Migrating → Active (timer resumes from accumulated value)

## LivenessMonitor — Migration Reset

After migration completes, the liveness clock resets to prevent false timeout from the migration gap. Add a method:

```rust
impl LivenessMonitor {
    /// Reset activity timestamp. Called after migration completion
    /// to prevent false liveness warnings from the migration gap.
    pub fn reset_activity(&mut self, workspace_id: &str, now_ms: u64) {
        if let Some(ts) = self.last_activity.get_mut(workspace_id) {
            *ts = now_ms;
        }
    }
}
```

The coordinator calls `liveness_monitor.reset_activity()` when processing the `MigrationComplete` state change event.

## Budget Meter Continuity

The resource meter is captured in `MigrationSnapshot.resource_meter` (task 16.2) and persists in the workspace actor across migration. The workspace actor is the same instance before and after migration — the meter is continuous by construction. **No code change needed** — just verification tests.

Budget behavior:
- Old agent at 79% token budget → Migrating → new agent inherits 79%
- Warning thresholds are per-crossing, not per-agent — no duplicate warning

## Inbox Continuity

The inbox (`VecDeque<Envelope>`) persists in the workspace actor. Envelopes delivered during Migrating state are appended (coordinator DeliverEnvelope still works in Migrating). After migration, the new agent receives all buffered envelopes through ReceiveEnvelopes. **No code change needed** — task 16.2 ensures DeliverEnvelope works in Migrating state.

## Trail Event Types

Three migration trail events (migration.md §10):

```rust
/// Trail event: migration started (written at step 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStartedEvent {
    pub workspace_id: WorkspaceId,
    pub old_agent: String,
    pub new_agent: String,
    pub reason: String,
    pub pre_migration_state: String,
}

/// Trail event: migration completed (written at step 7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationCompletedEvent {
    pub workspace_id: WorkspaceId,
    pub old_agent: String,
    pub new_agent: String,
    pub duration_ms: u64,
    pub restored_state: String,
}

/// Trail event: migration failed (written on failure after step 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationFailedEvent {
    pub workspace_id: WorkspaceId,
    pub old_agent: String,
    pub new_agent: String,
    pub reason: String,
    pub error: String,
    pub failed_at_step: u32,
    pub duration_ms: u64,
}
```

These are defined in `wacp-coordinator::migration` and constructed by the coordinator when processing migration lifecycle events. They are serialized as JSON payloads in trail entries with event types `migration_started`, `migration_completed`, `migration_failed`.

### Event Construction

```rust
impl MigrationCoordinator {
    /// Build trail event for migration start.
    pub fn started_event(ctx: &MigrationContext) -> MigrationStartedEvent {
        MigrationStartedEvent {
            workspace_id: ctx.workspace_id.clone(),
            old_agent: ctx.old_agent.clone(),
            new_agent: ctx.new_agent.agent_type.clone(),
            reason: ctx.reason.clone(),
            pre_migration_state: format!("{:?}", ctx.pre_migration_state),
        }
    }

    /// Build trail event for migration completion.
    pub fn completed_event(ctx: &MigrationContext, now_ms: u64) -> MigrationCompletedEvent {
        MigrationCompletedEvent {
            workspace_id: ctx.workspace_id.clone(),
            old_agent: ctx.old_agent.clone(),
            new_agent: ctx.new_agent.agent_type.clone(),
            duration_ms: now_ms.saturating_sub(ctx.started_at_ms),
            restored_state: format!("{:?}", ctx.pre_migration_state),
        }
    }

    /// Build trail event for migration failure.
    pub fn failed_event(ctx: &MigrationContext, error: &str, step: u32, now_ms: u64) -> MigrationFailedEvent {
        MigrationFailedEvent {
            workspace_id: ctx.workspace_id.clone(),
            old_agent: ctx.old_agent.clone(),
            new_agent: ctx.new_agent.agent_type.clone(),
            reason: ctx.reason.clone(),
            error: error.to_string(),
            failed_at_step: step,
            duration_ms: now_ms.saturating_sub(ctx.started_at_ms),
        }
    }
}
```

## Migration Timeout Wiring

The coordinator's periodic check cycle (same mechanism as workspace timeouts) includes migration timeouts:

```rust
// In coordinator periodic check:
let expired_migrations = self.migration.check_timeouts(now_ms);
for ws_id in expired_migrations {
    self.fail_migration(&ws_id, "migration timeout".to_string(), 6).await;
    // Trail: migration_failed event with step=6 (bind timeout)
}
```

## Tests

| Test | Verifies |
|------|----------|
| `timeout_paused_during_migrating` | Timer running in Active, transition to Migrating pauses, elapsed unchanged during gap |
| `timeout_resumes_after_migration` | Migrating → Active resumes timer from accumulated value |
| `timeout_continuous_across_migration` | Total elapsed = pre-migration active time + post-migration active time (no gap) |
| `liveness_reset_on_completion` | reset_activity updates last_activity timestamp |
| `liveness_no_false_warning_after_migration` | Migration gap does not trigger liveness warning when reset is called |
| `budget_preserved_across_migration` | Resource meter in workspace actor unchanged after MigrateBegin + MigrationComplete |
| `inbox_preserved_across_migration` | Envelopes in inbox before migration still present after MigrationComplete |
| `inbox_accepts_during_migration` | DeliverEnvelope during Migrating appends to inbox |
| `trail_event_started` | MigrationStartedEvent serializes with correct fields |
| `trail_event_completed` | MigrationCompletedEvent includes duration_ms |
| `trail_event_failed` | MigrationFailedEvent includes error, step, duration |
| `migration_timeout_triggers_failure` | Expired migration in check_timeouts → fail_migration called |
| `migration_timeout_records_step_6` | Timeout failure records failed_at_step = 6 |

## Acceptance Criteria

- Timeout timer is paused during Migrating and resumes on return to Active/Blocked. No wall time consumed during migration gap.
- Liveness monitor reset after migration completion — no false warnings from migration gap.
- Budget meter continuous — new agent inherits old agent's consumption exactly.
- Inbox continuous — all envelopes (pre-migration + during-migration) delivered to new agent.
- Three trail event types defined with all fields from migration.md §10.
- Event construction methods produce correct duration_ms from started_at_ms.
- Migration timeout integrated into coordinator's periodic check cycle.
- All existing resource enforcement tests pass.
- `cargo clippy` clean.
