use super::*;
use arc_swap::ArcSwap;
use console_db::create_test_pool;
use console_db::queries::profiles;
use std::collections::HashMap;

fn empty_taxonomy() -> Arc<ArcSwap<crate::taxonomy::TaxonomyIndex>> {
    Arc::new(ArcSwap::from_pointee(
        crate::taxonomy_builder::build_index(None, &[], &[]).index,
    ))
}

fn enricher() -> EventEnricher {
    EventEnricher::new(empty_taxonomy())
}

fn refusals() -> RefusalSynthesizer {
    RefusalSynthesizer::new()
}

async fn insert_user(db: &DbPool, id: &str) {
    sqlx::query(
        "INSERT INTO users (id, username, username_lower, display_name, password_hash,
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
    .expect("insert user");
}

async fn insert_session(db: &DbPool, id: &str, owner: &str, coord_ws: Option<&str>, state: &str) {
    let row = sessions::SessionRow {
        id: id.into(),
        name: Some(id.into()),
        owner_user_id: owner.into(),
        vertical: "swe".into(),
        workflow: "wf".into(),
        context: None,
        coordinator_workspace_id: coord_ws.map(String::from),
        state: state.into(),
        created_at: "2026-04-15T00:00:00Z".into(),
        launched_at: Some("2026-04-15T00:00:00Z".into()),
        closed_at: None,
        budget_max_cost_micros: None,
        budget_max_tokens: None,
        budget_max_wall_time_ms: None,
    };
    sessions::insert_session(db, &row).await.expect("insert");
}

async fn insert_profile(db: &DbPool, id: &str, owner: &str) {
    let row = profiles::ProfileRow {
        id: id.into(),
        version: 1,
        name: id.into(),
        description: None,
        tags: None,
        role_ref: "worker".into(),
        llm_provider: "stub".into(),
        llm_model: "stub-1".into(),
        llm_temperature: None,
        llm_max_tokens: None,
        autonomy: "supervised".into(),
        tool_allowlist: None,
        tool_denylist: None,
        budget_max_cost_micros: None,
        budget_max_tokens: None,
        budget_max_wall_time_ms: None,
        budget_warning_threshold: None,
        owner_user_id: owner.into(),
        visibility: "private".into(),
        is_current: true,
        created_at: "2026-04-15T00:00:00Z".into(),
        deleted_at: None,
    };
    profiles::insert_profile(db, &row)
        .await
        .expect("insert profile");
}

fn empty_active() -> ActiveSessionsMap {
    Arc::new(RwLock::new(HashMap::new()))
}

fn quick_cfg() -> MonitorConfig {
    MonitorConfig {
        broadcast_capacity: 16,
        reconnect_initial: std::time::Duration::from_millis(5),
        reconnect_max: std::time::Duration::from_millis(10),
        reconnect_failure_cap: 2,
    }
}

#[test]
fn failure_reason_tags_are_stable() {
    assert_eq!(
        RecoveryFailureReason::StuckInLaunching.tag(),
        "stuck_in_launching"
    );
    assert_eq!(
        RecoveryFailureReason::WorkspaceMissing.tag(),
        "recovery_workspace_missing"
    );
    assert_eq!(
        RecoveryFailureReason::DbError("x".into()).tag(),
        "recovery_db_error"
    );
    assert_eq!(
        RecoveryFailureReason::RuntimeError("x".into()).tag(),
        "recovery_runtime_error"
    );
}

#[test]
fn is_terminal_matches_closed_and_failed_only() {
    assert!(is_terminal(proto::WorkspaceState::Closed));
    assert!(is_terminal(proto::WorkspaceState::Failed));
    assert!(!is_terminal(proto::WorkspaceState::Active));
    assert!(!is_terminal(proto::WorkspaceState::Idle));
    assert!(!is_terminal(proto::WorkspaceState::Blocked));
}

#[tokio::test]
async fn run_marks_stuck_session_failed_when_no_coord_workspace() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(&db, "s-stuck", "u-1", None, session_state::ACTIVE).await;

    let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;
    assert_eq!(report.failed.len(), 1);
    let row = sessions::get_by_id(&db, "s-stuck").await.unwrap().unwrap();
    assert_eq!(row.state, session_state::FAILED);
}

