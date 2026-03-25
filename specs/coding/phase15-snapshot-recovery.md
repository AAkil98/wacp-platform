# Task 15.2: Snapshot-Accelerated Recovery

## Scope

Extend `RecoveryEngine` to load system snapshots before trail replay. When a valid snapshot exists, recovery starts from the snapshot's anchor sequence instead of entry 0. Fallback to full replay when no valid snapshot exists or the snapshot is corrupt.

## Functions

### `recover_with_snapshot`

```rust
pub fn recover_with_snapshot(
    trail: &dyn TrailStorage,
    snapshots: &dyn SnapshotStorage,
) -> Result<RecoveredState, RecoveryError>
```

1. Check `snapshots.read_latest_system()`.
2. If found: deserialize `SystemSnapshot`, set replay start to `sequence + 1`.
3. Replay trail entries from start point.
4. If no snapshot or corrupt: fall back to `recover(trail)`.

### `RecoveredState` extension

Add optional `snapshot_sequence: Option<u64>` to indicate which snapshot was loaded.

## Tests

| Test | Verifies |
|------|----------|
| `recovery_with_snapshot_skips_entries` | Snapshot at seq 5, trail has 10 entries → only entries 6–10 replayed |
| `recovery_without_snapshot` | No snapshot → full replay (same as before) |
| `recovery_corrupt_snapshot_fallback` | Corrupt snapshot data → falls back to full replay |
| `recovery_snapshot_state_loaded` | SystemSnapshot fields deserialize into RecoveredState |
| `recovery_empty_trail_with_snapshot` | Empty trail + snapshot → state from snapshot only |

## Acceptance Criteria

- Recovery with valid snapshot produces same final state as full replay.
- Corrupt snapshot triggers fallback, not error.
- `snapshot_sequence` field in RecoveredState records which snapshot was used.
- All existing recovery tests still pass.
- `cargo clippy` clean.
