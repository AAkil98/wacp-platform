---
id: wcon-w2-launch-flow
type: coding
status: final
created: 2026-04-15T04:25:00
revised: 2026-04-15T04:25:00
authors: [AAkil98, Claude Opus 4.6]
tags: [wiring, w2, launch, session, grpc, coordinator, agent]
depends_on: [wcon-w1-grpc-pool, wcon-wiring-phases, wcon-sessions]
---

# W2 — Launch Flow

## Table of Contents

- 1. Scope
- 2. Dependencies
- 3. Types & Signatures
- 4. Internal Design
- 5. Test Cases
- 6. Acceptance Criteria

---

## 1. Scope

Replace the SQLite-only LAUNCHING → ACTIVE transition at `console-api/src/routes/sessions.rs:445-454` with the real 5-step atomic gRPC sequence. Populate `sessions.coordinator_workspace_id` and each `session_assignments.workspace_id`. Roll back created workspaces on partial failure so the runtime never carries orphaned workspaces from a failed launch.

**Out of scope.** Streaming the launched session (that's W3). Cancellation of an already-launched session (that's W5). UI changes — the frontend currently waits on the session state transition.

**Files touched.**
- New: `wacp-console/crates/console-core/src/session_launcher.rs`.
- Modified: `wacp-console/crates/console-api/src/routes/sessions.rs` (launch handler body only).
- Modified: `wacp-console/crates/console-db/src/queries/session_assignments.rs` (new helper for setting `workspace_id`).
- Modified: `wacp-console/crates/console-core/src/lib.rs` (pub mod export).

## 2. Dependencies

- **`wcon-w1-grpc-pool`** — `AppState.grpc_pool` must exist.
- **`wcon-sessions` §5.3** — launch data-flow reference.
- **`wcon-architecture` §5** — launch orchestration model.
- **Existing proto definitions:** `wacp/proto/coordinator.proto` (`CreateSession`, `SubmitGoal`, `Dispatch`), `wacp/proto/agent.proto` (`SendEnvelope`). Review before coding — see §4.0.

## 3. Types & Signatures

### 3.1 Public API

```rust
pub struct SessionLauncher {
    pool: Arc<GrpcPool>,
    db: Arc<ConsoleDb>,
    validator: Arc<SessionValidator>,
}

impl SessionLauncher {
    pub fn new(pool: Arc<GrpcPool>, db: Arc<ConsoleDb>, validator: Arc<SessionValidator>) -> Self;

    /// Execute the full 5-step launch. Idempotent per session_id — re-invocation on a
    /// session whose state is already ACTIVE or beyond returns `LaunchOutcome::AlreadyActive`.
    pub async fn launch(&self, session_id: SessionId) -> Result<LaunchOutcome, LaunchError>;
}

pub enum LaunchOutcome {
    Active {
        coordinator_workspace_id: WorkspaceId,
        assignments: Vec<LaunchedAssignment>,
    },
    AlreadyActive { state: SessionState },
}

pub struct LaunchedAssignment {
    pub assignment_id: AssignmentId,
    pub workspace_id: WorkspaceId,
}
```

### 3.2 Error type

```rust
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("session {0} not found")]
    SessionNotFound(SessionId),
    #[error("session in unexpected state: expected LAUNCHING, got {0:?}")]
    UnexpectedState(SessionState),
    #[error("step {step}: {reason}")]
    Step {
        step: LaunchStep,
        reason: String,
        #[source]
        source: Option<tonic::Status>,
        recoverable: bool,
    },
    #[error("rollback failed: {0}")]
    RollbackFailed(String),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize)]
pub enum LaunchStep {
    CreateSession,
    SubmitGoal,
    Dispatch,
    SendEnvelope,
    Finalize,
}
```

`recoverable` distinguishes transient (network blip) from terminal (runtime rejected request) for the caller. The handler uses this to decide HTTP status (503 for recoverable, 422 / 502 for terminal).

### 3.3 Handler changes (`routes/sessions.rs`)

```rust
// before (lines 445–454): sessions::transition_state(LAUNCHING, ACTIVE, …).await?;

// after:
let launcher = SessionLauncher::new(state.grpc_pool.clone(), state.db.clone(), state.validator.clone());
match launcher.launch(session_id).await {
    Ok(LaunchOutcome::Active { .. }) => (/* 202 Accepted or 200 with session row */),
    Ok(LaunchOutcome::AlreadyActive { .. }) => (/* 409 Conflict */),
    Err(LaunchError::Step { recoverable: true, .. }) => return Err(ApiError::ServiceUnavailable(/*…*/)),
    Err(LaunchError::Step { recoverable: false, .. }) => return Err(ApiError::BadGateway(/*…*/)),
    // …
}
```

## 4. Internal Design

### 4.0 Proto shape review (pre-coding task W2.1)

Produce a short file `impl/archive/notes/w2-proto-shapes.md` before writing `session_launcher.rs`:

- `CoordinatorService::CreateSession` — request fields, response fields, error codes.
- `CoordinatorService::SubmitGoal` — same.
- `CoordinatorService::Dispatch` — request carries assignment spec, response has `workspace_id`.
- `AgentService::SendEnvelope` — envelope type required for the launch directive; distinguish from gate response envelopes.

