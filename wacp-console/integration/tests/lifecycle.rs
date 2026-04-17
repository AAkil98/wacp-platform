//! W7 — Happy-path lifecycle (T7.1, T7.2, T7.3).
//!
//! T7.1 boots runtime + console and exercises auth + `/api/health`.
//! T7.2 / T7.3 drive the runtime through coordinator state transitions
//! — a live agent binds, emits signals / checkpoints, and the
//! coordinator must fan those into gate events and terminal
//! transitions. §13.7.6b WA3.5 (checkpoint-approval gates) + WA3.6
//! (auto-integration on Complete) close the runtime-side gaps these
//! tests depend on. The tests bypass the full HTTP launch path and
//! drive the runtime directly via `CoordinatorService` + `wacp-sdk`
//! — the goal is to observe the monitor + DB state transitions that
//! WA3.5/WA3.6 make possible, not to re-test the API layer.

use std::sync::Arc;
use std::time::Duration;

use console_core::event_enricher::EventEnricher;
use console_core::refusal_synthesizer::RefusalSynthesizer;
use console_core::session_monitor::{self, MonitorConfig, WorkspaceSet};
use console_core::session_state;
use console_db::queries::sessions;
use console_integration::{ConsoleHarness, RuntimeHarness, TestClient};
use wacp_transport::wacp_v1;
use wacp_transport::wacp_v1::coordinator_service_client::CoordinatorServiceClient;
use wacp_transport::wacp_v1::highway_service_client::HighwayServiceClient;

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
async fn t7_2_open_ws_drive_gate_observe_resume() {
    // §13.7.6b WA3.5 un-ignore. Drives a workspace through:
    //   Idle → Active → (provisional checkpoint blocks) → Blocked
    //   → (RespondToGate Approve) → Active.
    // Asserts:
    // 1. The session monitor emits a `gates` channel frame for the
    //    checkpoint approval gate within 2 s of CreateCheckpoint.
    // 2. RespondToGate(Approve) → the monitor's WorkspaceChanges driver
    //    sees the Blocked → Active resume within 2 s.
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");

    let mut coord = coord_client(&rt).await;
    let submit = coord
        .submit_goal(wacp_v1::SubmitGoalRequest {
            description: "t7.2 goal".into(),
            context: vec![],
            client_request_id: String::new(),
        })
        .await
        .expect("submit_goal")
        .into_inner();
    let ws_id = submit.root_workspace_id;

    let client = TestClient::seed_user(
        &console.state,
        &console.base_url(),
        "u-1",
        "operator",
    )
    .await;
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_active_session(&console.state.db, &sid, "u-1", &ws_id).await;

    let (handle, _join) = session_monitor::spawn(
        sid.clone(),
        WorkspaceSet::new(ws_id.clone(), Vec::<String>::new()),
        console.state.grpc_pool.clone(),
        console.state.db.clone(),
        EventEnricher::new(console.state.taxonomy.clone()),
        RefusalSynthesizer::new(),
        MonitorConfig {
            broadcast_capacity: 64,
            reconnect_initial: Duration::from_millis(50),
            reconnect_max: Duration::from_millis(200),
            reconnect_failure_cap: 30,
        },
    );
    let broadcast_tx = handle.broadcast_tx.clone();
    console
        .state
        .active_sessions
        .write()
        .await
        .insert(sid.clone(), handle);

    let mut rx = broadcast_tx.subscribe();
    tokio::time::sleep(Duration::from_millis(200)).await; // monitor connect

    coord
        .send_directive(wacp_v1::SendDirectiveRequest {
            workspace_id: ws_id.clone(),
            payload: b"activate".to_vec(),
            client_request_id: String::new(),
        })
        .await
        .expect("send_directive");

    let agent = wacp_sdk::Agent::connect(wacp_sdk::AgentConfig {
        runtime_url: format!("http://{}", rt.agent_addr()),
        workspace_id: wacp_types::WorkspaceId::from(ws_id.as_str()),
        auth_token: "t7-2-agent-token".into(),
    })
    .await
    .expect("agent connect");

    // Provisional checkpoint → WA3.5 opens a gate + actor blocks.
    agent
        .checkpoint()
        .checkpoint_type("task_approval")
        .payload(b"propose")
        .status(wacp_types::CheckpointStatus::Provisional)
        .create()
        .await
        .expect("create_checkpoint");

    // Wait for the gate frame on the broadcast.
    let gate_id = wait_for_gate_frame(&mut rx, Duration::from_secs(2))
        .await
        .expect("gate frame within 2 s");

    // Approve via HighwayService::RespondToGate. WA3.5 routes the
    // approval to the workspace actor → Blocked → Active.
    let mut highway = HighwayServiceClient::connect(format!("http://{}", rt.highway_addr()))
        .await
        .expect("highway connect");
    let ack = highway
        .respond_to_gate(wacp_v1::GateResponse {
            gate_id: gate_id.clone(),
            decision: wacp_v1::GateDecision::Approve as i32,
            modifications: Vec::new(),
            client_request_id: String::new(),
        })
        .await
        .expect("respond_to_gate")
        .into_inner();
    assert!(ack.applied, "gate response must be applied");

    // Wait for the workspace_change frame showing the resume (current=active).
    let saw_resume = wait_for_resume_frame(&mut rx, &ws_id, Duration::from_secs(2)).await;
    assert!(
        saw_resume,
        "workspace must emit Blocked→Active workspace_change within 2 s"
    );

    agent.disconnect().await.ok();
    drop(client);
    drop(console);
    drop(rt);
}

