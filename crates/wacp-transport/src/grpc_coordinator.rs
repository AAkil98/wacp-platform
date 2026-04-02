use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::proto::wacp_v1;
use crate::proto::wacp_v1::coordinator_service_server::CoordinatorService;

/// Channel-based coordinator service implementation.
/// Forwards all requests to the coordinator actor via mpsc channel.
pub struct CoordinatorServiceImpl {
    coordinator_tx: mpsc::Sender<CoordinatorRequest>,
}

/// A coordinator request forwarded to the coordinator actor.
#[derive(Debug)]
pub enum CoordinatorRequest {
    SubmitGoal {
        request: wacp_v1::SubmitGoalRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::SubmitGoalResponse, Status>>,
    },
    Decompose {
        request: wacp_v1::DecomposeRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::DecomposeResponse, Status>>,
    },
    GetReadyTasks {
        request: wacp_v1::GetReadyTasksRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::GetReadyTasksResponse, Status>>,
    },
    CancelTask {
        request: wacp_v1::CancelTaskRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::CancelTaskResponse, Status>>,
    },
    Dispatch {
        request: wacp_v1::DispatchRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::DispatchResponse, Status>>,
    },
    AbortWorkspace {
        request: wacp_v1::AbortWorkspaceRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::AbortWorkspaceResponse, Status>>,
    },
    SuspendWorkspace {
        request: wacp_v1::SuspendWorkspaceRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::SuspendWorkspaceResponse, Status>>,
    },
    ResumeWorkspace {
        request: wacp_v1::ResumeWorkspaceRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::ResumeWorkspaceResponse, Status>>,
    },
    SendDirective {
        request: wacp_v1::SendDirectiveRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::SendDirectiveResponse, Status>>,
    },
    SendFeedback {
        request: wacp_v1::SendFeedbackRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::SendFeedbackResponse, Status>>,
    },
    TriggerIntegration {
        request: wacp_v1::TriggerIntegrationRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::TriggerIntegrationResponse, Status>>,
    },
    GetAllocatable {
        request: wacp_v1::GetAllocatableRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::GetAllocatableResponse, Status>>,
    },
    StreamSignals {
        request: wacp_v1::StreamSignalsRequest,
        tx: mpsc::Sender<Result<wacp_v1::SignalEvent, Status>>,
    },
}

impl CoordinatorServiceImpl {
    pub fn new(coordinator_tx: mpsc::Sender<CoordinatorRequest>) -> Self {
        Self { coordinator_tx }
    }

    /// Helper: send request to coordinator and await reply.
    async fn send_and_recv<T>(
        &self,
        make_req: impl FnOnce(tokio::sync::oneshot::Sender<Result<T, Status>>) -> CoordinatorRequest,
    ) -> Result<Response<T>, Status> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.coordinator_tx
            .send(make_req(reply_tx))
            .await
            .map_err(|_| Status::internal("coordinator unavailable"))?;

        reply_rx
            .await
            .map_err(|_| Status::internal("coordinator dropped reply"))?
            .map(Response::new)
    }
}

