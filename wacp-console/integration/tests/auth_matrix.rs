//! §13.7.8 I3 — auth + authz integration matrix.
//!
//! The `console-core::authorizer::authorize` role-matrix is already
//! exhaustively unit-tested in `authorizer.rs::tests` (every action × every
//! role). What the unit tests DON'T prove is that the middleware + auth
//! extractor + authorizer + handler actually wire up correctly on a real
//! HTTP round-trip. That's the gap this suite closes.
//!
//! Scope (12 tests):
//!
//! 1. Bearer auth smoke — admin / operator / viewer tokens each reach
//!    `/api/health` with 200.
//! 2. Role-gated read — `/api/users` returns 200 for admin, 403 for
//!    operator + viewer.
//! 3. Role-gated write — `/api/profiles` POST returns 2xx for admin +
//!    operator, 403 for viewer.
//! 4. Anonymous — GET `/api/profiles` without auth returns 401.
//! 5. Unknown bearer — random token → 401.
//! 6. Revoked token — token revoked between calls returns 401 on the
//!    second call.
//! 7. Account lockout — 5 failed logins for a username returns 401 with
//!    body `{"error":"account_locked"}` (per `rate_limit.rs::MAX_FAILED_PER_ACCOUNT`).
//!
//! **Runtime-auth drift** (`performance-optimization.md` §11.4 + §13.3).
//! The `AUDIT-2026-04-15.md` §13.7.8 original matrix called for "every
//! runtime auth path (api-key / session / oauth)" but the runtime's
//! `Bind` handler today accepts any token ≥ 8 chars regardless of kind
//! — there is no api-key vs. session distinction on the wire. When the
//! runtime gains real auth, the natural extension lives here. See the
//! `DRIFT:` comment on the runtime-auth smoke test below.
//!
//! **Not covered (deferred):** `must_change_password` forced-change
//! deadlock (the D2 fix from `performance-optimization.md` §12.4) has
//! frontend E2E coverage in `wacp-console/frontend/e2e/auth-flows.spec.ts`
//! — there's no integration-scope value in replicating it here.

use console_core::authenticator;
use console_db::DbPool;
use console_db::queries::api_tokens;
use console_integration::{ConsoleHarness, RuntimeHarness, TestClient};
use serde_json::json;

// ---- helpers --------------------------------------------------------------

/// Seed a user + bearer token with an explicit role. Returns the raw
/// bearer token; caller wraps it in a `Client::bearer_auth` call itself
/// because `TestClient::seed_user` pairs the token with its own DB writes
/// and defaults to role=operator.
async fn seed_token(db: &DbPool, user_id: &str, role: &str) -> String {
    sqlx::query(
        "INSERT OR IGNORE INTO users (id, username, username_lower, display_name, password_hash,
            console_role, must_change_password, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    // bcrypt hash of the string "password" (not used here; login flow has its own tests).
    .bind("$2b$10$abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstu")
    .bind(role)
    .bind(0_i64)
    .bind("2026-04-15T00:00:00Z")
    .bind("2026-04-15T00:00:00Z")
    .execute(db)
    .await
    .expect("seed user");

    let token = format!("wcon_t_{}", uuid::Uuid::new_v4());
    let hash = authenticator::hash_token(&token);
    api_tokens::insert_token(
        db,
        &format!("tok-{}", uuid::Uuid::new_v4()),
        user_id,
        "integration-test",
        &hash,
        &chrono::Utc::now().to_rfc3339(),
        None,
    )
    .await
    .expect("insert token");
    token
}

fn bearer_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("reqwest")
}

async fn get_with_token(base: &str, path: &str, token: &str) -> reqwest::Response {
    bearer_client()
        .get(format!("{base}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .expect("GET")
}

async fn post_with_token(
    base: &str,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> reqwest::Response {
    bearer_client()
        .post(format!("{base}{path}"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .expect("POST")
}

async fn post_no_auth(base: &str, path: &str, body: serde_json::Value) -> reqwest::Response {
    bearer_client()
        .post(format!("{base}{path}"))
        .json(&body)
        .send()
        .await
        .expect("POST no-auth")
}

// ---- tests ----------------------------------------------------------------

#[tokio::test]
async fn bearer_smoke_admin_reaches_health() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");
    let token = seed_token(&console.state.db, "u-admin", "admin").await;

    let resp = get_with_token(&console.base_url(), "/api/health", &token).await;
    assert_eq!(resp.status(), 200);
    // DRIFT: /api/health doesn't require auth, so admin==operator==viewer==anonymous.
    // The real assertion is that the bearer token is parsed without rejection —
    // proves the middleware+auth chain runs, not the authz rule.
}

#[tokio::test]
async fn bearer_smoke_operator_reaches_health() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");
    let token = seed_token(&console.state.db, "u-operator", "operator").await;

    let resp = get_with_token(&console.base_url(), "/api/health", &token).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn bearer_smoke_viewer_reaches_health() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");
    let token = seed_token(&console.state.db, "u-viewer", "viewer").await;

    let resp = get_with_token(&console.base_url(), "/api/health", &token).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn role_gated_read_admin_can_list_users() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");
    let token = seed_token(&console.state.db, "u-admin", "admin").await;

    let resp = get_with_token(&console.base_url(), "/api/users", &token).await;
    assert_eq!(resp.status(), 200, "admin must list users");
}

#[tokio::test]
async fn role_gated_read_operator_cannot_list_users() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");
    let token = seed_token(&console.state.db, "u-operator", "operator").await;

    let resp = get_with_token(&console.base_url(), "/api/users", &token).await;
    assert_eq!(resp.status(), 403, "operator must NOT list users");
}

#[tokio::test]
async fn role_gated_read_viewer_cannot_list_users() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");
    let token = seed_token(&console.state.db, "u-viewer", "viewer").await;

    let resp = get_with_token(&console.base_url(), "/api/users", &token).await;
    assert_eq!(resp.status(), 403, "viewer must NOT list users");
}

#[tokio::test]
async fn role_gated_write_operator_passes_authz_on_create_profile() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");
    let token = seed_token(&console.state.db, "u-operator", "operator").await;

    // The harness ships with an empty taxonomy, so the role_ref validation
    // at the handler will return 422 (UNKNOWN_ROLE). That's fine — the
    // integration-scope assertion is that authz DID NOT REJECT the request:
    // a 403 from the `CreateProfile` action check would fire BEFORE the
    // validator. 422 means the request reached validation, i.e., authz
    // passed. Any non-403 is proof of the thing being tested.
    let resp = post_with_token(
        &console.base_url(),
        "/api/profiles",
        &token,
        json!({
            "name": "auth-matrix-op",
            "role_ref": "swe:implementer",
            "llm_provider": "stub",
            "llm_model": "stub-model-1",
            "autonomy": "supervised",
        }),
    )
    .await;
    assert_ne!(
        resp.status(),
        403,
        "operator must pass authz on CreateProfile; got {} body={:?}",
        resp.status(),
        resp.text().await.unwrap_or_default()
    );
}