Cite proto file + line numbers. This artifact gates implementation — without it, the types below are guesses.

### 4.1 The 5-step sequence

```
Step 1: CreateSession        → get coordinator_workspace_id
Step 2: SubmitGoal           → submit workflow description, optional context
Step 3: Dispatch (per assign) → get per-assignment workspace_id
Step 4: SendEnvelope (per)   → directive with LLM config, tools, context
Step 5: Finalize             → transition session LAUNCHING → ACTIVE,
                               write coordinator_workspace_id + assignment workspace_ids
                               in a single transaction
```

Steps 3 and 4 run sequentially per assignment (not parallelized in W2 — can revisit in a later perf pass).

### 4.2 Rollback

On any step failure at index ≥ 3 (i.e., at least one workspace has been created):
1. Collect all `workspace_id`s successfully returned by `Dispatch` so far.
2. For each, call `CoordinatorService::AbortWorkspace(workspace_id, reason="launch_rollback")`. Tolerate individual failures — log but do not fail the rollback chain.
3. Transition session LAUNCHING → FAILED with `reason = "launch_step_{step}: {original_reason}"`.

Step 1 and Step 2 failures need no rollback (no workspaces created yet) — just transition to FAILED.

### 4.3 Finalize transaction

Step 5 is a single SQLite transaction:
```sql
UPDATE sessions SET state = 'active', coordinator_workspace_id = ? WHERE id = ?;
UPDATE session_assignments SET workspace_id = ? WHERE id = ?;  -- per assignment
```
If the transaction fails, the launch is *not* considered successful — revert via rollback (§4.2) and propagate the error. The coordinator workspace still exists at the runtime though; log prominently and mark session FAILED with `reason = "finalize_db_failed"`. This is a narrow failure window (db crash between runtime work and commit); W5 recovery handles the next restart.

### 4.4 Idempotency

- `launch()` first loads session row. If state ≥ ACTIVE, return `AlreadyActive` immediately (the handler maps this to 409 Conflict, no double-work).
- If state is LAUNCHING but `coordinator_workspace_id` is already non-NULL, we crashed between Step 1 and Step 5. Don't auto-resume in W2 — that's W5 recovery's job. Return `LaunchError::UnexpectedState` with a specific `stuck_in_launching` reason. W5 will handle this pattern.

### 4.5 Logging

One structured log event per step on enter + exit, with `session_id`, `step`, `duration_ms`, `outcome` (`ok` | `err:{reason}`). One summary event at launch end.

## 5. Test Cases

### 5.1 Unit (no gRPC)

- **T2.1** `LaunchError::Step { recoverable: … }` maps to correct HTTP status via handler.
- **T2.2** `launch()` on session in state ACTIVE returns `AlreadyActive` without any gRPC call (assert via mock pool that no channel was exercised).

### 5.2 Mock runtime — happy path

- **T2.3** Full launch with 3 assignments → one CreateSession + one SubmitGoal + 3 Dispatch + 3 SendEnvelope calls, in order. Session row shows `state='active'` + `coordinator_workspace_id` set. Assignment rows show `workspace_id` set.

### 5.3 Mock runtime — failure paths (5 cases, one per step)

- **T2.4** Step 1 fails → `LaunchError::Step { step: CreateSession, recoverable: true }`. Session in FAILED, no gRPC calls after step 1, no AbortWorkspace issued.
- **T2.5** Step 2 fails → FAILED, no rollback needed.
- **T2.6** Step 3 fails on second assignment → Session FAILED, rollback issues `AbortWorkspace` on the one already-dispatched workspace. Assert via mock.
- **T2.7** Step 4 fails on third assignment → rollback aborts workspaces 1, 2, 3.
- **T2.8** Step 5 (finalize DB transaction) fails → rollback aborts all workspaces, reason `finalize_db_failed`.

### 5.4 Real runtime (W7 sweep)

Defer end-to-end assertion to W7.2. W2 phase-close accepts mock-layer green.

## 6. Acceptance Criteria

- [ ] `impl/archive/notes/w2-proto-shapes.md` lands before any `session_launcher.rs` commit.
- [ ] `cargo test -p console-core --lib session_launcher::` — all green, ≥ 8 tests (T2.1–T2.8).
- [ ] `cargo test -p console-api --lib routes::sessions::launch::` — handler tests green.
- [ ] `cargo build --workspace` green.
- [ ] `git grep '// The actual gRPC launch sequence'` returns zero matches.
- [ ] Manual: launch a session via UI → database check (`sqlite3 console.db 'SELECT coordinator_workspace_id FROM sessions'`) returns a non-NULL UUID; runtime check (`curl '/v1/workspaces'`) lists the same workspace.

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-w1-grpc-pool | W1 — gRPC Pool → AppState | precedes (requires AppState.grpc_pool) |
| wcon-wiring-phases | Wiring Phases | parent (§3 W2 row) |
| wcon-sessions | Session System | constrains (§5.3 launch flow, §4.2 assignment schema) |
| wcon-architecture | System Architecture | constrains (§5 launch orchestration) |

*WACP Workspace — authored by AKIL Abderrahim and Claude Opus 4.6*
