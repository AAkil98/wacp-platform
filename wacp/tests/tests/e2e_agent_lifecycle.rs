//! E2E: Agent lifecycle + envelope exchange (6 tests — E1–E6)

use wacp_integration_tests::e2e::E2eHarness;
use wacp_transport::wacp_v1;

// ── E1: Single worker lifecycle ──

#[tokio::test]
async fn e1_single_worker_lifecycle() {
    let mut harness = E2eHarness::start().await;
    harness.dispatch_workspace("ws-e1", "task-e1");

    let mut client = harness.agent_client().await;

    // Bind
    let bind = client
        .bind(wacp_v1::BindRequest {
            workspace_id: "ws-e1".into(),
            auth_token: "tok".into(),
            client_request_id: "r1".into(),
        })
        .await
        .unwrap();
    assert_eq!(bind.get_ref().workspace_id, "ws-e1");

    // Signal ready
    client
        .emit_signal(wacp_v1::EmitSignalRequest {
            r#type: wacp_v1::SignalType::Ready.into(),
            ..Default::default()
        })
        .await
        .unwrap();

    // Create checkpoint
    let cp = client
        .create_checkpoint(wacp_v1::CreateCheckpointRequest {
            r#type: "artifact".into(),
            payload: b"final output".to_vec(),
            intent: "deliverable".into(),
            status: wacp_v1::CheckpointStatus::Final.into(),
            confidence: wacp_v1::Confidence::High.into(),
            resource_usage: None,
            client_request_id: String::new(),
        })
        .await
        .unwrap();
    assert!(!cp.get_ref().checkpoint_id.is_empty());

    // Signal complete
    client
        .emit_signal(wacp_v1::EmitSignalRequest {
            r#type: wacp_v1::SignalType::Complete.into(),
            ..Default::default()
        })
        .await
        .unwrap();

    harness.shutdown().await;
}

// ── E2: Multi-worker parallel ──

#[tokio::test]
async fn e2_multi_worker_parallel() {
    let mut harness = E2eHarness::start().await;
    harness.dispatch_workspace("ws-p1", "task-p1");
    harness.dispatch_workspace("ws-p2", "task-p2");
    harness.dispatch_workspace("ws-p3", "task-p3");

    // Connect 3 agents concurrently
    let (mut c1, mut c2, mut c3) = tokio::join!(
        harness.agent_client(),
        harness.agent_client(),
        harness.agent_client(),
    );

    // All 3 bind to different workspaces
    let (b1, b2, b3) = tokio::join!(
        c1.bind(wacp_v1::BindRequest {
            workspace_id: "ws-p1".into(),
            auth_token: "t1".into(),
            client_request_id: String::new(),
        }),
        c2.bind(wacp_v1::BindRequest {
            workspace_id: "ws-p2".into(),
            auth_token: "t2".into(),
            client_request_id: String::new(),
        }),
        c3.bind(wacp_v1::BindRequest {
            workspace_id: "ws-p3".into(),
            auth_token: "t3".into(),
            client_request_id: String::new(),
        }),
    );

    assert_eq!(b1.unwrap().get_ref().workspace_id, "ws-p1");
    assert_eq!(b2.unwrap().get_ref().workspace_id, "ws-p2");
    assert_eq!(b3.unwrap().get_ref().workspace_id, "ws-p3");

    // All 3 signal complete
    let (r1, r2, r3) = tokio::join!(
        c1.emit_signal(wacp_v1::EmitSignalRequest {
            r#type: wacp_v1::SignalType::Complete.into(),
            ..Default::default()
        }),
        c2.emit_signal(wacp_v1::EmitSignalRequest {
            r#type: wacp_v1::SignalType::Complete.into(),
            ..Default::default()
        }),
        c3.emit_signal(wacp_v1::EmitSignalRequest {
            r#type: wacp_v1::SignalType::Complete.into(),
            ..Default::default()
        }),
    );

    r1.unwrap();
    r2.unwrap();
    r3.unwrap();

    harness.shutdown().await;
}