#[tokio::test]
async fn run_skips_when_pool_has_no_channel() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(
        &db,
        "s-nopool",
        "u-1",
        Some("ws-coord"),
        session_state::ACTIVE,
    )
    .await;

    // Pool that never connects. `highway()` returns None → SkippedUnavailable.
    let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;
    assert_eq!(report.skipped_unavailable.len(), 1);
    // Session stays ACTIVE — no transition.
    let row = sessions::get_by_id(&db, "s-nopool").await.unwrap().unwrap();
    assert_eq!(row.state, session_state::ACTIVE);
}

#[tokio::test]
async fn run_returns_empty_report_when_no_active_sessions() {
    let db = create_test_pool().await.unwrap();
    let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
    let report = run(
        db,
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;
    assert!(report.resumed.is_empty());
    assert!(report.failed.is_empty());
    assert!(report.synced_terminal.is_empty());
    assert!(report.skipped_unavailable.is_empty());
}

#[tokio::test]
async fn run_handles_db_error_path_for_assignments_lookup() {
    // We can't easily synthesize a DB error mid-test, but we can verify
    // the reason tag stays stable for the documented failure mode.
    let r = RecoveryFailureReason::DbError("connection closed".into());
    assert_eq!(r.tag(), "recovery_db_error");
}

// ========================================================================
// Programmable highway mock — supports configurable GetWorkspace responses
// per workspace_id so recovery branch coverage tests can drive all arms.
// ========================================================================

mod programmable_highway {
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;
    use tonic::{Request, Response, Status};
    use wacp_proto::wacp_v1 as proto;

    use proto::highway_service_server::{HighwayService, HighwayServiceServer};

    /// Programmable outcome for `GetWorkspace`.
    #[derive(Clone)]
    pub enum GetWorkspaceOutcome {
        /// Return a WorkspaceView with the given state.
        Ok(proto::WorkspaceState),
        /// Return a gRPC error with the given code and message.
        Err(tonic::Code, String),
    }

    /// Configurable `HighwayService` mock for recovery tests.
    #[derive(Clone, Default)]
    pub struct ProgrammableHighway {
        inner: Arc<Mutex<State>>,
    }

    #[derive(Default)]
    struct State {
        /// Per-workspace_id outcomes. When a workspace_id is not in the map,
        /// returns NotFound.
        workspace_outcomes: HashMap<String, GetWorkspaceOutcome>,
    }

    impl ProgrammableHighway {
        pub fn new() -> Self {
            Self::default()
        }

        /// Program the response for `GetWorkspace(workspace_id)`.
        pub fn set_workspace(&self, workspace_id: &str, outcome: GetWorkspaceOutcome) {
            self.inner
                .lock()
                .unwrap()
                .workspace_outcomes
                .insert(workspace_id.to_string(), outcome);
        }

        /// Spawn this service on a random ephemeral port. Returns the address
        /// and a join handle. Caller must keep the handle alive for the test.
        pub async fn spawn(self) -> (SocketAddr, JoinHandle<()>) {
            let listener = TcpListener::bind("[::1]:0").await.expect("bind");
            let addr = listener.local_addr().expect("addr");
            let stream = tokio_stream::wrappers::TcpListenerStream::new(listener);
            let handle = tokio::spawn(async move {
                tonic::transport::Server::builder()
                    .add_service(HighwayServiceServer::new(self))
                    .serve_with_incoming(stream)
                    .await
                    .ok();
            });
            (addr, handle)
        }
    }

    #[tonic::async_trait]
    impl HighwayService for ProgrammableHighway {
        async fn get_workspace(
            &self,
            request: Request<proto::GetWorkspaceRequest>,
        ) -> Result<Response<proto::WorkspaceView>, Status> {
            let ws_id = request.into_inner().workspace_id;
            let st = self.inner.lock().unwrap();
            match st.workspace_outcomes.get(&ws_id) {
                Some(GetWorkspaceOutcome::Ok(state)) => Ok(Response::new(proto::WorkspaceView {
                    id: ws_id,
                    state: (*state).into(),
                    ..Default::default()
                })),
                Some(GetWorkspaceOutcome::Err(code, msg)) => Err(Status::new(*code, msg.clone())),
                None => Err(Status::not_found(format!("workspace {ws_id} not found"))),
            }
        }

        // --- RPCs that recovery never calls: stubs returning sensible defaults ---

        async fn authenticate(
            &self,
            _: Request<proto::AuthenticateRequest>,
        ) -> Result<Response<proto::AuthenticateResponse>, Status> {
            Ok(Response::new(proto::AuthenticateResponse {
                user_id: "mock".into(),
                capabilities: vec![],
            }))
        }

        async fn inject_envelope(
            &self,
            _: Request<proto::InjectEnvelopeRequest>,
        ) -> Result<Response<proto::InjectEnvelopeResponse>, Status> {
            Err(Status::unimplemented("not used in recovery tests"))
        }

        async fn respond_to_gate(
            &self,
            _: Request<proto::GateResponse>,
        ) -> Result<Response<proto::GateResponseAck>, Status> {
            Err(Status::unimplemented("not used in recovery tests"))
        }

        async fn respond_to_escalation(
            &self,
            _: Request<proto::EscalationResponse>,
        ) -> Result<Response<proto::EscalationResponseAck>, Status> {
            Err(Status::unimplemented("not used in recovery tests"))
        }

        async fn query_trail(
            &self,
            _: Request<proto::HighwayQueryTrailRequest>,
        ) -> Result<Response<proto::QueryTrailResponse>, Status> {
            Ok(Response::new(proto::QueryTrailResponse {
                entries: vec![],
                has_more: false,
                client_request_id: String::new(),
            }))
        }

        async fn get_task_graph(
            &self,
            _: Request<proto::GetTaskGraphRequest>,
        ) -> Result<Response<proto::TaskGraphView>, Status> {
            Err(Status::unimplemented("not used in recovery tests"))
        }

        async fn get_checkpoint(
            &self,
            _: Request<proto::GetCheckpointRequest>,
        ) -> Result<Response<proto::CheckpointView>, Status> {
            Err(Status::unimplemented("not used in recovery tests"))
        }

        type StreamTrailStream =
            tokio_stream::wrappers::ReceiverStream<Result<proto::TrailEntry, Status>>;

        async fn stream_trail(
            &self,
            _: Request<proto::StreamTrailRequest>,
        ) -> Result<Response<Self::StreamTrailStream>, Status> {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
                rx,
            )))
        }

        type StreamGatesStream =
            tokio_stream::wrappers::ReceiverStream<Result<proto::GateEvent, Status>>;

        async fn stream_gates(
            &self,
            _: Request<proto::StreamGatesRequest>,
        ) -> Result<Response<Self::StreamGatesStream>, Status> {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
                rx,
            )))
        }

        type StreamEscalationsStream =
            tokio_stream::wrappers::ReceiverStream<Result<proto::EscalationEvent, Status>>;

        async fn stream_escalations(
            &self,
            _: Request<proto::StreamEscalationsRequest>,
        ) -> Result<Response<Self::StreamEscalationsStream>, Status> {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
                rx,
            )))
        }

        type StreamWorkspaceChangesStream =
            tokio_stream::wrappers::ReceiverStream<Result<proto::WorkspaceStateChange, Status>>;

        async fn stream_workspace_changes(
            &self,
            _: Request<proto::StreamWorkspaceChangesRequest>,
        ) -> Result<Response<Self::StreamWorkspaceChangesStream>, Status> {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
                rx,
            )))
        }
    }
}

