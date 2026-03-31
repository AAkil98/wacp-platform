//! E2E: Integration pipeline (2 tests — E18–E19)

use wacp_integration_tests::e2e::E2eHarness;
use wacp_integration_tests::*;
use wacp_transport::wacp_v1;
use wacp_types::*;

// ── E18: Direct merge — worker completes with final checkpoint ──

#[tokio::test]
async fn e18_direct_merge_lifecycle() {
    let mut harness = E2eHarness::start().await;
    harness.dispatch_workspace("ws-merge", "task-merge");

    let mut client = harness.agent_client().await;

    // Bind
    client
        .bind(wacp_v1::BindRequest {
            workspace_id: "ws-merge".into(),
            auth_token: "tok".into(),
            client_request_id: String::new(),
        })
        .await
        .unwrap();

    // Signal ready + started
    client
        .emit_signal(wacp_v1::EmitSignalRequest {
            r#type: wacp_v1::SignalType::Ready.into(),
            ..Default::default()
        })
        .await
        .unwrap();

    client
        .emit_signal(wacp_v1::EmitSignalRequest {
            r#type: wacp_v1::SignalType::Started.into(),
            ..Default::default()
        })
        .await
        .unwrap();

    // Create final checkpoint
    let cp = client
        .create_checkpoint(wacp_v1::CreateCheckpointRequest {
            r#type: "artifact".into(),
            payload: b"final deliverable".to_vec(),
            intent: "integration-ready output".into(),
            status: wacp_v1::CheckpointStatus::Final.into(),
            confidence: wacp_v1::Confidence::High.into(),
            resource_usage: Some(wacp_v1::ResourceUsage {
                tokens: 1000,
                wall_time_ms: 5000,
                storage_bytes: 2048,
                network_bytes: 0,
                cost_micros: 50,
            }),
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
            ..Default::default()
        })
        .await
        .unwrap();

    harness.shutdown().await;
}

// ── E19: Two workers with overlapping work — coordinator-level conflict ──

#[tokio::test]
async fn e19_two_workers_overlapping() {
    let (mut coord, mut rx) = make_coordinator();
    dispatch(&mut coord, "ws-w1", "task-w1");
    dispatch(&mut coord, "ws-w2", "task-w2");

    // Activate both by routing directives
    let env1 = make_envelope("dir-w1", "ws-root", "ws-w1", "directive");
    coord.route_envelope(&WorkspaceId::from("ws-w1"), env1).await;

    let env2 = make_envelope("dir-w2", "ws-root", "ws-w2", "directive");
    coord.route_envelope(&WorkspaceId::from("ws-w2"), env2).await;

    drain_events(&mut coord, &mut rx, 10, 200).await;

    // Both should be active
    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-w1")).unwrap().status,
        WorkspaceState::Active
    );
    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-w2")).unwrap().status,
        WorkspaceState::Active
    );

    // Abort w1 (simulating conflict resolution by failing one)
    coord.abort_workspace(&WorkspaceId::from("ws-w1")).await;
    drain_events(&mut coord, &mut rx, 10, 200).await;

    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-w1")).unwrap().status,
        WorkspaceState::Failed
    );
    // w2 remains active — independent
    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-w2")).unwrap().status,
        WorkspaceState::Active
    );
}
