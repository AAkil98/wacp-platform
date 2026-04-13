use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use wacp_types::*;

use crate::TransportError;
use crate::error_mapping::*;
use crate::grpc_agent::{AgentRequest, AgentServiceImpl};
use crate::grpc_highway::{HighwayRequest, HighwayServiceImpl};
use crate::in_process::*;
use crate::messages::*;
use crate::proto::wacp_v1 as pb;
use pb::agent_service_server::AgentService as AgentServiceTrait;
use pb::highway_service_server::HighwayService as HighwayServiceTrait;

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
    assert!(matches!(
        msg,
        AgentInbound::EmitSignal {
            signal_type: SignalType::Ready,
            ..
        }
    ));
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

    assert!(matches!(
        msg1,
        AgentInbound::EmitSignal {
            signal_type: SignalType::Ready,
            ..
        }
    ));
    assert!(matches!(
        msg2,
        AgentInbound::EmitSignal {
            signal_type: SignalType::Started,
            ..
        }
    ));
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

    assert_eq!(
        error_category_to_grpc_code(ErrorCategory::PermissionDenied),
        Code::PermissionDenied
    );
    assert_eq!(
        error_category_to_grpc_code(ErrorCategory::IllegalTransition),
        Code::FailedPrecondition
    );
    assert_eq!(
        error_category_to_grpc_code(ErrorCategory::ValidationFailed),
        Code::InvalidArgument
    );
    assert_eq!(
        error_category_to_grpc_code(ErrorCategory::BudgetExceeded),
        Code::ResourceExhausted
    );
    assert_eq!(
        error_category_to_grpc_code(ErrorCategory::Timeout),
        Code::DeadlineExceeded
    );
    assert_eq!(
        error_category_to_grpc_code(ErrorCategory::NotFound),
        Code::NotFound
    );
    assert_eq!(
        error_category_to_grpc_code(ErrorCategory::DeliveryFailed),
        Code::Unavailable
    );
    assert_eq!(
        error_category_to_grpc_code(ErrorCategory::Internal),
        Code::Internal
    );
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
        agent_addr: ([127, 0, 0, 1], 19090).into(),
        highway_addr: ([127, 0, 0, 1], 19091).into(),
        coordinator_addr: ([127, 0, 0, 1], 19092).into(),
        tls: None,
    })
    .await
    .unwrap();

    // Channels exist and are open.
    assert!(!handles.agent_request_rx.is_closed());
    assert!(!handles.highway_request_rx.is_closed());
    assert!(!handles.coordinator_request_rx.is_closed());

    // Give the server tasks a moment to bind, then let them be dropped.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

// ── Helpers ─────────────────────────────────────────────

fn agent_svc() -> (AgentServiceImpl, mpsc::Receiver<AgentRequest>) {
    let (tx, rx) = mpsc::channel(16);
    (AgentServiceImpl::new(tx), rx)
}

fn highway_svc() -> (HighwayServiceImpl, mpsc::Receiver<HighwayRequest>) {
    let (tx, rx) = mpsc::channel(16);
    (HighwayServiceImpl::new(tx), rx)
}

// ═══════════════════════════════════════════════════════════
// AgentServiceImpl — RPC forwarding tests (16 tests)
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn agent_svc_bind_forwards_and_returns() {
    let (svc, mut rx) = agent_svc();
    let req = pb::BindRequest {
        workspace_id: "ws-1".into(),
        auth_token: "tok-1".into(),
        client_request_id: "r1".into(),
    };
    let (result, forwarded) = tokio::join!(svc.bind(tonic::Request::new(req)), async {
        match rx.recv().await.unwrap() {
            AgentRequest::Bind { request, reply } => {
                reply
                    .send(Ok(pb::BindResponse {
                        workspace_id: "ws-1".into(),
                        state: pb::WorkspaceState::Active.into(),
                        role: "worker".into(),
                        ..Default::default()
                    }))
                    .ok();
                request
            }
            other => panic!("expected Bind, got {other:?}"),
        }
    });
    assert_eq!(forwarded.workspace_id, "ws-1");
    assert_eq!(forwarded.auth_token, "tok-1");
    let resp = result.unwrap().into_inner();
    assert_eq!(resp.workspace_id, "ws-1");
    assert_eq!(resp.role, "worker");
}

