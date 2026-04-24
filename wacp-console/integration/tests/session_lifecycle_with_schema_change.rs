//! §13.7.8 I5 follow-up — context-schema evolution (closes the last
//! deferred sub-scenario from the integration matrix; see
//! `impl/archive/context-schema-evolution-plan.md`).
//!
//! Validation fires on `POST /api/sessions/:id/launch` (handler at
//! `console-api/src/routes/sessions.rs:405–445`), reading
//! `state.taxonomy.load()` fresh on each call. So evolving the schema via
//! `POST /api/taxonomy/reload` is immediately visible to the next launch,
//! with no settle delay (`ArcSwap::store` is atomic and synchronous from
//! the caller's view).
//!
//! Four scenarios:
//!
//! 1. **launch_rejected_after_field_added_as_required** — v1 → v2_breaking
//!    reload causes launch of a v1-shaped session to fail with
//!    `MISSING_CONTEXT` (v2_breaking's added `region` field).
//!
//! 2. **launch_rejected_after_field_type_narrowed** — same v1 → v2_breaking
//!    reload; the v1 context has `priority: "high"` (string), v2_breaking
//!    narrows `priority` to `Number`, so launch yields `INVALID_CONTEXT`.
//!
//! 3. **active_session_preserved_across_schema_evolution** — DB-seed a
//!    session in state=ACTIVE under v1, then reload to v2_breaking. Assert
//!    the session row is untouched (state + context unchanged). Proves
//!    schema evolution is creation-gating only, never retroactive.
//!
//! 4. **additive_evolution_accepts_session_without_new_optional_field** —
//!    v1 → v2_additive reload; the added `notes` field is optional.
//!    v1-shaped context stays valid — launch violations do NOT include
//!    any context-schema codes (MISSING_ASSIGNMENT noise is expected and
//!    ignored via containment-style assertion).
//!
//! **Containment-style assertions on `violations`.** `validate_session`
//! accumulates all 12 checks without short-circuiting; an un-seeded
//! session launch always yields at least one `MISSING_ASSIGNMENT` per
//! vertical role. These tests assert only *which* context-schema codes
//! are / are not in the violations list — pre-seeding profiles to clear
//! the assignment noise would quadruple setup cost for zero signal on
//! the schema-evolution contract.
//!
//! **Sequential by construction.** Each test spawns an isolated runtime +
//! console + mock REST; no parallel reloads. `ArcSwap::store` would be
//! atomic even under concurrency, but do not add parallelism here without
//! thinking about it.

use std::collections::HashMap;
use std::net::SocketAddr;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use console_db::DbPool;
use console_db::queries::sessions;
use console_integration::{ConsoleHarness, RuntimeHarness, TestClient};
use console_test_support::fixtures;
use console_test_support::mock_rest::RestState;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use wacp_taxonomy::VerticalManifest;

// ---- mock REST server (lifted from taxonomy_reload.rs, force_500 dropped)

struct MockRest {
    state: RestState,
    addr: SocketAddr,
    _handle: JoinHandle<()>,
}

