# Task 9.3: Ownership Domains + Causation

## Scope

Add `EscalationRouter` (workspace → owner fast lookup for escalation routing), `resolve_owner()` (ownership resolution at creation), `resolve_originator()` (originator resolution at creation), `causal_impact()` (active workspaces in a user's causal domain), `is_causal_boundary()` (detect originator transitions).

**Already implemented in 9.1:** `by_owner()`, `by_originator()`, `causal_descendants()`, `transfer_owner()`, `originator_index`, `owner_index`.

**Does NOT produce:** Port rights graph (task 9.4). Compound operations wiring (task 9.5).

## Dependencies

- `wacp-types` (`UserId`, `WorkspaceId`, `Originator`)
- `wacp-coordinator` tree.rs (existing — `WorkspaceTree`, `WorkspaceNode`)

## Types

### New: `EscalationRouter`

```rust
pub struct EscalationRouter {
    routing: HashMap<String, UserId>,
}
```

Derived index: workspace id → owner. Maintained on workspace registration and ownership transfer. Avoids tree lookup on every escalation signal.

## Functions

### `EscalationRouter`

- `new() -> Self` — empty router.
- `register(&mut self, id: &WorkspaceId, owner: &UserId)` — register workspace owner. Called on workspace creation.
- `update(&mut self, id: &WorkspaceId, new_owner: &UserId)` — update after ownership transfer.
- `route(&self, id: &WorkspaceId) -> Option<&UserId>` — lookup owner for escalation routing.
- `remove(&mut self, id: &WorkspaceId)` — remove entry (not needed for protocol but useful for cleanup).

### Free function: `resolve_owner`

```rust
pub fn resolve_owner(parent_owner: &UserId, explicit: Option<&UserId>) -> UserId
```

Returns `explicit` if provided, otherwise `parent_owner`. Used at workspace creation.

### Free function: `resolve_originator`

```rust
pub fn resolve_originator(
    parent_originator: &Originator,
    is_injection: bool,
    injector: Option<&UserId>,
) -> Originator
```

If `is_injection`, returns `Originator::User(injector)`. Otherwise inherits `parent_originator`. Panics if `is_injection` is true but `injector` is `None`.

### New on `WorkspaceTree`: `causal_impact`

```rust
pub fn causal_impact(&self, user_id: &UserId) -> Vec<WorkspaceId>
```

All **active** (non-terminal) workspaces with `Originator::User(user_id)`. Used when a human's state changes and the coordinator needs to identify affected workspaces.

### New on `WorkspaceTree`: `is_causal_boundary`

```rust
pub fn is_causal_boundary(&self, id: &WorkspaceId) -> bool
```

True if the node's originator differs from its parent's originator. Root returns false (no parent). Used for diagnostics and injection-point queries.

## Tests

| Test | Verifies |
|------|----------|
| `escalation_router_register_and_route` | Register a workspace, route returns the owner |
| `escalation_router_update` | After update, route returns new owner |
| `escalation_router_unknown_returns_none` | Route for unregistered workspace returns None |
| `resolve_owner_inherits_parent` | No explicit override → parent owner returned |
| `resolve_owner_explicit_override` | Explicit owner → override returned |
| `resolve_originator_delegation_inherits` | Non-injection → parent originator returned |
| `resolve_originator_injection_sets_user` | Injection → `Originator::User(injector)` |
| `resolve_originator_system_parent_inherited` | System parent + delegation → System child |
| `causal_impact_active_only` | Only non-terminal workspaces returned |
| `causal_impact_empty_for_unknown_user` | Unknown user returns empty |
| `is_causal_boundary_root_false` | Root has no parent → not a boundary |
| `is_causal_boundary_same_originator_false` | Child inherits parent originator → not a boundary |
| `is_causal_boundary_different_originator_true` | Child has different originator → is a boundary |

## Acceptance Criteria

- `EscalationRouter` compiles with register/update/route.
- `resolve_owner` and `resolve_originator` produce correct results per spec §5.2 and §6.1.
- `causal_impact` returns only active workspaces.
- `is_causal_boundary` detects originator transitions.
- All 13 tests pass.
- All existing tests continue to pass.
- `cargo clippy` clean.