#[tokio::test]
async fn agent_svc_bind_coordinator_unavailable() {
    let (svc, rx) = agent_svc();
    drop(rx);
    let err = svc
        .bind(tonic::Request::new(pb::BindRequest::default()))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::Internal);
}

#[tokio::test]
async fn agent_svc_bind_reply_dropped() {
    let (svc, mut rx) = agent_svc();
    let (result, _) = tokio::join!(
        svc.bind(tonic::Request::new(pb::BindRequest::default())),
        async {
            match rx.recv().await.unwrap() {
                AgentRequest::Bind { reply, .. } => drop(reply),
                _ => panic!("expected Bind"),
            }
        }
    );
    assert_eq!(result.unwrap_err().code(), tonic::Code::Internal);
}

#[tokio::test]
async fn agent_svc_send_envelope_forwards_and_returns() {
    let (svc, mut rx) = agent_svc();
    let req = pb::SendEnvelopeRequest {
        to_workspace: "ws-target".into(),
        r#type: "feedback".into(),
        payload: b"data".to_vec(),
        ..Default::default()
    };
    let (result, _) = tokio::join!(svc.send_envelope(tonic::Request::new(req)), async {
        match rx.recv().await.unwrap() {
            AgentRequest::SendEnvelope { request, reply, .. } => {
                assert_eq!(request.to_workspace, "ws-target");
                assert_eq!(request.r#type, "feedback");
                reply
                    .send(Ok(pb::SendEnvelopeResponse {
                        envelope_id: "env-1".into(),
                        ..Default::default()
                    }))
                    .ok();
            }
            other => panic!("expected SendEnvelope, got {other:?}"),
        }
    });
    assert_eq!(result.unwrap().into_inner().envelope_id, "env-1");
}

#[tokio::test]
async fn agent_svc_send_envelope_error_propagated() {
    let (svc, mut rx) = agent_svc();
    let (result, _) = tokio::join!(
        svc.send_envelope(tonic::Request::new(pb::SendEnvelopeRequest::default())),
        async {
            match rx.recv().await.unwrap() {
                AgentRequest::SendEnvelope { reply, .. } => {
                    reply
                        .send(Err(tonic::Status::permission_denied("no port right")))
                        .ok();
                }
                _ => panic!("expected SendEnvelope"),
            }
        }
    );
    assert_eq!(result.unwrap_err().code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn agent_svc_emit_signal_forwards_and_returns() {
    let (svc, mut rx) = agent_svc();
    let req = pb::EmitSignalRequest {
        r#type: pb::SignalType::Ready.into(),
        reason: "starting".into(),
        ..Default::default()
    };
    let (result, _) = tokio::join!(svc.emit_signal(tonic::Request::new(req)), async {
        match rx.recv().await.unwrap() {
            AgentRequest::EmitSignal { request, reply, .. } => {
                assert_eq!(request.r#type, pb::SignalType::Ready as i32);
                assert_eq!(request.reason, "starting");
                reply.send(Ok(pb::EmitSignalResponse::default())).ok();
            }
            other => panic!("expected EmitSignal, got {other:?}"),
        }
    });
    assert!(result.is_ok());
}

#[tokio::test]
async fn agent_svc_emit_signal_error_propagated() {
    let (svc, mut rx) = agent_svc();
    let (result, _) = tokio::join!(
        svc.emit_signal(tonic::Request::new(pb::EmitSignalRequest::default())),
        async {
            match rx.recv().await.unwrap() {
                AgentRequest::EmitSignal { reply, .. } => {
                    reply
                        .send(Err(tonic::Status::invalid_argument("bad signal")))
                        .ok();
                }
                _ => panic!("expected EmitSignal"),
            }
        }
    );
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn agent_svc_create_checkpoint_forwards_and_returns() {
    let (svc, mut rx) = agent_svc();
    let req = pb::CreateCheckpointRequest {
        r#type: "artifact".into(),
        payload: b"output".to_vec(),
        ..Default::default()
    };
    let (result, _) = tokio::join!(svc.create_checkpoint(tonic::Request::new(req)), async {
        match rx.recv().await.unwrap() {
            AgentRequest::CreateCheckpoint { request, reply, .. } => {
                assert_eq!(request.r#type, "artifact");
                reply
                    .send(Ok(pb::CreateCheckpointResponse {
                        checkpoint_id: "cp-1".into(),
                        content_hash: "sha256-abc".into(),
                        ..Default::default()
                    }))
                    .ok();
            }
            other => panic!("expected CreateCheckpoint, got {other:?}"),
        }
    });
    let resp = result.unwrap().into_inner();
    assert_eq!(resp.checkpoint_id, "cp-1");
    assert_eq!(resp.content_hash, "sha256-abc");
}

#[tokio::test]
async fn agent_svc_create_checkpoint_error_propagated() {
    let (svc, mut rx) = agent_svc();
    let (result, _) = tokio::join!(
        svc.create_checkpoint(tonic::Request::new(pb::CreateCheckpointRequest::default())),
        async {
            match rx.recv().await.unwrap() {
                AgentRequest::CreateCheckpoint { reply, .. } => {
                    reply
                        .send(Err(tonic::Status::resource_exhausted("budget")))
                        .ok();
                }
                _ => panic!("expected CreateCheckpoint"),
            }
        }
    );
    assert_eq!(result.unwrap_err().code(), tonic::Code::ResourceExhausted);
}

#[tokio::test]
async fn agent_svc_query_trail_forwards_and_returns() {
    let (svc, mut rx) = agent_svc();
    let (result, _) = tokio::join!(
        svc.query_trail(tonic::Request::new(pb::QueryTrailRequest {
            workspace_id: "ws-1".into(),
            event_type: "signal_emitted".into(),
            limit: 50,
            ..Default::default()
        })),
        async {
            match rx.recv().await.unwrap() {
                AgentRequest::QueryTrail { request, reply, .. } => {
                    assert_eq!(request.workspace_id, "ws-1");
                    assert_eq!(request.event_type, "signal_emitted");
                    assert_eq!(request.limit, 50);
                    reply
                        .send(Ok(pb::QueryTrailResponse {
                            entries: vec![pb::TrailEntry {
                                id: "te-1".into(),
                                ..Default::default()
                            }],
                            has_more: false,
                            ..Default::default()
                        }))
                        .ok();
                }
                other => panic!("expected QueryTrail, got {other:?}"),
            }
        }
    );
    let resp = result.unwrap().into_inner();
    assert_eq!(resp.entries.len(), 1);
    assert_eq!(resp.entries[0].id, "te-1");
}

#[tokio::test]
async fn agent_svc_query_trail_empty() {
    let (svc, mut rx) = agent_svc();
    let (result, _) = tokio::join!(
        svc.query_trail(tonic::Request::new(pb::QueryTrailRequest::default())),
        async {
            match rx.recv().await.unwrap() {
                AgentRequest::QueryTrail { reply, .. } => {
                    reply.send(Ok(pb::QueryTrailResponse::default())).ok();
                }
                _ => panic!("expected QueryTrail"),
            }
        }
    );
    assert!(result.unwrap().into_inner().entries.is_empty());
}

#[tokio::test]
async fn agent_svc_read_resource_unimplemented() {
    let (svc, _rx) = agent_svc();
    let err = svc
        .read_resource(tonic::Request::new(pb::ReadResourceRequest::default()))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unimplemented);
}

#[tokio::test]
async fn agent_svc_receive_envelopes_delivers() {
    let (svc, mut rx) = agent_svc();
    let (stream_result, envelope_tx) = tokio::join!(
        svc.receive_envelopes(tonic::Request::new(pb::ReceiveEnvelopesRequest {})),
        async {
            match rx.recv().await.unwrap() {
                AgentRequest::SubscribeEnvelopes { tx, .. } => tx,
                other => panic!("expected SubscribeEnvelopes, got {other:?}"),
            }
        }
    );
    envelope_tx
        .send(Ok(pb::Envelope {
            id: "env-in-1".into(),
            ..Default::default()
        }))
        .await
        .unwrap();
    let mut stream = stream_result.unwrap().into_inner();
    let env = stream.next().await.unwrap().unwrap();
    assert_eq!(env.id, "env-in-1");
}

#[tokio::test]
async fn agent_svc_receive_envelopes_closes() {
    let (svc, mut rx) = agent_svc();
    let (stream_result, envelope_tx) = tokio::join!(
        svc.receive_envelopes(tonic::Request::new(pb::ReceiveEnvelopesRequest {})),
        async {
            match rx.recv().await.unwrap() {
                AgentRequest::SubscribeEnvelopes { tx, .. } => tx,
                _ => panic!("expected SubscribeEnvelopes"),
            }
        }
    );
    drop(envelope_tx);
    let mut stream = stream_result.unwrap().into_inner();
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn agent_svc_receive_commands_delivers() {
    let (svc, mut rx) = agent_svc();
    let (stream_result, command_tx) = tokio::join!(
        svc.receive_commands(tonic::Request::new(pb::ReceiveCommandsRequest {})),
        async {
            match rx.recv().await.unwrap() {
                AgentRequest::SubscribeCommands { tx, .. } => tx,
                other => panic!("expected SubscribeCommands, got {other:?}"),
            }
        }
    );
    command_tx
        .send(Ok(pb::Command {
            command: Some(pb::command::Command::Feedback(pb::Envelope {
                id: "env-fb".into(),
                ..Default::default()
            })),
        }))
        .await
        .unwrap();
    let mut stream = stream_result.unwrap().into_inner();
    let cmd = stream.next().await.unwrap().unwrap();
    assert!(cmd.command.is_some());
}

#[tokio::test]
async fn agent_svc_receive_commands_closes() {
    let (svc, mut rx) = agent_svc();
    let (stream_result, command_tx) = tokio::join!(
        svc.receive_commands(tonic::Request::new(pb::ReceiveCommandsRequest {})),
        async {
            match rx.recv().await.unwrap() {
                AgentRequest::SubscribeCommands { tx, .. } => tx,
                _ => panic!("expected SubscribeCommands"),
            }
        }
    );
    drop(command_tx);
    let mut stream = stream_result.unwrap().into_inner();
    assert!(stream.next().await.is_none());
}

// ═══════════════════════════════════════════════════════════
// HighwayServiceImpl — RPC forwarding tests (19 tests)
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn highway_svc_authenticate_forwards_and_returns() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.authenticate(tonic::Request::new(pb::AuthenticateRequest {
            auth_token: "human-tok".into(),
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::Authenticate { request, reply } => {
                    assert_eq!(request.auth_token, "human-tok");
                    reply
                        .send(Ok(pb::AuthenticateResponse {
                            user_id: "user-1".into(),
                            capabilities: vec!["inject".into(), "gate".into()],
                        }))
                        .ok();
                }
                other => panic!("expected Authenticate, got {other:?}"),
            }
        }
    );
    let resp = result.unwrap().into_inner();
    assert_eq!(resp.user_id, "user-1");
    assert_eq!(resp.capabilities.len(), 2);
}

#[tokio::test]
async fn highway_svc_authenticate_error_propagated() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.authenticate(tonic::Request::new(pb::AuthenticateRequest::default())),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::Authenticate { reply, .. } => {
                    reply
                        .send(Err(tonic::Status::unauthenticated("bad token")))
                        .ok();
                }
                _ => panic!("expected Authenticate"),
            }
        }
    );
    assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn highway_svc_inject_envelope_forwards_and_returns() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.inject_envelope(tonic::Request::new(pb::InjectEnvelopeRequest {
            to_workspace: "ws-target".into(),
            r#type: "feedback".into(),
            payload: b"human feedback".to_vec(),
            ..Default::default()
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::InjectEnvelope { request, reply } => {
                    assert_eq!(request.to_workspace, "ws-target");
                    reply
                        .send(Ok(pb::InjectEnvelopeResponse {
                            envelope_id: "env-inj-1".into(),
                            ..Default::default()
                        }))
                        .ok();
                }
                other => panic!("expected InjectEnvelope, got {other:?}"),
            }
        }
    );
    assert_eq!(result.unwrap().into_inner().envelope_id, "env-inj-1");
}

#[tokio::test]
async fn highway_svc_inject_envelope_error_propagated() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.inject_envelope(tonic::Request::new(pb::InjectEnvelopeRequest::default())),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::InjectEnvelope { reply, .. } => {
                    reply
                        .send(Err(tonic::Status::failed_precondition("workspace closed")))
                        .ok();
                }
                _ => panic!("expected InjectEnvelope"),
            }
        }
    );
    assert_eq!(result.unwrap_err().code(), tonic::Code::FailedPrecondition);
}