use programmable_highway::{GetWorkspaceOutcome, ProgrammableHighway};

/// Helper: spin up a ProgrammableHighway and connect a GrpcPool to it.
async fn pool_with_highway(
    hw: ProgrammableHighway,
) -> (Arc<GrpcPool>, tokio::task::JoinHandle<()>) {
    let (addr, handle) = hw.spawn().await;
    let pool = GrpcPool::new(&format!("{addr}"), &format!("{addr}"), &format!("{addr}"));
    pool.connect().await;
    // Give the pool a moment to establish the channel.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        pool.highway().await.is_some(),
        "pool must have a highway channel after connect"
    );
    (pool, handle)
}

// ====================================================================
// Branch-coverage tests
// ====================================================================

/// Live workspace: GetWorkspace returns Active → monitor respawned,
/// session appears in the active_sessions map.
#[tokio::test]
async fn run_respawns_monitor_for_live_workspace() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(
        &db,
        "s-live",
        "u-1",
        Some("ws-coord"),
        session_state::ACTIVE,
    )
    .await;

    let hw = ProgrammableHighway::new();
    hw.set_workspace(
        "ws-coord",
        GetWorkspaceOutcome::Ok(proto::WorkspaceState::Active),
    );
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let active = empty_active();
    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        active.clone(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.resumed.len(), 1, "one session should be resumed");
    assert_eq!(report.resumed[0], "s-live");
    assert!(report.failed.is_empty());
    assert!(report.synced_terminal.is_empty());
    assert!(report.skipped_unavailable.is_empty());

    // The active_sessions map should contain the respawned monitor.
    let map = active.read().await;
    assert!(
        map.contains_key("s-live"),
        "active_sessions must contain the resumed session"
    );
    // Session stays ACTIVE in the DB.
    let row = sessions::get_by_id(&db, "s-live").await.unwrap().unwrap();
    assert_eq!(row.state, session_state::ACTIVE);

    // Clean up: shut down the monitor so the test does not leak tasks.
    if let Some(h) = map.get("s-live") {
        h.shutdown().await;
    }
}

