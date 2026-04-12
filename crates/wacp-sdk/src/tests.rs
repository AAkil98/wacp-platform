use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::{Request, Response, Status};
use wacp_transport::wacp_v1;
use wacp_transport::wacp_v1::agent_service_server::{
    AgentService as AgentServiceTrait, AgentServiceServer,
};
use wacp_types::*;

use crate::*;

// ── Existing tests ──────────────────────────────────────

#[test]
fn agent_config_constructable() {
    let config = AgentConfig {
        runtime_url: "http://localhost:9090".into(),
        workspace_id: WorkspaceId::from("ws-1"),
        auth_token: "token".into(),
    };
    assert_eq!(config.workspace_id, WorkspaceId::from("ws-1"));
    assert_eq!(config.runtime_url, "http://localhost:9090");
}

#[tokio::test]
async fn connect_fails_on_no_server() {
    let config = AgentConfig {
        runtime_url: "http://127.0.0.1:1".into(), // nothing listening
        workspace_id: WorkspaceId::from("ws-1"),
        auth_token: "token".into(),
    };
    let result = Agent::connect(config).await;
    assert!(result.is_err());
}

#[test]
fn error_display() {
    let err = Error::MissingField("checkpoint_type".into());
    assert_eq!(err.to_string(), "missing required field: checkpoint_type");

    let err = Error::NotConnected;
    assert_eq!(err.to_string(), "not connected");
}

// ── Mock gRPC server ────────────────────────────────────

struct MockState {
    bind_response: wacp_v1::BindResponse,
    signal_response: wacp_v1::EmitSignalResponse,
    checkpoint_response: wacp_v1::CreateCheckpointResponse,
    envelope_send_response: wacp_v1::SendEnvelopeResponse,
    trail_response: wacp_v1::QueryTrailResponse,

    bind_calls: Vec<wacp_v1::BindRequest>,
    signal_calls: Vec<wacp_v1::EmitSignalRequest>,
    checkpoint_calls: Vec<wacp_v1::CreateCheckpointRequest>,
    envelope_send_calls: Vec<wacp_v1::SendEnvelopeRequest>,
    trail_calls: Vec<wacp_v1::QueryTrailRequest>,

    envelope_tx: Option<mpsc::Sender<Result<wacp_v1::Envelope, Status>>>,
    command_tx: Option<mpsc::Sender<Result<wacp_v1::Command, Status>>>,

    bind_error: Option<Status>,
    signal_error: Option<Status>,
    checkpoint_error: Option<Status>,
}

impl MockState {
    fn new() -> Self {
        Self {
            bind_response: test_bind_response(),
            signal_response: wacp_v1::EmitSignalResponse {
                timestamp: None,
                client_request_id: String::new(),
            },
            checkpoint_response: wacp_v1::CreateCheckpointResponse {
                checkpoint_id: "cp-001".into(),
                content_hash: "sha256-abc123".into(),
                timestamp: None,
                client_request_id: String::new(),
            },
            envelope_send_response: wacp_v1::SendEnvelopeResponse {
                envelope_id: "env-out-001".into(),
                timestamp: None,
                client_request_id: String::new(),
            },
            trail_response: wacp_v1::QueryTrailResponse {
                entries: vec![],
                has_more: false,
                client_request_id: String::new(),
            },
            bind_calls: vec![],
            signal_calls: vec![],
            checkpoint_calls: vec![],
            envelope_send_calls: vec![],
            trail_calls: vec![],
            envelope_tx: None,
            command_tx: None,
            bind_error: None,
            signal_error: None,
            checkpoint_error: None,
        }
    }
}