#[tokio::test]
async fn highway_svc_respond_to_gate_forwards_and_returns() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.respond_to_gate(tonic::Request::new(pb::GateResponse {
            gate_id: "gate-1".into(),
            decision: pb::GateDecision::Approve.into(),
            ..Default::default()
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::RespondToGate { request, reply } => {
                    assert_eq!(request.gate_id, "gate-1");
                    assert_eq!(request.decision, pb::GateDecision::Approve as i32);
                    reply
                        .send(Ok(pb::GateResponseAck {
                            gate_id: "gate-1".into(),
                            applied: true,
                            ..Default::default()
                        }))
                        .ok();
                }
                other => panic!("expected RespondToGate, got {other:?}"),
            }
        }
    );
    let ack = result.unwrap().into_inner();
    assert!(ack.applied);
}

#[tokio::test]
async fn highway_svc_respond_to_gate_already_resolved() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.respond_to_gate(tonic::Request::new(pb::GateResponse {
            gate_id: "gate-old".into(),
            decision: pb::GateDecision::Reject.into(),
            ..Default::default()
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::RespondToGate { reply, .. } => {
                    reply
                        .send(Ok(pb::GateResponseAck {
                            gate_id: "gate-old".into(),
                            applied: false,
                            ..Default::default()
                        }))
                        .ok();
                }
                _ => panic!("expected RespondToGate"),
            }
        }
    );
    assert!(!result.unwrap().into_inner().applied);
}

