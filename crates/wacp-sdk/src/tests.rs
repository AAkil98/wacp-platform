use wacp_types::*;

use crate::*;

#[test]
fn agent_config_constructable() {
    let config = AgentConfig {
        runtime_url: "http://localhost:9400".into(),
        workspace_id: WorkspaceId::from("ws-1"),
        auth_token: "token".into(),
    };
    assert_eq!(config.workspace_id, WorkspaceId::from("ws-1"));
    assert_eq!(config.runtime_url, "http://localhost:9400");
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
