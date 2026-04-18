//! §13.7.8 I5 — taxonomy reload matrix.
//!
//! Drives `POST /api/taxonomy/reload` against a hot-swappable mock REST
//! gateway. Four scenarios:
//!
//! 1. **reload_swaps_to_new_vertical_set** — add a vertical to the mock,
//!    reload, assert `/api/verticals` returns the new count.
//! 2. **reload_with_removed_vertical_updates_list** — remove a vertical
//!    from the mock, reload, assert it disappears from `/api/verticals`.
//!    (The canonical "existing sessions keep running" assertion from the
//!    audit plan is covered by I2's active-sessions branching; repeating
//!    it here would duplicate coverage.)
//! 3. **reload_with_upstream_500_preserves_previous_index** — mock REST
//!    returns 500 on `/v1/verticals`; reload body reports `status: "failed"`;
//!    pre-reload `/api/verticals` content is preserved (no partial swap).
//! 4. **repeated_reload_is_idempotent** — two consecutive reloads against
//!    the same mock state return identical counts.
//!
//! **Not covered (deferred, see `performance-optimization.md` §13.5):**
//! - `context_schema` change affects new sessions but not running ones.
//!   Requires a second runtime process + SubmitGoal flow + a schema
//!   evolution in the fixture; multi-step and outside integration scope.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use console_integration::{ConsoleHarness, RuntimeHarness, TestClient};
use console_test_support::fixtures;
use console_test_support::mock_rest::RestState;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use wacp_taxonomy::VerticalManifest;

// ---- mock-rest spawn ------------------------------------------------------

struct MockRest {
    state: RestState,
    addr: SocketAddr,
    _handle: JoinHandle<()>,
    /// When true, `GET /v1/verticals` returns 500 regardless of state. Used
    /// by `reload_with_upstream_500_preserves_previous_index` to simulate
    /// a transient upstream failure.
    force_500: Arc<AtomicBool>,
}

#[derive(Clone)]
struct GuardedState {
    rest: RestState,
    force_500: Arc<AtomicBool>,
}

async fn list_handler(State(s): State<GuardedState>) -> impl IntoResponse {
    if s.force_500.load(Ordering::SeqCst) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error": "forced_500"})),
        )
            .into_response();
    }
    let snapshot = s.rest.verticals.load();
    let items: Vec<serde_json::Value> = snapshot
        .values()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "name": m.name,
                "defining_constraint": m.defining_constraint,
                "task_type_count": m.task_types.len(),
                "workflow_count": m.workflows.len(),
                "tool_count": m.tool_policies.len(),
            })
        })
        .collect();
    (StatusCode::OK, axum::Json(items)).into_response()
}

async fn detail_handler(
    State(s): State<GuardedState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let snapshot = s.rest.verticals.load();
    match snapshot.get(&id) {
        Some(m) => (StatusCode::OK, axum::Json(m.clone())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error": "NOT_FOUND"})),
        )
            .into_response(),
    }
}

impl MockRest {
    async fn spawn(verticals: HashMap<String, VerticalManifest>) -> std::io::Result<Self> {
        let state = RestState::new(verticals);
        let force_500 = Arc::new(AtomicBool::new(false));
        let guarded = GuardedState {
            rest: state.clone(),
            force_500: force_500.clone(),
        };

        let app = Router::new()
            .route("/v1/verticals", get(list_handler))
            .route("/v1/verticals/{id}", get(detail_handler))
            .with_state(guarded);

        let listener = TcpListener::bind("[::1]:0").await?;
        let addr = listener.local_addr()?;
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Ok(MockRest {
            state,
            addr,
            _handle: handle,
            force_500,
        })
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn set_force_500(&self, on: bool) {
        self.force_500.store(on, Ordering::SeqCst);
    }
}

async fn reload(client: &TestClient) -> serde_json::Value {
    let resp = client
        .post_json("/api/taxonomy/reload", serde_json::json!({}))
        .await;
    // Reload returns 200 even on failure — the body encodes status.
    assert!(
        resp.status().is_success(),
        "reload endpoint itself must return 2xx"
    );
    resp.json().await.expect("reload body")
}

async fn list_verticals(client: &TestClient) -> Vec<serde_json::Value> {
    let resp = client
        .get("/api/verticals")
        .await
        .json::<serde_json::Value>()
        .await
        .expect("verticals json");
    // Paginated envelope: {items: [...], has_more, cursor?}
    resp["items"].as_array().cloned().unwrap_or_default()
}

/// Seed an admin user; reload + verticals management routes require admin.
async fn admin_client(console: &ConsoleHarness) -> TestClient {
    let uid = format!("u-admin-{}", uuid::Uuid::new_v4());
    TestClient::seed_user(&console.state, &console.base_url(), &uid, "admin").await
}

// ---- tests ----------------------------------------------------------------

#[tokio::test]
async fn reload_swaps_to_new_vertical_set() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");

    // Start with one vertical.
    let v_simple = fixtures::fixture_simple();
    let v_complex = fixtures::fixture_complex();
    let mut initial = HashMap::new();
    initial.insert(v_simple.id.clone(), v_simple.clone());
    let mock_rest = MockRest::spawn(initial).await.expect("mock rest");