#[tokio::test]
async fn highway_svc_respond_to_escalation_feedback() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.respond_to_escalation(tonic::Request::new(pb::EscalationResponse {
            escalation_id: "esc-1".into(),
            action: Some(pb::escalation_response::Action::Feedback(pb::Envelope {
                r#type: "feedback".into(),
                payload: b"try this approach".to_vec(),
                ..Default::default()
            })),
            ..Default::default()
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::RespondToEscalation { request, reply } => {
                    assert_eq!(request.escalation_id, "esc-1");
                    assert!(matches!(
                        request.action,
                        Some(pb::escalation_response::Action::Feedback(_))
                    ));
                    reply
                        .send(Ok(pb::EscalationResponseAck {
                            escalation_id: "esc-1".into(),
                            applied: true,
                            ..Default::default()
                        }))
                        .ok();
                }
                other => panic!("expected RespondToEscalation, got {other:?}"),
            }
        }
    );
    assert!(result.unwrap().into_inner().applied);
}

#[tokio::test]
async fn highway_svc_respond_to_escalation_abort() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.respond_to_escalation(tonic::Request::new(pb::EscalationResponse {
            escalation_id: "esc-2".into(),
            action: Some(pb::escalation_response::Action::Abort(true)),
            ..Default::default()
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::RespondToEscalation { request, reply } => {
                    assert!(matches!(
                        request.action,
                        Some(pb::escalation_response::Action::Abort(true))
                    ));
                    reply
                        .send(Ok(pb::EscalationResponseAck {
                            escalation_id: "esc-2".into(),
                            applied: true,
                            ..Default::default()
                        }))
                        .ok();
                }
                _ => panic!("expected RespondToEscalation"),
            }
        }
    );
    assert!(result.is_ok());
}