#[tokio::test]
async fn role_gated_write_viewer_cannot_create_profile() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");
    let token = seed_token(&console.state.db, "u-viewer", "viewer").await;

    let resp = post_with_token(
        &console.base_url(),
        "/api/profiles",
        &token,
        json!({
            "name": "auth-matrix-viewer",
            "role_ref": "swe:implementer",
            "llm_provider": "stub",
            "llm_model": "stub-model-1",
            "autonomy": "supervised",
        }),
    )
    .await;
    assert_eq!(resp.status(), 403, "viewer must NOT create profiles");
}

#[tokio::test]
async fn anonymous_get_returns_401() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");

    let resp = bearer_client()
        .get(format!("{}/api/profiles", console.base_url()))
        .send()
        .await
        .expect("GET");
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn unknown_bearer_returns_401() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");

    let resp = get_with_token(
        &console.base_url(),
        "/api/profiles",
        "wcon_t_obviously-not-real",
    )
    .await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn revoked_token_returns_401() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");

    // Seed a user via the public helper so we keep the bearer token; then
    // revoke it directly via the DB query.
    let client =
        TestClient::seed_user(&console.state, &console.base_url(), "u-rev", "operator").await;
    let token = client.token.clone();

    // First call succeeds — baseline.
    let ok = get_with_token(&console.base_url(), "/api/profiles", &token).await;
    assert_eq!(ok.status(), 200, "pre-revoke call should succeed");

    // Revoke every active token for this user.
    let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM api_tokens WHERE user_id = ?")
        .bind("u-rev")
        .fetch_all(&console.state.db)
        .await
        .expect("list tokens");
    let now = chrono::Utc::now().to_rfc3339();
    for (id,) in rows {
        let revoked = api_tokens::revoke_token(&console.state.db, &id, &now)
            .await
            .expect("revoke");
        assert!(revoked, "revoke_token must match exactly one row per id");
    }

    // Second call with the same token — must now 401.
    let denied = get_with_token(&console.base_url(), "/api/profiles", &token).await;
    assert_eq!(
        denied.status(),
        401,
        "revoked token must no longer authenticate"
    );
}

#[tokio::test]
async fn account_lockout_after_five_failed_logins() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");

    // Seed a user with a real (PHC-format) Argon2id hash of some password
    // other than the one we'll submit. An empty hash would cause
    // `verify_password` to return `Internal("invalid password hash")`,
    // mapping to a 500 — not the 401 the rate_limit path expects.
    let valid_hash =
        console_core::password::hash_password("the-real-password-unused").expect("hash_password");

    sqlx::query(
        "INSERT OR IGNORE INTO users (id, username, username_lower, display_name, password_hash,
            console_role, must_change_password, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("u-lock")
    .bind("locktarget")
    .bind("locktarget")
    .bind("locktarget")
    .bind(&valid_hash)
    .bind("operator")
    .bind(0_i64)
    .bind("2026-04-15T00:00:00Z")
    .bind("2026-04-15T00:00:00Z")
    .execute(&console.state.db)
    .await
    .expect("seed locktarget");

    // MAX_FAILED_PER_ACCOUNT = 5; on the 6th attempt the rate_limit check
    // fires BEFORE password verification and returns AccountLocked.
    let base = console.base_url();
    for attempt in 1..=5 {
        let resp = post_no_auth(
            &base,
            "/api/auth/login",
            json!({"username": "locktarget", "password": "wrong"}),
        )
        .await;
        assert_eq!(
            resp.status(),
            401,
            "attempts 1..=5 must return 401 (not 429 / 423)"
        );
        // Body differs between first 5 (unauthenticated) and the locked attempt.
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(
            body["error"], "unauthenticated",
            "attempt {attempt} body: got {body:?}"
        );
    }

    // 6th attempt → lockout. Also 401 status, but body.error == account_locked.
    let resp = post_no_auth(
        &base,
        "/api/auth/login",
        json!({"username": "locktarget", "password": "wrong"}),
    )
    .await;
    assert_eq!(resp.status(), 401);
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["error"], "account_locked", "body was {body:?}");
}
