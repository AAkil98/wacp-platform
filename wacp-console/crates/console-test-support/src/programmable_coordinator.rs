//! Programmable `CoordinatorService` mock for launcher / recovery tests.
//!
//! The default `MockCoordinatorService` in `mock_grpc.rs` is stateless and
//! always returns canned successes. For failure-path tests the launcher needs:
//!   - per-RPC configurable success/error behavior (including different
//!     behaviors for sequential calls to the same RPC)
//!   - request capture so tests can assert *what* was sent
//!   - call counts for ordering assertions
//!
//! Usage:
//! ```ignore
//! let programmable = ProgrammableCoordinator::new();
//! programmable.set_submit_goal_ok(SubmitGoalResponse { goal_id: "g1".into(), root_workspace_id: "ws-0".into() });
//! programmable.queue_dispatch_response(DispatchResponse { workspace_id: "ws-1".into(), task_id: "t1".into() });
//! programmable.fail_dispatch_at(1, Status::unavailable("boom"));
//! let (handle, runtime) = ProgrammableCoordinator::spawn(programmable.clone()).await?;
//! // … run launcher against `runtime.coordinator_addr` …
//! assert_eq!(programmable.submit_goal_calls().len(), 1);
//! assert_eq!(programmable.abort_calls().len(), 1);  // rollback aborted the root
//! ```

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tonic::{Request, Response, Status};

use wacp_proto::wacp_v1 as proto;

use proto::coordinator_service_server::{CoordinatorService, CoordinatorServiceServer};

/// A fully-recording, programmable `CoordinatorService` suitable for tests
/// that need to inject failures at specific call sites.
#[derive(Clone, Default)]
pub struct ProgrammableCoordinator {
    inner: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    // --- scripted responses ---
    submit_goal_response: Option<Result<proto::SubmitGoalResponse, Status>>,
    decompose_response: Option<Result<proto::DecomposeResponse, Status>>,
    /// FIFO queue of Dispatch responses. One is popped per call. An error
    /// enqueued at position N causes the Nth call to fail.
    dispatch_queue: VecDeque<Result<proto::DispatchResponse, Status>>,
    abort_response: Option<Result<proto::AbortWorkspaceResponse, Status>>,

    // --- recordings ---
    submit_goal_calls: Vec<proto::SubmitGoalRequest>,
    decompose_calls: Vec<proto::DecomposeRequest>,
    dispatch_calls: Vec<proto::DispatchRequest>,
    abort_calls: Vec<proto::AbortWorkspaceRequest>,
}

impl ProgrammableCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    // ---------- programming ----------

    pub fn set_submit_goal_ok(&self, resp: proto::SubmitGoalResponse) {
        self.inner.lock().unwrap().submit_goal_response = Some(Ok(resp));
    }

    pub fn set_submit_goal_err(&self, err: Status) {
        self.inner.lock().unwrap().submit_goal_response = Some(Err(err));
    }

    pub fn set_decompose_ok(&self, resp: proto::DecomposeResponse) {
        self.inner.lock().unwrap().decompose_response = Some(Ok(resp));
    }

    pub fn set_decompose_err(&self, err: Status) {
        self.inner.lock().unwrap().decompose_response = Some(Err(err));
    }

    /// Queue one Dispatch response. Call once per expected dispatch; errors
    /// in the queue will surface at that position.
    pub fn queue_dispatch(&self, outcome: Result<proto::DispatchResponse, Status>) {
        self.inner.lock().unwrap().dispatch_queue.push_back(outcome);
    }

    pub fn set_abort_ok(&self) {
        self.inner.lock().unwrap().abort_response =
            Some(Ok(proto::AbortWorkspaceResponse::default()));
    }

    pub fn set_abort_err(&self, err: Status) {
        self.inner.lock().unwrap().abort_response = Some(Err(err));
    }

    // ---------- inspection ----------

    pub fn submit_goal_calls(&self) -> Vec<proto::SubmitGoalRequest> {
        self.inner.lock().unwrap().submit_goal_calls.clone()
    }

    pub fn decompose_calls(&self) -> Vec<proto::DecomposeRequest> {
        self.inner.lock().unwrap().decompose_calls.clone()
    }

    pub fn dispatch_calls(&self) -> Vec<proto::DispatchRequest> {
        self.inner.lock().unwrap().dispatch_calls.clone()
    }

    pub fn abort_calls(&self) -> Vec<proto::AbortWorkspaceRequest> {
        self.inner.lock().unwrap().abort_calls.clone()
    }

    // ---------- spawn ----------

    /// Spawn this service on a random ephemeral port. Returns the listening
    /// address and a join handle (the handle is dropped when the returned
    /// guard drops; the caller is responsible for keeping the guard alive
    /// for the lifetime of the test).
    pub async fn spawn(self) -> Result<(SocketAddr, JoinHandle<()>), std::io::Error> {
        let listener = TcpListener::bind("[::1]:0").await?;
        let addr = listener.local_addr()?;
        let stream = tokio_stream::wrappers::TcpListenerStream::new(listener);
        let handle = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(CoordinatorServiceServer::new(self))
                .serve_with_incoming(stream)
                .await
                .ok();
        });
        Ok((addr, handle))
    }
}

