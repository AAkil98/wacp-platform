//! Integration tests for `GET /api/auth/bootstrap-state` (plan §3.6 P5.A).
//!
//! Three states tested:
//! 1. Fresh DB (no admin) — has_admin_user=false; token returned only if
//!    the XDG bootstrap-token file exists.
//! 2. After admin seeded — has_admin_user=true; token field is null even
//!    if the file is still on disk (security gate per plan §4 acceptance #3).
//! 3. Path-shape — bootstrap_token_path always ends in `bootstrap-token`
//!    regardless of admin state.
//!
//! Endpoint is unauthenticated by design — pre-login UI consumes it
//! before any session cookie or bearer token exists.

use console_integration::{ConsoleHarness, RuntimeHarness};
use serde_json::Value;

async fn get_unauth(base_url: &str, path: &str) -> reqwest::Response {
    reqwest::Client::new()
        .get(format!("{base_url}{path}"))
        .send()
        .await
        .expect("GET")
}

#[tokio::test]
async fn bootstrap_state_no_admin_returns_has_admin_false_with_path() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");

    let resp = get_unauth(&console.base_url(), "/api/auth/bootstrap-state").await;

    assert_eq!(resp.status(), 200, "endpoint must be unauthenticated");
    let body: Value = resp.json().await.expect("json body");

    assert_eq!(body["has_admin_user"], false);
    let path = body["bootstrap_token_path"].as_str().expect("path string");
    assert!(
        path.ends_with("bootstrap-token"),
        "expected path ending in bootstrap-token, got {path}"
    );
    // bootstrap_token may be null (no file on test host) or a string (file
    // exists from a prior local boot). Both shapes are valid; security gate
    // is verified in the next test.
}

#[tokio::test]
async fn bootstrap_state_with_admin_returns_has_admin_true_no_token() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");

    // Seed an admin directly via SQL to avoid touching the bootstrap flow.
    sqlx::query(
        "INSERT INTO users (id, username, username_lower, display_name, password_hash,
            console_role, must_change_password, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("admin-id")
    .bind("admin")
    .bind("admin")
    .bind("Admin")
    .bind("$2b$10$abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstu")
    .bind("admin")
    .bind(0_i64)
    .bind("2026-04-23T00:00:00Z")
    .bind("2026-04-23T00:00:00Z")
    .execute(&console.state.db)
    .await
    .expect("seed admin");

    let resp = get_unauth(&console.base_url(), "/api/auth/bootstrap-state").await;

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("json body");

    assert_eq!(body["has_admin_user"], true);
    // Security gate: token field must be null once an admin exists, even
    // if the bootstrap-token file is still on disk. Plan §4 acceptance #3.
    assert!(
        body["bootstrap_token"].is_null(),
        "token leaked after admin present: {body:?}"
    );
    // Path is still surfaced for support purposes — it's not a credential.
    let path = body["bootstrap_token_path"].as_str().expect("path string");
    assert!(path.ends_with("bootstrap-token"));
}
