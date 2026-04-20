use super::*;
use arc_swap::ArcSwap;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use console_core::authenticator;
use console_core::config::RuntimeConfig;
use console_core::session_monitor::{Frame, MonitorCmd, PendingState, SessionMonitorHandle};
use console_core::taxonomy_builder;
use console_db::create_test_pool;
use console_db::queries::api_tokens;
use console_runtime::grpc_pool::GrpcPool;
use console_test_support::mock_runtime::MockRuntime;
use std::collections::HashMap;
use tokio::sync::{RwLock, broadcast, mpsc};
use tower::ServiceExt;

struct Fixture {
    state: Arc<AppState>,
    token: String,
    sid: String,
}

/// Build a fixture with a session in `state`. `coord_ws` controls whether
/// the row gets a coordinator workspace id (None → cancel arm sees the
/// "missing coord workspace" 409 path on AbortWorkspace).
async fn fixture(owner: &str, state_str: &str, coord_ws: Option<&str>) -> (Fixture, MockRuntime) {
    let rt = MockRuntime::start().await.expect("mock runtime");
    let pool = GrpcPool::new(
        &rt.agent_addr.to_string(),
        &rt.highway_addr.to_string(),
        &rt.coordinator_addr.to_string(),
    );
    pool.connect().await;

    let db = create_test_pool().await.expect("db");
    seed_user(&db, owner).await;
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_session(&db, &sid, owner, coord_ws, state_str).await;
    let token = mint_bearer(&db, owner).await;

    let taxonomy = Arc::new(ArcSwap::from_pointee(
        taxonomy_builder::build_index(None, &[], &[]).index,
    ));

    let state = Arc::new(AppState {
        db,
        taxonomy,
        runtime_config: RuntimeConfig {
            agent_address: rt.agent_addr.to_string(),
            highway_address: rt.highway_addr.to_string(),
            coordinator_address: rt.coordinator_addr.to_string(),
            rest_address: rt.rest_addr.to_string(),
        },
        grpc_pool: pool,
        active_sessions: Arc::new(RwLock::new(HashMap::new())),
    });

    (Fixture { state, token, sid }, rt)
}

/// Variant: pool dialed at unreachable ports. Coordinator client returns
/// None → AbortWorkspace handler returns 503; BestEffortAbort tolerates.
async fn fixture_with_dead_pool(owner: &str, state_str: &str, coord_ws: Option<&str>) -> Fixture {
    let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
    pool.connect().await;

    let db = create_test_pool().await.expect("db");
    seed_user(&db, owner).await;
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_session(&db, &sid, owner, coord_ws, state_str).await;
    let token = mint_bearer(&db, owner).await;

    let taxonomy = Arc::new(ArcSwap::from_pointee(
        taxonomy_builder::build_index(None, &[], &[]).index,
    ));

    let state = Arc::new(AppState {
        db,
        taxonomy,
        runtime_config: RuntimeConfig {
            agent_address: "[::1]:1".into(),
            highway_address: "[::1]:1".into(),
            coordinator_address: "[::1]:1".into(),
            rest_address: "[::1]:1".into(),
        },
        grpc_pool: pool,
        active_sessions: Arc::new(RwLock::new(HashMap::new())),
    });

    Fixture { state, token, sid }
}

