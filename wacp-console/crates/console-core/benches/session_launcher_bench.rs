//! `SessionLauncher::launch` wall-time sweep across N assignments.
//!
//! Scenario: drive the full `SubmitGoal → Decompose(N) → Dispatch×N → DB
//! finalize` sequence against `ProgrammableCoordinator` (in-process,
//! canned responses — no real `wacp-runtime` child process). Measures
//! the end-to-end launch latency at N ∈ {1, 3, 10, 30}.
//!
//! HEALTH-LOG §8 / backend-perf-baseline-plan C3. Closes the placeholder
//! left by the C3 landing when `InjectableCoordinator` was still pinned
//! to `console-integration`. The mock now lives in `console-test-support`
//! (see `impl/archive/health-log-residual-plan.md` P1.a); this bench uses
//! the already-standalone `ProgrammableCoordinator` rather than
//! `InjectableCoordinator` since the latter forwards to a real runtime
//! upstream — wrong shape for a bench hot loop.
//!
//! Baseline target: informational only. Real numbers recorded in
//! `docs/perf-baseline-2026-04-20.md` the first time this bench lands
//! on `main`; regressions are triaged against that baseline.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use tokio::runtime::Runtime;

use console_core::session_launcher::SessionLauncher;
use console_core::session_state;
use console_db::DbPool;
use console_db::create_test_pool;
use console_db::queries::session_assignments::SessionAssignmentRow;
use console_db::queries::sessions::SessionRow;
use console_runtime::grpc_pool::GrpcPool;
use console_runtime::proto;
use console_test_support::ProgrammableCoordinator;

struct Harness {
    db: DbPool,
    pool: Arc<GrpcPool>,
    coord: ProgrammableCoordinator,
}

async fn build_harness() -> Harness {
    let db = create_test_pool().await.expect("test pool");
    insert_user(&db, "u-1").await;
    insert_profile(&db, "profile-1").await;

    let coord = ProgrammableCoordinator::new();
    let (addr, _handle) = coord.clone().spawn().await.expect("spawn coord");
    // Agent/highway dial unused ports — launcher never touches them.
    let pool = GrpcPool::new("[::1]:1", "[::1]:1", &addr.to_string());
    pool.connect().await;

    Harness { db, pool, coord }
}

async fn seed_session(db: &DbPool, session_id: &str, n: usize) {
    let session = SessionRow {
        id: session_id.into(),
        name: Some(format!("bench-session-{session_id}")),
        owner_user_id: "u-1".into(),
        vertical: "healthcare".into(),
        workflow: "bench-workflow".into(),
        context: Some(r#"{"patient_id":"P1"}"#.into()),
        coordinator_workspace_id: None,
        state: session_state::LAUNCHING.into(),
        created_at: "2026-04-22T00:00:00Z".into(),
        launched_at: None,
        closed_at: None,
        budget_max_cost_micros: Some(1_000_000),
        budget_max_tokens: Some(100_000),
        budget_max_wall_time_ms: Some(60_000),
    };
    console_db::queries::sessions::insert_session(db, &session)
        .await
        .expect("insert session");

    for i in 0..n {
        let asgn = SessionAssignmentRow {
            id: format!("asgn-{session_id}-{i}"),
            session_id: session_id.into(),
            role_ref: format!("role-{i}"),
            stage_id: None,
            slot_position: i as i64,
            profile_id: "profile-1".into(),
            profile_version: 1,
            workspace_id: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
        };
        console_db::queries::session_assignments::insert_assignment(db, &asgn)
            .await
            .expect("insert assignment");
    }
}

/// Re-program coord for a launch of N assignments. Queues N dispatch
/// responses (launch consumes exactly N on happy path, so the queue
/// drains to empty between iterations — no clear needed).
fn program_coord(coord: &ProgrammableCoordinator, n: usize) {
    coord.set_submit_goal_ok(proto::SubmitGoalResponse {
        goal_id: "g-bench".into(),
        root_workspace_id: "ws-root".into(),
    });
    let task_ids: Vec<String> = (0..n).map(|i| format!("t-{i}")).collect();
    coord.set_decompose_ok(proto::DecomposeResponse { task_ids });
    for i in 0..n {
        coord.queue_dispatch(Ok(proto::DispatchResponse {
            workspace_id: format!("ws-{i}"),
            task_id: format!("t-{i}"),
        }));
    }
}

async fn insert_user(db: &DbPool, user_id: &str) {
    sqlx::query(
        "INSERT INTO users (id, username, username_lower, display_name, password_hash,
            console_role, must_change_password, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind("placeholder-hash")
    .bind("operator")
    .bind(0_i64)
    .bind("2026-04-22T00:00:00Z")
    .bind("2026-04-22T00:00:00Z")
    .execute(db)
    .await
    .expect("insert user");
}

async fn insert_profile(db: &DbPool, profile_id: &str) {
    sqlx::query(
        "INSERT INTO profiles (id, version, name, description, tags, role_ref, llm_provider, llm_model,
            llm_temperature, llm_max_tokens, autonomy, tool_allowlist, tool_denylist,
            budget_max_cost_micros, budget_max_tokens, budget_max_wall_time_ms, budget_warning_threshold,
            owner_user_id, visibility, is_current, created_at, deleted_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(profile_id)
    .bind(1_i64)
    .bind(format!("bench-{profile_id}"))
    .bind::<Option<String>>(None)
    .bind::<Option<String>>(None)
    .bind("role-0")
    .bind("anthropic")
    .bind("claude-sonnet-4-6")
    .bind(0.7_f64)
    .bind(4096_i64)
    .bind("supervised")
    .bind::<Option<String>>(Some(r#"["tool-a"]"#.into()))
    .bind::<Option<String>>(None)
    .bind(500_000_i64)
    .bind(50_000_i64)
    .bind(30_000_i64)
    .bind(0.8_f64)
    .bind("u-1")
    .bind("private")
    .bind(true)
    .bind("2026-04-22T00:00:00Z")
    .bind::<Option<String>>(None)
    .execute(db)
    .await
    .expect("insert profile");
}

fn bench_launch(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("session_launcher_launch");
    group.measurement_time(Duration::from_secs(5));

    // Shared monotonic counter — criterion calls `iter_custom`'s closure
    // multiple times per sample and resets its local `iters` index, so a
    // per-closure-local counter produces duplicate session_ids across
    // samples. An atomic u64 outside the closure keeps every session_id
    // unique across the full bench run.
    let sid_counter = AtomicU64::new(0);

    for &n in &[1usize, 3, 10, 30] {
        // One harness per N. The launcher is stateless re: harness
        // identity, so we can reuse it across iters — only the session
        // row + coord queues need refresh.
        let harness = rt.block_on(build_harness());
        let launcher = SessionLauncher::new(harness.pool.clone(), harness.db.clone());

        group.bench_with_input(
            BenchmarkId::from_parameter(n),
            &n,
            |b, &n_assignments| {
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let mut total = Duration::ZERO;
                        for _ in 0..iters {
                            let seq = sid_counter.fetch_add(1, Ordering::Relaxed);
                            let sid = format!("bench-n{n_assignments}-s{seq}");
                            seed_session(&harness.db, &sid, n_assignments).await;
                            program_coord(&harness.coord, n_assignments);
                            let start = Instant::now();
                            black_box(launcher.launch(&sid).await.expect("launch"));
                            total += start.elapsed();
                        }
                        total
                    })
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_launch);
criterion_main!(benches);