    let db = console_db::create_test_pool().await.expect("db");
    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");
    // The reload handler reads `runtime.rest_address` from the settings
    // table (not AppState.runtime_config), falling back to the hardcoded
    // default. Point settings at our mock.
    console_core::settings::set(
        &console.state.db,
        "runtime.rest_address",
        &serde_json::Value::String(mock_rest.url()),
    )
    .await
    .expect("set runtime.rest_address");
    let client = admin_client(&console).await;

    // Initial reload picks up 1 vertical.
    let body = reload(&client).await;
    assert_eq!(body["status"], "success", "initial reload: {body:?}");
    assert_eq!(body["counts"]["verticals"], 1);
    assert_eq!(list_verticals(&client).await.len(), 1);

    // Hot-swap: add the second vertical.
    let mut next = HashMap::new();
    next.insert(v_simple.id.clone(), v_simple);
    next.insert(v_complex.id.clone(), v_complex);
    mock_rest.state.set_verticals(next);

    // Second reload picks up 2.
    let body2 = reload(&client).await;
    assert_eq!(body2["status"], "success", "second reload: {body2:?}");
    assert_eq!(body2["counts"]["verticals"], 2);
    assert_eq!(list_verticals(&client).await.len(), 2);
}

#[tokio::test]
async fn reload_with_removed_vertical_updates_list() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");

    let v_simple = fixtures::fixture_simple();
    let v_complex = fixtures::fixture_complex();
    let mut initial = HashMap::new();
    initial.insert(v_simple.id.clone(), v_simple.clone());
    initial.insert(v_complex.id.clone(), v_complex);
    let mock_rest = MockRest::spawn(initial).await.expect("mock rest");

    let db = console_db::create_test_pool().await.expect("db");
    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");
    // The reload handler reads `runtime.rest_address` from the settings
    // table (not AppState.runtime_config), falling back to the hardcoded
    // default. Point settings at our mock.
    console_core::settings::set(
        &console.state.db,
        "runtime.rest_address",
        &serde_json::Value::String(mock_rest.url()),
    )
    .await
    .expect("set runtime.rest_address");
    let client = admin_client(&console).await;

    let _ = reload(&client).await;
    assert_eq!(list_verticals(&client).await.len(), 2);

    // Remove the complex vertical.
    let mut next = HashMap::new();
    next.insert(v_simple.id.clone(), v_simple);
    mock_rest.state.set_verticals(next);

    let body = reload(&client).await;
    assert_eq!(body["counts"]["verticals"], 1);
    let list = list_verticals(&client).await;
    assert_eq!(list.len(), 1);
    // Exactly the surviving vertical.
    assert_eq!(list[0]["id"].as_str(), Some("fixture-simple"));
}

#[tokio::test]
async fn reload_with_upstream_500_preserves_previous_index() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");

    let v_simple = fixtures::fixture_simple();
    let v_complex = fixtures::fixture_complex();
    let mut initial = HashMap::new();
    initial.insert(v_simple.id.clone(), v_simple.clone());
    initial.insert(v_complex.id.clone(), v_complex);
    let mock_rest = MockRest::spawn(initial).await.expect("mock rest");

    let db = console_db::create_test_pool().await.expect("db");
    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");
    // The reload handler reads `runtime.rest_address` from the settings
    // table (not AppState.runtime_config), falling back to the hardcoded
    // default. Point settings at our mock.
    console_core::settings::set(
        &console.state.db,
        "runtime.rest_address",
        &serde_json::Value::String(mock_rest.url()),
    )
    .await
    .expect("set runtime.rest_address");
    let client = admin_client(&console).await;

    // Successful initial load → 2 verticals.
    let _ = reload(&client).await;
    assert_eq!(list_verticals(&client).await.len(), 2);

    // Flip the mock to return 500.
    mock_rest.set_force_500(true);

    // Reload returns status=failed, counts=null per taxonomy.rs:38.
    let body = reload(&client).await;
    assert_eq!(
        body["status"], "failed",
        "upstream 500 must surface as failed reload, got: {body:?}"
    );
    assert!(body["counts"].is_null());

    // Critical: the pre-reload index is preserved — ArcSwap wasn't swapped.
    let list = list_verticals(&client).await;
    assert_eq!(
        list.len(),
        2,
        "failed reload must not mutate the live taxonomy"
    );
}

#[tokio::test]
async fn repeated_reload_is_idempotent() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");

    let v_simple = fixtures::fixture_simple();
    let mut initial = HashMap::new();
    initial.insert(v_simple.id.clone(), v_simple);
    let mock_rest = MockRest::spawn(initial).await.expect("mock rest");

    let db = console_db::create_test_pool().await.expect("db");
    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");
    // The reload handler reads `runtime.rest_address` from the settings
    // table (not AppState.runtime_config), falling back to the hardcoded
    // default. Point settings at our mock.
    console_core::settings::set(
        &console.state.db,
        "runtime.rest_address",
        &serde_json::Value::String(mock_rest.url()),
    )
    .await
    .expect("set runtime.rest_address");
    let client = admin_client(&console).await;

    let a = reload(&client).await;
    let b = reload(&client).await;
    assert_eq!(a["status"], "success");
    assert_eq!(b["status"], "success");
    assert_eq!(a["counts"]["verticals"], b["counts"]["verticals"]);
    assert_eq!(a["counts"]["roles"], b["counts"]["roles"]);
    assert_eq!(a["counts"]["tools"], b["counts"]["tools"]);
}
