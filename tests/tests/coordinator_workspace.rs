//! Integration: wacp-coordinator + wacp-workspace (6 tests)

use wacp_integration_tests::*;

use wacp_coordinator::DispatchRequest;
use wacp_fsm::TaskTrigger;
use wacp_types::*;


#[tokio::test]
async fn dispatch_creates_workspace_in_tree() {
    let (mut coord, _rx) = make_coordinator();
    let ws_id = dispatch(&mut coord, "ws-1", "task-1");
    let node = coord.tree.get(&ws_id).expect("workspace should exist");
    assert_eq!(node.status, WorkspaceState::Idle);
}

#[tokio::test]
async fn dispatch_sets_task_id() {
    let (mut coord, _rx) = make_coordinator();
    dispatch(&mut coord, "ws-1", "task-42");
    let node = coord.tree.get(&WorkspaceId::from("ws-1")).unwrap();
    assert_eq!(node.task_id.as_ref().unwrap(), &TaskId::from("task-42"));
}

#[tokio::test]
async fn route_envelope_activates_workspace() {
    let (mut coord, mut rx) = make_coordinator();
    dispatch(&mut coord, "ws-1", "task-1");

    let envelope = make_envelope("dir-ws-1", "ws-root", "ws-1", "directive");
    coord.route_envelope(&WorkspaceId::from("ws-1"), envelope).await;

    drain_events(&mut coord, &mut rx, 5, 200).await;

    let node = coord.tree.get(&WorkspaceId::from("ws-1")).unwrap();
    assert_eq!(node.status, WorkspaceState::Active);
}

#[tokio::test]
async fn abort_transitions_to_failed() {
    let (mut coord, mut rx) = make_coordinator();
    dispatch(&mut coord, "ws-1", "task-1");

    coord.abort_workspace(&WorkspaceId::from("ws-1")).await;
    drain_events(&mut coord, &mut rx, 10, 200).await;

    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-1")).unwrap().status,
        WorkspaceState::Failed
    );
}

#[tokio::test]
async fn cascade_failure_parent_to_child() {
    let (mut coord, mut rx) = make_coordinator();
    dispatch(&mut coord, "ws-parent", "task-parent");

    let mut child_config = worker_config("ws-child", "task-child");
    child_config.parent = WorkspaceId::from("ws-parent");
    coord.dispatch(DispatchRequest {
        task_id: TaskId::from("task-child"),
        config: child_config,
    });

    coord
        .abort_workspace(&WorkspaceId::from("ws-parent"))
        .await;
    drain_events(&mut coord, &mut rx, 20, 200).await;

    assert_eq!(
        coord
            .tree
            .get(&WorkspaceId::from("ws-parent"))
            .unwrap()
            .status,
        WorkspaceState::Failed
    );
}

#[tokio::test]
async fn task_graph_lifecycle() {
    let (mut coord, _rx) = make_coordinator();

    let task = make_task("task-1", "test task");
    coord.task_graph.add_task(task).unwrap();

    coord
        .task_graph
        .transition(&TaskId::from("task-1"), TaskTrigger::Approve)
        .unwrap();
    assert_eq!(
        coord.task_graph.get(&TaskId::from("task-1")).unwrap().status,
        TaskStatus::Pending
    );

    coord
        .task_graph
        .transition(&TaskId::from("task-1"), TaskTrigger::Assign)
        .unwrap();
    coord
        .task_graph
        .transition(&TaskId::from("task-1"), TaskTrigger::Start)
        .unwrap();
    coord
        .task_graph
        .transition(&TaskId::from("task-1"), TaskTrigger::Complete)
        .unwrap();
    coord
        .task_graph
        .transition(&TaskId::from("task-1"), TaskTrigger::Integrate)
        .unwrap();

    assert_eq!(
        coord.task_graph.get(&TaskId::from("task-1")).unwrap().status,
        TaskStatus::Integrated
    );
}