async fn wait_for_gate_frame(
    rx: &mut tokio::sync::broadcast::Receiver<console_core::session_monitor::Frame>,
    timeout: Duration,
) -> Option<String> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(frame)) => {
                let json = serde_json::to_value(&frame).unwrap_or_default();
                if json.get("channel").and_then(|c| c.as_str()) == Some("gates")
                    && let Some(id) = json["event"]["gate_id"].as_str()
                {
                    return Some(id.to_string());
                }
            }
            _ => return None,
        }
    }
    None
}

async fn wait_for_resume_frame(
    rx: &mut tokio::sync::broadcast::Receiver<console_core::session_monitor::Frame>,
    ws_id: &str,
    timeout: Duration,
) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(frame)) => {
                let json = serde_json::to_value(&frame).unwrap_or_default();
                if json.get("channel").and_then(|c| c.as_str()) == Some("workspaces")
                    && json["event"]["workspace_id"].as_str() == Some(ws_id)
                    && json["event"]["current"] == "active"
                    && json["event"]["previous"] == "blocked"
                {
                    return true;
                }
            }
            _ => return false,
        }
    }
    false
}

#[tokio::test]
async fn t7_3_session_completes_emits_final_frame() {
    // §13.7.6b WA3.6 un-ignore. Drives a workspace through
    // Idle → Active → Integrating → Closed via an agent's Complete
    // signal, bypassing the HTTP launch path. Asserts:
    // 1. The session monitor observes the root-workspace Closed transition
    //    and emits a `session` channel Frame with state "completed".
    // 2. The DB session row transitions ACTIVE → COMPLETED.
    // 3. The active_sessions map drops the session.
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");

    // Create a workspace directly via the coordinator gRPC.
    let mut coord = coord_client(&rt).await;
    let submit = coord
        .submit_goal(wacp_v1::SubmitGoalRequest {
            description: "t7.3 goal".into(),
            context: vec![],
            client_request_id: String::new(),
        })
        .await
        .expect("submit_goal")
        .into_inner();
    let ws_id = submit.root_workspace_id;
    assert!(!ws_id.is_empty());

    // Seed user + active session referencing the workspace.
    let client = TestClient::seed_user(
        &console.state,
        &console.base_url(),
        "u-1",
        "operator",
    )
    .await;
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_active_session(&console.state.db, &sid, "u-1", &ws_id).await;

    // Spawn a monitor against the session, watching the workspace as root.
    let (handle, join) = session_monitor::spawn(
        sid.clone(),
        WorkspaceSet::new(ws_id.clone(), Vec::<String>::new()),
        console.state.grpc_pool.clone(),
        console.state.db.clone(),
        EventEnricher::new(console.state.taxonomy.clone()),
        RefusalSynthesizer::new(),
        MonitorConfig {
            broadcast_capacity: 16,
            reconnect_initial: Duration::from_millis(50),
            reconnect_max: Duration::from_millis(200),
            reconnect_failure_cap: 30,
        },
    );
    let broadcast_tx = handle.broadcast_tx.clone();
    console
        .state
        .active_sessions
        .write()
        .await
        .insert(sid.clone(), handle);

    // Subscribe to the monitor's broadcast before the workspace transitions
    // so we don't miss the terminal lifecycle frame (broadcast channels
    // only replay to existing subscribers).
    let mut rx = broadcast_tx.subscribe();

    // Give the monitor's stream drivers a moment to connect to the
    // runtime's gRPC streams before triggering transitions. Without this,
    // the activation/completion events can happen before the
    // WorkspaceChanges driver has established its subscription.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Activate the workspace by sending a directive envelope.
    coord
        .send_directive(wacp_v1::SendDirectiveRequest {
            workspace_id: ws_id.clone(),
            payload: b"activate".to_vec(),
            client_request_id: String::new(),
        })
        .await
        .expect("send_directive");

    // Bind an agent and emit Complete → WA3.6 auto-integrates to Closed.
    let agent = wacp_sdk::Agent::connect(wacp_sdk::AgentConfig {
        runtime_url: format!("http://{}", rt.agent_addr()),
        workspace_id: wacp_types::WorkspaceId::from(ws_id.as_str()),
        auth_token: "t7-3-agent-token".into(),
    })
    .await
    .expect("agent connect");
    agent
        .signal(wacp_types::SignalType::Complete)
        .await
        .expect("emit complete");

    // The monitor's WorkspaceChanges driver sees the root workspace's
    // StateChanged (Active→Integrating→Closed) and emits a SessionLifecycle
    // frame with state="completed". Wait for it.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut saw_completed = false;
    let mut observed_frames = Vec::new();
    while std::time::Instant::now() < deadline && !saw_completed {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(frame)) => {
                let json = serde_json::to_value(&frame).unwrap_or_default();
                observed_frames.push(json.clone());
                if json.get("channel").and_then(|c| c.as_str()) == Some("session")
                    && json["event"]["kind"] == "session_lifecycle"
                    && json["event"]["state"] == "completed"
                {
                    saw_completed = true;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_completed,
        "expected session lifecycle completed frame (observed {} frames)",
        observed_frames.len()
    );

    // DB row transitioned to COMPLETED. Allow a short grace window for
    // `record_terminal` to land after the frame fans out.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut final_state = String::new();
    while std::time::Instant::now() < deadline {
        let row = sessions::get_by_id(&console.state.db, &sid)
            .await
            .expect("get session")
            .expect("session row");
        if row.state == session_state::COMPLETED {
            final_state = row.state;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(final_state, session_state::COMPLETED);

    // The monitor task itself exits cleanly on terminal state (per
    // session_monitor::spawn docstring). Removing the handle from
    // `active_sessions` is the caller's responsibility (production: API
    // layer) — not asserted here. We instead assert the task's JoinHandle
    // resolves within a bounded window, proving the run-loop did exit.
    let exit = tokio::time::timeout(Duration::from_secs(2), join).await;
    assert!(
        exit.is_ok() && exit.unwrap().is_ok(),
        "monitor task must exit after terminal state"
    );

    agent.disconnect().await.ok();
    drop(client);
    drop(console);
    drop(rt);
}

// ---- helpers ---------------------------------------------------------------

async fn coord_client(
    rt: &RuntimeHarness,
) -> CoordinatorServiceClient<tonic::transport::Channel> {
    let coord_url = format!("http://{}", rt.coordinator_addr());
    let channel = tonic::transport::Channel::from_shared(coord_url)
        .expect("coord url")
        .connect()
        .await
        .expect("coord connect");
    CoordinatorServiceClient::new(channel)
}

async fn seed_active_session(
    db: &console_db::DbPool,
    sid: &str,
    owner: &str,
    coord_ws: &str,
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
    .bind("2026-04-15T00:00:00Z")
    .bind("2026-04-15T00:00:00Z")
    .execute(db)
    .await
    .expect("seed user");
    let row = sessions::SessionRow {
        id: sid.into(),
        name: Some(sid.into()),
        owner_user_id: owner.into(),
        vertical: "swe".into(),
        workflow: "wf".into(),
        context: None,
        coordinator_workspace_id: Some(coord_ws.into()),
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
        .expect("insert session");
}

#[allow(dead_code)]
fn _silence_unused() -> Arc<()> {
    Arc::new(())
}
