//! §13.7.8 I2 — startup recovery matrix.
//!
//! Scenarios covering every realized branch of `console_core::recovery::run`
//! against a scripted runtime state:
//!
//!   - resumed: ACTIVE + live workspace → respawn monitor
//!   - stuck: ACTIVE with no `coordinator_workspace_id` → FAILED
//!   - notfound: ACTIVE + workspace the runtime doesn't know → FAILED
//!   - unavailable: ACTIVE + runtime down → session stays ACTIVE
//!   - terminal_aborted: ACTIVE + runtime reports `Failed` (via AbortWorkspace)
//!     → FAILED
//!   - terminal_closed: ACTIVE + runtime reports `Closed` (via agent Complete
//!     → WA3.6 auto-integration) → COMPLETED (§11.4 follow-up, 2026-04-22)
//!   - mixed: three sessions, three distinct outcomes in one pass
//!   - report_fields: direct `recovery::run` call asserting RecoveryReport
//!     counters
//!
//! **Not covered (deferred, see `HEALTH-LOG.md` §13.2):**
//! - DB-degraded boot. `FaultyDb::hold_write_lock` holds a WRITE lock, but
//!   `recovery::run` only reads from sessions on boot — the write lock
//!   doesn't block the read path. A different fault-injection mode would
//!   be needed; filed as §13.2 follow-up.

use std::time::Duration;

use console_core::recovery::{self, RecoveryFailureReason};
use console_core::session_state;
use console_db::DbPool;
use console_db::queries::sessions;
use console_integration::{ConsoleHarness, RuntimeHarness};
use tonic::transport::Channel;
use wacp_transport::wacp_v1;
use wacp_transport::wacp_v1::coordinator_service_client::CoordinatorServiceClient;
use wacp_transport::wacp_v1::highway_service_client::HighwayServiceClient;

// ---- helpers --------------------------------------------------------------

async fn coord_client(rt: &RuntimeHarness) -> CoordinatorServiceClient<Channel> {
    let url = format!("http://{}", rt.coordinator_addr());
    let channel = Channel::from_shared(url)
        .expect("coord url")
        .connect()
        .await
        .expect("coord connect");
    CoordinatorServiceClient::new(channel)
}

/// Create a live workspace in the runtime via `SubmitGoal`. Returns the
/// resulting `root_workspace_id`, valid for later `GetWorkspace` probes.
async fn submit_live_goal(rt: &RuntimeHarness, desc: &str) -> String {
    let mut client = coord_client(rt).await;
    let resp = client
        .submit_goal(wacp_v1::SubmitGoalRequest {
            description: desc.into(),
            context: Vec::new(),
            client_request_id: String::new(),
        })
        .await
        .expect("submit_goal")
        .into_inner();
    assert!(!resp.root_workspace_id.is_empty());
    resp.root_workspace_id
}

/// Close a workspace so `GetWorkspace` will report a terminal state.
async fn abort_workspace(rt: &RuntimeHarness, ws_id: &str) {
    let mut client = coord_client(rt).await;
    client
        .abort_workspace(wacp_v1::AbortWorkspaceRequest {
            workspace_id: ws_id.into(),
            reason: "recovery-matrix-test".into(),
            client_request_id: String::new(),
        })
        .await
        .expect("abort_workspace");
}

async fn highway_client(rt: &RuntimeHarness) -> HighwayServiceClient<Channel> {
    let url = format!("http://{}", rt.highway_addr());
    let channel = Channel::from_shared(url)
        .expect("highway url")
        .connect()
        .await
        .expect("highway connect");
    HighwayServiceClient::new(channel)
}

