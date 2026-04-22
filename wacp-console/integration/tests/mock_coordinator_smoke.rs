//! Smoke test for the generic [`InjectableCoordinator`] mock.
//!
//! Proves both the forward path (empty injection queue → pass through to
//! the real runtime) and the inject path (pushed `Status` short-circuits
//! the forward). The I1 `launch_failure_matrix` suite relies on both.

use console_integration::{InjectableCoordinator, RuntimeHarness};
use tonic::transport::Channel;
use tonic::{Code, Status};
use wacp_transport::wacp_v1;
use wacp_transport::wacp_v1::coordinator_service_client::CoordinatorServiceClient;

async fn client_for(addr: &str) -> CoordinatorServiceClient<Channel> {
    let url = format!("http://{addr}");
    let channel = Channel::from_shared(url)
        .expect("shared url")
        .connect()
        .await
        .expect("connect mock");
    CoordinatorServiceClient::new(channel)
}

#[tokio::test]
async fn forwards_when_no_injection() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let mock = InjectableCoordinator::spawn(rt.coordinator_addr())
        .await
        .expect("mock");
    let mut client = client_for(&mock.addr()).await;

    // SubmitGoal against a forwarding mock should reach the real runtime
    // and return a non-empty root_workspace_id — proof the forward leg works.
    let resp = client
        .submit_goal(wacp_v1::SubmitGoalRequest {
            description: "smoke-forward".into(),
            context: Vec::new(),
            client_request_id: String::new(),
        })
        .await
        .expect("submit_goal forward")
        .into_inner();
    assert!(!resp.root_workspace_id.is_empty());
    assert_eq!(mock.submit_goal_count(), 1);
    assert_eq!(mock.dispatch_count(), 0);
}

#[tokio::test]
async fn injects_status_when_queued() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let mock = InjectableCoordinator::spawn(rt.coordinator_addr())
        .await
        .expect("mock");
    let mut client = client_for(&mock.addr()).await;

    // Push one injection; the next call fails, a follow-up call forwards.
    mock.inject_submit_goal(Status::unavailable("injected-boom"))
        .await;

    let err = client
        .submit_goal(wacp_v1::SubmitGoalRequest {
            description: "smoke-inject".into(),
            context: Vec::new(),
            client_request_id: String::new(),
        })
        .await
        .expect_err("injected submit_goal");
    assert_eq!(err.code(), Code::Unavailable);
    assert!(err.message().contains("injected-boom"));

    // Queue drained — second call should forward normally.
    let resp = client
        .submit_goal(wacp_v1::SubmitGoalRequest {
            description: "smoke-inject-drained".into(),
            context: Vec::new(),
            client_request_id: String::new(),
        })
        .await
        .expect("forward after drain")
        .into_inner();
    assert!(!resp.root_workspace_id.is_empty());
    assert_eq!(mock.submit_goal_count(), 2);
}
