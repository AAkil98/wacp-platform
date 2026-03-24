# Task 10.2: Gate Enforcement

## Scope

Add `GateController` to `wacp-coordinator` — manages the `draft → pending` approval gate on the task lifecycle. Creates gate events when tasks enter draft, tracks pending gates, resolves gates via human response or timeout, applies fallback action on timeout.

**Does NOT produce:** Highway streaming (Phase 13). Actual tokio timers (Phase 12). Dispatch after approval (10.3).

## Dependencies

- `wacp-types` (`GateEvent`, `GateId`, `GateDecision`, `GateType`, `TaskId`)
- `wacp-fsm` (`TaskTrigger::Approve`, `TaskTrigger::Cancel`)
- Task 10.1 (enhanced task graph)

## Types

### New: `GateFallback`

```rust
pub enum GateFallback {
    AutoApprove,
    Cancel,
}
```

### New: `PendingGate`

```rust
pub struct PendingGate {
    pub gate_id: GateId,
    pub task_id: TaskId,
    pub task_name: String,
    pub timeout_ms: u64,
    pub fallback: GateFallback,
    pub created_at: u64,
}
```

### New: `GateResolution`

```rust
pub enum GateResolution {
    Approved { source: String },
    Rejected,
    TimedOut { action: GateFallback },
}
```

### New: `GateController`

```rust
pub struct GateController {
    pending: HashMap<String, PendingGate>,
    next_id: u64,
    default_timeout_ms: u64,
    default_fallback: GateFallback,
}
```

## Functions

### `GateController::new(default_timeout_ms, default_fallback) -> Self`

### `open_gate(&mut self, task_id: TaskId, task_name: String, timeout_ms: Option<u64>, fallback: Option<GateFallback>) -> GateEvent`

Create a pending gate for a task. Returns a `GateEvent` for delivery to the highway. Uses defaults for timeout/fallback if not specified.

### `resolve(&mut self, gate_id: &GateId, decision: GateDecision) -> Option<GateResolution>`

Resolve a gate via human response. Returns `None` if gate not found (already resolved). First response wins.

### `timeout(&mut self, gate_id: &GateId) -> Option<GateResolution>`

Resolve a gate via timeout. Returns `None` if already resolved. Applies fallback action.

### `is_pending(&self, gate_id: &GateId) -> bool`

### `pending_for_task(&self, task_id: &TaskId) -> Option<&PendingGate>`

### `pending_count(&self) -> usize`

## Tests

| Test | Verifies |
|------|----------|
| `open_gate_creates_pending` | After open_gate, gate is pending |
| `open_gate_returns_event` | Returned GateEvent has correct task_id and gate_type |
| `resolve_approve_removes_gate` | Approve resolves and removes from pending |
| `resolve_reject_removes_gate` | Reject resolves and removes from pending |
| `resolve_already_resolved_returns_none` | Second resolve returns None |
| `timeout_auto_approve` | Timeout with AutoApprove fallback returns Approved |
| `timeout_cancel` | Timeout with Cancel fallback returns Rejected |
| `timeout_already_resolved_returns_none` | Timeout after resolve returns None |
| `pending_for_task_lookup` | Find pending gate by task id |
| `default_timeout_applied` | Gate uses default timeout when none specified |

## Acceptance Criteria

- `GateController` tracks pending gates.
- Human approval/rejection resolves gates (first response wins).
- Timeout applies configured fallback action.
- Gate events contain task metadata for highway display.
- All 10 tests pass.
- `cargo clippy` clean.