/// Drive a workspace all the way to internal `Closed` state through the
/// runtime's real WA3.6 auto-integration path — activate via directive,
/// bind an agent via `wacp-sdk`, emit `Complete`, then poll `GetWorkspace`
/// until the runtime reports `Closed`. Panics on a 5 s timeout.
///
/// Plan deviation: the §B.1 original shape was a `RuntimeHarness::mark_
/// workspace_closed(ws_id)` test-only method that pokes `WorkspaceTree::
/// get_mut(...).status = Closed`. That only works for in-process runtime;
/// `RuntimeHarness` spawns `wacp-runtime` as a child binary (see its
/// module docstring), so direct state-poking isn't reachable without
/// adding a test-only admin gRPC surface — exactly the "scope-creepy"
/// branch the plan flagged. The `wacp-sdk`-based path used in
/// `lifecycle.rs:311–321` is the short-path helper the plan was looking
/// for: ~15 lines of test code, no admin RPC, exercises the real WA3.6
/// chain. Kept local to this file since only recovery tests need it.
async fn drive_to_closed(rt: &RuntimeHarness, ws_id: &str) {
    let mut coord = coord_client(rt).await;
    coord
        .send_directive(wacp_v1::SendDirectiveRequest {
            workspace_id: ws_id.into(),
            payload: b"activate".to_vec(),
            client_request_id: String::new(),
        })
        .await
        .expect("send_directive");

    let agent = wacp_sdk::Agent::connect(wacp_sdk::AgentConfig {
        runtime_url: format!("http://{}", rt.agent_addr()),
        workspace_id: wacp_types::WorkspaceId::from(ws_id),
        auth_token: "recovery-closed-token".into(),
    })
    .await
    .expect("agent connect");
    agent
        .signal(wacp_types::SignalType::Complete)
        .await
        .expect("emit complete");

    let mut highway = highway_client(rt).await;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let view = highway
            .get_workspace(wacp_v1::GetWorkspaceRequest {
                workspace_id: ws_id.into(),
            })
            .await
            .expect("get_workspace")
            .into_inner();
        if view.state == wacp_v1::WorkspaceState::Closed as i32 {
            return;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "workspace {ws_id} did not reach Closed within 5s (last state: {})",
                view.state
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
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

async fn seed_active_session(db: &DbPool, sid: &str, owner: &str, coord_ws: Option<&str>) {
    let row = sessions::SessionRow {
        id: sid.into(),
        name: Some(sid.into()),
        owner_user_id: owner.into(),
        vertical: "swe".into(),
        workflow: "wf".into(),
        context: None,
        coordinator_workspace_id: coord_ws.map(Into::into),
        state: session_state::ACTIVE.into(),
        created_at: "2026-04-15T00:00:00Z".into(),
        launched_at: Some("2026-04-15T00:00:00Z".into()),
        closed_at: None,
        budget_max_cost_micros: None,
        budget_max_tokens: None,
        budget_max_wall_time_ms: None,
    };
    sessions::insert_session(db, &row)
        .await
        .expect("seed session");
}

async fn session_state(db: &DbPool, sid: &str) -> String {
    sessions::get_by_id(db, sid)
        .await
        .expect("get")
        .expect("row")
        .state
}

// ---- tests ----------------------------------------------------------------

#[tokio::test]
async fn resumed_active_session_with_live_workspace() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");

    // First, create a real workspace in the runtime the console can find.
    let live_ws = submit_live_goal(&rt, "recovery-resume-goal").await;

    // Seed the console DB with an ACTIVE session pointing at it.
    let db = console_db::create_test_pool().await.expect("db");
    seed_user(&db, "u-1").await;
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_active_session(&db, &sid, "u-1", Some(&live_ws)).await;

    // Spawn the console → recovery runs on boot.
    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");

    // Recovery should have respawned a monitor. `spawn_with_db` runs
    // recovery synchronously before returning, so inspect immediately.
    assert!(
        console
            .state
            .active_sessions
            .read()
            .await
            .contains_key(&sid),
        "resumed session must be in active_sessions map"
    );
    // DB row still ACTIVE — resume doesn't mutate state.
    assert_eq!(session_state(&db, &sid).await, session_state::ACTIVE);
}

#[tokio::test]
async fn stuck_without_coord_ws_marked_failed() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let db = console_db::create_test_pool().await.expect("db");
    seed_user(&db, "u-1").await;
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    // ACTIVE row without coordinator_workspace_id — "stuck" shape.
    seed_active_session(&db, &sid, "u-1", None).await;

    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");

    assert!(
        !console
            .state
            .active_sessions
            .read()
            .await
            .contains_key(&sid),
        "stuck session must not have a monitor"
    );
    assert_eq!(session_state(&db, &sid).await, session_state::FAILED);
}