#[tonic::async_trait]
impl CoordinatorService for ProgrammableCoordinator {
    async fn submit_goal(
        &self,
        request: Request<proto::SubmitGoalRequest>,
    ) -> Result<Response<proto::SubmitGoalResponse>, Status> {
        let mut st = self.inner.lock().unwrap();
        st.submit_goal_calls.push(request.into_inner());
        match st.submit_goal_response.clone() {
            Some(Ok(resp)) => Ok(Response::new(resp)),
            Some(Err(e)) => Err(e),
            None => Err(Status::unimplemented("submit_goal not programmed")),
        }
    }

    async fn decompose(
        &self,
        request: Request<proto::DecomposeRequest>,
    ) -> Result<Response<proto::DecomposeResponse>, Status> {
        let mut st = self.inner.lock().unwrap();
        st.decompose_calls.push(request.into_inner());
        match st.decompose_response.clone() {
            Some(Ok(resp)) => Ok(Response::new(resp)),
            Some(Err(e)) => Err(e),
            None => Err(Status::unimplemented("decompose not programmed")),
        }
    }

    async fn dispatch(
        &self,
        request: Request<proto::DispatchRequest>,
    ) -> Result<Response<proto::DispatchResponse>, Status> {
        let mut st = self.inner.lock().unwrap();
        st.dispatch_calls.push(request.into_inner());
        match st.dispatch_queue.pop_front() {
            Some(Ok(resp)) => Ok(Response::new(resp)),
            Some(Err(e)) => Err(e),
            None => Err(Status::unimplemented("dispatch: queue exhausted")),
        }
    }

    async fn abort_workspace(
        &self,
        request: Request<proto::AbortWorkspaceRequest>,
    ) -> Result<Response<proto::AbortWorkspaceResponse>, Status> {
        let mut st = self.inner.lock().unwrap();
        st.abort_calls.push(request.into_inner());
        match st.abort_response.clone() {
            Some(Ok(resp)) => Ok(Response::new(resp)),
            Some(Err(e)) => Err(e),
            // Default: abort succeeds silently — matches runtime behavior and
            // lets tests skip programming it unless they care.
            None => Ok(Response::new(proto::AbortWorkspaceResponse::default())),
        }
    }

    // --- RPCs the launcher never calls — delegate to sensible defaults. ---

    async fn get_ready_tasks(
        &self,
        _request: Request<proto::GetReadyTasksRequest>,
    ) -> Result<Response<proto::GetReadyTasksResponse>, Status> {
        Ok(Response::new(proto::GetReadyTasksResponse {
            tasks: vec![],
        }))
    }

    async fn cancel_task(
        &self,
        _request: Request<proto::CancelTaskRequest>,
    ) -> Result<Response<proto::CancelTaskResponse>, Status> {
        Ok(Response::new(proto::CancelTaskResponse {}))
    }

    async fn suspend_workspace(
        &self,
        _request: Request<proto::SuspendWorkspaceRequest>,
    ) -> Result<Response<proto::SuspendWorkspaceResponse>, Status> {
        Ok(Response::new(proto::SuspendWorkspaceResponse {}))
    }

    async fn resume_workspace(
        &self,
        _request: Request<proto::ResumeWorkspaceRequest>,
    ) -> Result<Response<proto::ResumeWorkspaceResponse>, Status> {
        Ok(Response::new(proto::ResumeWorkspaceResponse {}))
    }

    async fn send_directive(
        &self,
        _request: Request<proto::SendDirectiveRequest>,
    ) -> Result<Response<proto::SendDirectiveResponse>, Status> {
        Ok(Response::new(proto::SendDirectiveResponse {
            envelope_id: String::new(),
        }))
    }

    async fn send_feedback(
        &self,
        _request: Request<proto::SendFeedbackRequest>,
    ) -> Result<Response<proto::SendFeedbackResponse>, Status> {
        Ok(Response::new(proto::SendFeedbackResponse {
            envelope_id: String::new(),
        }))
    }

    async fn trigger_integration(
        &self,
        _request: Request<proto::TriggerIntegrationRequest>,
    ) -> Result<Response<proto::TriggerIntegrationResponse>, Status> {
        Ok(Response::new(proto::TriggerIntegrationResponse {
            result: "accepted".into(),
            detail: String::new(),
        }))
    }

    async fn get_allocatable(
        &self,
        _request: Request<proto::GetAllocatableRequest>,
    ) -> Result<Response<proto::GetAllocatableResponse>, Status> {
        Ok(Response::new(proto::GetAllocatableResponse {
            remaining: None,
        }))
    }

    type StreamSignalsStream =
        tokio_stream::wrappers::ReceiverStream<Result<proto::SignalEvent, Status>>;

    async fn stream_signals(
        &self,
        _request: Request<proto::StreamSignalsRequest>,
    ) -> Result<Response<Self::StreamSignalsStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}
