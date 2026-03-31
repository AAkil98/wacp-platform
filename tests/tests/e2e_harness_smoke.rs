//! E2E harness smoke test — verifies the harness starts, accepts connections,
//! and processes basic agent + highway RPCs. This is the T4.1 infrastructure test.

use wacp_integration_tests::e2e::E2eHarness;
use wacp_transport::wacp_v1;

#[tokio::test]
async fn harness_starts_and_accepts_agent_bind() {
    let mut harness = E2eHarness::start().await;
    harness.dispatch_workspace("ws-smoke", "task-smoke");

    let mut client = harness.agent_client().await;
    let resp = client
        .bind(wacp_v1::BindRequest {
            workspace_id: "ws-smoke".into(),
            auth_token: "tok".into(),
            client_request_id: "r1".into(),
        })
        .await
        .unwrap();

    assert_eq!(resp.get_ref().workspace_id, "ws-smoke");
    harness.shutdown().await;
}

#[tokio::test]
async fn harness_accepts_highway_authenticate() {
    let harness = E2eHarness::start().await;

    let mut client = harness.highway_client().await;
    let resp = client
        .authenticate(wacp_v1::AuthenticateRequest {
            auth_token: "admin".into(),
        })
        .await
        .unwrap();

    assert_eq!(resp.get_ref().user_id, "user-admin");
    assert!(!resp.get_ref().capabilities.is_empty());
    harness.shutdown().await;
}

#[tokio::test]
async fn harness_agent_signal_and_checkpoint() {
    let mut harness = E2eHarness::start().await;
    harness.dispatch_workspace("ws-e2e", "task-e2e");

    let mut client = harness.agent_client().await;

    // Bind
    client
        .bind(wacp_v1::BindRequest {
            workspace_id: "ws-e2e".into(),
            auth_token: "tok".into(),
            client_request_id: String::new(),
        })
        .await
        .unwrap();

    // Signal ready
    client
        .emit_signal(wacp_v1::EmitSignalRequest {
            r#type: wacp_v1::SignalType::Ready.into(),
            reason: String::new(),
            context: vec![],
            client_request_id: String::new(),
        })
        .await
        .unwrap();

    // Create checkpoint
    let cp = client
        .create_checkpoint(wacp_v1::CreateCheckpointRequest {
            r#type: "artifact".into(),
            payload: b"output".to_vec(),
            intent: "test".into(),
            status: wacp_v1::CheckpointStatus::Final.into(),
            confidence: wacp_v1::Confidence::High.into(),
            resource_usage: None,
            client_request_id: String::new(),
        })
        .await
        .unwrap();

    assert!(!cp.get_ref().checkpoint_id.is_empty());
    assert!(!cp.get_ref().content_hash.is_empty());

    // Signal complete
    client
        .emit_signal(wacp_v1::EmitSignalRequest {
            r#type: wacp_v1::SignalType::Complete.into(),
            reason: String::new(),
            context: vec![],
            client_request_id: String::new(),
        })
        .await
        .unwrap();

    harness.shutdown().await;
}