async fn list_handler(State(rest): State<RestState>) -> impl IntoResponse {
    let snapshot = rest.verticals.load();
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
    State(rest): State<RestState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let snapshot = rest.verticals.load();
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
    async fn spawn(initial: HashMap<String, VerticalManifest>) -> std::io::Result<Self> {
        let state = RestState::new(initial);
        let app = Router::new()
            .route("/v1/verticals", get(list_handler))
            .route("/v1/verticals/{id}", get(detail_handler))
            .with_state(state.clone());

        let listener = TcpListener::bind("[::1]:0").await?;
        let addr = listener.local_addr()?;
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(MockRest {
            state,
            addr,
            _handle: handle,
        })
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

// ---- test helpers ---------------------------------------------------------

fn single_vertical(m: VerticalManifest) -> HashMap<String, VerticalManifest> {
    let mut h = HashMap::new();
    h.insert(m.id.clone(), m);
    h
}

/// Seed an admin and return a bearer-auth TestClient. Reload + sessions
/// endpoints both need a real user.
async fn admin_client(console: &ConsoleHarness) -> TestClient {
    let uid = format!("u-admin-{}", uuid::Uuid::new_v4());
    TestClient::seed_user(&console.state, &console.base_url(), &uid, "admin").await
}

async fn point_at_mock(console: &ConsoleHarness, mock: &MockRest) {
    // The reload handler reads `runtime.rest_address` from the settings
    // table, not from AppState.runtime_config — see HEALTH-LOG §13.5
    // "Harness finding worth noting".
    console_core::settings::set(
        &console.state.db,
        "runtime.rest_address",
        &serde_json::Value::String(mock.url()),
    )
    .await
    .expect("set runtime.rest_address");
}

async fn reload(client: &TestClient) {
    let resp = client
        .post_json("/api/taxonomy/reload", serde_json::json!({}))
        .await;
    assert!(resp.status().is_success(), "reload endpoint must 2xx");
    let body: serde_json::Value = resp.json().await.expect("reload body");
    assert_eq!(body["status"], "success", "reload body: {body:?}");
}

/// Create a session via `POST /api/sessions`. No validation fires at create
/// time; any well-formed body returns 201 with the session id.
async fn create_session(
    client: &TestClient,
    vertical: &str,
    workflow: &str,
    context: serde_json::Value,
) -> String {
    let resp = client
        .post_json(
            "/api/sessions",
            serde_json::json!({
                "name": "evolution-test",
                "vertical": vertical,
                "workflow": workflow,
                "context": context,
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "create_session unexpected status",
    );
    let body: serde_json::Value = resp.json().await.expect("create body");
    body["id"]
        .as_str()
        .expect("session id in create response")
        .to_string()
}

/// Launch a session via `POST /api/sessions/:id/launch`. Returns the
/// response untouched so the caller can assert on status + body shape.
async fn launch_session(client: &TestClient, sid: &str) -> reqwest::Response {
    client
        .post_json(
            &format!("/api/sessions/{sid}/launch"),
            serde_json::json!({}),
        )
        .await
}

/// Extract the `details.violations[].code` array from a 422 launch response.
/// Shape per `console-api/src/error.rs:49–63`: the ApiError body wraps the
/// validation details as `{ "error": "validation_failed", "message": ...,
/// "details": { "violations": [...], "warnings": [...] } }`.
async fn violation_codes(resp: reqwest::Response) -> Vec<String> {
    let body: serde_json::Value = resp.json().await.expect("launch body");
    body["details"]["violations"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v["code"].as_str().map(str::to_string))
        .collect()
}

/// DB-direct seed of a session in state=ACTIVE. Mirrors the pattern from
/// `recovery_matrix.rs:174` — skips the full create + launch path (which
/// would require seeded profiles + a live coordinator) and writes straight
/// into the sessions table.
async fn seed_active_session_with_context(
    db: &DbPool,
    sid: &str,
    owner: &str,
    vertical: &str,
    workflow: &str,
    context_json: serde_json::Value,
) {
    sqlx::query(
        "INSERT OR IGNORE INTO users (id, username, username_lower, display_name, password_hash,
            console_role, must_change_password, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(owner)
    .bind(owner)
    .bind(owner)
    .bind(owner)
    .bind("h")
    .bind("operator")
    .bind(0_i64)
    .bind("2026-04-24T00:00:00Z")
    .bind("2026-04-24T00:00:00Z")
    .execute(db)
    .await
    .expect("seed user");

    let row = sessions::SessionRow {
        id: sid.into(),
        name: Some(sid.into()),
        owner_user_id: owner.into(),
        vertical: vertical.into(),
        workflow: workflow.into(),
        context: Some(context_json.to_string()),
        coordinator_workspace_id: None,
        state: console_core::session_state::ACTIVE.into(),
        created_at: "2026-04-24T00:00:00Z".into(),
        launched_at: Some("2026-04-24T00:00:00Z".into()),
        closed_at: None,
        budget_max_cost_micros: None,
        budget_max_tokens: None,
        budget_max_wall_time_ms: None,
    };
    sessions::insert_session(db, &row).await.expect("seed row");
}

/// A v1-shaped context that satisfies `fixture_context_v1` (project_id +
/// priority as Enum<low|medium|high>). Reused across tests.
fn v1_context() -> serde_json::Value {
    serde_json::json!({
        "project_id": "p-evolve",
        "priority": "high",
    })
}

// ---- tests ----------------------------------------------------------------

#[tokio::test]
async fn launch_rejected_after_field_added_as_required() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let mock = MockRest::spawn(single_vertical(fixtures::fixture_context_v1()))
        .await
        .expect("mock rest");
    let db = console_db::create_test_pool().await.expect("db");
    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");
    point_at_mock(&console, &mock).await;
    let client = admin_client(&console).await;

    reload(&client).await; // pick up v1
    let sid = create_session(&client, "fixture-evolution", "evolve-flow", v1_context()).await;

    // Evolve: swap to v2_breaking (narrows priority + adds required region).
    mock.state
        .set_verticals(single_vertical(fixtures::fixture_context_v2_breaking()));
    reload(&client).await;

    let resp = launch_session(&client, &sid).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let codes = violation_codes(resp).await;
    assert!(
        codes.iter().any(|c| c == "MISSING_CONTEXT"),
        "expected MISSING_CONTEXT (for added `region`) in violations; got {codes:?}",
    );
}

#[tokio::test]
async fn launch_rejected_after_field_type_narrowed() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let mock = MockRest::spawn(single_vertical(fixtures::fixture_context_v1()))
        .await
        .expect("mock rest");
    let db = console_db::create_test_pool().await.expect("db");
    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");
    point_at_mock(&console, &mock).await;
    let client = admin_client(&console).await;

    reload(&client).await;
    // v1 stores `priority: "high"` as a string.
    let sid = create_session(&client, "fixture-evolution", "evolve-flow", v1_context()).await;

    // Evolve: v2_breaking narrows priority from Enum<string> to Number.
    mock.state
        .set_verticals(single_vertical(fixtures::fixture_context_v2_breaking()));
    reload(&client).await;

    let resp = launch_session(&client, &sid).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let codes = violation_codes(resp).await;
    assert!(
        codes.iter().any(|c| c == "INVALID_CONTEXT"),
        "expected INVALID_CONTEXT (for narrowed `priority` string→number) in \
         violations; got {codes:?}",
    );
}

#[tokio::test]
async fn active_session_preserved_across_schema_evolution() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let mock = MockRest::spawn(single_vertical(fixtures::fixture_context_v1()))
        .await
        .expect("mock rest");
    let db = console_db::create_test_pool().await.expect("db");
    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");
    point_at_mock(&console, &mock).await;
    let client = admin_client(&console).await;
    reload(&client).await; // pick up v1

    // Seed a session directly in ACTIVE state (bypassing create+launch —
    // that path would need seeded profiles + a live coordinator roundtrip
    // and is exercised elsewhere). The session's context is v1-shaped.
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    let ctx = v1_context();
    seed_active_session_with_context(
        &db,
        &sid,
        "u-preserve",
        "fixture-evolution",
        "evolve-flow",
        ctx.clone(),
    )
    .await;

    let before = sessions::get_by_id(&db, &sid)
        .await
        .expect("get row")
        .expect("row exists");

    // Evolve: swap to v2_breaking (would reject this session on a fresh
    // launch — but the seeded session is already past the validation gate).
    mock.state
        .set_verticals(single_vertical(fixtures::fixture_context_v2_breaking()));
    reload(&client).await;

    let after = sessions::get_by_id(&db, &sid)
        .await
        .expect("get row after reload")
        .expect("row still exists");

    assert_eq!(after.state, before.state, "state must not change on reload");
    assert_eq!(
        after.state,
        console_core::session_state::ACTIVE,
        "seeded ACTIVE must survive evolution",
    );
    assert_eq!(
        after.context, before.context,
        "stored context must not be re-written by reload",
    );
}

#[tokio::test]
async fn additive_evolution_accepts_session_without_new_optional_field() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let mock = MockRest::spawn(single_vertical(fixtures::fixture_context_v1()))
        .await
        .expect("mock rest");
    let db = console_db::create_test_pool().await.expect("db");
    let console = ConsoleHarness::spawn_with_db(&rt, db.clone())
        .await
        .expect("console");
    point_at_mock(&console, &mock).await;
    let client = admin_client(&console).await;

    reload(&client).await;
    let sid = create_session(&client, "fixture-evolution", "evolve-flow", v1_context()).await;

    // Evolve: v2_additive adds optional `notes`. v1-shaped context still
    // valid. (The session will still fail launch on MISSING_ASSIGNMENT
    // noise — no profiles seeded — but that's orthogonal.)
    mock.state
        .set_verticals(single_vertical(fixtures::fixture_context_v2_additive()));
    reload(&client).await;

    let resp = launch_session(&client, &sid).await;
    // Launch still returns 422 because no assignments exist — but the
    // 422 must NOT be due to context validation under additive evolution.
    let codes = violation_codes(resp).await;
    assert!(
        !codes.iter().any(|c| c == "MISSING_CONTEXT"),
        "additive evolution must not produce MISSING_CONTEXT; got {codes:?}",
    );
    assert!(
        !codes.iter().any(|c| c == "INVALID_CONTEXT"),
        "additive evolution must not produce INVALID_CONTEXT; got {codes:?}",
    );
}