#[tokio::test]
async fn workspace_notfound_marked_failed() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let db = console_db::create_test_pool().await.expect("db");
    seed_user(&db, "u-1").await;
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    // ACTIVE row with a workspace id the runtime does NOT know about.
    seed_active_session(&db, &sid, "u-1", Some("ws-does-not-exist")).await;

    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");

    assert!(
        !console
            .state
            .active_sessions
            .read()
            .await
            .contains_key(&sid),
        "not-found session must not have a monitor"
    );
    assert_eq!(session_state(&db, &sid).await, session_state::FAILED);
}

#[tokio::test]
async fn runtime_unavailable_keeps_session_active() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    // Seed a session that would otherwise probe fine.
    let live_ws = submit_live_goal(&rt, "unavailable-probe-goal").await;
    let db = console_db::create_test_pool().await.expect("db");
    seed_user(&db, "u-1").await;
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_active_session(&db, &sid, "u-1", Some(&live_ws)).await;

    // Kill the runtime BEFORE spawning console. The GrpcPool will fail to
    // connect, so `pool.highway().await` returns None → SkippedUnavailable.
    drop(rt);

    // ConsoleHarness requires a RuntimeHarness reference, but we've dropped
    // the process. Spawn a fresh one that will be killed right away to
    // satisfy the ABI, then immediately kill so recovery encounters a
    // down runtime. Easier path: spawn a *second* runtime, kill it, and
    // pass its (now-stale) addresses to the console.
    let mut rt_dead = RuntimeHarness::spawn_default().await.expect("runtime-dead");
    rt_dead.kill();
    // Give the OS a moment to tear down the listen socket.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let console = ConsoleHarness::spawn_with_db(&rt_dead, db.clone())
        .await
        .expect("console");

    assert!(
        !console
            .state
            .active_sessions
            .read()
            .await
            .contains_key(&sid),
        "unavailable session must not have a monitor (probe was skipped)"
    );
    // Session stays ACTIVE — recovery re-probes on next restart.
    assert_eq!(session_state(&db, &sid).await, session_state::ACTIVE);
}

#[tokio::test]
async fn terminal_workspace_aborted_marked_failed() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let aborted_ws = submit_live_goal(&rt, "terminal-abort-goal").await;
    // `AbortWorkspace` invokes `cascade_failure`, landing the workspace in
    // internal `WorkspaceState::Failed` (wacp-coordinator/src/tree.rs:256).
    abort_workspace(&rt, &aborted_ws).await;

    let db = console_db::create_test_pool().await.expect("db");
    seed_user(&db, "u-1").await;
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_active_session(&db, &sid, "u-1", Some(&aborted_ws)).await;

    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");

    assert!(
        !console
            .state
            .active_sessions
            .read()
            .await
            .contains_key(&sid),
        "terminal session must not have a monitor"
    );
    // Failed → FAILED per `recovery::recover_one` (recovery.rs:173–175).
    // Sibling Closed → COMPLETED branch covered by
    // `terminal_workspace_closed_marked_completed` below.
    assert_eq!(session_state(&db, &sid).await, session_state::FAILED);
}

#[tokio::test]
async fn terminal_workspace_closed_marked_completed() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let closed_ws = submit_live_goal(&rt, "terminal-closed-goal").await;
    // `drive_to_closed` routes Active → Integrating → Closed through the
    // real WA3.6 auto-integration path (agent emits Complete via wacp-sdk).
    // No admin RPC; no in-process state-poking. See helper doc for the
    // §B.1 plan-deviation rationale.
    drive_to_closed(&rt, &closed_ws).await;

    let db = console_db::create_test_pool().await.expect("db");
    seed_user(&db, "u-1").await;
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_active_session(&db, &sid, "u-1", Some(&closed_ws)).await;

    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");

    assert!(
        !console
            .state
            .active_sessions
            .read()
            .await
            .contains_key(&sid),
        "terminal session must not have a monitor"
    );
    // Closed (non-Failed terminal) → COMPLETED via `recovery::recover_one`
    // (recovery.rs:173–175). Covers the branch HEALTH-LOG §11.4's
    // follow-up note flagged as missing integration coverage after the
    // 2026-04-18 enum-offset sweep renamed the prior
    // `terminal_workspace_closed_marked_completed` test to
    // `terminal_workspace_aborted_marked_failed` (its coincidentally-green
    // path was actually driving Failed, not Closed — see §11.4).
    assert_eq!(session_state(&db, &sid).await, session_state::COMPLETED);
}