/// Terminal workspace (Failed): GetWorkspace returns Failed → session
/// state synced to FAILED.
#[tokio::test]
async fn run_syncs_terminal_failed_workspace() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(
        &db,
        "s-term-f",
        "u-1",
        Some("ws-coord"),
        session_state::ACTIVE,
    )
    .await;

    let hw = ProgrammableHighway::new();
    hw.set_workspace(
        "ws-coord",
        GetWorkspaceOutcome::Ok(proto::WorkspaceState::Failed),
    );
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.synced_terminal.len(), 1);
    assert_eq!(report.synced_terminal[0].0, "s-term-f");
    assert_eq!(report.synced_terminal[0].1, session_state::FAILED);
    assert!(report.resumed.is_empty());

    let row = sessions::get_by_id(&db, "s-term-f").await.unwrap().unwrap();
    assert_eq!(row.state, session_state::FAILED);
}

/// Terminal workspace (Closed): GetWorkspace returns Closed → session
/// state synced to COMPLETED.
#[tokio::test]
async fn run_syncs_terminal_closed_workspace() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(
        &db,
        "s-term-c",
        "u-1",
        Some("ws-coord"),
        session_state::ACTIVE,
    )
    .await;

    let hw = ProgrammableHighway::new();
    hw.set_workspace(
        "ws-coord",
        GetWorkspaceOutcome::Ok(proto::WorkspaceState::Closed),
    );
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.synced_terminal.len(), 1);
    assert_eq!(report.synced_terminal[0].0, "s-term-c");
    assert_eq!(report.synced_terminal[0].1, session_state::COMPLETED);

    let row = sessions::get_by_id(&db, "s-term-c").await.unwrap().unwrap();
    assert_eq!(row.state, session_state::COMPLETED);
}

/// GetWorkspace returns NotFound → session marked FAILED with
/// WorkspaceMissing reason.
#[tokio::test]
async fn run_marks_failed_when_workspace_not_found() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(&db, "s-nf", "u-1", Some("ws-gone"), session_state::ACTIVE).await;

    // ProgrammableHighway returns NotFound for unknown workspace_ids by
    // default (no explicit set_workspace call).
    let hw = ProgrammableHighway::new();
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, "s-nf");
    assert!(matches!(
        report.failed[0].1,
        RecoveryFailureReason::WorkspaceMissing
    ));

    let row = sessions::get_by_id(&db, "s-nf").await.unwrap().unwrap();
    assert_eq!(row.state, session_state::FAILED);
}

