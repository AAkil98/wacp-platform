# Task 9.4: Port Rights Graph

## Scope

Add `PortRightsGraph` to `wacp-coordinator` — a directed multigraph of send/receive/send-once rights between workspaces. Three indices (by holder, by target, by right id). Full lifecycle: create, transfer, consume (send-once), revoke, expire (terminal workspace). Envelope send validation.

**Does NOT produce:** Initial rights at workspace creation based on role (task 9.5 compound operations). Trail event recording (Phase 13 gRPC routing).

## Dependencies

- `wacp-types` (`WorkspaceId`, `PortRightType`)
- `wacp-coordinator` (existing crate)

## Types

### New: `PortRightEntry`

Extended port right with id and status, held by the graph. Distinct from `wacp_types::PortRight` which is the wire type.

```rust
pub struct PortRightEntry {
    pub id: String,
    pub holder: WorkspaceId,
    pub target: WorkspaceId,
    pub kind: PortRightType,
    pub status: PortRightStatus,
}
```

### New: `PortRightStatus`

```rust
pub enum PortRightStatus {
    Active,
    Consumed,   // send-once used
    Revoked,    // coordinator revoked
    Expired,    // holder or target terminal
}
```

### New: `PortRightError`

```rust
pub enum PortRightError {
    NotFound,
    NotActive,
    NotSendOnce,
    ReceiveNotTransferable,
    NoRights,
    NoRightToTarget(WorkspaceId),
}
```

### New: `PortRightsGraph`

```rust
pub struct PortRightsGraph {
    by_holder: HashMap<String, Vec<String>>,   // workspace → right ids
    by_target: HashMap<String, Vec<String>>,   // workspace → right ids targeting it
    by_id: HashMap<String, PortRightEntry>,    // right id → entry
    next_id: u64,                               // monotonic id generator
}
```

## Functions

- `new() -> Self`
- `create(&mut self, holder: WorkspaceId, target: WorkspaceId, kind: PortRightType) -> String` — create a right, return its id.
- `transfer(&mut self, right_id: &str, new_holder: &WorkspaceId) -> Result<WorkspaceId, PortRightError>` — transfer to new holder, return old holder. Receive rights not transferable.
- `consume(&mut self, right_id: &str) -> Result<(), PortRightError>` — consume a send-once right.
- `revoke(&mut self, right_id: &str) -> Result<(), PortRightError>` — coordinator revokes a right.
- `expire_workspace(&mut self, workspace_id: &WorkspaceId)` — expire all rights held by or targeting this workspace.
- `validate_send(&self, sender: &WorkspaceId, target: &WorkspaceId) -> Result<String, PortRightError>` — check if sender has an active send/send-once right to target, return the right id.
- `get(&self, right_id: &str) -> Option<&PortRightEntry>` — lookup by id.
- `rights_held_by(&self, holder: &WorkspaceId) -> Vec<&PortRightEntry>` — all active rights held by a workspace.
- `active_count(&self) -> usize` — count of active rights.

## Tests

| Test | Verifies |
|------|----------|
| `create_right` | Created right is active and retrievable by id |
| `validate_send_with_right` | validate_send succeeds when an active Send right exists |
| `validate_send_no_right` | validate_send fails when no right exists |
| `validate_send_send_once` | validate_send succeeds with SendOnce right |
| `consume_send_once` | consume transitions to Consumed, subsequent validate_send fails |
| `consume_non_send_once_rejected` | consume on Send right returns NotSendOnce |
| `revoke_right` | revoke transitions to Revoked, validate_send fails |
| `revoke_non_active_rejected` | revoking an already-revoked right returns NotActive |
| `transfer_send_right` | transfer moves holder, old holder loses access, new holder gains it |
| `transfer_receive_rejected` | transfer on Receive right returns ReceiveNotTransferable |
| `transfer_non_active_rejected` | transfer on revoked right returns NotActive |
| `expire_workspace_both_directions` | expire_workspace marks all rights held by and targeting the workspace as Expired |
| `rights_held_by_returns_active_only` | rights_held_by filters to active status |
| `active_count_tracks_correctly` | active_count decrements after consume/revoke/expire |
| `multiple_rights_same_pair` | two Send rights between same pair both valid |

## Acceptance Criteria

- `PortRightsGraph` compiles with all methods.
- Full lifecycle: create → transfer/consume/revoke/expire.
- `validate_send` enforces CH-1 (no right, no delivery).
- `consume` enforces CH-3 (send-once consumed after delivery).
- `transfer` enforces CH-5 (receive not transferable).
- All 15 tests pass.
- `cargo clippy` clean.
