---
id: wcon-w5-cancel-recovery
type: coding
status: final
created: 2026-04-15T04:40:00
revised: 2026-04-15T04:40:00
authors: [AAkil98, Claude Opus 4.6]
tags: [wiring, w5, cancel, recovery, startup, lifecycle]
depends_on: [wcon-w3-session-monitor, wcon-wiring-phases, wcon-sessions]
---

# W5 — Cancel & Recovery

## Table of Contents

- 1. Scope
- 2. Dependencies
- 3. Types & Signatures
- 4. Internal Design
- 5. Test Cases
- 6. Acceptance Criteria

---

## 1. Scope

Fill two currently-empty arms of `CancelAction` — `BestEffortAbort` and `AbortWorkspace` — with real `CoordinatorService::AbortWorkspace` gRPC calls, and stand up a startup recovery sequence that iterates all sessions in state `ACTIVE`, verifies each against the runtime via `GetWorkspace`, and either respawns the monitor (resume) or marks the session `FAILED` with `reason='recovery_workspace_missing'`.

**Files touched.**
- Modified: `wacp-console/crates/console-api/src/routes/sessions.rs` — cancel handler match arms.
- New: `wacp-console/crates/console-core/src/recovery.rs`.
- Modified: `wacp-console/crates/console/src/main.rs` — call `recovery::run(...)` after pool connect, before `serve`.
- Modified: `wacp-console/crates/console-core/src/lib.rs` — pub mod export.

## 2. Dependencies

- **`wcon-w1-grpc-pool`**, **`wcon-w3-session-monitor`** — both required for cancel (pool) and recovery (spawn monitors).
- **`wcon-sessions` §7.3 (cancel), §8.2 (recovery)** — lifecycle semantics.

## 3. Types & Signatures

### 3.1 Cancel handler

```rust
// routes/sessions.rs (excerpt)
match cancel_action {
    CancelAction::BestEffortAbort => {
        let coord = state.grpc_pool.coordinator().await;
        if let Some(ws_id) = session.coordinator_workspace_id {
            let _ = coord.abort_workspace(tonic::Request::new(
                AbortWorkspaceRequest { workspace_id: ws_id.into(), reason: req.reason.clone().unwrap_or_default() }
            )).await;    // tolerated
        }
        sessions::transition_state(&state.db, session_id, ACTIVE, CANCELLED, req.reason).await?;
    }
    CancelAction::AbortWorkspace => {
        let coord = state.grpc_pool.coordinator().await;
        let ws_id = session.coordinator_workspace_id.ok_or(ApiError::Conflict("no coordinator workspace".into()))?;
        coord.abort_workspace(tonic::Request::new(
            AbortWorkspaceRequest { workspace_id: ws_id.into(), reason: req.reason.clone().unwrap_or_default() }
        )).await.map_err(ApiError::from_tonic)?;
        sessions::transition_state(&state.db, session_id, ACTIVE, CANCELLED, req.reason).await?;
    }
}
// shutdown monitor if registered
if let Some(handle) = state.active_sessions.write().await.remove(&session_id) {
    let _ = handle.cmd_tx.send(MonitorCmd::Shutdown).await;
}
```

Distinction in wording:
- **BestEffortAbort**: abort is nice-to-have. Cancel succeeds even if abort fails; the session is marked CANCELLED locally.
- **AbortWorkspace**: abort is required. If runtime rejects abort, the cancel fails (5xx); session stays ACTIVE.

### 3.2 Recovery entry

```rust
pub async fn run(
    db: Arc<ConsoleDb>,
    pool: Arc<GrpcPool>,
    enricher: Arc<EventEnricher>,
    refusals: Arc<RefusalSynthesizer>,
    active: ActiveSessionsMap,
    cfg: MonitorConfig,
) -> RecoveryReport;

#[derive(Debug)]
pub struct RecoveryReport {
    pub resumed: Vec<SessionId>,
    pub failed: Vec<(SessionId, RecoveryFailureReason)>,
}

#[derive(Debug)]
pub enum RecoveryFailureReason {
    WorkspaceMissing,
    WorkspaceTerminal { final_state: String },
    RuntimeUnavailable,
    DbError(String),
}
```

The returned report is logged (structured) and exposed for a future `/api/admin/recovery` endpoint — out of scope here.

## 4. Internal Design

### 4.1 Cancel cleanup ordering

1. Call runtime abort (tolerated or required per action).
2. DB transition ACTIVE → CANCELLED.
3. Shutdown the active monitor (if any). Monitor exit is async; the handler doesn't wait for it beyond sending the shutdown command.
4. Respond 200 with the updated session row.