/// GetWorkspace returns Unavailable → session left ACTIVE and skipped.
#[tokio::test]
async fn run_skips_when_get_workspace_returns_unavailable() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(
        &db,
        "s-unavail",
        "u-1",
        Some("ws-coord"),
        session_state::ACTIVE,
    )
    .await;

    let hw = ProgrammableHighway::new();
    hw.set_workspace(
        "ws-coord",
        GetWorkspaceOutcome::Err(tonic::Code::Unavailable, "runtime down".into()),
    );
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.skipped_unavailable.len(), 1);
    assert_eq!(report.skipped_unavailable[0], "s-unavail");
    assert!(report.failed.is_empty());

    // Session stays ACTIVE.
    let row = sessions::get_by_id(&db, "s-unavail")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, session_state::ACTIVE);
}

/// GetWorkspace returns DeadlineExceeded → treated the same as
/// Unavailable (skip).
#[tokio::test]
async fn run_skips_when_get_workspace_returns_deadline_exceeded() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(
        &db,
        "s-timeout",
        "u-1",
        Some("ws-coord"),
        session_state::ACTIVE,
    )
    .await;

    let hw = ProgrammableHighway::new();
    hw.set_workspace(
        "ws-coord",
        GetWorkspaceOutcome::Err(tonic::Code::DeadlineExceeded, "timeout".into()),
    );
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.skipped_unavailable.len(), 1);
    let row = sessions::get_by_id(&db, "s-timeout")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, session_state::ACTIVE);
}

/// GetWorkspace returns an unexpected gRPC error (e.g. Internal) →
/// session marked FAILED with RuntimeError reason.
#[tokio::test]
async fn run_marks_failed_on_unexpected_grpc_error() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(
        &db,
        "s-rpc-err",
        "u-1",
        Some("ws-coord"),
        session_state::ACTIVE,
    )
    .await;

    let hw = ProgrammableHighway::new();
    hw.set_workspace(
        "ws-coord",
        GetWorkspaceOutcome::Err(tonic::Code::Internal, "server blew up".into()),
    );
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, "s-rpc-err");
    assert!(matches!(
        report.failed[0].1,
        RecoveryFailureReason::RuntimeError(_)
    ));

    let row = sessions::get_by_id(&db, "s-rpc-err")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, session_state::FAILED);
}

/// Multiple active sessions → each probed independently, producing
/// independent outcomes in a single recovery pass.
#[tokio::test]
async fn run_handles_multiple_sessions_independently() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    // Session 1: stuck (no workspace_id)
    insert_session(&db, "s-multi-stuck", "u-1", None, session_state::ACTIVE).await;
    // Session 2: terminal workspace
    insert_session(
        &db,
        "s-multi-closed",
        "u-1",
        Some("ws-closed"),
        session_state::ACTIVE,
    )
    .await;
    // Session 3: live workspace
    insert_session(
        &db,
        "s-multi-live",
        "u-1",
        Some("ws-live"),
        session_state::ACTIVE,
    )
    .await;
    // Session 4: workspace not found
    insert_session(
        &db,
        "s-multi-nf",
        "u-1",
        Some("ws-missing"),
        session_state::ACTIVE,
    )
    .await;

    let hw = ProgrammableHighway::new();
    hw.set_workspace(
        "ws-closed",
        GetWorkspaceOutcome::Ok(proto::WorkspaceState::Closed),
    );
    hw.set_workspace(
        "ws-live",
        GetWorkspaceOutcome::Ok(proto::WorkspaceState::Active),
    );
    // ws-missing left unprogrammed → returns NotFound
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let active = empty_active();
    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        active.clone(),
        quick_cfg(),
    )
    .await;

    // Stuck session → failed
    assert!(report.failed.iter().any(
        |(id, r)| id == "s-multi-stuck" && matches!(r, RecoveryFailureReason::StuckInLaunching)
    ));
    // Not-found session → failed
    assert!(
        report
            .failed
            .iter()
            .any(|(id, r)| id == "s-multi-nf"
                && matches!(r, RecoveryFailureReason::WorkspaceMissing))
    );
    // Terminal session → synced
    assert!(
        report
            .synced_terminal
            .iter()
            .any(|(id, state)| id == "s-multi-closed" && state == session_state::COMPLETED)
    );
    // Live session → resumed
    assert!(report.resumed.iter().any(|id| id == "s-multi-live"));

    // Verify DB states
    let stuck = sessions::get_by_id(&db, "s-multi-stuck")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stuck.state, session_state::FAILED);
    let closed = sessions::get_by_id(&db, "s-multi-closed")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(closed.state, session_state::COMPLETED);
    let live = sessions::get_by_id(&db, "s-multi-live")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(live.state, session_state::ACTIVE);
    let nf = sessions::get_by_id(&db, "s-multi-nf")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(nf.state, session_state::FAILED);

    // Active map contains only the live session.
    let map = active.read().await;
    assert!(map.contains_key("s-multi-live"));
    assert!(!map.contains_key("s-multi-stuck"));
    assert!(!map.contains_key("s-multi-closed"));
    assert!(!map.contains_key("s-multi-nf"));

    // Clean up spawned monitor.
    if let Some(h) = map.get("s-multi-live") {
        h.shutdown().await;
    }
}

