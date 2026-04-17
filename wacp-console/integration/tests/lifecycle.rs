//! W7 — Happy-path lifecycle (T7.1, T7.2, T7.3).
//!
//! T7.1 boots runtime + console and exercises auth + `/api/health`.
//! T7.2 / T7.3 drive the runtime through coordinator state transitions —
//! a live agent binds, emits signals / checkpoints, and the coordinator
//! must fan those into gate events and terminal transitions. The `wacp-llm`
//! stub provider landed in `AUDIT-2026-04-15.md` §13.7.6 and is exercised
//! end-to-end by `llm_stub_e2e.rs`. The remaining gap is runtime-side:
//! `AgentService::Bind` does not return a real directive, `EmitSignal`
//! does not advance workspace state in the coordinator, and
//! `CreateCheckpoint` does not fan into highway gates. Until those
//! handlers are wired, T7.2 / T7.3 stay `#[ignore]`'d with reasons
//! pointing at the specific runtime work, not the absent stub.

use std::time::Duration;

use console_integration::{ConsoleHarness, RuntimeHarness};

#[tokio::test]
async fn t7_1_runtime_boots_console_attaches_health_reports_ok() {
    let rt = RuntimeHarness::spawn_default()
        .await
        .expect("runtime harness");
    let console = ConsoleHarness::spawn(&rt).await.expect("console harness");
    let client = console_integration::TestClient::seed_user(
        &console.state,
        &console.base_url(),
        "u-1",
        "operator",
    )
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
        assert_eq!(
            checks.get(key).and_then(|v| v.as_str()),
            Some("ok"),
            "{key}"
        );
    }
    drop(client);
    drop(console);
    drop(rt);
}

#[tokio::test]
#[ignore = "needs runtime-side agent→coordinator wiring: AgentService::CreateCheckpoint currently returns OK without emitting a highway gate event. Stub LLM provider is already landed (§13.7.6) and exercised by llm_stub_e2e.rs; this test un-ignores when the coordinator fans provisional checkpoints into gates."]
async fn t7_2_open_ws_drive_gate_observe_resume() {
    // When the runtime's AgentService is wired to the highway, this test
    // launches a session, spawns a StubAgent per dispatched workspace,
    // opens the WS, observes the gate frame, approves via W4's
    // /api/sessions/:id/gates/:gid endpoint, then asserts the runtime
    // emits a workspace-resume trail entry within 2 s on the WS.
    let _ = (Duration::from_secs(2),);
}

#[tokio::test]
#[ignore = "needs runtime-side agent→coordinator wiring: AgentService::EmitSignal does not drive the workspace FSM today, so a Complete signal does not transition the workspace to Integrating→Closed. Stub LLM provider is landed (§13.7.6); this test un-ignores once the signal handler connects to the WorkspaceActor state machine."]
async fn t7_3_session_completes_emits_final_frame() {
    // When EmitSignal advances workspace state, this test runs a session
    // through to COMPLETED and asserts the final WS frame + DB row +
    // active_sessions removal.
}