#[tokio::test]
async fn highway_svc_respond_to_escalation_delegate() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.respond_to_escalation(tonic::Request::new(pb::EscalationResponse {
            escalation_id: "esc-3".into(),
            action: Some(pb::escalation_response::Action::DelegateToCoordinator(true)),
            ..Default::default()
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::RespondToEscalation { request, reply } => {
                    assert!(matches!(
                        request.action,
                        Some(pb::escalation_response::Action::DelegateToCoordinator(true))
                    ));
                    reply
                        .send(Ok(pb::EscalationResponseAck {
                            escalation_id: "esc-3".into(),
                            applied: true,
                            ..Default::default()
                        }))
                        .ok();
                }
                _ => panic!("expected RespondToEscalation"),
            }
        }
    );
    assert!(result.is_ok());
}

#[tokio::test]
async fn highway_svc_get_workspace_success() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.get_workspace(tonic::Request::new(pb::GetWorkspaceRequest {
            workspace_id: "ws-1".into(),
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::GetWorkspace { request, reply } => {
                    assert_eq!(request.workspace_id, "ws-1");
                    reply
                        .send(Ok(pb::WorkspaceView {
                            id: "ws-1".into(),
                            state: pb::WorkspaceState::Active.into(),
                            role: "worker".into(),
                            ..Default::default()
                        }))
                        .ok();
                }
                other => panic!("expected GetWorkspace, got {other:?}"),
            }
        }
    );
    let view = result.unwrap().into_inner();
    assert_eq!(view.id, "ws-1");
    assert_eq!(view.state, pb::WorkspaceState::Active as i32);
}

