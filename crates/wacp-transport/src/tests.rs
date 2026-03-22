use wacp_types::*;

use crate::error_mapping::*;
use crate::in_process::*;
use crate::messages::*;
use crate::proto::wacp_v1 as pb;

#[tokio::test]
async fn agent_send_recv() {
    let (mut server, client) = InProcessTransport::connect_agent();

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
}

#[tokio::test]
async fn server_send_to_agent() {
    let (server, mut client) = InProcessTransport::connect_agent();

    server
        .send(AgentOutbound::BindResponse {
            workspace_id: WorkspaceId::from("ws-1"),
            state: WorkspaceState::Idle,
            role: "worker".into(),
        })
        .await
        .unwrap();

    let msg = client.recv().await.unwrap();
    assert!(matches!(msg, AgentOutbound::BindResponse { .. }));
}

#[tokio::test]
async fn multiple_agents() {
    let (mut server1, client1) = InProcessTransport::connect_agent();
    let (mut server2, client2) = InProcessTransport::connect_agent();

    client1
        .send(AgentInbound::EmitSignal {
            signal_type: SignalType::Ready,
            reason: None,
            context: None,
        })
        .await
        .unwrap();

    client2
        .send(AgentInbound::EmitSignal {
            signal_type: SignalType::Started,
            reason: None,
            context: None,
        })
        .await
        .unwrap();

    let msg1 = server1.recv().await.unwrap();
    let msg2 = server2.recv().await.unwrap();

    assert!(matches!(msg1, AgentInbound::EmitSignal { signal_type: SignalType::Ready, .. }));
    assert!(matches!(msg2, AgentInbound::EmitSignal { signal_type: SignalType::Started, .. }));
}

#[tokio::test]
async fn disconnect_detected() {
    let (mut server, client) = InProcessTransport::connect_agent();
    drop(client);

    let result = server.recv().await;
    assert!(result.is_err());
}

// ── Proto codegen verification ──

#[test]
fn proto_types_generated() {
    // Verify core proto types are importable and constructable.
    let _ts = pb::Timestamp {
        physical_us: 1000,
        logical: 0,
    };
    let _env = pb::Envelope {
        id: "env-1".into(),
        from_workspace: "ws-1".into(),
        to_workspace: "ws-2".into(),
        r#type: "directive".into(),
        payload: vec![],
        in_reply_to: String::new(),
        timestamp: None,
        priority: pb::EnvelopePriority::Normal.into(),
        origin: pb::EnvelopeOrigin::Agent.into(),
    };
    let _signal = pb::Signal {
        r#type: pb::SignalType::Ready.into(),
        workspace_id: "ws-1".into(),
        timestamp: None,
        reason: String::new(),
        context: vec![],
    };
    let _bind = pb::BindRequest {
        workspace_id: "ws-1".into(),
        auth_token: "token".into(),
        client_request_id: String::new(),
    };

    // Verify enum values match protocol constants.
    assert_eq!(pb::SignalType::Ready as i32, 1);
    assert_eq!(pb::SignalType::Migrate as i32, 11);
    assert_eq!(pb::WorkspaceState::Idle as i32, 1);
    assert_eq!(pb::WorkspaceState::Failed as i32, 9);
    assert_eq!(pb::TaskStatus::Draft as i32, 1);
    assert_eq!(pb::TaskStatus::Cancelled as i32, 8);
}

// ── Error mapping verification ──

#[test]
fn error_mapping_all_categories() {
    use tonic::Code;

    assert_eq!(error_category_to_grpc_code(ErrorCategory::PermissionDenied), Code::PermissionDenied);
    assert_eq!(error_category_to_grpc_code(ErrorCategory::IllegalTransition), Code::FailedPrecondition);
    assert_eq!(error_category_to_grpc_code(ErrorCategory::ValidationFailed), Code::InvalidArgument);
    assert_eq!(error_category_to_grpc_code(ErrorCategory::BudgetExceeded), Code::ResourceExhausted);
    assert_eq!(error_category_to_grpc_code(ErrorCategory::Timeout), Code::DeadlineExceeded);
    assert_eq!(error_category_to_grpc_code(ErrorCategory::NotFound), Code::NotFound);
    assert_eq!(error_category_to_grpc_code(ErrorCategory::DeliveryFailed), Code::Unavailable);
    assert_eq!(error_category_to_grpc_code(ErrorCategory::Internal), Code::Internal);
}

#[test]
fn protocol_error_to_status_works() {
    let err = wacp_types::ProtocolError {
        category: ErrorCategory::PermissionDenied,
        rule: "§5.5".into(),
        message: "worker cannot send directive".into(),
        request_id: None,
    };
    let status = protocol_error_to_status(&err);
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
    assert_eq!(status.message(), "worker cannot send directive");
}

// ── gRPC server start test ──

#[tokio::test]
async fn grpc_server_starts() {
    use crate::grpc_server::*;

    // Use random ports to avoid conflicts.
    let handles = start_grpc_server(GrpcServerConfig {
        agent_addr: ([127, 0, 0, 1], 19400).into(),
        highway_addr: ([127, 0, 0, 1], 19401).into(),
    })
    .await
    .unwrap();

    // Channels exist and are open.
    assert!(!handles.agent_request_rx.is_closed());
    assert!(!handles.highway_request_rx.is_closed());

    // Give the server tasks a moment to bind, then let them be dropped.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}
