# Task 11.1: Integration Pipeline + Ordering

## Scope

Replace the stub integration engine with a full pipeline: `IntegrationQueue` (sequential, one-at-a-time), `IntegrationDecision` (accept/revise/reject), `IntegrationPipeline` (7-step procedure), ordering heuristics (FIFO for initial impl). Rewrite `integration.rs` while preserving the existing types used by tests.

**Does NOT produce:** Actual merge strategy execution (11.2), conflict detection (11.3), salvage (11.4).

## Dependencies

- `wacp-types` (`Checkpoint`, `CheckpointStatus`, `Confidence`, `MergeStrategy`, `WorkspaceId`)
- Tasks 10.1–10.4 (task graph, scheduling ops)

## Types

### New: `IntegrationQueue`

```rust
pub struct IntegrationQueue {
    pending: VecDeque<WorkspaceId>,
    in_progress: Option<WorkspaceId>,
}
```

### New: `IntegrationDecision`

```rust
pub enum IntegrationDecision {
    Accept { strategy: MergeStrategy },
    Revise { feedback: String },
    Reject { reason: String },
}
```

### New: `IntegrationPipeline`

Stateless operations for running the integration procedure.

### New: `CheckpointRef`

```rust
pub struct CheckpointRef {
    pub checkpoint_id: CheckpointId,
    pub content_hash: String,
    pub intent: String,
    pub confidence: Confidence,
}
```

### Preserved: `IntegrationRequest`, `IntegrationResult`, `Conflict`, `ConflictResolution`, `ResolutionOutcome`, `IntegrationEngine`

Keep existing types for backward compatibility with existing tests.

## Functions

### `IntegrationQueue`

- `new() -> Self`
- `push(&mut self, id: WorkspaceId)` — add to queue
- `next(&mut self) -> Option<WorkspaceId>` — pop front if nothing in progress
- `complete(&mut self)` — mark current integration done
- `is_active(&self) -> bool` — integration in progress?
- `pending_count(&self) -> usize`
- `current(&self) -> Option<&WorkspaceId>`

### `IntegrationPipeline`

- `find_final_checkpoint(checkpoints: &[Checkpoint]) -> Option<CheckpointRef>` — locate most recent final checkpoint
- `decide(checkpoint: &CheckpointRef) -> IntegrationDecision` — rule-based: low confidence → revise, else accept with strategy selection
- `select_strategy(confidence: Confidence) -> MergeStrategy` — high → direct, medium → layered, low → evaluated

## Tests

| Test | Verifies |
|------|----------|
| `queue_push_and_next` | Push → next returns the workspace |
| `queue_one_at_a_time` | next returns None while integration in progress |
| `queue_complete_allows_next` | After complete, next returns the next entry |
| `queue_fifo_order` | Multiple pushes return in FIFO order |
| `queue_pending_count` | pending_count reflects queue size |
| `find_final_checkpoint_found` | Locates final checkpoint in a list |
| `find_final_checkpoint_none` | Returns None when no final checkpoint exists |
| `find_final_prefers_latest` | With multiple finals, returns the last one |
| `decide_high_confidence_accepts` | High confidence → Accept with Direct strategy |
| `decide_low_confidence_revises` | Low confidence → Revise |
| `decide_medium_accepts_layered` | Medium confidence → Accept with Layered strategy |
| `select_strategy_mapping` | High→Direct, Medium→Layered, Low→Evaluated |

## Acceptance Criteria

- `IntegrationQueue` enforces sequential integration (one at a time).
- `IntegrationPipeline` provides the 7-step procedure's decision logic.
- Existing integration tests continue to pass (IntegrationEngine preserved).
- All 12 new tests pass.
- `cargo clippy` clean.
