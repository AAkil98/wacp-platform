//! §13.7.8 I1 — launch failure matrix.
//!
//! Ten scenarios covering every non-OK return from `SubmitGoal`,
//! `Decompose`, `Dispatch` plus the three rollback permutations
//! (single-target success, partial-target failure, total-target failure).
//!
//! Each test drives [`SessionLauncher`] directly — same pattern as WA5's
//! T7.5 — with the [`InjectableCoordinator`] from §13.7.8 P0 standing
//! between it and the real runtime. Bypassing HTTP keeps the asserted
//! surface tight: we prove the launcher's error shape + rollback
//! behaviour, not the handler wrapping.
//!
//! **Perf-opt watch (`HEALTH-LOG.md` §13.1).**
//! Any `LaunchError::reason_code()` output that looks inconsistent across
//! the matrix (e.g., a dispatch failure producing a submit_goal-shaped
//! code) is a public-contract drift — file under §13.1 with the exact
//! diff.

use console_core::session_launcher::{LaunchError, LaunchStep, SessionLauncher};
use console_core::session_state;
use console_db::DbPool;
use console_db::queries::{session_assignments, sessions};
use console_integration::{InjectableCoordinator, RuntimeHarness};
use console_runtime::grpc_pool::GrpcPool;
use tonic::{Code, Status};

// ---- test bodies ----------------------------------------------------------

#[tokio::test]
async fn submit_goal_unavailable() {
    let ctx = TestCtx::build().await;
    ctx.mock
        .inject_submit_goal(Status::unavailable("rt down"))
        .await;

    let err = ctx.launcher.launch(&ctx.sid).await.expect_err("launch");
    let (step, reason, _) = expect_step(&err);
    assert_eq!(step, LaunchStep::SubmitGoal);
    assert!(reason.contains("Unavailable") || reason.to_lowercase().contains("unavailable"));
    assert_eq!(ctx.mock.submit_goal_count(), 1);
    assert_eq!(ctx.mock.decompose_count(), 0);
    assert_eq!(ctx.mock.dispatch_count(), 0);
    assert_eq!(ctx.mock.abort_count(), 0, "no workspaces to roll back");
    assert_session_failed(&ctx.db, &ctx.sid).await;
}

#[tokio::test]
async fn submit_goal_invalid_argument() {
    let ctx = TestCtx::build().await;
    ctx.mock
        .inject_submit_goal(Status::invalid_argument("bad goal"))
        .await;

    let err = ctx.launcher.launch(&ctx.sid).await.expect_err("launch");
    let (step, reason, status) = expect_step(&err);
    assert_eq!(step, LaunchStep::SubmitGoal);
    assert_eq!(status.map(|s| s.code()), Some(Code::InvalidArgument));
    assert!(reason.to_lowercase().contains("bad goal"));
    assert_session_failed(&ctx.db, &ctx.sid).await;
}

#[tokio::test]
async fn submit_goal_unauthenticated() {
    let ctx = TestCtx::build().await;
    ctx.mock
        .inject_submit_goal(Status::unauthenticated("missing creds"))
        .await;

    let err = ctx.launcher.launch(&ctx.sid).await.expect_err("launch");
    let (step, _, status) = expect_step(&err);
    assert_eq!(step, LaunchStep::SubmitGoal);
    assert_eq!(status.map(|s| s.code()), Some(Code::Unauthenticated));
    assert_eq!(ctx.mock.abort_count(), 0);
}

#[tokio::test]
async fn decompose_unavailable_rolls_back_root() {
    let ctx = TestCtx::build().await;
    ctx.mock
        .inject_decompose(Status::unavailable("decompose flaked"))
        .await;

    let err = ctx.launcher.launch(&ctx.sid).await.expect_err("launch");
    let (step, _, _) = expect_step(&err);
    assert_eq!(step, LaunchStep::Decompose);
    assert_eq!(ctx.mock.submit_goal_count(), 1);
    assert_eq!(ctx.mock.decompose_count(), 1);
    assert_eq!(ctx.mock.dispatch_count(), 0);
    assert_eq!(
        ctx.mock.abort_count(),
        1,
        "root workspace must be aborted when decompose fails"
    );
    assert_session_failed(&ctx.db, &ctx.sid).await;
}

#[tokio::test]
async fn decompose_internal_rollback_root() {
    let ctx = TestCtx::build().await;
    ctx.mock
        .inject_decompose(Status::internal("decompose panicked"))
        .await;

    let err = ctx.launcher.launch(&ctx.sid).await.expect_err("launch");
    let (step, _, status) = expect_step(&err);
    assert_eq!(step, LaunchStep::Decompose);
    assert_eq!(status.map(|s| s.code()), Some(Code::Internal));
    assert_eq!(ctx.mock.abort_count(), 1);
}