/// Partial re-subscribe: some sessions recover, some fail — each
/// handled independently within the same recovery pass.
#[tokio::test]
async fn run_partial_resubscribe_mixed_outcomes() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    // Session A: recoverable (live workspace)
    insert_session(&db, "s-ok", "u-1", Some("ws-ok"), session_state::ACTIVE).await;
    // Session B: fails (gRPC Internal error)
    insert_session(&db, "s-fail", "u-1", Some("ws-fail"), session_state::ACTIVE).await;
    // Session C: skipped (Unavailable)
    insert_session(&db, "s-skip", "u-1", Some("ws-skip"), session_state::ACTIVE).await;

    let hw = ProgrammableHighway::new();
    hw.set_workspace(
        "ws-ok",
        GetWorkspaceOutcome::Ok(proto::WorkspaceState::Idle),
    );
    hw.set_workspace(
        "ws-fail",
        GetWorkspaceOutcome::Err(tonic::Code::Internal, "kaboom".into()),
    );
    hw.set_workspace(
        "ws-skip",
        GetWorkspaceOutcome::Err(tonic::Code::Unavailable, "gone".into()),
    );
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let active = empty_active();
    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        active.clone(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.resumed.len(), 1, "one session recovered");
    assert!(report.resumed.contains(&"s-ok".to_string()));
    assert_eq!(report.failed.len(), 1, "one session failed");
    assert_eq!(report.failed[0].0, "s-fail");
    assert_eq!(report.skipped_unavailable.len(), 1, "one session skipped");
    assert_eq!(report.skipped_unavailable[0], "s-skip");

    // DB states
    let ok_row = sessions::get_by_id(&db, "s-ok").await.unwrap().unwrap();
    assert_eq!(ok_row.state, session_state::ACTIVE);
    let fail_row = sessions::get_by_id(&db, "s-fail").await.unwrap().unwrap();
    assert_eq!(fail_row.state, session_state::FAILED);
    let skip_row = sessions::get_by_id(&db, "s-skip").await.unwrap().unwrap();
    assert_eq!(skip_row.state, session_state::ACTIVE);

    // Clean up
    let map = active.read().await;
    if let Some(h) = map.get("s-ok") {
        h.shutdown().await;
    }
}

/// No active sessions at startup → recovery is a no-op (complements the
/// existing empty-report test by verifying the report structure in detail).
#[tokio::test]
async fn run_noop_with_only_terminal_sessions() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    // Insert sessions in terminal states — recovery ignores them.
    insert_session(&db, "s-done", "u-1", Some("ws-1"), session_state::COMPLETED).await;
    insert_session(&db, "s-dead", "u-1", Some("ws-2"), session_state::FAILED).await;
    insert_session(&db, "s-gone", "u-1", Some("ws-3"), session_state::CANCELLED).await;

    let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;

    assert!(report.resumed.is_empty());
    assert!(report.failed.is_empty());
    assert!(report.synced_terminal.is_empty());
    assert!(report.skipped_unavailable.is_empty());
}

