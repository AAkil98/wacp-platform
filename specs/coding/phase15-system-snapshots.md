# Task 15.1: System Snapshots

## Scope

Implement `FileSnapshotStorage` (filesystem backend for the existing `SnapshotStorage` trait), add `Serialize`/`Deserialize` derives to all coordinator state types, and add `capture_snapshot()` to the coordinator that serializes its full state.

Snapshot format: 32-byte SHA-256 checksum prefix + JSON payload. Workspace snapshots: `snapshots/ws-<id>.snapshot`. System snapshots: `snapshots/system-<seq>.snapshot` + `system-latest.snapshot` symlink.

## Dependencies

- `sha2` (already in wacp-trail deps)
- `serde`/`serde_json` (already in wacp-trail and wacp-coordinator needs serde added)

## Types

### `FileSnapshotStorage`

```rust
pub struct FileSnapshotStorage {
    dir: PathBuf,
}
```

Implements `SnapshotStorage`. File layout follows storage.md §7.

### `SystemSnapshot`

```rust
#[derive(Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub sequence: u64,
    pub tree: serde_json::Value,
    pub task_graph: serde_json::Value,
    pub topology: serde_json::Value,
}
```

Defined in wacp-coordinator. Used by capture_snapshot() and recovery.

## Tests

| Test | Verifies |
|------|----------|
| `fs_snapshot_workspace_write_read` | Write workspace snapshot, read back matches |
| `fs_snapshot_workspace_overwrite` | Second write overwrites first |
| `fs_snapshot_workspace_delete` | Delete removes file, read returns None |
| `fs_snapshot_workspace_not_found` | Read nonexistent workspace returns None |
| `fs_snapshot_system_write_read` | Write system snapshot, read back matches |
| `fs_snapshot_system_latest` | read_latest_system returns most recent |
| `fs_snapshot_system_checksum` | Corrupt checksum detected on read |
| `coordinator_capture_snapshot` | capture_snapshot produces valid JSON |
| `snapshot_roundtrip` | Serialize coordinator state → write → read → deserialize → matches |

## Acceptance Criteria

- `FileSnapshotStorage` implements all 5 trait methods.
- Checksum verification on every read. Corrupt files return error, not garbage.
- Coordinator types (WorkspaceTree, WorkspaceNode, TaskGraph, VisibilityGraph, EscalationRouter, PortRightsGraph, PortRightEntry, PortRightStatus, TopologySet) all derive Serialize + Deserialize.
- `capture_snapshot()` serializes tree + task_graph + topology as JSON.
- All existing coordinator tests still pass.
- `cargo clippy` clean.