#[tokio::test]
async fn dispatch_fails_on_task_1_rolls_back_root_only() {
    let ctx = TestCtx::build().await;
    // First Dispatch fails; no task workspace was ever created.
    ctx.mock
        .inject_dispatch(Status::unavailable("dispatch flaked"))
        .await;

    let err = ctx.launcher.launch(&ctx.sid).await.expect_err("launch");
    let (step, _, _) = expect_step(&err);
    assert_eq!(step, LaunchStep::Dispatch);
    assert_eq!(ctx.mock.dispatch_count(), 1, "short-circuits after failure");
    assert_eq!(
        ctx.mock.abort_count(),
        1,
        "only the root workspace needs rollback"
    );
    assert_session_failed(&ctx.db, &ctx.sid).await;
}

#[tokio::test]
async fn dispatch_fails_on_task_2_rolls_back_root_plus_first() {
    // 3 assignments → Decompose produces 3 tasks → Dispatch runs 3×.
    // Script: pass the first Dispatch (real runtime creates task_1_ws),
    // fail the second. Rollback targets = root + task_1_ws = 2 aborts.
    let ctx = TestCtx::build_with_assignments(3).await;
    ctx.mock.pass_dispatch().await;
    ctx.mock
        .inject_dispatch(Status::unavailable("dispatch #2 flaked"))
        .await;

    let err = ctx.launcher.launch(&ctx.sid).await.expect_err("launch");
    let (step, _, _) = expect_step(&err);
    assert_eq!(step, LaunchStep::Dispatch);
    assert_eq!(
        ctx.mock.dispatch_count(),
        2,
        "short-circuits after 2nd fails"
    );
    assert_eq!(
        ctx.mock.abort_count(),
        2,
        "rollback: root workspace + the one successful task workspace"
    );
    assert_session_failed(&ctx.db, &ctx.sid).await;
}

#[tokio::test]
async fn dispatch_fails_on_last_of_three_rolls_back_all() {
    // Same shape; failure on the 3rd (final) Dispatch. Rollback targets =
    // root + 2 task workspaces = 3 aborts.
    let ctx = TestCtx::build_with_assignments(3).await;
    ctx.mock.pass_dispatch().await;
    ctx.mock.pass_dispatch().await;
    ctx.mock
        .inject_dispatch(Status::unavailable("dispatch #3 flaked"))
        .await;

    let err = ctx.launcher.launch(&ctx.sid).await.expect_err("launch");
    let (step, _, _) = expect_step(&err);
    assert_eq!(step, LaunchStep::Dispatch);
    assert_eq!(ctx.mock.dispatch_count(), 3);
    assert_eq!(
        ctx.mock.abort_count(),
        3,
        "rollback: root + two successful task workspaces"
    );
    assert_session_failed(&ctx.db, &ctx.sid).await;
}

#[tokio::test]
async fn rollback_partial_failure_does_not_propagate() {
    // Dispatch's first call fails → rollback tries to abort the root
    // workspace. Abort returns Unavailable. The launcher swallows the abort
    // failure (logged as warn per launcher `:391`) and still returns the
    // Dispatch step error + marks the session FAILED.
    let ctx = TestCtx::build().await;
    ctx.mock
        .inject_dispatch(Status::unavailable("dispatch flaked"))
        .await;
    ctx.mock
        .inject_abort(Status::unavailable("abort flaked"))
        .await;

    let err = ctx.launcher.launch(&ctx.sid).await.expect_err("launch");
    let (step, _, _) = expect_step(&err);
    assert_eq!(
        step,
        LaunchStep::Dispatch,
        "abort failure must not mask the real cause"
    );
    assert_eq!(
        ctx.mock.abort_count(),
        1,
        "rollback was attempted exactly once — partial failure is tolerated not retried"
    );
    // Even though abort failed, the row still transitions FAILED. The
    // runtime's root workspace may linger; that's a cleanup concern, not
    // a correctness one — see perf-opt §13.1 for follow-up.
    assert_session_failed(&ctx.db, &ctx.sid).await;
}

#[tokio::test]
async fn rollback_total_failure_does_not_hang_or_panic() {
    // Every abort fails. The launcher must not retry beyond the configured
    // pass; must not deadlock; must still return the original step error.
    //
    // The current launcher does ONE abort per workspace (no retry loop).
    // So "every abort fails" == "one failed abort per workspace" and the
    // loop still terminates in O(workspaces) time.
    let ctx = TestCtx::build().await;
    ctx.mock
        .inject_dispatch(Status::unavailable("dispatch flaked"))
        .await;
    // Enough aborts queued to cover every possible rollback target. We only
    // have a root workspace here, so one is enough; pad in case.
    for _ in 0..4 {
        ctx.mock
            .inject_abort(Status::unavailable("abort flaked"))
            .await;
    }

    // 5 s is conservative; real walltime < 1 s. Prevents hangs from silently
    // becoming test-suite-long.
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        ctx.launcher.launch(&ctx.sid),
    )
    .await
    .expect("launch did not hang");
    let err = result.expect_err("launch must error");
    let (step, _, _) = expect_step(&err);
    assert_eq!(step, LaunchStep::Dispatch);
    assert_session_failed(&ctx.db, &ctx.sid).await;
}