fn test_bind_response() -> wacp_v1::BindResponse {
    wacp_v1::BindResponse {
        workspace_id: "ws-test".into(),
        state: wacp_v1::WorkspaceState::Active.into(),
        role: "worker".into(),
        directive: Some(wacp_v1::Envelope {
            id: "env-directive-1".into(),
            from_workspace: "ws-root".into(),
            to_workspace: "ws-test".into(),
            r#type: "directive".into(),
            payload: b"implement feature X".to_vec(),
            in_reply_to: String::new(),
            timestamp: None,
            priority: wacp_v1::EnvelopePriority::Normal.into(),
            origin: wacp_v1::EnvelopeOrigin::Agent.into(),
        }),
        context: b"workspace context data".to_vec(),
        visibility: vec!["res-alpha".into(), "res-beta".into()],
        authority: vec!["res-alpha".into()],
        budget: None,
    }
}

struct MockAgentSvc {
    state: Arc<Mutex<MockState>>,
}

#[tonic::async_trait]
impl AgentServiceTrait for MockAgentSvc {
    async fn bind(
        &self,
        req: Request<wacp_v1::BindRequest>,
    ) -> Result<Response<wacp_v1::BindResponse>, Status> {
        let mut s = self.state.lock().await;
        s.bind_calls.push(req.into_inner());
        if let Some(err) = s.bind_error.take() {
            return Err(err);
        }
        Ok(Response::new(s.bind_response.clone()))
    }

    async fn send_envelope(
        &self,
        req: Request<wacp_v1::SendEnvelopeRequest>,
    ) -> Result<Response<wacp_v1::SendEnvelopeResponse>, Status> {
        let mut s = self.state.lock().await;
        s.envelope_send_calls.push(req.into_inner());
        Ok(Response::new(s.envelope_send_response.clone()))
    }

    async fn emit_signal(
        &self,
        req: Request<wacp_v1::EmitSignalRequest>,
    ) -> Result<Response<wacp_v1::EmitSignalResponse>, Status> {
        let mut s = self.state.lock().await;
        s.signal_calls.push(req.into_inner());
        if let Some(err) = s.signal_error.take() {
            return Err(err);
        }
        Ok(Response::new(s.signal_response.clone()))
    }

    async fn create_checkpoint(
        &self,
        req: Request<wacp_v1::CreateCheckpointRequest>,
    ) -> Result<Response<wacp_v1::CreateCheckpointResponse>, Status> {
        let mut s = self.state.lock().await;
        s.checkpoint_calls.push(req.into_inner());
        if let Some(err) = s.checkpoint_error.take() {
            return Err(err);
        }
        Ok(Response::new(s.checkpoint_response.clone()))
    }

    async fn query_trail(
        &self,
        req: Request<wacp_v1::QueryTrailRequest>,
    ) -> Result<Response<wacp_v1::QueryTrailResponse>, Status> {
        let mut s = self.state.lock().await;
        s.trail_calls.push(req.into_inner());
        Ok(Response::new(s.trail_response.clone()))
    }

    async fn read_resource(
        &self,
        _req: Request<wacp_v1::ReadResourceRequest>,
    ) -> Result<Response<wacp_v1::ReadResourceResponse>, Status> {
        Err(Status::unimplemented("not implemented in mock"))
    }

    type ReceiveEnvelopesStream = ReceiverStream<Result<wacp_v1::Envelope, Status>>;

