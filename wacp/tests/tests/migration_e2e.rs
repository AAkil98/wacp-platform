//! Integration: migration lifecycle — wacp-coordinator + wacp-workspace + wacp-fsm (3 tests)

use wacp_integration_tests::*;

use wacp_types::*;

#[tokio::test]
async fn migration_workspace_transitions() {
    let (mut coord, mut rx) = make_coordinator();
    dispatch(&mut coord, "ws-migrate", "task-m");

    // Activate workspace first
    let envelope = make_envelope("dir-m", "ws-root", "ws-migrate", "directive");
    coord
        .route_envelope(&WorkspaceId::from("ws-migrate"), envelope)
        .await;
    drain_events(&mut coord, &mut rx, 5, 200).await;

    assert_eq!(
        coord
            .tree
            .get(&WorkspaceId::from("ws-migrate"))
            .unwrap()
            .status,
        WorkspaceState::Active
    );
}

#[tokio::test]
async fn abort_from_any_non_terminal_state() {
    let (mut coord, mut rx) = make_coordinator();
    dispatch(&mut coord, "ws-abort", "task-abort");

    // Abort from Idle (no activation needed)
    coord.abort_workspace(&WorkspaceId::from("ws-abort")).await;
    drain_events(&mut coord, &mut rx, 10, 200).await;

    assert_eq!(
        coord
            .tree
            .get(&WorkspaceId::from("ws-abort"))
            .unwrap()
            .status,
        WorkspaceState::Failed
    );
}

#[tokio::test]
async fn multiple_workspace_independent_lifecycles() {
    let (mut coord, mut rx) = make_coordinator();
    dispatch(&mut coord, "ws-a", "task-a");
    dispatch(&mut coord, "ws-b", "task-b");

    // Activate ws-a only
    let env_a = make_envelope("dir-a", "ws-root", "ws-a", "directive");
    coord
        .route_envelope(&WorkspaceId::from("ws-a"), env_a)
        .await;
    drain_events(&mut coord, &mut rx, 5, 200).await;

    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-a")).unwrap().status,
        WorkspaceState::Active
    );
    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-b")).unwrap().status,
        WorkspaceState::Idle
    );

    // Abort ws-a, ws-b should stay Idle
    coord.abort_workspace(&WorkspaceId::from("ws-a")).await;
    drain_events(&mut coord, &mut rx, 10, 200).await;

    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-a")).unwrap().status,
        WorkspaceState::Failed
    );
    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-b")).unwrap().status,
        WorkspaceState::Idle
    );
}