async fn seed_user(db: &console_db::DbPool, id: &str) {
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

async fn seed_session(
    db: &console_db::DbPool,
    sid: &str,
    owner: &str,
    coord_ws: Option<&str>,
    state: &str,
) {
    let row = sessions::SessionRow {
        id: sid.into(),
        name: Some(sid.into()),
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
    sessions::insert_session(db, &row)
        .await
        .expect("insert session");
}

async fn mint_bearer(db: &console_db::DbPool, owner: &str) -> String {
    let plain = format!("wcon_t_{}", uuid::Uuid::new_v4());
    let hash = authenticator::hash_token(&plain);
    api_tokens::insert_token(
        db,
        &format!("tok-{}", uuid::Uuid::new_v4()),
        owner,
        "test",
        &hash,
        "2026-04-15T00:00:00Z",
        None,
    )
    .await
    .expect("insert token");
    plain
}

async fn install_dummy_monitor(state: &AppState, sid: &str) -> mpsc::Receiver<MonitorCmd> {
    let (cmd_tx, cmd_rx) = mpsc::channel(4);
    let handle = SessionMonitorHandle {
        session_id: sid.into(),
        cmd_tx,
        broadcast_tx: broadcast::channel::<Frame>(8).0,
        pending: Arc::new(PendingState::default()),
    };
    state
        .active_sessions
        .write()
        .await
        .insert(sid.into(), handle);
    cmd_rx
}

fn cancel_request(sid: &str, token: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{sid}/cancel"))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn read_json(resp: axum::response::Response) -> serde_json::Value {
    let body = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null)
}

#[tokio::test]
async fn cancel_active_calls_abort_then_marks_cancelled_and_drops_monitor() {
    let (fx, _rt) = fixture("u-1", session_state::ACTIVE, Some("ws-coord")).await;
    let mut cmd_rx = install_dummy_monitor(&fx.state, &fx.sid).await;
    let app = router().with_state(fx.state.clone());

    let resp = app
        .oneshot(cancel_request(
            &fx.sid,
            &fx.token,
            serde_json::json!({ "reason": "operator stop" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = read_json(resp).await;
    assert_eq!(v["state"], session_state::CANCELLED);

    let row = sessions::get_by_id(&fx.state.db, &fx.sid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, session_state::CANCELLED);

    // Monitor handle removed; shutdown command was sent.
    assert!(fx.state.active_sessions.read().await.get(&fx.sid).is_none());
    assert!(matches!(cmd_rx.try_recv(), Ok(MonitorCmd::Shutdown)));
}

#[tokio::test]
async fn cancel_active_with_runtime_unavailable_returns_503_and_keeps_active() {
    let fx = fixture_with_dead_pool("u-1", session_state::ACTIVE, Some("ws-coord")).await;
    let _cmd_rx = install_dummy_monitor(&fx.state, &fx.sid).await;
    let app = router().with_state(fx.state.clone());

    let resp = app
        .oneshot(cancel_request(&fx.sid, &fx.token, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Session stays ACTIVE; monitor stays registered.
    let row = sessions::get_by_id(&fx.state.db, &fx.sid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, session_state::ACTIVE);
    assert!(fx.state.active_sessions.read().await.get(&fx.sid).is_some());
}

#[tokio::test]
async fn cancel_active_without_coordinator_workspace_returns_409() {
    let (fx, _rt) = fixture("u-1", session_state::ACTIVE, None).await;
    let app = router().with_state(fx.state.clone());

    let resp = app
        .oneshot(cancel_request(&fx.sid, &fx.token, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let row = sessions::get_by_id(&fx.state.db, &fx.sid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, session_state::ACTIVE);
}

#[tokio::test]
async fn cancel_already_cancelled_returns_409() {
    let (fx, _rt) = fixture("u-1", session_state::CANCELLED, Some("ws-coord")).await;
    let app = router().with_state(fx.state.clone());

    let resp = app
        .oneshot(cancel_request(&fx.sid, &fx.token, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn cancel_launching_uses_best_effort_and_succeeds_even_with_dead_pool() {
    // BestEffortAbort tolerates a runtime failure — the cancel still
    // succeeds and the session is marked CANCELLED.
    let fx = fixture_with_dead_pool("u-1", session_state::LAUNCHING, Some("ws-coord")).await;
    let app = router().with_state(fx.state.clone());

    let resp = app
        .oneshot(cancel_request(&fx.sid, &fx.token, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let row = sessions::get_by_id(&fx.state.db, &fx.sid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, session_state::CANCELLED);
}

#[tokio::test]
async fn cancel_configuring_no_op_succeeds_without_runtime_call() {
    let fx = fixture_with_dead_pool("u-1", session_state::CONFIGURING, None).await;
    let app = router().with_state(fx.state.clone());

    let resp = app
        .oneshot(cancel_request(&fx.sid, &fx.token, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let row = sessions::get_by_id(&fx.state.db, &fx.sid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, session_state::CANCELLED);
}

#[tokio::test]
async fn cancel_cross_owner_returns_403() {
    let (fx, _rt) = fixture("u-owner", session_state::ACTIVE, Some("ws-coord")).await;
    seed_user(&fx.state.db, "u-stranger").await;
    let stranger_token = mint_bearer(&fx.state.db, "u-stranger").await;
    let app = router().with_state(fx.state.clone());

    let resp = app
        .oneshot(cancel_request(
            &fx.sid,
            &stranger_token,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
