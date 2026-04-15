//! W7 — Happy-path lifecycle (T7.1, T7.2, T7.3).
//!
//! T7.1 boots runtime + console and exercises auth + `/api/health`.
//! T7.2 / T7.3 require driving the runtime through coordinator state
//! transitions, which depends on a programmable LLM provider that the
//! runtime does not ship — see the W7 deviation note in
//! `impl/wiring-phases.md`. Those are `#[ignore]`'d with explicit
//! reasons rather than silently skipped.

use std::time::Duration;

use console_integration::{ConsoleHarness, RuntimeHarness};

#[tokio::test]
async fn t7_1_runtime_boots_console_attaches_health_reports_ok() {
    let rt = RuntimeHarness::spawn_default()
        .await
        .expect("runtime harness");
    let console = ConsoleHarness::spawn(&rt).await.expect("console harness");
    let client =
        console_integration::TestClient::seed_user(&console.state, &console.base_url(), "u-1", "operator")
            .await;

    let resp = client.get("/api/health").await;
    assert!(resp.status().is_success(), "health: {}", resp.status());
    let body: serde_json::Value = resp.json().await.expect("json");

    // The /api/health response schema is `{status, version, checks{...}}`
    // (see console-api/src/handlers/health.rs). Assert the grpc pool
    // reports each runtime channel as healthy — this proves W1 wiring
    // (AppState → GrpcPool → live gRPC server) end-to-end.
    assert_eq!(body["status"], "healthy", "got {body:?}");
    assert!(body["version"].is_string());
    let checks = body["checks"].as_object().expect("checks object");
    for key in [
        "database",
        "runtime_agent",
        "runtime_highway",
        "runtime_coordinator",
        "runtime_rest",
    ] {
        assert_eq!(checks.get(key).and_then(|v| v.as_str()), Some("ok"), "{key}");
    }
    drop(client);
    drop(console);
    drop(rt);
}

#[tokio::test]
#[ignore = "needs LLM stub: T7.2 requires the runtime to dispatch a workspace and emit a gate event, which depends on a model invocation."]
async fn t7_2_open_ws_drive_gate_observe_resume() {
    // When a programmable mock LLM ships, this test launches a session,
    // opens the WS, uses TestClient to approve the gate via W4's
    // /api/sessions/:id/gates/:gid endpoint, then asserts the runtime
    // emits a workspace-resume trail entry within 2 s on the WS.
    let _ = (Duration::from_secs(2),);
}

#[tokio::test]
#[ignore = "needs LLM stub: T7.3 requires driving the coordinator workspace to a terminal state, which depends on agent and LLM cooperation."]
async fn t7_3_session_completes_emits_final_frame() {
    // When a scripted coordinator fixture exists, this test runs a session
    // through to COMPLETED and asserts the final WS frame + DB row +
    // active_sessions removal.
}
