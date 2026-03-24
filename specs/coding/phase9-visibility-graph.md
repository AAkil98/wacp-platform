# Task 9.2: Visibility Graph

## Scope

Add `VisibilityGraph` to `wacp-coordinator` — a directed graph where `A → B` means workspace A can read workspace B's state. Forward/reverse `HashSet` per node, additive-only grants, grantor-scoped grant validation, implicit self-visibility.

**Does NOT produce:** Role-based default visibility at creation (that is part of compound operations, task 9.5). Authority set management (frozen at creation, already in `WorkspaceState`).

## Dependencies

- `wacp-types` (`WorkspaceId`)
- `wacp-coordinator` (existing crate)

## Types

### New: `VisibilityGraph`

```rust
pub struct VisibilityGraph {
    /// Forward: workspace → set of workspaces it can see
    can_see: HashMap<WorkspaceId, HashSet<WorkspaceId>>,
    /// Reverse: workspace → set of workspaces that can see it
    seen_by: HashMap<WorkspaceId, HashSet<WorkspaceId>>,
}
```

### New: `VisibilityError`

```rust
pub enum VisibilityError {
    /// Grant target is not within the grantor's visibility scope
    GrantorCannotSee(WorkspaceId),
    /// Workspace not registered in the graph
    NotRegistered(WorkspaceId),
}
```

## Functions

### `new() -> Self`

Empty graph.

### `register(id: &WorkspaceId)`

Register a workspace in the graph with an empty visibility set. Called on workspace creation. Idempotent — registering an already-registered workspace is a no-op.

### `grant(viewer: &WorkspaceId, target: &WorkspaceId) -> bool`

Add a directed edge: `viewer` can see `target`. Returns `true` if newly granted, `false` if already visible. No grantor check — used by the coordinator (which has total visibility).

### `grant_checked(viewer: &WorkspaceId, target: &WorkspaceId, grantor: &WorkspaceId) -> Result<bool, VisibilityError>`

Grant with grantor scope validation: the grantor must be able to see `target`. Returns `Ok(true)` if newly granted, `Ok(false)` if already visible. Used by delegates granting visibility to their children.

### `can_see(viewer: &WorkspaceId, target: &WorkspaceId) -> bool`

Check if `viewer` can see `target`. Implicit self-visibility: returns `true` when `viewer == target` without consulting the set.

### `visible_to(viewer: &WorkspaceId) -> HashSet<WorkspaceId>`

Return the full visibility set for a workspace, including self. Returns a clone with `viewer` included.

### `who_can_see(target: &WorkspaceId) -> HashSet<WorkspaceId>`

Reverse query: all workspaces that can see `target`, including self. Returns a clone with `target` included.

### `grant_count() -> usize`

Total number of directed edges in the graph (for diagnostics).

## Internal Design

- Self-visibility is implicit per invariant VI-1 — not stored in the sets, checked in `can_see()`.
- `visible_to` and `who_can_see` return clones with self included (the caller gets the complete picture including the implicit self edge).
- No `revoke` method — visibility is additive only (invariant VI-2).
- Both `can_see` and `seen_by` maps are maintained on every `grant` for O(1) lookups in both directions.

## Tests

| Test | Verifies |
|------|----------|
| `self_visibility_implicit` | `can_see(A, A)` is true without any grant |
| `grant_creates_edge` | After `grant(A, B)`, `can_see(A, B)` is true |
| `grant_not_symmetric` | After `grant(A, B)`, `can_see(B, A)` is false |
| `grant_idempotent` | Second `grant(A, B)` returns false |
| `grant_checked_succeeds` | Grantor that can see target allows the grant |
| `grant_checked_rejects_invisible_target` | Grantor that cannot see target returns GrantorCannotSee |
| `visible_to_includes_self` | `visible_to(A)` contains A even with no grants |
| `visible_to_includes_grants` | `visible_to(A)` contains all granted targets |
| `who_can_see_includes_self` | `who_can_see(A)` contains A |
| `who_can_see_tracks_reverse` | After `grant(A, B)` and `grant(C, B)`, `who_can_see(B)` contains A, B, C |
| `unregistered_workspace_invisible` | `can_see(A, X)` is false for unregistered X (unless A == X) |
| `register_idempotent` | Registering the same workspace twice does not clear its grants |
| `multiple_grants_accumulate` | Granting A → B, A → C, A → D yields `visible_to(A)` with all four (including self) |
| `grant_count_accurate` | `grant_count()` matches the number of non-self edges |

## Acceptance Criteria

- `VisibilityGraph` compiles with all methods.
- Self-visibility is implicit (VI-1).
- No revoke method exists (VI-2).
- Grantor-scoped grants enforce containment (VI-4).
- All 14 tests pass.
- `cargo clippy` clean, `cargo test` green.
