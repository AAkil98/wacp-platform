# Task 15.4: Retention + Compaction

## Scope

Background compaction task that runs tier transitions, merges small warm segments, cleans up unreferenced checkpoint payloads, and deletes segments beyond cold retention. Every deletion preceded by a trail entry (no-silent-deletion invariant).

## Types

### `CompactionConfig`

```rust
pub struct CompactionConfig {
    pub hot_limit: u32,
    pub warm_retention_days: u32,
    pub cold_retention_days: Option<u64>,  // None = indefinite
    pub compaction_interval_minutes: u32,
}
```

### `CompactionTask`

```rust
pub struct CompactionTask {
    tier_manager: TierManager,
    config: CompactionConfig,
}
```

## Functions

- `CompactionTask::new(config, trail_dir, cold_destination)` — create task
- `CompactionTask::run_once()` — execute one compaction cycle
- `CompactionTask::merge_warm_segments(threshold)` — merge small warm segments into larger ones
- `CompactionTask::cleanup_cold_segments()` — delete segments beyond cold retention
- `CompactionTask::cleanup_checkpoints(checkpoint_store, trail_index)` — remove unreferenced payloads

## Tests

| Test | Verifies |
|------|----------|
| `compaction_empty_trail` | run_once on empty directory succeeds |
| `compaction_triggers_hot_warm` | Hot segments beyond limit get compressed |
| `compaction_cold_deletion` | Segments beyond cold retention are deleted |
| `compaction_indefinite_retention` | cold_retention=None prevents deletion |
| `merge_warm_combines_files` | Two small warm files merge into one |

## Acceptance Criteria

- Compaction runs without errors on empty state.
- Tier transitions happen on threshold.
- Cold retention enforced (delete old segments).
- Indefinite retention prevents all deletion.
- All tests pass, clippy clean.