// ---- fixtures -------------------------------------------------------------

/// Bundles a runtime + injectable coordinator mock + console DB + a
/// `LAUNCHING` session with one assignment. Drop order cleans children.
struct TestCtx {
    _rt: RuntimeHarness,
    mock: InjectableCoordinator,
    db: DbPool,
    launcher: SessionLauncher,
    sid: String,
}

impl TestCtx {
    async fn build() -> Self {
        Self::build_with_assignments(1).await
    }

    async fn build_with_assignments(count: usize) -> Self {
        let rt = RuntimeHarness::spawn_default().await.expect("runtime");
        let mock = InjectableCoordinator::spawn(&rt).await.expect("mock");
        let db = console_db::create_test_pool().await.expect("db");

        seed_user(&db, "u-1").await;
        let sid = format!("s-{}", uuid::Uuid::new_v4());
        seed_launching_session(&db, &sid, "u-1").await;
        for i in 0..count {
            let pid = format!("p-{i}");
            let aid = format!("a-{i}");
            seed_profile(&db, &pid, "u-1", "swe:implementer", &format!("role-{i}")).await;
            seed_assignment(&db, &sid, &aid, "swe:implementer", i as i64, &pid).await;
        }

        let pool = GrpcPool::new(&rt.agent_addr(), &rt.highway_addr(), &mock.addr());
        pool.connect().await;
        assert!(
            pool.coordinator().await.is_some(),
            "mock coordinator channel must be connected"
        );

        let launcher = SessionLauncher::new(pool.clone(), db.clone());
        TestCtx {
            _rt: rt,
            mock,
            db,
            launcher,
            sid,
        }
    }
}

fn expect_step(err: &LaunchError) -> (LaunchStep, String, Option<&tonic::Status>) {
    match err {
        LaunchError::Step {
            step,
            reason,
            source,
            ..
        } => (*step, reason.clone(), source.as_ref()),
        other => panic!("expected LaunchError::Step, got {other:?}"),
    }
}

async fn assert_session_failed(db: &DbPool, sid: &str) {
    let row = sessions::get_by_id(db, sid)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(
        row.state,
        session_state::FAILED,
        "rollback must mark the session FAILED (W2 §3.4)"
    );
}

async fn seed_user(db: &DbPool, id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO users (id, username, username_lower, display_name, password_hash,
            console_role, must_change_password, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind("h")
    .bind("operator")
    .bind(0_i64)
    .bind("2026-04-15T00:00:00Z")
    .bind("2026-04-15T00:00:00Z")
    .execute(db)
    .await
    .expect("seed user");
}

async fn seed_launching_session(db: &DbPool, sid: &str, owner: &str) {
    let row = sessions::SessionRow {
        id: sid.into(),
        name: Some(sid.into()),
        owner_user_id: owner.into(),
        vertical: "swe".into(),
        workflow: "wf".into(),
        context: None,
        coordinator_workspace_id: None,
        state: session_state::LAUNCHING.into(),
        created_at: "2026-04-15T00:00:00Z".into(),
        launched_at: None,
        closed_at: None,
        budget_max_cost_micros: None,
        budget_max_tokens: None,
        budget_max_wall_time_ms: None,
    };
    sessions::insert_session(db, &row)
        .await
        .expect("insert launching session");
}

async fn seed_profile(db: &DbPool, pid: &str, owner: &str, role_ref: &str, name: &str) {
    sqlx::query(
        "INSERT INTO profiles (id, version, name, role_ref, llm_provider, llm_model,
            autonomy, owner_user_id, visibility, is_current, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(pid)
    .bind(1_i64)
    .bind(name)
    .bind(role_ref)
    .bind("stub")
    .bind("stub-model-1")
    .bind("supervised")
    .bind(owner)
    .bind("private")
    .bind(1_i64)
    .bind("2026-04-15T00:00:00Z")
    .execute(db)
    .await
    .expect("insert profile");
}

async fn seed_assignment(
    db: &DbPool,
    sid: &str,
    aid: &str,
    role_ref: &str,
    slot: i64,
    profile_id: &str,
) {
    let row = session_assignments::SessionAssignmentRow {
        id: aid.into(),
        session_id: sid.into(),
        role_ref: role_ref.into(),
        stage_id: None,
        slot_position: slot,
        profile_id: profile_id.into(),
        profile_version: 1,
        workspace_id: None,
        budget_max_cost_micros: None,
        budget_max_tokens: None,
        budget_max_wall_time_ms: None,
    };
    session_assignments::insert_assignment(db, &row)
        .await
        .expect("insert assignment");
}
