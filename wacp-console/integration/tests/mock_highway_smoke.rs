//! Smoke test for `MockHighwayServer` + `HighwayConfig::script_workspace`.
//! Proves the spawn helper wires a server that responds to scripted
//! `GetWorkspace` calls with the scripted state and `not_found` for
//! unscripted IDs. Underpins §13.2 P3+ tests.

use console_test_support::MockHighwayServer;
use wacp_transport::wacp_v1 as proto;
use wacp_transport::wacp_v1::highway_service_client::HighwayServiceClient;

async fn client(server: &MockHighwayServer) -> HighwayServiceClient<tonic::transport::Channel> {
    let url = format!("http://{}", server.addr());
    let channel = tonic::transport::Channel::from_shared(url)
        .expect("valid url")
        .connect()
        .await
        .expect("connect to mock highway");
    HighwayServiceClient::new(channel)
}

#[tokio::test]
async fn get_workspace_returns_scripted_state_for_known_id() {
    let server = MockHighwayServer::spawn().await.expect("spawn");
    server
        .config()
        .script_workspace("ws-failed-1", proto::WorkspaceState::Failed);

    let mut client = client(&server).await;
    let response = client
        .get_workspace(proto::GetWorkspaceRequest {
            workspace_id: "ws-failed-1".into(),
        })
        .await
        .expect("get_workspace ok")
        .into_inner();

    assert_eq!(response.id, "ws-failed-1");
    assert_eq!(response.state, proto::WorkspaceState::Failed as i32);
}

#[tokio::test]
async fn get_workspace_returns_not_found_for_unscripted_id() {
    let server = MockHighwayServer::spawn().await.expect("spawn");

    let mut client = client(&server).await;
    let err = client
        .get_workspace(proto::GetWorkspaceRequest {
            workspace_id: "ws-unknown".into(),
        })
        .await
        .expect_err("expected not_found for unscripted workspace");

    assert_eq!(err.code(), tonic::Code::NotFound);
}