#[tokio::test]
async fn highway_svc_get_workspace_not_found() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.get_workspace(tonic::Request::new(pb::GetWorkspaceRequest {
            workspace_id: "ws-gone".into(),
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::GetWorkspace { reply, .. } => {
                    reply
                        .send(Err(tonic::Status::not_found("workspace not found")))
                        .ok();
                }
                _ => panic!("expected GetWorkspace"),
            }
        }
    );
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn highway_svc_get_task_graph_success() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.get_task_graph(tonic::Request::new(pb::GetTaskGraphRequest {})),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::GetTaskGraph { reply } => {
                    reply
                        .send(Ok(pb::TaskGraphView {
                            tasks: vec![pb::Task {
                                id: "task-1".into(),
                                ..Default::default()
                            }],
                        }))
                        .ok();
                }
                other => panic!("expected GetTaskGraph, got {other:?}"),
            }
        }
    );
    let view = result.unwrap().into_inner();
    assert_eq!(view.tasks.len(), 1);
    assert_eq!(view.tasks[0].id, "task-1");
}

#[tokio::test]
async fn highway_svc_get_checkpoint_success() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.get_checkpoint(tonic::Request::new(pb::GetCheckpointRequest {
            checkpoint_id: "cp-1".into(),
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::GetCheckpoint { request, reply } => {
                    assert_eq!(request.checkpoint_id, "cp-1");
                    reply
                        .send(Ok(pb::CheckpointView {
                            payload: b"artifact data".to_vec(),
                            ..Default::default()
                        }))
                        .ok();
                }
                other => panic!("expected GetCheckpoint, got {other:?}"),
            }
        }
    );
    assert_eq!(result.unwrap().into_inner().payload, b"artifact data");
}

#[tokio::test]
async fn highway_svc_get_checkpoint_not_found() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.get_checkpoint(tonic::Request::new(pb::GetCheckpointRequest {
            checkpoint_id: "cp-gone".into(),
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::GetCheckpoint { reply, .. } => {
                    reply
                        .send(Err(tonic::Status::not_found("checkpoint not found")))
                        .ok();
                }
                _ => panic!("expected GetCheckpoint"),
            }
        }
    );
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn highway_svc_query_trail_success() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.query_trail(tonic::Request::new(pb::HighwayQueryTrailRequest {
            workspace_id: "ws-1".into(),
            limit: 25,
            ..Default::default()
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::QueryTrail { request, reply } => {
                    assert_eq!(request.workspace_id, "ws-1");
                    assert_eq!(request.limit, 25);
                    reply
                        .send(Ok(pb::QueryTrailResponse {
                            entries: vec![pb::TrailEntry {
                                id: "te-1".into(),
                                ..Default::default()
                            }],
                            has_more: true,
                            ..Default::default()
                        }))
                        .ok();
                }
                other => panic!("expected QueryTrail, got {other:?}"),
            }
        }
    );
    let resp = result.unwrap().into_inner();
    assert_eq!(resp.entries.len(), 1);
    assert!(resp.has_more);
}

#[tokio::test]
async fn highway_svc_stream_trail_returns_stream() {
    let (svc, _rx) = highway_svc();
    let result = svc
        .stream_trail(tonic::Request::new(pb::StreamTrailRequest::default()))
        .await;
    assert!(result.is_ok());
    assert!(result.unwrap().into_inner().next().await.is_none());
}

