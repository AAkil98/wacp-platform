//! Integration: wacp-transport + wacp-coordinator (5 tests)

use wacp_integration_tests::*;

use wacp_transport::in_process::InProcessTransport;
use wacp_transport::messages::{AgentInbound, AgentOutbound};
use wacp_types::*;

#[tokio::test]
async fn agent_bind_via_transport() {
    let (mut coord, _rx) = make_coordinator();
    dispatch(&mut coord, "ws-1", "task-1");

    let (mut server, client) = InProcessTransport::connect_agent();

    client
        .send(AgentInbound::Bind {
            workspace_id: WorkspaceId::from("ws-1"),
            auth_token: "token".into(),
        })
        .await
        .unwrap();

    let msg = server.recv().await.unwrap();
    assert!(
        matches!(msg, AgentInbound::Bind { workspace_id, .. } if workspace_id == WorkspaceId::from("ws-1"))
    );

    // Coordinator confirms workspace exists
    assert!(coord.tree.get(&WorkspaceId::from("ws-1")).is_some());

    // Send bind response
    server
        .send(AgentOutbound::BindResponse {
            workspace_id: WorkspaceId::from("ws-1"),
            state: WorkspaceState::Idle,
            role: "worker".into(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn emit_signal_via_transport() {
    let (mut server, client) = InProcessTransport::connect_agent();

    client
        .send(AgentInbound::EmitSignal {
            signal_type: SignalType::Ready,
            reason: Some("starting".into()),
            context: None,
        })
        .await
        .unwrap();

    let msg = server.recv().await.unwrap();
    match msg {
        AgentInbound::EmitSignal {
            signal_type,
            reason,
            ..
        } => {
            assert_eq!(signal_type, SignalType::Ready);
            assert_eq!(reason, Some("starting".into()));
        }
        other => panic!("expected EmitSignal, got {other:?}"),
    }
}

#[tokio::test]
async fn send_envelope_via_transport() {
    let (mut server, client) = InProcessTransport::connect_agent();

    client
        .send(AgentInbound::SendEnvelope {
            to_workspace: WorkspaceId::from("ws-target"),
            envelope_type: "feedback".into(),
            payload: b"data".to_vec(),
            in_reply_to: None,
            priority: EnvelopePriority::Urgent,
        })
        .await
        .unwrap();

    let msg = server.recv().await.unwrap();
    match msg {
        AgentInbound::SendEnvelope {
            to_workspace,
            envelope_type,
            priority,
            ..
        } => {
            assert_eq!(to_workspace, WorkspaceId::from("ws-target"));
            assert_eq!(envelope_type, "feedback");
            assert_eq!(priority, EnvelopePriority::Urgent);
        }
        other => panic!("expected SendEnvelope, got {other:?}"),
    }
}

#[tokio::test]
async fn transport_disconnect_detected() {
    let (mut server, client) = InProcessTransport::connect_agent();
    drop(client);
    let result = server.recv().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn multiple_agents_on_separate_transports() {
    let (mut coord, _rx) = make_coordinator();
    dispatch(&mut coord, "ws-1", "task-1");
    dispatch(&mut coord, "ws-2", "task-2");
    dispatch(&mut coord, "ws-3", "task-3");

    let (mut s1, c1) = InProcessTransport::connect_agent();
    let (mut s2, c2) = InProcessTransport::connect_agent();
    let (mut s3, c3) = InProcessTransport::connect_agent();

    c1.send(AgentInbound::Bind {
        workspace_id: WorkspaceId::from("ws-1"),
        auth_token: "t1".into(),
    })
    .await
    .unwrap();

    c2.send(AgentInbound::Bind {
        workspace_id: WorkspaceId::from("ws-2"),
        auth_token: "t2".into(),
    })
    .await
    .unwrap();

    c3.send(AgentInbound::Bind {
        workspace_id: WorkspaceId::from("ws-3"),
        auth_token: "t3".into(),
    })
    .await
    .unwrap();

    // All 3 servers receive their respective bind requests
    let m1 = s1.recv().await.unwrap();
    let m2 = s2.recv().await.unwrap();
    let m3 = s3.recv().await.unwrap();

    assert!(
        matches!(m1, AgentInbound::Bind { workspace_id, .. } if workspace_id == WorkspaceId::from("ws-1"))
    );
    assert!(
        matches!(m2, AgentInbound::Bind { workspace_id, .. } if workspace_id == WorkspaceId::from("ws-2"))
    );
    assert!(
        matches!(m3, AgentInbound::Bind { workspace_id, .. } if workspace_id == WorkspaceId::from("ws-3"))
    );
}