The monitor will then observe the `WorkspaceChange { Aborted }` event and emit a final frame. The order (DB first, then monitor shutdown) ensures that if the client re-reads the session right after a 200, they see CANCELLED.

### 4.2 Recovery sequence

Single loop at startup, after pool connect, before `serve`:

```
let active_rows = sessions::list_active(&db).await?;
for row in active_rows {
    if row.coordinator_workspace_id.is_none() {
        // Stuck in LAUNCHING — W2 finalize failure. Mark FAILED.
        mark_failed(&db, row.id, "stuck_in_launching").await;
        continue;
    }
    let ws = match coord.get_workspace(row.coordinator_workspace_id.unwrap()).await {
        Ok(ws) => ws,
        Err(s) if s.code() == Code::NotFound => {
            mark_failed(&db, row.id, "recovery_workspace_missing").await;
            continue;
        }
        Err(s) if s.code() == Code::Unavailable => {
            // Runtime flaky at startup — skip this row, leave ACTIVE, next restart will retry.
            report.failed.push((row.id, RuntimeUnavailable));
            continue;
        }
        Err(e) => { mark_failed(&db, row.id, format!("recovery_error: {e}")).await; continue; }
    };
    if ws.state.is_terminal() {
        // Runtime already finished it while we were down. Sync state.
        sync_terminal_state(&db, row.id, ws.state).await;
        continue;
    }
    // Still live — respawn monitor.
    let handle = SessionMonitor::spawn(row.id, row.workspace_set(), pool.clone(), db.clone(), enricher.clone(), refusals.clone(), cfg.clone()).await;
    active.write().await.insert(row.id, handle);
    report.resumed.push(row.id);
}
```

### 4.3 Start-time budget

Recovery should not block startup indefinitely if the runtime is slow. Per-session `GetWorkspace` has a 5-second timeout. On `Unavailable` response, we leave the session in ACTIVE and retry on next restart (the row stays "stuck" until the runtime is reachable, which is the right semantics — don't mark something FAILED just because the runtime had a hiccup at our boot moment).

### 4.4 Monitor spawn ordering

Recovery is single-threaded through the active-sessions list. Spawning monitors is async but the outer loop awaits each spawn. A large fleet could slow startup; revisit if recovery time > 5 s with > 50 active sessions. Acceptable for W5's scope.

## 5. Test Cases

### 5.1 Cancel — mock runtime

- **T5.1** `BestEffortAbort` with runtime Ok → session CANCELLED; abort called once; monitor shutdown sent.
- **T5.2** `BestEffortAbort` with runtime Unavailable → session still CANCELLED; abort was attempted; monitor shutdown sent.
- **T5.3** `AbortWorkspace` with runtime Unavailable → 503 returned; session stays ACTIVE; no monitor shutdown.
- **T5.4** `AbortWorkspace` on session without `coordinator_workspace_id` → 409 Conflict.
- **T5.5** Cancel on session already CANCELLED → 409 Conflict, no runtime call.

### 5.2 Recovery — mock runtime

- **T5.6** 3 active sessions, 1 live + 1 missing + 1 terminal → report `resumed=[live], failed=[(missing, WorkspaceMissing)], synced=[terminal]`. Active map contains 1 handle.
- **T5.7** `Unavailable` runtime at startup → session stays ACTIVE, `failed=[(…, RuntimeUnavailable)]`, startup continues.
- **T5.8** Stuck-in-LAUNCHING row (no `coordinator_workspace_id`) → FAILED with reason `stuck_in_launching`.
- **T5.9** Recovery of session that resumes cleanly → monitor runs, WS clients can connect.

### 5.3 Real runtime (W7 sweep)

- W7.3 covers runtime-restart mid-session; W7.7 covers console-restart.

## 6. Acceptance Criteria

- [ ] `cargo test -p console-api --lib routes::sessions::cancel::` — all green, ≥ 5 tests.
- [ ] `cargo test -p console-core --lib recovery::` — all green, ≥ 4 tests.
- [ ] `git grep '// Full abort via CoordinatorService.AbortWorkspace' wacp-console/` returns zero.
- [ ] Manual: launch → cancel (best-effort) → observe session CANCELLED, runtime workspace aborted. Then: launch → `kill -TERM` console → restart → session visible, monitor running, WS client can reconnect and see live frames.
- [ ] `main.rs` startup order: pool → recovery → serve. Recovery report logged structurally.

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-w3-session-monitor | W3 — Session Monitor | precedes (recovery respawns monitors; cancel shuts one down) |
| wcon-wiring-phases | Wiring Phases | parent (§3 W5 row) |
| wcon-sessions | Session System | constrains (§7.3 cancel, §8.2 recovery) |

*WACP Workspace — authored by AKIL Abderrahim and Claude Opus 4.6*