#[tokio::test]
async fn highway_svc_stream_gates_returns_stream() {
    let (svc, _rx) = highway_svc();
    let result = svc
        .stream_gates(tonic::Request::new(pb::StreamGatesRequest::default()))
        .await;
    assert!(result.is_ok());
    assert!(result.unwrap().into_inner().next().await.is_none());
}

#[tokio::test]
async fn highway_svc_stream_escalations_returns_stream() {
    let (svc, _rx) = highway_svc();
    let result = svc
        .stream_escalations(tonic::Request::new(pb::StreamEscalationsRequest::default()))
        .await;
    assert!(result.is_ok());
    assert!(result.unwrap().into_inner().next().await.is_none());
}

#[tokio::test]
async fn highway_svc_stream_workspace_changes_returns_stream() {
    let (svc, _rx) = highway_svc();
    let result = svc
        .stream_workspace_changes(tonic::Request::new(
            pb::StreamWorkspaceChangesRequest::default(),
        ))
        .await;
    assert!(result.is_ok());
    assert!(result.unwrap().into_inner().next().await.is_none());
}

// ═══════════════════════════════════════════════════════════
// Proto roundtrip tests (3 tests)
// ═══════════════════════════════════════════════════════════

#[test]
fn proto_envelope_roundtrip() {
    use prost::Message;
    let env = pb::Envelope {
        id: "env-1".into(),
        from_workspace: "ws-a".into(),
        to_workspace: "ws-b".into(),
        r#type: "directive".into(),
        payload: b"test payload".to_vec(),
        in_reply_to: "env-0".into(),
        timestamp: Some(pb::Timestamp {
            physical_us: 1_000_000,
            logical: 5,
        }),
        priority: pb::EnvelopePriority::Urgent.into(),
        origin: pb::EnvelopeOrigin::Human.into(),
    };
    let bytes = env.encode_to_vec();
    let decoded = pb::Envelope::decode(bytes.as_slice()).unwrap();
    assert_eq!(decoded, env);
}

#[test]
fn proto_signal_roundtrip() {
    use prost::Message;
    let sig = pb::Signal {
        r#type: pb::SignalType::Escalation.into(),
        workspace_id: "ws-1".into(),
        timestamp: Some(pb::Timestamp {
            physical_us: 500_000,
            logical: 0,
        }),
        reason: "need human review".into(),
        context: b"escalation context".to_vec(),
    };
    let bytes = sig.encode_to_vec();
    let decoded = pb::Signal::decode(bytes.as_slice()).unwrap();
    assert_eq!(decoded, sig);
}

#[test]
fn proto_checkpoint_roundtrip() {
    use prost::Message;
    let cp = pb::Checkpoint {
        id: "cp-1".into(),
        workspace_id: "ws-1".into(),
        r#type: "artifact".into(),
        payload: b"binary data".to_vec(),
        content_hash: "sha256-abc123".into(),
        intent: "final output".into(),
        parent_checkpoint: "cp-0".into(),
        status: pb::CheckpointStatus::Final.into(),
        confidence: pb::Confidence::High.into(),
        timestamp: Some(pb::Timestamp {
            physical_us: 2_000_000,
            logical: 1,
        }),
        resource_usage: Some(pb::ResourceUsage {
            tokens: 500,
            wall_time_ms: 1000,
            storage_bytes: 4096,
            network_bytes: 0,
            cost_micros: 50,
        }),
    };
    let bytes = cp.encode_to_vec();
    let decoded = pb::Checkpoint::decode(bytes.as_slice()).unwrap();
    assert_eq!(decoded, cp);
}

