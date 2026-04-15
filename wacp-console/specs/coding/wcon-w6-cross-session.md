---
id: wcon-w6-cross-session
type: coding
status: final
created: 2026-04-15T04:45:00
revised: 2026-04-15T04:45:00
authors: [AAkil98, Claude Opus 4.6]
tags: [wiring, w6, cross-session, pending, aggregation, rbac]
depends_on: [wcon-w3-session-monitor, wcon-wiring-phases, wcon-api, wcon-auth]
---

# W6 — Cross-Session Endpoints

## Table of Contents

- 1. Scope
- 2. Dependencies
- 3. Types & Signatures
- 4. Internal Design
- 5. Test Cases
- 6. Acceptance Criteria

---

## 1. Scope

Wire the three cross-session aggregation endpoints that currently return hardcoded empty arrays. Each endpoint reads from `AppState.active_sessions` (populated by W3 monitor spawn / W5 recovery), collects pending items across all active monitors, and filters by the caller's ownership scope.

**Files touched.**
- Modified: `wacp-console/crates/console-api/src/routes/highway.rs` — three endpoint bodies.
- Possibly new: a small aggregator module at `console-api/src/routes/highway/pending.rs` to keep handlers lean.

## 2. Dependencies

- **`wcon-w3-session-monitor`** — `ActiveSessionsMap` and `PendingState` come from W3.
- **`wcon-api`** — response schemas (including pagination envelope).
- **`wcon-auth`** — ownership filter semantics.

## 3. Types & Signatures

### 3.1 Endpoints

```
GET /api/gates/pending
GET /api/escalations/pending
GET /api/refusals/pending
```

Query parameters: `limit` (default 50, max 500), `cursor` (opaque). Response envelope per `wcon-api.md`.

### 3.2 Aggregator

```rust
pub async fn aggregate_pending_gates(
    active: &ActiveSessionsMap,
    user: &AuthUser,
    filter: PendingFilter,
) -> Vec<EnrichedGate>;
```

Pattern repeats for escalations and refusals. `PendingFilter` carries pagination cursor, optional `session_id` scope, and optional `vertical` scope.

### 3.3 Ownership

```rust
struct PendingFilter {
    pub limit: usize,
    pub cursor: Option<PendingCursor>,
    pub session_id: Option<SessionId>,
    pub vertical: Option<VerticalId>,
}

#[derive(Clone)]
struct PendingCursor { session_id: SessionId, item_id: String }
```

Cursor is opaque to the client; implementation encodes `(session_id, item_id)` pair base64. Stable across paginated reads because pending lists are monotone-increasing within a session (W3 adds on stream event, removes on resolve).

## 4. Internal Design

### 4.1 Aggregation pass

```
read active_sessions under RwLock::read() — brief
for (session_id, handle) in sessions {
    if !user.can_see_session(session_id) { continue; }
    handle.pending.gates.read().await.iter().for_each(|g| collected.push(g.clone()));
}
sort by (session_id, item_id) to keep cursor stable
apply cursor + limit
return
```

Cloning per item keeps the inner locks held for microseconds. For 10 active sessions × 50 pending gates, aggregation is ~500 clones — negligible.

### 4.2 Ownership model

From `wcon-auth.md`:
- User sees sessions they own.
- Admin sees all sessions.
- Non-admin attempt to query with `session_id` scoping to a session they don't own → 403 Forbidden (don't silently return empty — make the authz failure explicit).

### 4.3 Empty active map

If `active_sessions` is empty (no one launched anything), all three endpoints return `{"items": [], "next_cursor": null}` with HTTP 200. This is the trivial replacement of the pre-W6 hardcoded empty response.

### 4.4 Consistency

Pending state is *eventually consistent* with the monitor's stream. A client fetching `/pending` right after resolving a gate may still see the gate for up to one stream-frame RTT (typically < 100 ms). W4's optimistic removal closes most of this window; this §4.4 documents the residual.

## 5. Test Cases

### 5.1 Unit

- **T6.1** `PendingCursor` round-trips through base64 encode/decode.
- **T6.2** `aggregate_pending_gates` sorts results by `(session_id, item_id)`.

### 5.2 Handler

- **T6.3** Two active sessions, same owner, 3 + 2 pending gates → endpoint returns 5 items.
- **T6.4** Two active sessions, different owners, non-admin caller → endpoint returns only the caller's session's gates.
- **T6.5** Admin caller → returns gates from all sessions.
- **T6.6** `session_id` scope to owned session → returns scoped gates.
- **T6.7** `session_id` scope to non-owned session, non-admin → 403 Forbidden.
- **T6.8** Pagination: 120 gates across sessions, `limit=50` → 3 pages, cursor continues.
- **T6.9** Empty active map → `{"items": []}`.
- **T6.10** Escalations endpoint — analog of T6.3.
- **T6.11** Refusals endpoint — analog of T6.3.

## 6. Acceptance Criteria

- [ ] `cargo test -p console-api --lib routes::highway::pending::` — all green, ≥ 11 tests.
- [ ] `git grep 'Json(serde_json::json!({ "items": \[\] }))' wacp-console/` returns zero.
- [ ] Manual: two sessions launched with pending gates → both endpoints return merged list. Pagination respects limit.
- [ ] Nav badge count in the frontend matches API response (visual sanity check during W7).

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-w3-session-monitor | W3 — Session Monitor | precedes (owns PendingState + ActiveSessionsMap) |
| wcon-w4-highway-forwarding | W4 — Highway Forwarding | complements (optimistic pending removal on resolve) |
| wcon-wiring-phases | Wiring Phases | parent (§3 W6 row) |
| wcon-api | Console REST API | contracts (response envelope + cursor format) |
| wcon-auth | Authentication & Authorization | constrains (ownership + admin bypass) |

*WACP Workspace — authored by AKIL Abderrahim and Claude Opus 4.6*
