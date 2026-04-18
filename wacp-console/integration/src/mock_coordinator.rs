//! Reusable failure-injection mock `CoordinatorService` for integration
//! tests. Forwards every RPC to a real `wacp-runtime` child, but lets a
//! test push per-RPC [`tonic::Status`] failures into a queue that is
//! drained ahead of the forward. A single empty queue entry means "let
//! this call pass through"; pushing `N` failures in a row means the next
//! `N` calls of that RPC fail in order.
//!
//! This is the generalization of WA5's `failure_proxy::FailureProxy`
//! (see `tests/chaos.rs`) — WA5 hard-coded "fail Dispatch on the Nth
//! call"; this version exposes queues for `SubmitGoal`, `Decompose`,
//! `Dispatch`, and `AbortWorkspace` so a single mock drives every
//! launch-failure scenario I1 (`launch_failure_matrix.rs`) needs.
//!
//! Every non-injection RPC (`get_ready_tasks`, `cancel_task`,
//! `suspend_workspace`, `resume_workspace`, `send_directive`,
//! `send_feedback`, `trigger_integration`, `get_allocatable`,
//! `stream_signals`) is a pure forward. If you need to inject into one
//! of those, add a queue to [`InjectionState`] following the same
//! pattern — not a deep refactor.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::transport::{Channel, Server};
use tonic::{Request, Response, Status};
use wacp_transport::wacp_v1;
use wacp_transport::wacp_v1::coordinator_service_client::CoordinatorServiceClient;
use wacp_transport::wacp_v1::coordinator_service_server::{
    CoordinatorService, CoordinatorServiceServer,
};

use crate::runtime_harness::RuntimeHarness;

/// Per-RPC failure queue. A `None` element means "let the next call
/// pass through"; `Some(status)` means "fail the next call with this
/// status". Elements are popped from the front in call order.
#[derive(Default)]
struct InjectionState {
    submit_goal: std::collections::VecDeque<Status>,
    decompose: std::collections::VecDeque<Status>,
    dispatch: std::collections::VecDeque<Status>,
    abort: std::collections::VecDeque<Status>,
}

/// Mock `CoordinatorService` server. Spawned by [`InjectableCoordinator::spawn`];
/// lives until `Drop`.
pub struct InjectableCoordinator {
    addr: SocketAddr,
    state: Arc<Mutex<InjectionState>>,
    submit_goal_count: Arc<AtomicU32>,
    decompose_count: Arc<AtomicU32>,
    dispatch_count: Arc<AtomicU32>,
    abort_count: Arc<AtomicU32>,
    handle: Option<JoinHandle<()>>,
}

impl InjectableCoordinator {
    /// Spawn the mock service in front of the runtime referenced by
    /// `rt`. The returned address is what the Console should point its
    /// `GrpcPool`'s coordinator channel at.
    pub async fn spawn(rt: &RuntimeHarness) -> std::io::Result<Self> {
        let coord_url = format!("http://{}", rt.coordinator_addr());
        let channel = Channel::from_shared(coord_url)
            .map_err(|e| std::io::Error::other(format!("bad coord url: {e}")))?
            .connect()
            .await
            .map_err(|e| std::io::Error::other(format!("connect: {e}")))?;
        let upstream = CoordinatorServiceClient::new(channel);

        let state = Arc::new(Mutex::new(InjectionState::default()));
        let submit_goal_count = Arc::new(AtomicU32::new(0));
        let decompose_count = Arc::new(AtomicU32::new(0));
        let dispatch_count = Arc::new(AtomicU32::new(0));
        let abort_count = Arc::new(AtomicU32::new(0));

        let svc = ForwardingInjector {
            upstream,
            state: state.clone(),
            submit_goal_count: submit_goal_count.clone(),
            decompose_count: decompose_count.clone(),
            dispatch_count: dispatch_count.clone(),
            abort_count: abort_count.clone(),
        };

        let listener = TcpListener::bind("[::1]:0").await?;
        let addr = listener.local_addr()?;
        let incoming = TcpListenerStream::new(listener);
        let handle = tokio::spawn(async move {
            let _ = Server::builder()
                .add_service(CoordinatorServiceServer::new(svc))
                .serve_with_incoming(incoming)
                .await;
        });

        Ok(InjectableCoordinator {
            addr,
            state,
            submit_goal_count,
            decompose_count,
            dispatch_count,
            abort_count,
            handle: Some(handle),
        })
    }

    /// Address in `[::1]:PORT` form — drop into the Console's
    /// `GrpcPool::new(..., coordinator_addr=…)` slot.
    pub fn addr(&self) -> String {
        format!("[::1]:{}", self.addr.port())
    }

    pub async fn inject_submit_goal(&self, status: Status) {
        self.state.lock().await.submit_goal.push_back(status);
    }
    pub async fn inject_decompose(&self, status: Status) {
        self.state.lock().await.decompose.push_back(status);
    }
    pub async fn inject_dispatch(&self, status: Status) {
        self.state.lock().await.dispatch.push_back(status);
    }
    pub async fn inject_abort(&self, status: Status) {
        self.state.lock().await.abort.push_back(status);
    }