// ═══════════════════════════════════════════════════════════
// In-process transport — additional tests (5 tests)
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn in_process_all_inbound_variants() {
    let (mut server, client) = InProcessTransport::connect_agent();

    let variants = [
        AgentInbound::Bind {
            workspace_id: WorkspaceId::from("ws-1"),
            auth_token: "tok".into(),
        },
        AgentInbound::EmitSignal {
            signal_type: SignalType::Ready,
            reason: None,
            context: None,
        },
        AgentInbound::SendEnvelope {
            to_workspace: WorkspaceId::from("ws-2"),
            envelope_type: "feedback".into(),
            payload: vec![],
            in_reply_to: None,
            priority: EnvelopePriority::Normal,
        },
        AgentInbound::CreateCheckpoint {
            checkpoint_type: "artifact".into(),
            payload: b"data".to_vec(),
            intent: "output".into(),
            status: CheckpointStatus::Provisional,
            confidence: Confidence::High,
            resource_usage: None,
        },
        AgentInbound::QueryTrail {
            workspace_id: None,
            event_type: None,
            limit: 100,
        },
    ];

    for variant in variants {
        client.send(variant).await.unwrap();
    }

    for _ in 0..5 {
        server.recv().await.unwrap();
    }
}

#[tokio::test]
async fn in_process_all_outbound_variants() {
    let (server, mut client) = InProcessTransport::connect_agent();

    server
        .send(AgentOutbound::BindResponse {
            workspace_id: WorkspaceId::from("ws-1"),
            state: WorkspaceState::Active,
            role: "worker".into(),
        })
        .await
        .unwrap();
    assert!(matches!(
        client.recv().await.unwrap(),
        AgentOutbound::BindResponse { .. }
    ));

    server
        .send(AgentOutbound::Error(ProtocolError {
            category: ErrorCategory::PermissionDenied,
            rule: "§5".into(),
            message: "denied".into(),
            request_id: None,
        }))
        .await
        .unwrap();
    assert!(matches!(
        client.recv().await.unwrap(),
        AgentOutbound::Error(_)
    ));
}

#[tokio::test]
async fn in_process_server_send_fails_after_client_drop() {
    let (server, client) = InProcessTransport::connect_agent();
    drop(client);
    let result = server
        .send(AgentOutbound::BindResponse {
            workspace_id: WorkspaceId::from("ws-1"),
            state: WorkspaceState::Idle,
            role: "worker".into(),
        })
        .await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), TransportError::SendFailed(_)));
}

#[tokio::test]
async fn in_process_client_recv_fails_after_server_drop() {
    let (server, mut client) = InProcessTransport::connect_agent();
    drop(server);
    let result = client.recv().await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        TransportError::ConnectionClosed
    ));
}

#[tokio::test]
async fn in_process_bidirectional_concurrent() {
    let (mut server, mut client) = InProcessTransport::connect_agent();

    let (send_result, recv_result) = tokio::join!(
        async {
            client
                .send(AgentInbound::EmitSignal {
                    signal_type: SignalType::Complete,
                    reason: None,
                    context: None,
                })
                .await
        },
        async {
            server
                .send(AgentOutbound::BindResponse {
                    workspace_id: WorkspaceId::from("ws-1"),
                    state: WorkspaceState::Active,
                    role: "worker".into(),
                })
                .await
        }
    );
    send_result.unwrap();
    recv_result.unwrap();

    let inbound = server.recv().await.unwrap();
    assert!(matches!(
        inbound,
        AgentInbound::EmitSignal {
            signal_type: SignalType::Complete,
            ..
        }
    ));
    let outbound = client.recv().await.unwrap();
    assert!(matches!(outbound, AgentOutbound::BindResponse { .. }));
}

// ═══════════════════════════════════════════════════════════
// Highway error edge cases (2 tests)
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn highway_svc_coordinator_unavailable() {
    let (svc, rx) = highway_svc();
    drop(rx);
    let err = svc
        .authenticate(tonic::Request::new(pb::AuthenticateRequest::default()))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::Internal);
    assert!(err.message().contains("coordinator unavailable"));
}

#[tokio::test]
async fn highway_svc_reply_dropped() {
    let (svc, mut rx) = highway_svc();
    let (result, _) = tokio::join!(
        svc.get_workspace(tonic::Request::new(pb::GetWorkspaceRequest {
            workspace_id: "ws-1".into(),
        })),
        async {
            match rx.recv().await.unwrap() {
                HighwayRequest::GetWorkspace { reply, .. } => drop(reply),
                _ => panic!("expected GetWorkspace"),
            }
        }
    );
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Internal);
    assert!(err.message().contains("coordinator dropped reply"));
}
