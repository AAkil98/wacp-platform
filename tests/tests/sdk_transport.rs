//! Integration: wacp-sdk + wacp-transport (5 tests)

use wacp_transport::in_process::InProcessTransport;
use wacp_transport::messages::{AgentInbound, AgentOutbound};
use wacp_types::*;

#[tokio::test]
async fn agent_bind_via_in_process() {
    let (mut server, mut client) = InProcessTransport::connect_agent();

    client
        .send(AgentInbound::Bind {
            workspace_id: WorkspaceId::from("ws-sdk-1"),
            auth_token: "sdk-token".into(),
        })
        .await
        .unwrap();

    let msg = server.recv().await.unwrap();
    match msg {
        AgentInbound::Bind { workspace_id, auth_token } => {
            assert_eq!(workspace_id, WorkspaceId::from("ws-sdk-1"));
            assert_eq!(auth_token, "sdk-token");
        }
        other => panic!("expected Bind, got {other:?}"),
    }

    // Server responds
    server
        .send(AgentOutbound::BindResponse {
            workspace_id: WorkspaceId::from("ws-sdk-1"),
            state: WorkspaceState::Idle,
            role: "worker".into(),
        })
        .await
        .unwrap();

    let resp = client.recv().await.unwrap();
    assert!(matches!(resp, AgentOutbound::BindResponse { workspace_id, .. } if workspace_id == WorkspaceId::from("ws-sdk-1")));
}

#[tokio::test]
async fn signal_checkpoint_lifecycle() {
    let (mut server, client) = InProcessTransport::connect_agent();

    // Signal ready
    client
        .send(AgentInbound::EmitSignal {
            signal_type: SignalType::Ready,
            reason: None,
            context: None,
        })
        .await
        .unwrap();

    let msg = server.recv().await.unwrap();
    assert!(matches!(msg, AgentInbound::EmitSignal { signal_type: SignalType::Ready, .. }));

    // Create checkpoint
    client
        .send(AgentInbound::CreateCheckpoint {
            checkpoint_type: "artifact".into(),
            payload: b"output".to_vec(),
            intent: "final".into(),
            status: CheckpointStatus::Final,
            confidence: Confidence::High,
            resource_usage: None,
        })
        .await
        .unwrap();

    let msg = server.recv().await.unwrap();
    assert!(matches!(msg, AgentInbound::CreateCheckpoint { checkpoint_type, .. } if checkpoint_type == "artifact"));

    // Signal complete
    client
        .send(AgentInbound::EmitSignal {
            signal_type: SignalType::Complete,
            reason: None,
            context: None,
        })
        .await
        .unwrap();

    let msg = server.recv().await.unwrap();
    assert!(matches!(msg, AgentInbound::EmitSignal { signal_type: SignalType::Complete, .. }));
}

#[tokio::test]
async fn concurrent_agents_separate_transports() {
    let (mut s1, c1) = InProcessTransport::connect_agent();
    let (mut s2, c2) = InProcessTransport::connect_agent();

    // Both agents signal simultaneously
    let (r1, r2) = tokio::join!(
        c1.send(AgentInbound::EmitSignal {
            signal_type: SignalType::Ready,
            reason: None,
            context: None,
        }),
        c2.send(AgentInbound::EmitSignal {
            signal_type: SignalType::Started,
            reason: None,
            context: None,
        }),
    );
    r1.unwrap();
    r2.unwrap();

    let m1 = s1.recv().await.unwrap();
    let m2 = s2.recv().await.unwrap();

    assert!(matches!(m1, AgentInbound::EmitSignal { signal_type: SignalType::Ready, .. }));
    assert!(matches!(m2, AgentInbound::EmitSignal { signal_type: SignalType::Started, .. }));
}

#[tokio::test]
async fn agent_disconnect_clean() {
    let (mut server, client) = InProcessTransport::connect_agent();

    client
        .send(AgentInbound::EmitSignal {
            signal_type: SignalType::Ready,
            reason: None,
            context: None,
        })
        .await
        .unwrap();
    server.recv().await.unwrap();

    // Drop client = disconnect
    drop(client);
    let result = server.recv().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn query_trail_via_transport() {
    let (mut server, client) = InProcessTransport::connect_agent();

    client
        .send(AgentInbound::QueryTrail {
            workspace_id: Some(WorkspaceId::from("ws-1")),
            event_type: Some("signal_emitted".into()),
            limit: 50,
        })
        .await
        .unwrap();

    let msg = server.recv().await.unwrap();
    match msg {
        AgentInbound::QueryTrail { workspace_id, event_type, limit } => {
            assert_eq!(workspace_id, Some(WorkspaceId::from("ws-1")));
            assert_eq!(event_type, Some("signal_emitted".into()));
            assert_eq!(limit, 50);
        }
        other => panic!("expected QueryTrail, got {other:?}"),
    }
}
