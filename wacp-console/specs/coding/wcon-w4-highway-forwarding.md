---
id: wcon-w4-highway-forwarding
type: coding
status: final
created: 2026-04-15T04:35:00
revised: 2026-04-15T04:35:00
authors: [AAkil98, Claude Opus 4.6]
tags: [wiring, w4, highway, gates, escalations, directives, audit]
depends_on: [wcon-w3-session-monitor, wcon-wiring-phases, wcon-highway]
---

# W4 — Highway Forwarding

## Table of Contents

- 1. Scope
- 2. Dependencies
- 3. Types & Signatures
- 4. Internal Design
- 5. Test Cases
- 6. Acceptance Criteria

---

## 1. Scope

Replace the four audit-only highway endpoints with real `HighwayService` gRPC calls. The existing audit-log write stays, but only fires after a successful runtime response (invariant: no audit row for a runtime-rejected action).

**Files touched.**
- Modified: `wacp-console/crates/console-api/src/routes/highway.rs` — four handler bodies.
- Modified (possibly): `wacp-console/crates/console-api/src/error.rs` — if a new `ApiError::HighwayRuntimeRejected` variant is needed.
- Modified: `wacp-console/crates/console-api/src/routes/highway_test.rs` (if the file exists; otherwise new tests inline).

**Out of scope.** Any changes to the gate/escalation/directive data model. UI changes. Cross-session pending endpoints (W6).

## 2. Dependencies

