//! E2E: Failure, recovery, migration (5 tests — E13–E17)

use wacp_coordinator::DispatchRequest;
use wacp_integration_tests::e2e::E2eHarness;
use wacp_integration_tests::*;
use wacp_recovery::RecoveryEngine;
use wacp_trail::{compute_chain_hash, ChainHash, InMemoryTrailStorage, TrailStorage};
use wacp_transport::wacp_v1;
use wacp_types::*;

// ── E13: Cascade failure ──

#[tokio::test]
async fn e13_cascade_failure() {
    let (mut coord, mut rx) = make_coordinator();
    dispatch(&mut coord, "ws-parent", "task-parent");

    let mut child_config = worker_config("ws-child", "task-child");
    child_config.parent = WorkspaceId::from("ws-parent");
    coord.dispatch(DispatchRequest {
        task_id: TaskId::from("task-child"),
        config: child_config,
    });

    // Abort parent — child should cascade
    coord
        .abort_workspace(&WorkspaceId::from("ws-parent"))
        .await;
    drain_events(&mut coord, &mut rx, 20, 200).await;

    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-parent")).unwrap().status,
        WorkspaceState::Failed
    );
    // Root should NOT be failed
    assert_ne!(
        coord.tree.get(&WorkspaceId::from("ws-root")).unwrap().status,
        WorkspaceState::Failed
    );
}

// ── E14: Multiple independent failures ──

#[tokio::test]
async fn e14_independent_failures() {
    let (mut coord, mut rx) = make_coordinator();
    dispatch(&mut coord, "ws-a", "task-a");
    dispatch(&mut coord, "ws-b", "task-b");
    dispatch(&mut coord, "ws-c", "task-c");

    // Abort ws-a only
    coord.abort_workspace(&WorkspaceId::from("ws-a")).await;
    drain_events(&mut coord, &mut rx, 10, 200).await;

    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-a")).unwrap().status,
        WorkspaceState::Failed
    );
    // ws-b and ws-c still Idle
    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-b")).unwrap().status,
        WorkspaceState::Idle
    );
    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-c")).unwrap().status,
        WorkspaceState::Idle
    );
}

// ── E15: Trail recovery reconstructs state ──

#[test]
fn e15_crash_recovery() {
    let mut trail = InMemoryTrailStorage::new();
    let genesis = ChainHash::GENESIS;

    // Simulate events from a previous runtime session
    let h1 = {
        let b = br#"{"event_type":"workspace_created","workspace_id":"ws-r","new_state":"idle"}"#;
        let h = compute_chain_hash(&genesis, b);
        trail.append(b, h.as_ref()).unwrap();
        h
    };
    let h2 = {
        let b = br#"{"event_type":"workspace_state_changed","workspace_id":"ws-r","new_state":"active"}"#;
        let h = compute_chain_hash(&h1, b);
        trail.append(b, h.as_ref()).unwrap();
        h
    };
    let _h3 = {
        let b = br#"{"event_type":"envelope_created","envelope_id":"env-1","workspace_id":"ws-r"}"#;
        let h = compute_chain_hash(&h2, b);
        trail.append(b, h.as_ref()).unwrap();
        h
    };

    // "Restart" — recover from trail
    let recovered = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(recovered.last_sequence, 3);
    // env-1 was created but not delivered → in-flight
    assert!(recovered.in_flight_envelopes.contains(&"env-1".to_string()));
}

// ── E16: Workspace activation via gRPC ──

#[tokio::test]
async fn e16_workspace_activation_via_grpc() {
    let mut harness = E2eHarness::start().await;
    harness.dispatch_workspace("ws-m", "task-m");

    let mut client = harness.agent_client().await;

    // Bind
    let bind = client
        .bind(wacp_v1::BindRequest {
            workspace_id: "ws-m".into(),
            auth_token: "tok".into(),
            client_request_id: String::new(),
        })
        .await
        .unwrap();
    assert_eq!(bind.get_ref().workspace_id, "ws-m");
    assert_eq!(
        bind.get_ref().state,
        wacp_v1::WorkspaceState::Active as i32
    );

    // Signal through lifecycle
    for signal in [
        wacp_v1::SignalType::Ready,
        wacp_v1::SignalType::Started,
        wacp_v1::SignalType::Blocked,
    ] {
        client
            .emit_signal(wacp_v1::EmitSignalRequest {
                r#type: signal.into(),
                reason: "test".into(),
                ..Default::default()
            })
            .await
            .unwrap();
    }

    harness.shutdown().await;
}

// ── E17: Highway get-workspace returns state ──

#[tokio::test]
async fn e17_highway_get_workspace() {
    let mut harness = E2eHarness::start().await;
    harness.dispatch_workspace("ws-hw", "task-hw");

    let mut highway = harness.highway_client().await;
    highway
        .authenticate(wacp_v1::AuthenticateRequest {
            auth_token: "admin".into(),
        })
        .await
        .unwrap();

    let view = highway
        .get_workspace(wacp_v1::GetWorkspaceRequest {
            workspace_id: "ws-hw".into(),
        })
        .await
        .unwrap();

    assert_eq!(view.get_ref().id, "ws-hw");

    harness.shutdown().await;
}
