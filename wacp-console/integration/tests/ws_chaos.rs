//! §13.7.8 I4 — WebSocket chaos matrix.
//!
//! Three scenarios against the console's `/api/sessions/:id/ws` endpoint:
//!
//! 1. **Broadcast-cap exhaustion → control/lag frame.** The server's WS
//!    task subscribes to the monitor's `broadcast_tx`. When the server's
//!    receiver falls behind (capacity exceeded), the tokio broadcast
//!    channel returns `RecvError::Lagged(missed)` and the server emits
//!    `{channel: "control", event: {type: "lag", missed: N}}` per
//!    `console-api/src/routes/ws.rs:124`. Driven the deterministic way
//!    from perf-opt §11.5 T7.8: push directly into `broadcast_tx` faster
//!    than `socket.send` can drain, not via real producers (loopback TCP
//!    absorbs small bursts before the receiver Lags).
//! 2. **Client abrupt disconnect → receiver count drops.** Client opens
//!    WS, server's broadcast receiver count goes to 1; client drops
//!    socket; within a short window the receiver count returns to 0
//!    (the server's select loop detects the closed peer and exits,
//!    dropping its `broadcast::Receiver`). Proves no receiver leak.
//! 3. **Malformed text frame silently ignored.** ws.rs:96 drops any
//!    incoming text from the client via `Some(Ok(Message::Text(_))) => {}`.
//!    Test: send a non-JSON text, then push a real broadcast frame —
//!    server still delivers. Protects against a future change that
//!    accidentally closes the socket on bad input (which would break
//!    any UI that sends ping-like text; Playwright auth-flows.spec.ts
//!    is a latent consumer).
//!
//! **Not covered (deferred, see `HEALTH-LOG.md` §13.4):**
//! - **Gap-fill replay.** The AUDIT §13.7.8 scenario calls for an
//!   `/api/sessions/:id/trail?since=<seq>` REST endpoint that returns
//!   dropped frames after a Lagged. The endpoint **does not exist**
//!   — `routes/sessions.rs` + `routes/ws.rs` surfaces a trail-streaming
//!   WS channel but no REST replay. Filed as a spec-vs-impl drift, not
//!   a regression; reaching out to the feature is a separate work item.
//! - **Partial-frame delivery after Lagged.** tokio's `broadcast::channel`
//!   delivers whole `Frame` values to receivers, never splits. The
//!   "no partial frames" invariant would test tokio, not the console.
//!   Skipped for low signal.

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use console_core::event_enricher::{EnrichedTrailEntry, EventEnricher};
use console_core::refusal_synthesizer::RefusalSynthesizer;
use console_core::session_monitor::{
    self, Channel as FrameChannel, Frame, FrameEvent, MonitorConfig, WorkspaceSet,
};
use console_core::taxonomy_builder;
use console_db::queries::sessions;
use console_integration::{ConsoleHarness, RuntimeHarness, TestClient};
use tokio_tungstenite::tungstenite::Message;

// ---- helpers --------------------------------------------------------------