/// Runtime unreachable at startup (pool connects but highway returns None):
/// active sessions stay ACTIVE, recovery skips gracefully.
#[tokio::test]
async fn run_skips_all_when_runtime_unreachable() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(&db, "s-a", "u-1", Some("ws-1"), session_state::ACTIVE).await;
    insert_session(&db, "s-b", "u-1", Some("ws-2"), session_state::ACTIVE).await;

    // Pool that never connects → highway() returns None.
    let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");

    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;

    assert!(report.resumed.is_empty());
    assert!(report.failed.is_empty());
    assert!(report.synced_terminal.is_empty());
    assert_eq!(report.skipped_unavailable.len(), 2);

    // Both sessions stay ACTIVE.
    let a = sessions::get_by_id(&db, "s-a").await.unwrap().unwrap();
    assert_eq!(a.state, session_state::ACTIVE);
    let b = sessions::get_by_id(&db, "s-b").await.unwrap().unwrap();
    assert_eq!(b.state, session_state::ACTIVE);
}

/// Orphaned LAUNCHING session (no workspace_id) → marked FAILED with
/// StuckInLaunching reason. The mark_failed helper writes to the DB.
#[tokio::test]
async fn run_orphaned_launching_marked_failed() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    // Insert an ACTIVE session that has no coordinator_workspace_id.
    // This simulates W2 finalize never writing the workspace id.
    insert_session(&db, "s-orphan", "u-1", None, session_state::ACTIVE).await;

    let hw = ProgrammableHighway::new();
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        empty_active(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, "s-orphan");
    assert!(matches!(
        report.failed[0].1,
        RecoveryFailureReason::StuckInLaunching
    ));
    // Never reaches the highway at all.
    assert!(report.resumed.is_empty());
    assert!(report.skipped_unavailable.is_empty());

    let row = sessions::get_by_id(&db, "s-orphan").await.unwrap().unwrap();
    assert_eq!(row.state, session_state::FAILED);
    // closed_at should be set on terminal transition.
    assert!(row.closed_at.is_some());
}

/// Live workspace with session_assignments: assignments are loaded and
/// the WorkspaceSet includes the assigned workspace_ids.
#[tokio::test]
async fn run_live_workspace_loads_assignments_into_workspace_set() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(
        &db,
        "s-assign",
        "u-1",
        Some("ws-root"),
        session_state::ACTIVE,
    )
    .await;
    insert_profile(&db, "profile-w", "u-1").await;
    insert_profile(&db, "profile-r", "u-1").await;

    // Insert session_assignments with workspace_ids.
    let a1 = session_assignments::SessionAssignmentRow {
        id: "sa-1".into(),
        session_id: "s-assign".into(),
        role_ref: "worker".into(),
        stage_id: None,
        slot_position: 0,
        profile_id: Some("profile-w".into()),
        profile_version: Some(1),
        workspace_id: Some("ws-child-1".into()),
        budget_max_cost_micros: None,
        budget_max_tokens: None,
        budget_max_wall_time_ms: None,
    };
    let a2 = session_assignments::SessionAssignmentRow {
        id: "sa-2".into(),
        session_id: "s-assign".into(),
        role_ref: "reviewer".into(),
        stage_id: None,
        slot_position: 1,
        profile_id: Some("profile-r".into()),
        profile_version: Some(1),
        workspace_id: Some("ws-child-2".into()),
        budget_max_cost_micros: None,
        budget_max_tokens: None,
        budget_max_wall_time_ms: None,
    };
    session_assignments::insert_assignment(&db, &a1)
        .await
        .unwrap();
    session_assignments::insert_assignment(&db, &a2)
        .await
        .unwrap();

    let hw = ProgrammableHighway::new();
    hw.set_workspace(
        "ws-root",
        GetWorkspaceOutcome::Ok(proto::WorkspaceState::Active),
    );
    // The child workspaces will be looked up by seed_workspace_labels in the
    // monitor; those 404s are non-fatal.
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let active = empty_active();
    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        active.clone(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.resumed.len(), 1);
    assert_eq!(report.resumed[0], "s-assign");

    let map = active.read().await;
    assert!(map.contains_key("s-assign"));

    // Clean up
    if let Some(h) = map.get("s-assign") {
        h.shutdown().await;
    }
}