// ── E3: Agent disconnect ──

#[tokio::test]
async fn e3_agent_disconnect_detected() {
    let mut harness = E2eHarness::start().await;
    harness.dispatch_workspace("ws-disc", "task-disc");

    let mut client = harness.agent_client().await;
    client
        .bind(wacp_v1::BindRequest {
            workspace_id: "ws-disc".into(),
            auth_token: "tok".into(),
            client_request_id: String::new(),
        })
        .await
        .unwrap();

    // Drop client — connection closed
    drop(client);

    // Server should still be operational for new connections
    let mut client2 = harness.agent_client().await;
    let resp = client2
        .bind(wacp_v1::BindRequest {
            workspace_id: "ws-disc".into(),
            auth_token: "tok2".into(),
            client_request_id: String::new(),
        })
        .await
        .unwrap();
    assert_eq!(resp.get_ref().workspace_id, "ws-disc");

    harness.shutdown().await;
}

// ── E4: Worker-to-worker envelope ──

#[tokio::test]
async fn e4_worker_to_worker_envelope() {
    let mut harness = E2eHarness::start().await;
    harness.dispatch_workspace("ws-sender", "task-s");
    harness.dispatch_workspace("ws-receiver", "task-r");

    let mut sender = harness.agent_client().await;
    sender
        .bind(wacp_v1::BindRequest {
            workspace_id: "ws-sender".into(),
            auth_token: "ts".into(),
            client_request_id: String::new(),
        })
        .await
        .unwrap();

    // Send envelope from sender to receiver
    let env = sender
        .send_envelope(wacp_v1::SendEnvelopeRequest {
            to_workspace: "ws-receiver".into(),
            r#type: "feedback".into(),
            payload: b"looks good".to_vec(),
            in_reply_to: String::new(),
            priority: wacp_v1::EnvelopePriority::Normal.into(),
            client_request_id: String::new(),
        })
        .await
        .unwrap();

    assert!(!env.get_ref().envelope_id.is_empty());

    harness.shutdown().await;
}

// ── E5: Human injection via highway ──

#[tokio::test]
async fn e5_human_injection() {
    let mut harness = E2eHarness::start().await;
    harness.dispatch_workspace("ws-inject", "task-inj");

    let mut highway = harness.highway_client().await;

    // Authenticate
    let auth = highway
        .authenticate(wacp_v1::AuthenticateRequest {
            auth_token: "admin".into(),
        })
        .await
        .unwrap();
    assert_eq!(auth.get_ref().user_id, "user-admin");

    // Inject envelope into workspace
    let inj = highway
        .inject_envelope(wacp_v1::InjectEnvelopeRequest {
            to_workspace: "ws-inject".into(),
            r#type: "feedback".into(),
            payload: b"human feedback".to_vec(),
            priority: wacp_v1::EnvelopePriority::Urgent.into(),
            client_request_id: String::new(),
        })
        .await
        .unwrap();

    assert!(!inj.get_ref().envelope_id.is_empty());

    harness.shutdown().await;
}

// ── E6: Query trail returns empty (port-right enforcement is at coordinator level) ──

#[tokio::test]
async fn e6_query_trail_returns_empty() {
    let mut harness = E2eHarness::start().await;
    harness.dispatch_workspace("ws-q", "task-q");

    let mut client = harness.agent_client().await;
    client
        .bind(wacp_v1::BindRequest {
            workspace_id: "ws-q".into(),
            auth_token: "tok".into(),
            client_request_id: String::new(),
        })
        .await
        .unwrap();

    let trail = client
        .query_trail(wacp_v1::QueryTrailRequest {
            workspace_id: "ws-q".into(),
            event_type: String::new(),
            from: None,
            to: None,
            limit: 100,
            client_request_id: String::new(),
        })
        .await
        .unwrap();

    // Harness returns empty trail (no persistence layer wired)
    assert!(trail.get_ref().entries.is_empty());

    harness.shutdown().await;
}