async fn seed_session(db: &console_db::DbPool, sid: &str, owner: &str) {
    let row = sessions::SessionRow {
        id: sid.into(),
        name: Some(sid.into()),
        owner_user_id: owner.into(),
        vertical: "swe".into(),
        workflow: "wf".into(),
        context: None,
        coordinator_workspace_id: Some("ws-root".into()),
        state: "active".into(),
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

/// Spawn a monitor configured with a low broadcast capacity + register it
/// under `sid` in the console's `active_sessions`. Returns the handle so
/// the test can `broadcast_tx.send(...)` frames directly.
async fn spawn_monitor_with_cap(
    console: &ConsoleHarness,
    sid: &str,
    capacity: usize,
) -> session_monitor::SessionMonitorHandle {
    let taxonomy = Arc::new(ArcSwap::from_pointee(
        taxonomy_builder::build_index(None, &[], &[]).index,
    ));
    let (handle, join) = session_monitor::spawn(
        sid.to_string(),
        WorkspaceSet::new("ws-root".into(), Vec::<String>::new()),
        console.state.grpc_pool.clone(),
        console.state.db.clone(),
        EventEnricher::new(taxonomy),
        RefusalSynthesizer::new(),
        MonitorConfig {
            broadcast_capacity: capacity,
            reconnect_initial: Duration::from_millis(50),
            reconnect_max: Duration::from_millis(200),
            reconnect_failure_cap: 30,
        },
    );
    console
        .state
        .active_sessions
        .write()
        .await
        .insert(sid.to_string(), handle.clone());
    // Forget the join handle; the monitor spins its reconnect drivers but
    // this test doesn't observe them. Drop happens at shutdown.
    std::mem::forget(join);
    handle
}

/// Build a trivial trail frame — content doesn't matter here, just a
/// distinct payload per index so assertions can reason about delivery.
fn make_trail_frame(sid: &str, i: u64) -> Frame {
    Frame {
        channel: FrameChannel::Trail,
        session_id: sid.into(),
        event: FrameEvent::Trail(EnrichedTrailEntry {
            id: format!("t-{i}"),
            timestamp: "2026-04-15T00:00:00Z".into(),
            sequence_number: i,
            workspace_id: "ws-root".into(),
            workspace_label: "ws-root".into(),
            actor: "test".into(),
            event_type: "filler".into(),
            summary: format!("filler-{i}"),
            body_len: 0,
        }),
    }
}

// ---- tests ----------------------------------------------------------------

#[tokio::test]
async fn broadcast_cap_exhaustion_emits_control_lag_frame() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");
    let client =
        TestClient::seed_user(&console.state, &console.base_url(), "u-1", "operator").await;

    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_session(&console.state.db, &sid, "u-1").await;
    // capacity=4 is small enough that pushing 64 frames without a WS drain
    // guarantees Lagged.
    let handle = spawn_monitor_with_cap(&console, &sid, 4).await;

    let mut ws = client.open_ws(&format!("/api/sessions/{sid}/ws")).await;
    // Consume the welcome frame so the receiver is at position 0.
    let _ = ws
        .assert_frame_within(Duration::from_secs(5), |v| {
            v.get("channel").and_then(|c| c.as_str()) == Some("session")
        })
        .await;

    // Push 64 frames rapidly. The server's WS task can't flush to the
    // socket fast enough — broadcast falls behind capacity=4, returns
    // Lagged on the server's recv, server emits control/lag to client.
    for i in 0..64u64 {
        let _ = handle.broadcast_tx.send(make_trail_frame(&sid, i));
    }

    // Observe the control/lag frame within 2s.
    let lag = ws
        .assert_frame_within(Duration::from_secs(2), |v| {
            v.get("channel").and_then(|c| c.as_str()) == Some("control")
                && v["event"]["type"].as_str() == Some("lag")
        })
        .await;
    assert!(
        lag["event"]["missed"].as_u64().unwrap_or(0) > 0,
        "lag frame must report a positive missed count; got {lag:?}"
    );

    ws.close().await;
}

#[tokio::test]
async fn client_disconnect_drops_broadcast_receiver() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");
    let client =
        TestClient::seed_user(&console.state, &console.base_url(), "u-2", "operator").await;

    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_session(&console.state.db, &sid, "u-2").await;
    let handle = spawn_monitor_with_cap(&console, &sid, 16).await;

    // Before WS: monitor owns 0 external receivers.
    let before = handle.broadcast_tx.receiver_count();

    let mut ws = client.open_ws(&format!("/api/sessions/{sid}/ws")).await;
    let _ = ws
        .assert_frame_within(Duration::from_secs(5), |v| {
            v.get("channel").and_then(|c| c.as_str()) == Some("session")
        })
        .await;

    // Server WS task holds one receiver.
    let during = handle.broadcast_tx.receiver_count();
    assert!(
        during > before,
        "server must hold a receiver while client is connected: before={before} during={during}"
    );

    ws.close().await;

    // Within 1 s the server's select loop notices Close, exits, drops
    // the receiver — count returns to `before`.
    for _ in 0..20 {
        if handle.broadcast_tx.receiver_count() == before {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "receiver count didn't decrement after client disconnect: before={before} now={}",
        handle.broadcast_tx.receiver_count()
    );
}

#[tokio::test]
async fn malformed_text_frame_from_client_is_silently_ignored() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let console = ConsoleHarness::spawn(&rt).await.expect("console");
    let client =
        TestClient::seed_user(&console.state, &console.base_url(), "u-3", "operator").await;

    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_session(&console.state.db, &sid, "u-3").await;
    let handle = spawn_monitor_with_cap(&console, &sid, 16).await;

    let mut ws = client.open_ws(&format!("/api/sessions/{sid}/ws")).await;
    // Consume welcome.
    let _ = ws
        .assert_frame_within(Duration::from_secs(5), |v| {
            v.get("channel").and_then(|c| c.as_str()) == Some("session")
        })
        .await;

    // Send a bad text frame. Server per ws.rs:96 drops it; the connection
    // stays open. We can't observe the drop directly from the client, so
    // we verify by sending a real broadcast frame afterwards and confirming
    // it still arrives.
    ws.send_raw(Message::Text("not-json-at-all {{".into()))
        .await;
    ws.send_raw(Message::Text("another garbage line".into()))
        .await;

    // Now push a legit broadcast frame.
    let _ = handle.broadcast_tx.send(make_trail_frame(&sid, 42));

    // Assert it arrived — connection is still open.
    let trail = ws
        .assert_frame_within(Duration::from_secs(2), |v| {
            v.get("channel").and_then(|c| c.as_str()) == Some("trail")
        })
        .await;
    assert_eq!(
        trail["event"]["sequence_number"].as_u64(),
        Some(42),
        "connection must still be open + delivering broadcasts after malformed text: {trail:?}"
    );

    ws.close().await;
}