    async fn receive_envelopes(
        &self,
        _req: Request<wacp_v1::ReceiveEnvelopesRequest>,
    ) -> Result<Response<Self::ReceiveEnvelopesStream>, Status> {
        let (tx, rx) = mpsc::channel(64);
        self.state.lock().await.envelope_tx = Some(tx);
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type ReceiveCommandsStream = ReceiverStream<Result<wacp_v1::Command, Status>>;

    async fn receive_commands(
        &self,
        _req: Request<wacp_v1::ReceiveCommandsRequest>,
    ) -> Result<Response<Self::ReceiveCommandsStream>, Status> {
        let (tx, rx) = mpsc::channel(64);
        self.state.lock().await.command_tx = Some(tx);
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

async fn start_mock() -> (String, Arc<Mutex<MockState>>) {
    start_mock_with(MockState::new()).await
}

async fn start_mock_with(mock: MockState) -> (String, Arc<Mutex<MockState>>) {
    let state = Arc::new(Mutex::new(mock));
    let svc = MockAgentSvc {
        state: state.clone(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(AgentServiceServer::new(svc))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .ok();
    });
    (url, state)
}

async fn setup() -> (Agent, Arc<Mutex<MockState>>) {
    let (url, state) = start_mock().await;
    let agent = Agent::connect(AgentConfig {
        runtime_url: url,
        workspace_id: WorkspaceId::from("ws-test"),
        auth_token: "test-token".into(),
    })
    .await
    .unwrap();
    (agent, state)
}

async fn setup_with(mock: MockState) -> (Agent, Arc<Mutex<MockState>>) {
    let (url, state) = start_mock_with(mock).await;
    let agent = Agent::connect(AgentConfig {
        runtime_url: url,
        workspace_id: WorkspaceId::from("ws-test"),
        auth_token: "test-token".into(),
    })
    .await
    .unwrap();
    (agent, state)
}

// ── Agent connect (2 tests) ─────────────────────────────

#[tokio::test]
async fn connect_sends_correct_bind_request() {
    let (_, state) = setup().await;
    let calls = &state.lock().await.bind_calls;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].workspace_id, "ws-test");
    assert_eq!(calls[0].auth_token, "test-token");
}

#[tokio::test]
async fn connect_bind_error_propagates() {
    let mut mock = MockState::new();
    mock.bind_error = Some(Status::permission_denied("bad token"));
    let (url, _) = start_mock_with(mock).await;
    let result = Agent::connect(AgentConfig {
        runtime_url: url,
        workspace_id: WorkspaceId::from("ws-1"),
        auth_token: "bad".into(),
    })
    .await;
    assert!(matches!(result, Err(Error::RpcFailed(_))));
}

// ── Agent properties (6 tests) ──────────────────────────

#[tokio::test]
async fn agent_workspace_id() {
    let (agent, _) = setup().await;
    assert_eq!(agent.workspace_id(), &WorkspaceId::from("ws-test"));
}

#[tokio::test]
async fn agent_directive_present() {
    let (agent, _) = setup().await;
    let dir = agent.directive().unwrap();
    assert_eq!(dir.id, "env-directive-1");
    assert_eq!(dir.r#type, "directive");
    assert_eq!(dir.payload, b"implement feature X");
}

#[tokio::test]
async fn agent_directive_absent() {
    let mut mock = MockState::new();
    mock.bind_response.directive = None;
    let (agent, _) = setup_with(mock).await;
    assert!(agent.directive().is_none());
}

#[tokio::test]
async fn agent_context() {
    let (agent, _) = setup().await;
    assert_eq!(agent.context(), b"workspace context data");
}

#[tokio::test]
async fn agent_role() {
    let (agent, _) = setup().await;
    assert_eq!(agent.role(), "worker");
}

#[tokio::test]
async fn agent_visibility_and_authority() {
    let (agent, _) = setup().await;
    assert_eq!(agent.visibility(), &["res-alpha", "res-beta"]);
    assert_eq!(agent.authority(), &["res-alpha"]);
}

// ── Signal — all 11 types (5 tests) ────────────────────

#[tokio::test]
async fn signal_ready() {
    let (agent, state) = setup().await;
    agent.signal(SignalType::Ready).await.unwrap();
    let calls = &state.lock().await.signal_calls;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].r#type, wacp_v1::SignalType::Ready as i32);
}

#[tokio::test]
async fn signal_started() {
    let (agent, state) = setup().await;
    agent.signal(SignalType::Started).await.unwrap();
    assert_eq!(
        state.lock().await.signal_calls[0].r#type,
        wacp_v1::SignalType::Started as i32
    );
}

#[tokio::test]
async fn signal_complete() {
    let (agent, state) = setup().await;
    agent.signal(SignalType::Complete).await.unwrap();
    assert_eq!(
        state.lock().await.signal_calls[0].r#type,
        wacp_v1::SignalType::Complete as i32
    );
}

#[tokio::test]
async fn signal_blocked_and_failed_types() {
    let (agent, state) = setup().await;
    agent.signal(SignalType::Blocked).await.unwrap();
    agent.signal(SignalType::Failed).await.unwrap();
    let calls = &state.lock().await.signal_calls;
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].r#type, wacp_v1::SignalType::Blocked as i32);
    assert_eq!(calls[1].r#type, wacp_v1::SignalType::Failed as i32);
}

#[tokio::test]
async fn signal_remaining_types() {
    let (agent, state) = setup().await;
    let types = [
        (SignalType::Checkpoint, wacp_v1::SignalType::Checkpoint),
        (SignalType::Integrate, wacp_v1::SignalType::Integrate),
        (SignalType::Acknowledged, wacp_v1::SignalType::Acknowledged),
        (SignalType::Escalation, wacp_v1::SignalType::Escalation),
        (SignalType::Suspend, wacp_v1::SignalType::Suspend),
        (SignalType::Migrate, wacp_v1::SignalType::Migrate),
    ];
    for (sdk_type, _) in &types {
        agent.signal(*sdk_type).await.unwrap();
    }
    let calls = &state.lock().await.signal_calls;
    assert_eq!(calls.len(), types.len());
    for (i, (_, proto_type)) in types.iter().enumerate() {
        assert_eq!(calls[i].r#type, *proto_type as i32);
    }
}

// ── Signal convenience methods (3 tests) ────────────────

#[tokio::test]
async fn signal_blocked_with_reason() {
    let (agent, state) = setup().await;
    agent.signal_blocked("waiting for input").await.unwrap();
    let calls = &state.lock().await.signal_calls;
    assert_eq!(calls[0].r#type, wacp_v1::SignalType::Blocked as i32);
    assert_eq!(calls[0].reason, "waiting for input");
}

#[tokio::test]
async fn signal_failed_with_reason() {
    let (agent, state) = setup().await;
    agent.signal_failed("out of memory").await.unwrap();
    let calls = &state.lock().await.signal_calls;
    assert_eq!(calls[0].r#type, wacp_v1::SignalType::Failed as i32);
    assert_eq!(calls[0].reason, "out of memory");
}

#[tokio::test]
async fn signal_escalation_with_context() {
    let (agent, state) = setup().await;
    agent
        .signal_escalation(b"need human review")
        .await
        .unwrap();
    let calls = &state.lock().await.signal_calls;
    assert_eq!(calls[0].r#type, wacp_v1::SignalType::Escalation as i32);
    assert_eq!(calls[0].context, b"need human review");
}

// ── Checkpoint (2 tests) ────────────────────────────────

#[tokio::test]
async fn checkpoint_returns_id_and_hash() {
    let (agent, _) = setup().await;
    let result = agent
        .checkpoint()
        .checkpoint_type("artifact")
        .payload(b"output data")
        .create()
        .await
        .unwrap();
    assert_eq!(result.id, "cp-001");
    assert_eq!(result.content_hash, "sha256-abc123");
}

#[tokio::test]
async fn checkpoint_with_all_options() {
    let (agent, state) = setup().await;
    agent
        .checkpoint()
        .checkpoint_type("observation")
        .payload(b"metrics data")
        .intent("record performance metrics")
        .status(CheckpointStatus::Final)
        .confidence(Confidence::Medium)
        .resource_usage(ResourceUsage {
            tokens: 1000,
            wall_time_ms: 500,
            storage_bytes: 2048,
            network_bytes: 0,
            cost_micros: 100,
        })
        .create()
        .await
        .unwrap();

    let calls = &state.lock().await.checkpoint_calls;
    assert_eq!(calls[0].r#type, "observation");
    assert_eq!(calls[0].payload, b"metrics data");
    assert_eq!(calls[0].intent, "record performance metrics");
    assert_eq!(
        calls[0].status,
        wacp_v1::CheckpointStatus::Final as i32
    );
    assert_eq!(calls[0].confidence, wacp_v1::Confidence::Medium as i32);
    let usage = calls[0].resource_usage.as_ref().unwrap();
    assert_eq!(usage.tokens, 1000);
    assert_eq!(usage.wall_time_ms, 500);
    assert_eq!(usage.storage_bytes, 2048);
    assert_eq!(usage.network_bytes, 0);
    assert_eq!(usage.cost_micros, 100);
}

// ── CheckpointBuilder (3 tests) ─────────────────────────

#[tokio::test]
async fn checkpoint_builder_methods_chain() {
    let (agent, state) = setup().await;
    agent
        .checkpoint()
        .checkpoint_type("artifact")
        .payload(b"data")
        .intent("test intent")
        .status(CheckpointStatus::Provisional)
        .confidence(Confidence::High)
        .resource_usage(ResourceUsage {
            tokens: 0,
            wall_time_ms: 0,
            storage_bytes: 0,
            network_bytes: 0,
            cost_micros: 0,
        })
        .create()
        .await
        .unwrap();
    assert_eq!(state.lock().await.checkpoint_calls.len(), 1);
}

#[tokio::test]
async fn checkpoint_missing_type_error() {
    let (agent, _) = setup().await;
    let result = agent.checkpoint().payload(b"data").create().await;
    assert!(matches!(
        result,
        Err(Error::MissingField(ref f)) if f == "checkpoint_type"
    ));
}

#[tokio::test]
async fn checkpoint_missing_payload_error() {
    let (agent, _) = setup().await;
    let result = agent.checkpoint().checkpoint_type("artifact").create().await;
    assert!(matches!(
        result,
        Err(Error::MissingField(ref f)) if f == "payload"
    ));
}

// ── Send envelope (2 tests) ─────────────────────────────

#[tokio::test]
async fn send_envelope_returns_id() {
    let (agent, _) = setup().await;
    let result = agent
        .send_envelope()
        .to(&WorkspaceId::from("ws-target"))
        .envelope_type("feedback")
        .send()
        .await
        .unwrap();
    assert_eq!(result.id, "env-out-001");
}

#[tokio::test]
async fn send_envelope_with_all_fields() {
    let (agent, state) = setup().await;
    agent
        .send_envelope()
        .to(&WorkspaceId::from("ws-target"))
        .envelope_type("query")
        .payload(b"what is the status?")
        .in_reply_to(&EnvelopeId::from("env-prev-1"))
        .priority(EnvelopePriority::Urgent)
        .send()
        .await
        .unwrap();

    let calls = &state.lock().await.envelope_send_calls;
    assert_eq!(calls[0].to_workspace, "ws-target");
    assert_eq!(calls[0].r#type, "query");
    assert_eq!(calls[0].payload, b"what is the status?");
    assert_eq!(calls[0].in_reply_to, "env-prev-1");
    assert_eq!(
        calls[0].priority,
        wacp_v1::EnvelopePriority::Urgent as i32
    );
}

// ── EnvelopeBuilder (3 tests) ───────────────────────────

#[tokio::test]
async fn envelope_builder_methods_chain() {
    let (agent, state) = setup().await;
    agent
        .send_envelope()
        .to(&WorkspaceId::from("ws-target"))
        .envelope_type("directive")
        .payload(b"task details")
        .in_reply_to(&EnvelopeId::from("env-0"))
        .priority(EnvelopePriority::Blocking)
        .send()
        .await
        .unwrap();
    assert_eq!(state.lock().await.envelope_send_calls.len(), 1);
}

#[tokio::test]
async fn envelope_missing_to_error() {
    let (agent, _) = setup().await;
    let result = agent
        .send_envelope()
        .envelope_type("feedback")
        .send()
        .await;
    assert!(matches!(
        result,
        Err(Error::MissingField(ref f)) if f == "to"
    ));
}

#[tokio::test]
async fn envelope_missing_type_error() {
    let (agent, _) = setup().await;
    let result = agent
        .send_envelope()
        .to(&WorkspaceId::from("ws-target"))
        .send()
        .await;
    assert!(matches!(
        result,
        Err(Error::MissingField(ref f)) if f == "envelope_type"
    ));
}

// ── Inbox stream (2 tests) ──────────────────────────────

#[tokio::test]
async fn inbox_receives_envelopes() {
    let (agent, state) = setup().await;
    let mut inbox = agent.inbox().await.unwrap();

    let tx = state.lock().await.envelope_tx.clone().unwrap();
    tx.send(Ok(wacp_v1::Envelope {
        id: "env-in-1".into(),
        from_workspace: "ws-other".into(),
        to_workspace: "ws-test".into(),
        r#type: "feedback".into(),
        payload: b"looks good".to_vec(),
        in_reply_to: String::new(),
        timestamp: None,
        priority: wacp_v1::EnvelopePriority::Normal.into(),
        origin: wacp_v1::EnvelopeOrigin::Agent.into(),
    }))
    .await
    .unwrap();

    let env = inbox.next().await.unwrap().unwrap();
    assert_eq!(env.id, "env-in-1");
    assert_eq!(env.payload, b"looks good");
}

#[tokio::test]
async fn inbox_stream_ends_on_close() {
    let (agent, state) = setup().await;
    let mut inbox = agent.inbox().await.unwrap();

    let tx = state.lock().await.envelope_tx.take().unwrap();
    drop(tx);

    assert!(inbox.next().await.is_none());
}

// ── Command stream (2 tests) ────────────────────────────

#[tokio::test]
async fn commands_receives_commands() {
    let (agent, state) = setup().await;
    let mut cmds = agent.commands().await.unwrap();

    let tx = state.lock().await.command_tx.clone().unwrap();
    tx.send(Ok(wacp_v1::Command {
        command: Some(wacp_v1::command::Command::Feedback(wacp_v1::Envelope {
            id: "env-fb-1".into(),
            from_workspace: "ws-root".into(),
            to_workspace: "ws-test".into(),
            r#type: "feedback".into(),
            payload: b"please revise".to_vec(),
            in_reply_to: String::new(),
            timestamp: None,
            priority: wacp_v1::EnvelopePriority::Normal.into(),
            origin: wacp_v1::EnvelopeOrigin::Agent.into(),
        })),
    }))
    .await
    .unwrap();

    let cmd = cmds.next().await.unwrap().unwrap();
    assert!(cmd.command.is_some());
}

#[tokio::test]
async fn commands_stream_ends_on_close() {
    let (agent, state) = setup().await;
    let mut cmds = agent.commands().await.unwrap();

    let tx = state.lock().await.command_tx.take().unwrap();
    drop(tx);

    assert!(cmds.next().await.is_none());
}

// ── Query trail (2 tests) ───────────────────────────────

#[tokio::test]
async fn query_trail_with_filters() {
    let mut mock = MockState::new();
    mock.trail_response = wacp_v1::QueryTrailResponse {
        entries: vec![wacp_v1::TrailEntry {
            id: "te-1".into(),
            timestamp: None,
            workspace_id: "ws-test".into(),
            actor: "agent-1".into(),
            event_type: "signal_emitted".into(),
            body: vec![],
            sequence_number: 1,
            chain_hash: vec![],
        }],
        has_more: false,
        client_request_id: String::new(),
    };
    let (agent, state) = setup_with(mock).await;

    let entries = agent
        .query_trail(Some("ws-test"), Some("signal_emitted"), 10)
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "te-1");

    let calls = &state.lock().await.trail_calls;
    assert_eq!(calls[0].workspace_id, "ws-test");
    assert_eq!(calls[0].event_type, "signal_emitted");
    assert_eq!(calls[0].limit, 10);
}

#[tokio::test]
async fn query_trail_empty_result() {
    let (agent, _) = setup().await;
    let entries = agent.query_trail(None, None, 100).await.unwrap();
    assert!(entries.is_empty());
}

// ── Disconnect (1 test) ─────────────────────────────────

#[tokio::test]
async fn disconnect_succeeds() {
    let (agent, _) = setup().await;
    agent.disconnect().await.unwrap();
}

// ── Error handling (3 tests) ────────────────────────────

#[tokio::test]
async fn connect_invalid_url() {
    let result = Agent::connect(AgentConfig {
        runtime_url: "not a valid url".into(),
        workspace_id: WorkspaceId::from("ws-1"),
        auth_token: "token".into(),
    })
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn signal_rpc_error_propagates() {
    let mut mock = MockState::new();
    mock.signal_error = Some(Status::internal("server error"));
    let (agent, _) = setup_with(mock).await;

    let result = agent.signal(SignalType::Ready).await;
    assert!(matches!(
        result,
        Err(Error::RpcFailed(ref s)) if s.code() == tonic::Code::Internal
    ));
}

#[tokio::test]
async fn checkpoint_rpc_error_propagates() {
    let mut mock = MockState::new();
    mock.checkpoint_error = Some(Status::resource_exhausted("budget exceeded"));
    let (agent, _) = setup_with(mock).await;

    let result = agent
        .checkpoint()
        .checkpoint_type("artifact")
        .payload(b"data")
        .create()
        .await;
    assert!(matches!(
        result,
        Err(Error::RpcFailed(ref s)) if s.code() == tonic::Code::ResourceExhausted
    ));
}

// ── Concurrent operations (2 tests) ─────────────────────

#[tokio::test]
async fn concurrent_signal_and_checkpoint() {
    let (agent, state) = setup().await;
    let (sig, cp) = tokio::join!(
        agent.signal(SignalType::Ready),
        agent
            .checkpoint()
            .checkpoint_type("artifact")
            .payload(b"data")
            .create()
    );
    sig.unwrap();
    cp.unwrap();

    let s = state.lock().await;
    assert_eq!(s.signal_calls.len(), 1);
    assert_eq!(s.checkpoint_calls.len(), 1);
}

#[tokio::test]
async fn concurrent_multiple_signals() {
    let (agent, state) = setup().await;
    let (r1, r2, r3) = tokio::join!(
        agent.signal(SignalType::Ready),
        agent.signal(SignalType::Started),
        agent.signal(SignalType::Complete),
    );
    r1.unwrap();
    r2.unwrap();
    r3.unwrap();
    assert_eq!(state.lock().await.signal_calls.len(), 3);
}

// ── Reconnection / resilience (1 test) ──────────────────

#[tokio::test]
async fn agent_resilient_after_rpc_error() {
    let mut mock = MockState::new();
    mock.signal_error = Some(Status::unavailable("temporary failure"));
    let (agent, state) = setup_with(mock).await;

    // First call fails (mock returns error, consumed via .take())
    assert!(agent.signal(SignalType::Ready).await.is_err());

    // Second call succeeds — client is still usable
    agent.signal(SignalType::Ready).await.unwrap();

    assert_eq!(state.lock().await.signal_calls.len(), 2);
}

// ── Builder defaults (3 tests) ──────────────────────────

#[tokio::test]
async fn checkpoint_builder_default_status_provisional() {
    let (agent, state) = setup().await;
    agent
        .checkpoint()
        .checkpoint_type("artifact")
        .payload(b"data")
        .create()
        .await
        .unwrap();
    assert_eq!(
        state.lock().await.checkpoint_calls[0].status,
        wacp_v1::CheckpointStatus::Provisional as i32
    );
}

#[tokio::test]
async fn checkpoint_builder_default_confidence_high() {
    let (agent, state) = setup().await;
    agent
        .checkpoint()
        .checkpoint_type("artifact")
        .payload(b"data")
        .create()
        .await
        .unwrap();
    assert_eq!(
        state.lock().await.checkpoint_calls[0].confidence,
        wacp_v1::Confidence::High as i32
    );
}

#[tokio::test]
async fn envelope_builder_default_priority_normal() {
    let (agent, state) = setup().await;
    agent
        .send_envelope()
        .to(&WorkspaceId::from("ws-target"))
        .envelope_type("feedback")
        .send()
        .await
        .unwrap();
    assert_eq!(
        state.lock().await.envelope_send_calls[0].priority,
        wacp_v1::EnvelopePriority::Normal as i32
    );
}

// ── Stream error mapping (2 tests) ──────────────────────

#[tokio::test]
async fn inbox_stream_maps_rpc_error() {
    let (agent, state) = setup().await;
    let mut inbox = agent.inbox().await.unwrap();

    let tx = state.lock().await.envelope_tx.clone().unwrap();
    tx.send(Err(Status::internal("stream error")))
        .await
        .unwrap();

    let item = inbox.next().await.unwrap();
    assert!(matches!(
        item,
        Err(Error::RpcFailed(ref s)) if s.code() == tonic::Code::Internal
    ));
}

#[tokio::test]
async fn command_stream_maps_rpc_error() {
    let (agent, state) = setup().await;
    let mut cmds = agent.commands().await.unwrap();

    let tx = state.lock().await.command_tx.clone().unwrap();
    tx.send(Err(Status::unavailable("server going away")))
        .await
        .unwrap();

    let item = cmds.next().await.unwrap();
    assert!(matches!(
        item,
        Err(Error::RpcFailed(ref s)) if s.code() == tonic::Code::Unavailable
    ));
}

// ── Proto mapping exhaustiveness (3 tests) ──────────────

#[tokio::test]
async fn signal_type_proto_mapping_exhaustive() {
    let (agent, state) = setup().await;
    let all_types = [
        SignalType::Ready,
        SignalType::Started,
        SignalType::Blocked,
        SignalType::Checkpoint,
        SignalType::Complete,
        SignalType::Failed,
        SignalType::Integrate,
        SignalType::Acknowledged,
        SignalType::Escalation,
        SignalType::Suspend,
        SignalType::Migrate,
    ];
    for t in &all_types {
        agent.signal(*t).await.unwrap();
    }
    let calls = &state.lock().await.signal_calls;
    assert_eq!(calls.len(), 11);
    for call in calls {
        assert_ne!(call.r#type, 0, "signal type mapped to Unspecified");
    }
}

#[tokio::test]
async fn checkpoint_status_proto_mapping() {
    let (agent, state) = setup().await;
    for status in [CheckpointStatus::Provisional, CheckpointStatus::Final] {
        agent
            .checkpoint()
            .checkpoint_type("artifact")
            .payload(b"data")
            .status(status)
            .create()
            .await
            .unwrap();
    }
    let calls = &state.lock().await.checkpoint_calls;
    assert_eq!(
        calls[0].status,
        wacp_v1::CheckpointStatus::Provisional as i32
    );
    assert_eq!(calls[1].status, wacp_v1::CheckpointStatus::Final as i32);
}

#[tokio::test]
async fn envelope_priority_proto_mapping() {
    let (agent, state) = setup().await;
    for priority in [
        EnvelopePriority::Normal,
        EnvelopePriority::Urgent,
        EnvelopePriority::Blocking,
    ] {
        agent
            .send_envelope()
            .to(&WorkspaceId::from("ws-target"))
            .envelope_type("feedback")
            .priority(priority)
            .send()
            .await
            .unwrap();
    }
    let calls = &state.lock().await.envelope_send_calls;
    assert_eq!(
        calls[0].priority,
        wacp_v1::EnvelopePriority::Normal as i32
    );
    assert_eq!(
        calls[1].priority,
        wacp_v1::EnvelopePriority::Urgent as i32
    );
    assert_eq!(
        calls[2].priority,
        wacp_v1::EnvelopePriority::Blocking as i32
    );
}