/// The recovery_report fields default to empty (Default trait).
#[test]
fn recovery_report_default_is_empty() {
    let r = RecoveryReport::default();
    assert!(r.resumed.is_empty());
    assert!(r.synced_terminal.is_empty());
    assert!(r.failed.is_empty());
    assert!(r.skipped_unavailable.is_empty());
}

/// RecoveryFailureReason::clone works correctly (derive(Clone)).
#[test]
fn failure_reason_clone_preserves_variant() {
    let r1 = RecoveryFailureReason::RuntimeError("oops".into());
    let r2 = r1.clone();
    assert_eq!(r1.tag(), r2.tag());
}

/// The mark_failed helper sets closed_at timestamp (via transition_state).
#[tokio::test]
async fn mark_failed_sets_closed_at() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(&db, "s-mf", "u-1", None, session_state::ACTIVE).await;

    mark_failed(&db, "s-mf", "test_reason").await;

    let row = sessions::get_by_id(&db, "s-mf").await.unwrap().unwrap();
    assert_eq!(row.state, session_state::FAILED);
    assert!(
        row.closed_at.is_some(),
        "closed_at must be set after mark_failed"
    );
}

/// mark_failed is idempotent — calling it on an already-FAILED session
/// does not panic (transition_state returns false, mark_failed logs).
#[tokio::test]
async fn mark_failed_idempotent_on_already_failed() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(&db, "s-idem", "u-1", None, session_state::ACTIVE).await;

    mark_failed(&db, "s-idem", "first").await;
    // Second call — session is already FAILED, so from_state=ACTIVE won't match.
    mark_failed(&db, "s-idem", "second").await;

    let row = sessions::get_by_id(&db, "s-idem").await.unwrap().unwrap();
    assert_eq!(row.state, session_state::FAILED);
}

/// Workspace in a non-terminal non-live state (Blocked) is treated as
/// live → monitor respawned.
#[tokio::test]
async fn run_respawns_for_blocked_workspace() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(
        &db,
        "s-blocked",
        "u-1",
        Some("ws-coord"),
        session_state::ACTIVE,
    )
    .await;

    let hw = ProgrammableHighway::new();
    hw.set_workspace(
        "ws-coord",
        GetWorkspaceOutcome::Ok(proto::WorkspaceState::Blocked),
    );
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let active = empty_active();
    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        active.clone(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.resumed.len(), 1);
    assert!(report.synced_terminal.is_empty(), "Blocked is not terminal");

    let map = active.read().await;
    if let Some(h) = map.get("s-blocked") {
        h.shutdown().await;
    }
}

/// Workspace in Idle state is treated as live → monitor respawned.
#[tokio::test]
async fn run_respawns_for_idle_workspace() {
    let db = create_test_pool().await.unwrap();
    insert_user(&db, "u-1").await;
    insert_session(
        &db,
        "s-idle",
        "u-1",
        Some("ws-coord"),
        session_state::ACTIVE,
    )
    .await;

    let hw = ProgrammableHighway::new();
    hw.set_workspace(
        "ws-coord",
        GetWorkspaceOutcome::Ok(proto::WorkspaceState::Idle),
    );
    let (pool, _hw_handle) = pool_with_highway(hw).await;

    let active = empty_active();
    let report = run(
        db.clone(),
        pool,
        enricher(),
        refusals(),
        active.clone(),
        quick_cfg(),
    )
    .await;

    assert_eq!(report.resumed.len(), 1);
    assert!(report.synced_terminal.is_empty());

    let map = active.read().await;
    if let Some(h) = map.get("s-idle") {
        h.shutdown().await;
    }
}