#[tonic::async_trait]
impl CoordinatorService for CoordinatorServiceImpl {
    async fn submit_goal(
        &self,
        request: Request<wacp_v1::SubmitGoalRequest>,
    ) -> Result<Response<wacp_v1::SubmitGoalResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::SubmitGoal { request: req, reply })
            .await
    }

    async fn decompose(
        &self,
        request: Request<wacp_v1::DecomposeRequest>,
    ) -> Result<Response<wacp_v1::DecomposeResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::Decompose { request: req, reply })
            .await
    }

    async fn get_ready_tasks(
        &self,
        request: Request<wacp_v1::GetReadyTasksRequest>,
    ) -> Result<Response<wacp_v1::GetReadyTasksResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::GetReadyTasks { request: req, reply })
            .await
    }

    async fn cancel_task(
        &self,
        request: Request<wacp_v1::CancelTaskRequest>,
    ) -> Result<Response<wacp_v1::CancelTaskResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::CancelTask { request: req, reply })
            .await
    }

    async fn dispatch(
        &self,
        request: Request<wacp_v1::DispatchRequest>,
    ) -> Result<Response<wacp_v1::DispatchResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::Dispatch { request: req, reply })
            .await
    }

    async fn abort_workspace(
        &self,
        request: Request<wacp_v1::AbortWorkspaceRequest>,
    ) -> Result<Response<wacp_v1::AbortWorkspaceResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::AbortWorkspace { request: req, reply })
            .await
    }

    async fn suspend_workspace(
        &self,
        request: Request<wacp_v1::SuspendWorkspaceRequest>,
    ) -> Result<Response<wacp_v1::SuspendWorkspaceResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::SuspendWorkspace { request: req, reply })
            .await
    }

    async fn resume_workspace(
        &self,
        request: Request<wacp_v1::ResumeWorkspaceRequest>,
    ) -> Result<Response<wacp_v1::ResumeWorkspaceResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::ResumeWorkspace { request: req, reply })
            .await
    }

    async fn send_directive(
        &self,
        request: Request<wacp_v1::SendDirectiveRequest>,
    ) -> Result<Response<wacp_v1::SendDirectiveResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::SendDirective { request: req, reply })
            .await
    }

    async fn send_feedback(
        &self,
        request: Request<wacp_v1::SendFeedbackRequest>,
    ) -> Result<Response<wacp_v1::SendFeedbackResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::SendFeedback { request: req, reply })
            .await
    }

    async fn trigger_integration(
        &self,
        request: Request<wacp_v1::TriggerIntegrationRequest>,
    ) -> Result<Response<wacp_v1::TriggerIntegrationResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::TriggerIntegration { request: req, reply })
            .await
    }

    async fn get_allocatable(
        &self,
        request: Request<wacp_v1::GetAllocatableRequest>,
    ) -> Result<Response<wacp_v1::GetAllocatableResponse>, Status> {
        let req = request.into_inner();
        self.send_and_recv(|reply| CoordinatorRequest::GetAllocatable { request: req, reply })
            .await
    }

    type StreamSignalsStream = ReceiverStream<Result<wacp_v1::SignalEvent, Status>>;

    async fn stream_signals(
        &self,
        request: Request<wacp_v1::StreamSignalsRequest>,
    ) -> Result<Response<Self::StreamSignalsStream>, Status> {
        let req = request.into_inner();
        let (event_tx, event_rx) = mpsc::channel(64);

        self.coordinator_tx
            .send(CoordinatorRequest::StreamSignals {
                request: req,
                tx: event_tx,
            })
            .await
            .map_err(|_| Status::internal("coordinator unavailable"))?;

        Ok(Response::new(ReceiverStream::new(event_rx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn coordinator_unavailable_returns_internal() {
        // Create a channel and immediately drop the receiver
        let (tx, _rx) = mpsc::channel::<CoordinatorRequest>(1);
        let svc = CoordinatorServiceImpl::new(tx);

        // Drop the receiver so sends fail
        drop(_rx);

        let result = svc
            .submit_goal(Request::new(wacp_v1::SubmitGoalRequest {
                description: "test".into(),
                context: vec![],
                client_request_id: String::new(),
            }))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Internal);
    }

    #[tokio::test]
    async fn submit_goal_round_trip() {
        let (tx, mut rx) = mpsc::channel::<CoordinatorRequest>(16);
        let svc = CoordinatorServiceImpl::new(tx);

        // Spawn a handler that processes the request
        tokio::spawn(async move {
            if let Some(CoordinatorRequest::SubmitGoal { request, reply }) = rx.recv().await {
                assert_eq!(request.description, "implement auth");
                reply
                    .send(Ok(wacp_v1::SubmitGoalResponse {
                        goal_id: "goal-1".into(),
                        root_workspace_id: "ws-root".into(),
                    }))
                    .ok();
            }
        });

        let response = svc
            .submit_goal(Request::new(wacp_v1::SubmitGoalRequest {
                description: "implement auth".into(),
                context: vec![],
                client_request_id: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(response.goal_id, "goal-1");
        assert_eq!(response.root_workspace_id, "ws-root");
    }

    #[tokio::test]
    async fn decompose_round_trip() {
        let (tx, mut rx) = mpsc::channel::<CoordinatorRequest>(16);
        let svc = CoordinatorServiceImpl::new(tx);

        tokio::spawn(async move {
            if let Some(CoordinatorRequest::Decompose { request, reply }) = rx.recv().await {
                assert_eq!(request.tasks.len(), 2);
                reply
                    .send(Ok(wacp_v1::DecomposeResponse {
                        task_ids: vec!["task-1".into(), "task-2".into()],
                    }))
                    .ok();
            }
        });

        let response = svc
            .decompose(Request::new(wacp_v1::DecomposeRequest {
                tasks: vec![
                    wacp_v1::TaskDefinition {
                        name: "plan".into(),
                        description: "plan the work".into(),
                        depends_on: vec![],
                        role: "planner".into(),
                        directive_payload: vec![],
                        tools: vec![],
                    },
                    wacp_v1::TaskDefinition {
                        name: "implement".into(),
                        description: "do the work".into(),
                        depends_on: vec!["plan".into()],
                        role: "implementer".into(),
                        directive_payload: vec![],
                        tools: vec![],
                    },
                ],
                client_request_id: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(response.task_ids, vec!["task-1", "task-2"]);
    }

    #[tokio::test]
    async fn dispatch_round_trip() {
        let (tx, mut rx) = mpsc::channel::<CoordinatorRequest>(16);
        let svc = CoordinatorServiceImpl::new(tx);

        tokio::spawn(async move {
            if let Some(CoordinatorRequest::Dispatch { request, reply }) = rx.recv().await {
                assert_eq!(request.task_id, "task-1");
                assert_eq!(request.role, "swe:implementer");
                reply
                    .send(Ok(wacp_v1::DispatchResponse {
                        workspace_id: "ws-impl-1".into(),
                        task_id: "task-1".into(),
                    }))
                    .ok();
            }
        });

        let response = svc
            .dispatch(Request::new(wacp_v1::DispatchRequest {
                task_id: "task-1".into(),
                role: "swe:implementer".into(),
                directive_payload: b"implement auth".to_vec(),
                tools: vec!["read_file".into(), "write_file".into()],
                budget: None,
                client_request_id: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(response.workspace_id, "ws-impl-1");
    }

    #[tokio::test]
    async fn abort_workspace_round_trip() {
        let (tx, mut rx) = mpsc::channel::<CoordinatorRequest>(16);
        let svc = CoordinatorServiceImpl::new(tx);

        tokio::spawn(async move {
            if let Some(CoordinatorRequest::AbortWorkspace { request, reply }) = rx.recv().await {
                assert_eq!(request.workspace_id, "ws-1");
                assert_eq!(request.reason, "timeout");
                reply.send(Ok(wacp_v1::AbortWorkspaceResponse {})).ok();
            }
        });

        let result = svc
            .abort_workspace(Request::new(wacp_v1::AbortWorkspaceRequest {
                workspace_id: "ws-1".into(),
                reason: "timeout".into(),
                client_request_id: String::new(),
            }))
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_allocatable_round_trip() {
        let (tx, mut rx) = mpsc::channel::<CoordinatorRequest>(16);
        let svc = CoordinatorServiceImpl::new(tx);

        tokio::spawn(async move {
            if let Some(CoordinatorRequest::GetAllocatable { reply, .. }) = rx.recv().await {
                reply
                    .send(Ok(wacp_v1::GetAllocatableResponse {
                        remaining: Some(wacp_v1::ResourceBudget {
                            max_tokens: 100_000,
                            max_wall_time_ms: 300_000,
                            max_storage_bytes: 10_000_000,
                            max_network_bytes: 0,
                            max_cost_micros: 5_000_000,
                            warning_threshold: 0.8,
                        }),
                    }))
                    .ok();
            }
        });

        let response = svc
            .get_allocatable(Request::new(wacp_v1::GetAllocatableRequest {
                client_request_id: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        let budget = response.remaining.unwrap();
        assert_eq!(budget.max_tokens, 100_000);
        assert_eq!(budget.max_cost_micros, 5_000_000);
    }

    #[tokio::test]
    async fn stream_signals_delivers_events() {
        let (tx, mut rx) = mpsc::channel::<CoordinatorRequest>(16);
        let svc = CoordinatorServiceImpl::new(tx);

        tokio::spawn(async move {
            if let Some(CoordinatorRequest::StreamSignals { tx, .. }) = rx.recv().await {
                tx.send(Ok(wacp_v1::SignalEvent {
                    workspace_id: "ws-child-1".into(),
                    signal_type: wacp_v1::SignalType::Complete.into(),
                    reason: String::new(),
                    context: vec![],
                    timestamp: 1234567890,
                }))
                .await
                .ok();
                // Drop tx to close stream
            }
        });

        let response = svc
            .stream_signals(Request::new(wacp_v1::StreamSignalsRequest {
                workspace_filter: String::new(),
                signal_type_filter: String::new(),
            }))
            .await
            .unwrap();

        let mut stream = response.into_inner();
        use tokio_stream::StreamExt;
        let event = stream.next().await.unwrap().unwrap();
        assert_eq!(event.workspace_id, "ws-child-1");
        assert_eq!(event.timestamp, 1234567890);
    }

    #[tokio::test]
    async fn trigger_integration_round_trip() {
        let (tx, mut rx) = mpsc::channel::<CoordinatorRequest>(16);
        let svc = CoordinatorServiceImpl::new(tx);

        tokio::spawn(async move {
            if let Some(CoordinatorRequest::TriggerIntegration { request, reply }) = rx.recv().await
            {
                assert_eq!(request.workspace_id, "ws-impl");
                reply
                    .send(Ok(wacp_v1::TriggerIntegrationResponse {
                        result: "accepted".into(),
                        detail: "all checks passed".into(),
                    }))
                    .ok();
            }
        });

        let response = svc
            .trigger_integration(Request::new(wacp_v1::TriggerIntegrationRequest {
                workspace_id: "ws-impl".into(),
                client_request_id: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(response.result, "accepted");
    }

    #[tokio::test]
    async fn send_directive_round_trip() {
        let (tx, mut rx) = mpsc::channel::<CoordinatorRequest>(16);
        let svc = CoordinatorServiceImpl::new(tx);

        tokio::spawn(async move {
            if let Some(CoordinatorRequest::SendDirective { request, reply }) = rx.recv().await {
                assert_eq!(request.workspace_id, "ws-1");
                reply
                    .send(Ok(wacp_v1::SendDirectiveResponse {
                        envelope_id: "env-1".into(),
                    }))
                    .ok();
            }
        });

        let response = svc
            .send_directive(Request::new(wacp_v1::SendDirectiveRequest {
                workspace_id: "ws-1".into(),
                payload: b"implement this".to_vec(),
                client_request_id: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(response.envelope_id, "env-1");
    }

    #[tokio::test]
    async fn all_request_variants_constructed() {
        // Verify every CoordinatorRequest variant can be created
        // (compile-time check + runtime construction)
        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::SubmitGoal {
            request: wacp_v1::SubmitGoalRequest::default(),
            reply: reply_tx,
        };

        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::Decompose {
            request: wacp_v1::DecomposeRequest::default(),
            reply: reply_tx,
        };

        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::GetReadyTasks {
            request: wacp_v1::GetReadyTasksRequest::default(),
            reply: reply_tx,
        };

        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::CancelTask {
            request: wacp_v1::CancelTaskRequest::default(),
            reply: reply_tx,
        };

        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::Dispatch {
            request: wacp_v1::DispatchRequest::default(),
            reply: reply_tx,
        };

        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::AbortWorkspace {
            request: wacp_v1::AbortWorkspaceRequest::default(),
            reply: reply_tx,
        };

        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::SuspendWorkspace {
            request: wacp_v1::SuspendWorkspaceRequest::default(),
            reply: reply_tx,
        };

        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::ResumeWorkspace {
            request: wacp_v1::ResumeWorkspaceRequest::default(),
            reply: reply_tx,
        };

        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::SendDirective {
            request: wacp_v1::SendDirectiveRequest::default(),
            reply: reply_tx,
        };

        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::SendFeedback {
            request: wacp_v1::SendFeedbackRequest::default(),
            reply: reply_tx,
        };

        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::TriggerIntegration {
            request: wacp_v1::TriggerIntegrationRequest::default(),
            reply: reply_tx,
        };

        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = CoordinatorRequest::GetAllocatable {
            request: wacp_v1::GetAllocatableRequest::default(),
            reply: reply_tx,
        };

        let (tx, _) = mpsc::channel(1);
        let _ = CoordinatorRequest::StreamSignals {
            request: wacp_v1::StreamSignalsRequest::default(),
            tx,
        };
    }
}