- **`wcon-w1-grpc-pool`** — pool clients.
- **`wcon-w3-session-monitor`** — monitor surfaces the resulting workspace-resume and trail-entry events; not strictly required for W4 to compile, but W4's acceptance depends on W3's broadcast working (we assert end-to-end that an approved gate produces the runtime's follow-up trail event).
- **`wcon-highway` §4, §5** — gate + escalation + directive semantics.

## 3. Types & Signatures

### 3.1 Handler bodies (pattern)

Pseudo-pattern shared by all four endpoints:

```rust
async fn resolve_gate(State(state): State<AppState>, Path(gate_id): Path<GateId>, Json(req): Json<ResolveGateRequest>)
    -> Result<Json<ResolveGateResponse>, ApiError>
{
    // 1. ownership + auth (unchanged)
    let auth = require_gate_owner(&state, user, &gate_id).await?;

    // 2. call runtime
    let highway = state.grpc_pool.highway().await;   // returns &HighwayServiceClient or a guard
    let runtime_resp = highway.respond_to_gate(
        tonic::Request::new(RespondToGateRequest { gate_id: gate_id.into(), decision: req.decision.into(), reason: req.reason.clone().unwrap_or_default() })
    ).await.map_err(ApiError::from_tonic)?;

    // 3. audit log (only on success)
    audit::append(&state.db, AuditEntry { kind: AuditKind::GateDecision { gate_id, decision: req.decision, runtime_applied: runtime_resp.applied }, user: user.id, ts: now() }).await.ok();

    // 4. optimistic in-memory pending removal — monitor's stream will confirm
    if let Some(handle) = state.active_sessions.read().await.get(&auth.session_id) {
        handle.pending.gates.write().await.retain(|g| g.id != gate_id);
    }

    Ok(Json(ResolveGateResponse { applied: runtime_resp.applied, runtime_ack_id: runtime_resp.ack_id }))
}
```

### 3.2 `ApiError::from_tonic` mapping

```rust
impl ApiError {
    fn from_tonic(s: tonic::Status) -> Self {
        match s.code() {
            tonic::Code::NotFound   => ApiError::NotFound(s.message().into()),
            tonic::Code::FailedPrecondition => ApiError::Conflict(s.message().into()),
            tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => ApiError::ServiceUnavailable(s.message().into()),
            tonic::Code::PermissionDenied => ApiError::Forbidden(s.message().into()),
            _ => ApiError::BadGateway(format!("runtime: {}", s.message())),
        }
    }
}
```

### 3.3 Batch resolve

```rust
pub struct BatchResolveRequest {
    pub decisions: Vec<GateDecisionInput>,
}
pub struct BatchResolveResponse {
    pub results: Vec<BatchResolveOutcome>,
}
pub struct BatchResolveOutcome {
    pub gate_id: GateId,
    pub status: BatchOutcomeStatus,   // Applied | RuntimeRejected { code, message } | AuthFailed
}
```

Each gate is forwarded sequentially (not parallel — runtime may serialize anyway, and per-gate ordering is easier to reason about for audit). Partial failure: continue through the batch, capture per-gate outcomes.

## 4. Internal Design

### 4.1 Four endpoints, one pattern

| Endpoint | Runtime RPC |
|----------|-------------|
| `POST /api/gates/:id/resolve` | `HighwayService::RespondToGate` |
| `POST /api/gates/batch` | loop of `RespondToGate` |
| `POST /api/escalations/:id/respond` | `HighwayService::RespondToEscalation` |
| `POST /api/directives/inject` | `HighwayService::InjectEnvelope` |

All four adopt the (runtime call → audit on success → optimistic in-memory removal) order.

### 4.2 Audit-only-on-success invariant

The current code audits regardless of runtime outcome (because there is no runtime call). After W4, audit fires only on gRPC `Ok(_)`. Verification rule: a new database-invariant test asserts `SELECT COUNT(*) FROM audit WHERE kind = 'gate_decision' AND applied = 0` can be populated via `RuntimeRejected` flow, and the audit is tagged with `applied=false` — the distinction matters for compliance review.

Wait — re-reading: existing schema probably doesn't have `applied`. If so, introduce it:
```sql
ALTER TABLE audit ADD COLUMN runtime_applied BOOLEAN;
```
via migration, default NULL for historical rows. Update audit insert to set `runtime_applied = runtime_resp.applied`.

If the schema change is out of scope for W4, defer and log the intent in this spec's deviation section (record in §4.2 bullet of `impl/archive/wiring-phases.md` at phase close).

### 4.3 Optimistic pending removal

Between the runtime ack and the monitor's `WorkspaceChange` / `Gate` stream removal, there's a small window where the cross-session pending endpoint (W6) would still list the resolved gate. Pre-empt this by mutating `pending.gates` in-place on the handle. Monitor will subsequently remove via its stream — idempotent remove (retain filter) is safe.

### 4.4 Directive injection specifics

`InjectEnvelope` takes a target `workspace_id` and envelope payload. Workspace ID comes from the request body. Validate that:
1. Workspace belongs to a session the user owns (auth check).
2. Session is ACTIVE (reject 409 Conflict otherwise).

Envelope schema is defined by proto + `wcon-highway.md` §6 (directives).

## 5. Test Cases

### 5.1 Mock runtime

- **T4.1** `resolve_gate` happy path: runtime Ok → audit row inserted with `runtime_applied=true`; pending gate removed from handle.
- **T4.2** `resolve_gate` runtime NotFound → 404 returned, no audit, no pending mutation.
- **T4.3** `resolve_gate` runtime Unavailable → 503, no audit, no pending mutation.
- **T4.4** `batch_resolve` 3 gates, 1 rejected → response reports `[Applied, RuntimeRejected, Applied]`; 2 audit rows inserted (not 3); 2 pending removals.
- **T4.5** `respond_escalation` happy path.
- **T4.6** `inject_directive` happy path — workspace_id validated, envelope reaches runtime via mock (assert request capture).
- **T4.7** `inject_directive` on session not ACTIVE → 409 Conflict, no runtime call.
- **T4.8** `inject_directive` cross-owner attempt → 403 Forbidden, no runtime call.

### 5.2 Invariant tests

- **T4.9** No test path inserts an audit row on a rejected runtime call — sweep via mock assertions + DB integrity.

### 5.3 Real runtime (W7 sweep)

- W7.2 covers approve-gate-resumes-workspace end-to-end.

## 6. Acceptance Criteria

- [ ] `cargo test -p console-api --lib routes::highway::` — all green, ≥ 9 new tests.
- [ ] `git grep 'TODO: Forward to HighwayService' wacp-console/` returns zero.
- [ ] Manual: launch session → gate appears on WS → approve via `curl -X POST /api/gates/:id/resolve` → runtime trail shows the approval ack within 2s → workspace resumes.
- [ ] Audit table shows one row per approved gate with `runtime_applied=true`; zero rows for rejected attempts.
- [ ] Schema migration (if added in §4.2) has a rollback script and is tested against a fresh DB.

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-w3-session-monitor | W3 — Session Monitor | precedes (optimistic pending mutation via handle; end-to-end test depends on monitor broadcast) |
| wcon-wiring-phases | Wiring Phases | parent (§3 W4 row) |
| wcon-highway | Highway Integration | constrains (§4 gates, §5 escalations, §6 directives) |
| wcon-auth | Authentication & Authorization | constrains (owner checks, 403 paths) |

*WACP Workspace — authored by AKIL Abderrahim and Claude Opus 4.6*
