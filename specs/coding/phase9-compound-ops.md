# Task 9.5: Compound Operations

## Scope

Add `TopologySet` to `wacp-coordinator` — bundles the four topology structures (tree, visibility graph, escalation router, port rights graph) and provides compound operations that span all of them atomically. Three compound operations: workspace creation, workspace termination, ownership transfer.

**Note:** `TaskGraph` remains separate — it is managed by the coordinator's scheduling logic (Phase 10), not the topology layer.

## Dependencies

- Tasks 9.1–9.4 (tree indices, visibility, ownership/causation, port rights)

## Types

### New: `TopologySet`

```rust
pub struct TopologySet {
    pub tree: WorkspaceTree,
    pub visibility: VisibilityGraph,
    pub escalation: EscalationRouter,
    pub port_rights: PortRightsGraph,
}
```

### New: `CreateWorkspaceParams`

```rust
pub struct CreateWorkspaceParams {
    pub id: WorkspaceId,
    pub parent: WorkspaceId,
    pub owner: UserId,
    pub originator: Originator,
    pub status: WorkspaceState,
    pub task_id: Option<TaskId>,
}
```

### New: `CascadeEffect`

```rust
pub struct CascadeEffect {
    pub failed: Vec<WorkspaceId>,
    pub reparented: Vec<WorkspaceId>,
    pub rights_expired: usize,
}
```

## Functions

### `TopologySet::new(root_id, root_owner) -> Self`

Initialize all structures with the root workspace: tree with root, visibility with root registered + coordinator self-entry, escalation router with root, port rights empty (root has no parent).

### `TopologySet::create_workspace(&mut self, params: CreateWorkspaceParams) -> Result<(), TreeError>`

Compound creation spanning all structures:
1. Tree: insert node
2. Visibility: register workspace, grant coordinator visibility to new workspace
3. Escalation: register workspace → owner
4. Port rights: create bidirectional send rights with parent + self receive right

### `TopologySet::terminate_workspace(&mut self, id: &WorkspaceId, status: WorkspaceState) -> CascadeEffect`

Compound termination:
1. Tree: update status
2. Port rights: expire all rights involving this workspace
3. If `Failed`: run failure cascade (tree), expire port rights for cascaded failures, update escalation router for reparented children

### `TopologySet::transfer_ownership(&mut self, id: &WorkspaceId, new_owner: UserId) -> Result<UserId, TreeError>`

Compound transfer:
1. Tree: transfer_owner (updates owner_index)
2. Escalation: update routing

## Tests

| Test | Verifies |
|------|----------|
| `create_workspace_updates_all` | After create: node in tree, registered in visibility, escalation routes to owner, 3 port rights created (parent→child send, child→parent send, child self-receive) |
| `create_workspace_coordinator_sees_child` | Coordinator (root) can see the new workspace after creation |
| `terminate_closed_expires_rights` | Terminating with Closed expires port rights, no cascade |
| `terminate_failed_cascades` | Terminating with Failed cascades same-owner children, reparents cross-owner |
| `terminate_failed_expires_cascaded_rights` | Port rights of cascaded-failed children are expired |
| `cascade_updates_escalation_router` | Reparented children still route to their owner |
| `transfer_ownership_updates_both` | After transfer: tree.by_owner reflects new owner, escalation routes to new owner |
| `create_then_terminate_lifecycle` | Full lifecycle: create → terminate → verify all structures consistent |

## Acceptance Criteria

- `TopologySet` bundles tree, visibility, escalation, port rights.
- `create_workspace` updates all 4 structures.
- `terminate_workspace` updates tree + port rights + cascade.
- `transfer_ownership` updates tree + escalation.
- All 8 tests pass.
- All existing tests continue to pass.
- `cargo clippy` clean.