#[tokio::test]
async fn multi_session_mixed_outcomes_in_one_pass() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");

    // 1 resumed, 1 stuck, 1 not-found.
    let live_ws = submit_live_goal(&rt, "mixed-live").await;
    let db = console_db::create_test_pool().await.expect("db");
    seed_user(&db, "u-1").await;

    let sid_live = format!("s-{}-live", uuid::Uuid::new_v4());
    let sid_stuck = format!("s-{}-stuck", uuid::Uuid::new_v4());
    let sid_nf = format!("s-{}-nf", uuid::Uuid::new_v4());
    seed_active_session(&db, &sid_live, "u-1", Some(&live_ws)).await;
    seed_active_session(&db, &sid_stuck, "u-1", None).await;
    seed_active_session(&db, &sid_nf, "u-1", Some("ws-does-not-exist")).await;

    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");

    // active_sessions has only the resumed one.
    let map = console.state.active_sessions.read().await;
    assert_eq!(map.len(), 1);
    assert!(map.contains_key(&sid_live));
    drop(map);

    // DB states reflect each outcome.
    assert_eq!(session_state(&db, &sid_live).await, session_state::ACTIVE);
    assert_eq!(session_state(&db, &sid_stuck).await, session_state::FAILED);
    assert_eq!(session_state(&db, &sid_nf).await, session_state::FAILED);
}

#[tokio::test]
async fn recovery_report_fields_sum_correctly() {
    // Direct `recovery::run` call with a hand-built state; bypasses
    // ConsoleHarness so we can inspect the returned RecoveryReport itself
    // rather than only its side-effects. Proves the report struct's
    // counters match what the DB + active_sessions end up showing.
    use arc_swap::ArcSwap;
    use console_core::event_enricher::EventEnricher;
    use console_core::refusal_synthesizer::RefusalSynthesizer;
    use console_core::session_monitor::MonitorConfig;
    use console_core::taxonomy_builder;
    use console_runtime::grpc_pool::GrpcPool;
    use std::collections::HashMap;
    use std::sync::Arc;

    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let live_ws = submit_live_goal(&rt, "report-live").await;
    let db = console_db::create_test_pool().await.expect("db");
    seed_user(&db, "u-1").await;
    let sid_live = format!("s-{}-live", uuid::Uuid::new_v4());
    let sid_stuck = format!("s-{}-stuck", uuid::Uuid::new_v4());
    seed_active_session(&db, &sid_live, "u-1", Some(&live_ws)).await;
    seed_active_session(&db, &sid_stuck, "u-1", None).await;

    let pool = GrpcPool::new(&rt.agent_addr(), &rt.highway_addr(), &rt.coordinator_addr());
    pool.connect().await;
    let taxonomy = Arc::new(ArcSwap::from_pointee(
        taxonomy_builder::build_index(None, &[], &[]).index,
    ));
    let active = Arc::new(tokio::sync::RwLock::new(HashMap::new()));

    let report = recovery::run(
        db,
        pool,
        EventEnricher::new(taxonomy),
        RefusalSynthesizer::new(),
        active,
        MonitorConfig::default(),
    )
    .await;

    assert_eq!(report.resumed, vec![sid_live]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, sid_stuck);
    assert!(matches!(
        report.failed[0].1,
        RecoveryFailureReason::StuckInLaunching
    ));
    assert!(report.synced_terminal.is_empty());
    assert!(report.skipped_unavailable.is_empty());
}