    pub fn submit_goal_count(&self) -> u32 {
        self.submit_goal_count.load(Ordering::SeqCst)
    }
    pub fn decompose_count(&self) -> u32 {
        self.decompose_count.load(Ordering::SeqCst)
    }
    pub fn dispatch_count(&self) -> u32 {
        self.dispatch_count.load(Ordering::SeqCst)
    }
    pub fn abort_count(&self) -> u32 {
        self.abort_count.load(Ordering::SeqCst)
    }
}

impl Drop for InjectableCoordinator {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

struct ForwardingInjector {
    upstream: CoordinatorServiceClient<Channel>,
    state: Arc<Mutex<InjectionState>>,
    submit_goal_count: Arc<AtomicU32>,
    decompose_count: Arc<AtomicU32>,
    dispatch_count: Arc<AtomicU32>,
    abort_count: Arc<AtomicU32>,
}

#[tonic::async_trait]
impl CoordinatorService for ForwardingInjector {
    async fn submit_goal(
        &self,
        request: Request<wacp_v1::SubmitGoalRequest>,
    ) -> Result<Response<wacp_v1::SubmitGoalResponse>, Status> {
        self.submit_goal_count.fetch_add(1, Ordering::SeqCst);
        if let Some(status) = self.state.lock().await.submit_goal.pop_front() {
            return Err(status);
        }
        self.upstream.clone().submit_goal(request).await
    }

    async fn decompose(
        &self,
        request: Request<wacp_v1::DecomposeRequest>,
    ) -> Result<Response<wacp_v1::DecomposeResponse>, Status> {
        self.decompose_count.fetch_add(1, Ordering::SeqCst);
        if let Some(status) = self.state.lock().await.decompose.pop_front() {
            return Err(status);
        }
        self.upstream.clone().decompose(request).await
    }

    async fn get_ready_tasks(
        &self,
        request: Request<wacp_v1::GetReadyTasksRequest>,
    ) -> Result<Response<wacp_v1::GetReadyTasksResponse>, Status> {
        self.upstream.clone().get_ready_tasks(request).await
    }

    async fn cancel_task(
        &self,
        request: Request<wacp_v1::CancelTaskRequest>,
    ) -> Result<Response<wacp_v1::CancelTaskResponse>, Status> {
        self.upstream.clone().cancel_task(request).await
    }

    async fn dispatch(
        &self,
        request: Request<wacp_v1::DispatchRequest>,
    ) -> Result<Response<wacp_v1::DispatchResponse>, Status> {
        self.dispatch_count.fetch_add(1, Ordering::SeqCst);
        if let Some(status) = self.state.lock().await.dispatch.pop_front() {
            return Err(status);
        }
        self.upstream.clone().dispatch(request).await
    }

    async fn abort_workspace(
        &self,
        request: Request<wacp_v1::AbortWorkspaceRequest>,
    ) -> Result<Response<wacp_v1::AbortWorkspaceResponse>, Status> {
        self.abort_count.fetch_add(1, Ordering::SeqCst);
        if let Some(status) = self.state.lock().await.abort.pop_front() {
            return Err(status);
        }
        self.upstream.clone().abort_workspace(request).await
    }

    async fn suspend_workspace(
        &self,
        request: Request<wacp_v1::SuspendWorkspaceRequest>,
    ) -> Result<Response<wacp_v1::SuspendWorkspaceResponse>, Status> {
        self.upstream.clone().suspend_workspace(request).await
    }

    async fn resume_workspace(
        &self,
        request: Request<wacp_v1::ResumeWorkspaceRequest>,
    ) -> Result<Response<wacp_v1::ResumeWorkspaceResponse>, Status> {
        self.upstream.clone().resume_workspace(request).await
    }

    async fn send_directive(
        &self,
        request: Request<wacp_v1::SendDirectiveRequest>,
    ) -> Result<Response<wacp_v1::SendDirectiveResponse>, Status> {
        self.upstream.clone().send_directive(request).await
    }

    async fn send_feedback(
        &self,
        request: Request<wacp_v1::SendFeedbackRequest>,
    ) -> Result<Response<wacp_v1::SendFeedbackResponse>, Status> {
        self.upstream.clone().send_feedback(request).await
    }

    async fn trigger_integration(
        &self,
        request: Request<wacp_v1::TriggerIntegrationRequest>,
    ) -> Result<Response<wacp_v1::TriggerIntegrationResponse>, Status> {
        self.upstream.clone().trigger_integration(request).await
    }

    async fn get_allocatable(
        &self,
        request: Request<wacp_v1::GetAllocatableRequest>,
    ) -> Result<Response<wacp_v1::GetAllocatableResponse>, Status> {
        self.upstream.clone().get_allocatable(request).await
    }

    type StreamSignalsStream = ReceiverStream<Result<wacp_v1::SignalEvent, Status>>;

    async fn stream_signals(
        &self,
        request: Request<wacp_v1::StreamSignalsRequest>,
    ) -> Result<Response<Self::StreamSignalsStream>, Status> {
        let upstream = self.upstream.clone().stream_signals(request).await?;
        let mut upstream = upstream.into_inner();
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            use tokio_stream::StreamExt;
            while let Some(item) = upstream.next().await {
                if tx.send(item).await.is_err() {
                    break;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
