# Task 11.2: Merge Strategies

## Scope

Add `MergeExecutor` with three strategies: direct (pass-through, no detection), layered (content overlap detection), evaluated (full 4-type conflict scan). Operates on `MergeContext` which carries extracted resource sets from source and parent.

## Types

### New: `MergeContext`

```rust
pub struct MergeContext {
    pub source_id: WorkspaceId,
    pub target_id: WorkspaceId,
    pub source_resources: HashSet<String>,  // resources modified by source
    pub parent_resources: HashSet<String>,  // resources modified by prior integrations
    pub checkpoint: CheckpointRef,
}
```

### New: `MergeResult`

```rust
pub enum MergeResult {
    Success,
    Conflicts(Vec<Conflict>),
}
```

### New: `MergeExecutor`

Stateless. Executes a strategy against a `MergeContext`.

## Functions

- `execute(strategy: MergeStrategy, ctx: &MergeContext) -> MergeResult`
- `merge_direct(ctx: &MergeContext) -> MergeResult` — always Success
- `merge_layered(ctx: &MergeContext) -> MergeResult` — detect content_overlap
- `merge_evaluated(ctx: &MergeContext) -> MergeResult` — detect all 4 conflict types

## Tests

| Test | Verifies |
|------|----------|
| `direct_no_conflicts` | Direct always succeeds even with overlapping resources |
| `layered_no_overlap` | Disjoint resources → Success |
| `layered_detects_overlap` | Overlapping resources → ContentOverlap conflicts |
| `evaluated_detects_overlap` | Overlap detected in evaluated mode |
| `evaluated_no_conflicts` | Clean context → Success |
| `execute_dispatches_correctly` | execute(Direct/Layered/Evaluated) dispatches to correct strategy |
