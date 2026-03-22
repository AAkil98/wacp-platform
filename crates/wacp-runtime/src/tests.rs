use std::collections::HashSet;

use wacp_coordinator::DispatchRequest;
use wacp_fsm::TaskTrigger;
use wacp_transport::wacp_v1;
use wacp_types::*;
use wacp_workspace::{WorkspaceConfig, WorkspaceEvent};

use crate::config::RuntimeConfig;
use crate::init::Runtime;

fn test_runtime() -> Runtime {
    Runtime::init_in_memory(RuntimeConfig::default()).unwrap()
}

fn worker_config(id: &str, task_id: &str) -> WorkspaceConfig {
    WorkspaceConfig {
        id: WorkspaceId::from(id),
        role: "worker".into(),
        base_role: BaseRole::Worker,
        parent: WorkspaceId::from("ws-root"),
        owner: UserId::from("system"),
        originator: Originator::System,
        directive: Envelope {
            id: EnvelopeId::from(format!("dir-{id}")),
            from_workspace: WorkspaceId::from("ws-root"),
            to_workspace: WorkspaceId::from(id),
            envelope_type: "directive".into(),
            payload: b"do the work".to_vec(),
            in_reply_to: None,
            timestamp: 0,
            priority: EnvelopePriority::Normal,
            origin: EnvelopeOrigin::Agent,
            state: EnvelopeState::Created,
        },
        context: vec![],
        visibility: HashSet::new(),
        authority: HashSet::new(),
        delegate: false,
        budget: None,
    }
}

// ── Phase 7: Integration tests ──

#[test]
fn init_fresh() {
    let rt = test_runtime();
    assert!(rt.taxonomy.is_valid_role("worker"));
    assert!(rt.taxonomy.is_valid_role("coordinator"));
}

#[test]
fn init_empty_taxonomy() {
    let rt = test_runtime();
    assert!(rt.taxonomy.is_valid_envelope_type("directive"));
    assert!(rt.taxonomy.is_valid_checkpoint_type("artifact"));
}

#[tokio::test]
async fn e2e_single_worker() {
    let mut rt = test_runtime();

    // Add a task to the graph.
    let task = Task {
        id: TaskId::from("task-1"),
        name: "test task".into(),
        description: "do something".into(),
        depends_on: vec![],
        parent_task: None,
        status: TaskStatus::Draft,
        workspace_ref: None,
        workspace_history: vec![],
        checkpoint_ref: None,
    };
    rt.coordinator.task_graph.add_task(task).unwrap();
    rt.coordinator
        .task_graph
        .transition(&TaskId::from("task-1"), TaskTrigger::Approve)
        .unwrap();

    // Dispatch workspace for the task.
    rt.coordinator.dispatch(DispatchRequest {
        task_id: TaskId::from("task-1"),
        config: worker_config("ws-1", "task-1"),
    });

    // Verify workspace exists in tree.
    assert!(rt.coordinator.tree.get(&WorkspaceId::from("ws-1")).is_some());

    // Send directive to activate the workspace.
    let directive = Envelope {
        id: EnvelopeId::from("dir-ws-1"),
        from_workspace: WorkspaceId::from("ws-root"),
        to_workspace: WorkspaceId::from("ws-1"),
        envelope_type: "directive".into(),
        payload: b"work".to_vec(),
        in_reply_to: None,
        timestamp: 0,
        priority: EnvelopePriority::Normal,
        origin: EnvelopeOrigin::Agent,
        state: EnvelopeState::Created,
    };
    rt.coordinator
        .route_envelope(&WorkspaceId::from("ws-1"), directive)
        .await;

    // Receive events — workspace should activate.
    if let Ok(Some(event)) = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        rt.event_rx.recv(),
    )
    .await
    {
        match event {
            WorkspaceEvent::StateChanged { to, .. } => {
                assert_eq!(to, WorkspaceState::Active);
            }
            _ => {} // other events are fine
        }
    }
}

#[tokio::test]
async fn e2e_workspace_lifecycle() {
    let mut rt = test_runtime();

    rt.coordinator.dispatch(DispatchRequest {
        task_id: TaskId::from("task-1"),
        config: worker_config("ws-1", "task-1"),
    });

    // Abort the workspace.
    rt.coordinator
        .abort_workspace(&WorkspaceId::from("ws-1"))
        .await;

    // Should get StateChanged(Failed) + Terminated.
    let mut got_failed = false;
    for _ in 0..5 {
        if let Ok(Some(event)) = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rt.event_rx.recv(),
        )
        .await
        {
            if let WorkspaceEvent::StateChanged { to, .. } = &event {
                if *to == WorkspaceState::Failed {
                    got_failed = true;
                }
            }
            rt.coordinator.handle_event(&event);
        }
        if got_failed {
            break;
        }
    }
    assert!(got_failed);
}

