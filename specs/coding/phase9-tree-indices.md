# Task 9.1: Workspace Tree Indices

## Scope

Add `originator` field to `WorkspaceNode`, add `originator_index` and `owner_index` to `WorkspaceTree`, add `siblings()` and causal traversal methods. Update all mutating operations (`insert`, `reparent`, `cascade_failure`) to maintain the new indices.

**Does NOT produce:** Visibility graph, ownership domains, port rights, compound operations (those are tasks 9.2–9.5).

## Dependencies

- `wacp-types` (provides `Originator`, `UserId`, `WorkspaceId`)
- `wacp-coordinator` tree.rs (existing implementation)

## Types

### Modified: `WorkspaceNode`

Add `originator` field:

```rust
pub struct WorkspaceNode {
    pub id: WorkspaceId,
    pub parent: Option<WorkspaceId>,
    pub children: Vec<WorkspaceId>,
    pub owner: UserId,
    pub originator: Originator,   // NEW — immutable after creation
    pub status: WorkspaceState,
    pub task_id: Option<TaskId>,
}
```

### Modified: `WorkspaceTree`

Add two index fields:

```rust
pub struct WorkspaceTree {
    nodes: HashMap<String, WorkspaceNode>,
    root: WorkspaceId,
    originator_index: HashMap<Originator, Vec<WorkspaceId>>,  // NEW
    owner_index: HashMap<UserId, Vec<WorkspaceId>>,           // NEW
}
```

### Modified: `TreeError`

Add variant for ownership transfer to self:

```rust
pub enum TreeError {
    ParentNotFound(String),
    DuplicateNode(String),
    NodeNotFound(String),
    SameOwner,               // NEW — transfer_owner called with current owner
}
```

## Functions

### New: `siblings`

```rust
pub fn siblings(&self, id: &WorkspaceId) -> Vec<WorkspaceId>
```

Return all children of `id`'s parent, excluding `id` itself. Root has no siblings.

### New: `by_originator`

```rust
pub fn by_originator(&self, originator: &Originator) -> &[WorkspaceId]
```

O(1) lookup into `originator_index`. Returns all workspaces with the given originator.

### New: `by_owner`

```rust
pub fn by_owner(&self, owner: &UserId) -> &[WorkspaceId]
```

O(1) lookup into `owner_index`. Returns all workspaces owned by the given user.

### New: `causal_descendants`

```rust
pub fn causal_descendants(
    &self,
    id: &WorkspaceId,
    originator: &Originator,
) -> Vec<WorkspaceId>
```

Intersection of `descendants(id)` and `by_originator(originator)`. Returns all descendants of `id` that have the given originator. Used for causal impact queries.

### New: `transfer_owner`

```rust
pub fn transfer_owner(
    &mut self,
    id: &WorkspaceId,
    new_owner: UserId,
) -> Result<UserId, TreeError>
```

Change a node's owner. Update `owner_index` (remove from old, add to new). Return the old owner. Originator is immutable — no transfer method.

### Modified: `insert`

After inserting the node, also:
1. Append to `originator_index[node.originator]`.
2. Append to `owner_index[node.owner]`.

### Modified: `new`

Initialize both indices with the root node's entries.

### Modified: `cascade_failure`

No index changes needed — `cascade_failure` changes status but not owner or originator. Reparented nodes are handled by `reparent`, which already exists but does not touch indices (it doesn't change owner/originator).

## Internal Design

- `originator_index` is append-only at insertion time — originator is immutable per the protocol (causation spec). No removal path.
- `owner_index` is updated on `insert` (append) and `transfer_owner` (remove from old, add to new). No removal on workspace termination — the index covers all workspaces including terminal ones.
- `by_originator` and `by_owner` return `&[WorkspaceId]` (slice of the Vec) for zero-copy reads. Return empty slice if the key has no entries.
- `causal_descendants` uses the simple intersection approach: compute `descendants()`, then filter by originator. The topology spec §2.2 confirms this is adequate for initial implementation.

## Tests

| Test | Verifies |
|------|----------|
| `originator_tracked_on_insert` | Inserting a node with `Originator::User(uid)` makes it appear in `by_originator` |
| `originator_index_immutable` | No public method can change a node's originator after insertion |
| `owner_tracked_on_insert` | Inserting a node makes it appear in `by_owner` |
| `transfer_owner_updates_index` | After `transfer_owner`, node appears under new owner and not old |
| `transfer_owner_same_owner_rejected` | `transfer_owner` with the current owner returns `SameOwner` error |
| `siblings_basic` | Two children of the same parent see each other as siblings |
| `siblings_root_empty` | Root has no siblings |
| `siblings_only_child_empty` | A node with no siblings returns empty |
| `causal_descendants_filters` | `causal_descendants(root, User(A))` returns only A-originated descendants |
| `causal_descendants_empty` | `causal_descendants` with non-matching originator returns empty |
| `by_originator_empty_key` | `by_originator` for a non-existent originator returns empty slice |
| `by_owner_empty_key` | `by_owner` for a non-existent owner returns empty slice |
| `root_in_both_indices` | Root node appears in both indices after `new()` |
| `cascade_preserves_indices` | After `cascade_failure`, originator and owner indices remain correct |

## Acceptance Criteria

- `WorkspaceNode` has `originator: Originator` field.
- `WorkspaceTree` has `originator_index` and `owner_index`.
- `insert` and `new` maintain both indices.
- `transfer_owner` updates `owner_index` atomically.
- `siblings`, `by_originator`, `by_owner`, `causal_descendants` return correct results.
- All 14 tests pass.
- All existing tree tests continue to pass (updated to include the new `originator` field).
- `cargo clippy` clean, `cargo test` green.
