//! W7 — Cross-session + concurrency (T7.7, T7.8, T7.9, T7.10).
//!
//! T7.9 covers the W6 ownership filter end-to-end: two sessions owned by
//! two users, the operator caller sees only their own pending list,
//! admin sees both. Implemented without LLM by inserting handles
//! directly into `active_sessions`.
//!
//! T7.7 / T7.8 / T7.10 need real session lifecycle and gate flow —
//! `#[ignore]`'d with reasons.

use std::sync::Arc;
use std::time::Duration;

use console_core::event_enricher::EnrichedGate;
use console_core::session_monitor::{Frame, MonitorCmd, PendingState, SessionMonitorHandle};
use console_db::queries::sessions;
use console_integration::{ConsoleHarness, RuntimeHarness, TestClient};
use tokio::sync::{broadcast, mpsc};

#[tokio::test]
async fn t7_9_pending_gates_filter_by_owner_and_admin_sees_all() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");

    seed_user(&console.state.db, "u-owner").await;
    seed_user(&console.state.db, "u-other").await;
    seed_user(&console.state.db, "u-admin").await;
    update_role(&console.state.db, "u-admin", "admin").await;

    let sid_owner = format!("s-owner-{}", uuid::Uuid::new_v4());
    let sid_other = format!("s-other-{}", uuid::Uuid::new_v4());
    seed_session(&console.state.db, &sid_owner, "u-owner").await;
    seed_session(&console.state.db, &sid_other, "u-other").await;

    let h_owner = install_handle_with_gate(&console.state, &sid_owner, "g-owner-1").await;
    let h_other = install_handle_with_gate(&console.state, &sid_other, "g-other-1").await;
    let _ = (h_owner, h_other);

    // u-owner should see only their gate.
    let owner_client =
        TestClient::seed_user(&console.state, &console.base_url(), "u-owner", "operator").await;
    let resp = owner_client.get("/api/gates/pending").await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.expect("json");
    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["session_id"], sid_owner);

    // u-admin should see both.
    let admin_client =
        TestClient::seed_user(&console.state, &console.base_url(), "u-admin", "admin").await;
    let resp = admin_client.get("/api/gates/pending").await;
    let body: serde_json::Value = resp.json().await.expect("json");
    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 2);
}

#[tokio::test]
#[ignore = "inherits T7.2 / T7.3 blocker (§13.7.6 next steps): needs AgentService::EmitSignal to advance workspace FSM so 10 stub-driven sessions can reach COMPLETED. Stub provider is landed and exercised by llm_stub_e2e.rs; this test un-ignores in the same change that closes T7.3."]
async fn t7_7_ten_concurrent_sessions_complete() {
    // Future: launch 10 sessions, wait for all to reach COMPLETED, assert
    // no monitor task held >50 MB resident.
}

#[tokio::test]
#[ignore = "inherits T7.2 blocker (§13.7.6 next steps): needs runtime-side gate emission so high-frequency frames can exist in the first place. Stub provider is landed; un-ignores alongside T7.2."]
async fn t7_8_slow_ws_consumer_does_not_starve_others() {
    // Future: open two WS clients, sleep one for 2 s between reads,
    // assert the other receives all frames + the slow one gets a
    // control-channel `lag` frame.
}

#[tokio::test]
#[ignore = "inherits T7.2 blocker (§13.7.6 next steps): needs a live gate event from the coordinator so W4→W6 latency can be measured. Un-ignores in the same change as T7.2."]
async fn t7_10_w4_resolve_clears_w6_pending_within_500ms() {
    // Future: gate appears via runtime stream (T7.2 prerequisite),
    // POST /api/sessions/:id/gates/:gid → assert /api/gates/pending no
    // longer lists it within 500 ms.
}

// ---- helpers ---------------------------------------------------------------

async fn seed_user(db: &console_db::DbPool, id: &str) {
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

async fn update_role(db: &console_db::DbPool, id: &str, role: &str) {
    sqlx::query("UPDATE users SET console_role = ? WHERE id = ?")
        .bind(role)
        .bind(id)
        .execute(db)
        .await
        .expect("update role");
}

async fn seed_session(db: &console_db::DbPool, sid: &str, owner: &str) {
    let row = sessions::SessionRow {
        id: sid.into(),
        name: Some(sid.into()),
        owner_user_id: owner.into(),
        vertical: "swe".into(),
        workflow: "wf".into(),
        context: None,
        coordinator_workspace_id: Some("ws-root".into()),
        state: console_core::session_state::ACTIVE.into(),
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

async fn install_handle_with_gate(
    state: &Arc<console_api::AppState>,
    sid: &str,
    gate_id: &str,
) -> SessionMonitorHandle {
    let pending = Arc::new(PendingState::default());
    pending.gates.write().await.push(EnrichedGate {
        gate_id: gate_id.into(),
        type_: "task_approval".into(),
        workspace_id: "ws-root".into(),
        workspace_label: "ws-root".into(),
        task_id: "t-1".into(),
        timeout_ms: 0,
        fallback_action: String::new(),
        created_at: String::new(),
        subject_len: 0,
    });
    let handle = SessionMonitorHandle {
        session_id: sid.into(),
        cmd_tx: mpsc::channel::<MonitorCmd>(1).0,
        broadcast_tx: broadcast::channel::<Frame>(8).0,
        pending,
    };
    state
        .active_sessions
        .write()
        .await
        .insert(sid.into(), handle.clone());
    handle
}

#[allow(dead_code)]
fn _silence_unused_duration() -> Duration {
    Duration::from_millis(500)
}