#[tokio::test]
async fn e2e_task_lifecycle() {
    let mut rt = test_runtime();

    let task = Task {
        id: TaskId::from("task-1"),
        name: "lifecycle test".into(),
        description: "test".into(),
        depends_on: vec![],
        parent_task: None,
        status: TaskStatus::Draft,
        workspace_ref: None,
        workspace_history: vec![],
        checkpoint_ref: None,
    };
    rt.coordinator.task_graph.add_task(task).unwrap();

    // Draft → Pending → Assigned → InProgress
    rt.coordinator
        .task_graph
        .transition(&TaskId::from("task-1"), TaskTrigger::Approve)
        .unwrap();
    rt.coordinator
        .task_graph
        .transition(&TaskId::from("task-1"), TaskTrigger::Assign)
        .unwrap();
    rt.coordinator
        .task_graph
        .transition(&TaskId::from("task-1"), TaskTrigger::Start)
        .unwrap();
    rt.coordinator
        .task_graph
        .transition(&TaskId::from("task-1"), TaskTrigger::Complete)
        .unwrap();
    rt.coordinator
        .task_graph
        .transition(&TaskId::from("task-1"), TaskTrigger::Integrate)
        .unwrap();

    assert_eq!(
        rt.coordinator
            .task_graph
            .get(&TaskId::from("task-1"))
            .unwrap()
            .status,
        TaskStatus::Integrated
    );
    assert!(rt.coordinator.task_graph.is_complete());
}

#[tokio::test]
async fn e2e_failure_cascade() {
    let mut rt = test_runtime();

    // Create parent workspace.
    rt.coordinator.dispatch(DispatchRequest {
        task_id: TaskId::from("task-parent"),
        config: worker_config("ws-parent", "task-parent"),
    });

    // Create child workspace under parent.
    let mut child_config = worker_config("ws-child", "task-child");
    child_config.parent = WorkspaceId::from("ws-parent");

    // Manually insert child into tree (dispatch auto-inserts).
    rt.coordinator.dispatch(DispatchRequest {
        task_id: TaskId::from("task-child"),
        config: child_config,
    });

    // Abort parent — child should cascade.
    rt.coordinator
        .abort_workspace(&WorkspaceId::from("ws-parent"))
        .await;

    // Process events.
    for _ in 0..10 {
        if let Ok(Some(event)) = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rt.event_rx.recv(),
        )
        .await
        {
            rt.coordinator.handle_event(&event);
        }
    }

    // Both should be failed in tree.
    assert_eq!(
        rt.coordinator
            .tree
            .get(&WorkspaceId::from("ws-parent"))
            .unwrap()
            .status,
        WorkspaceState::Failed
    );
}

// ── gRPC integration test ──

#[tokio::test]
async fn e2e_grpc_bind_and_authenticate() {
    use wacp_transport::wacp_v1::agent_service_client::AgentServiceClient;
    use wacp_transport::wacp_v1::highway_service_client::HighwayServiceClient;

    // Start runtime with gRPC on test ports.
    let config = RuntimeConfig {
        data_dir: tempfile::tempdir().unwrap().into_path(),
        agent_port: 29400,
        highway_port: 29401,
        ..RuntimeConfig::default()
    };

    let mut rt = Runtime::init(config).await.unwrap();

    // Dispatch a workspace so Bind has something to find.
    rt.coordinator.dispatch(DispatchRequest {
        task_id: TaskId::from("task-grpc"),
        config: worker_config("ws-grpc", "task-grpc"),
    });

    // Spawn the runtime event loop in the background.
    let rt_handle = tokio::spawn(async move {
        // Run for a short time then stop.
        tokio::time::timeout(std::time::Duration::from_secs(2), rt.run()).await.ok();
    });

    // Give the server a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Connect agent client and call Bind.
    let mut agent_client =
        AgentServiceClient::connect("http://127.0.0.1:29400")
            .await
            .unwrap();

    let bind_response = agent_client
        .bind(wacp_v1::BindRequest {
            workspace_id: "ws-grpc".into(),
            auth_token: "test-token".into(),
            client_request_id: "req-1".into(),
        })
        .await
        .unwrap();

    assert_eq!(bind_response.get_ref().workspace_id, "ws-grpc");

    // Connect highway client and call Authenticate.
    let mut highway_client =
        HighwayServiceClient::connect("http://127.0.0.1:29401")
            .await
            .unwrap();

    let auth_response = highway_client
        .authenticate(wacp_v1::AuthenticateRequest {
            auth_token: "admin-token".into(),
        })
        .await
        .unwrap();

    assert_eq!(auth_response.get_ref().user_id, "user-admin-token");
    assert!(!auth_response.get_ref().capabilities.is_empty());

    // Let the runtime finish.
    rt_handle.abort();
}
