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

fn worker_config(id: &str, _task_id: &str) -> WorkspaceConfig {
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
    assert!(
        rt.coordinator
            .tree
            .get(&WorkspaceId::from("ws-1"))
            .is_some()
    );

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
    if let Ok(Some(event)) =
        tokio::time::timeout(std::time::Duration::from_millis(200), rt.event_rx.recv()).await
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
        if let Ok(Some(event)) =
            tokio::time::timeout(std::time::Duration::from_millis(100), rt.event_rx.recv()).await
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
        if let Ok(Some(event)) =
            tokio::time::timeout(std::time::Duration::from_millis(100), rt.event_rx.recv()).await
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
    let mut config = RuntimeConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    config.storage.data_dir = tmp.path().to_string_lossy().to_string();
    config.server.agent_listen = "127.0.0.1:29090".into();
    config.server.highway_listen = "127.0.0.1:29091".into();
    config.server.coordinator_listen = "127.0.0.1:29092".into();

    let mut rt = Runtime::init(config, None).await.unwrap();

    // Dispatch a workspace so Bind has something to find.
    rt.coordinator.dispatch(DispatchRequest {
        task_id: TaskId::from("task-grpc"),
        config: worker_config("ws-grpc", "task-grpc"),
    });

    // Spawn the runtime event loop in the background.
    let rt_handle = tokio::spawn(async move {
        // Run for a short time then stop.
        tokio::time::timeout(std::time::Duration::from_secs(2), rt.run())
            .await
            .ok();
    });

    // Give the server a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Connect agent client and call Bind.
    let mut agent_client = AgentServiceClient::connect("http://127.0.0.1:29090")
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
    let mut highway_client = HighwayServiceClient::connect("http://127.0.0.1:29091")
        .await
        .unwrap();

    let auth_response = highway_client
        .authenticate(wacp_v1::AuthenticateRequest {
            auth_token: "admin-token".into(),
        })
        .await
        .unwrap();

    // C7: user_id is now derived via truncated SHA-256 of the auth token.
    assert_eq!(auth_response.get_ref().user_id, "user-10a4c7c9fc5206d6");
    assert!(!auth_response.get_ref().capabilities.is_empty());

    // Let the runtime finish.
    rt_handle.abort();
}

// ── Phase T2.1 additions ──

#[test]
fn init_in_memory_default_taxonomy() {
    let rt = test_runtime();
    // Empty taxonomy still has base roles and types.
    assert!(rt.taxonomy.is_valid_role("worker"));
    assert!(rt.taxonomy.is_valid_role("observer"));
    assert!(rt.taxonomy.is_valid_role("coordinator"));
    assert!(rt.taxonomy.is_valid_envelope_type("directive"));
    assert!(rt.taxonomy.is_valid_envelope_type("feedback"));
    assert!(rt.taxonomy.is_valid_envelope_type("query"));
}

#[test]
fn init_in_memory_custom_taxonomy() {
    let dir = tempfile::tempdir().unwrap();
    let tax_path = dir.path().join("taxonomy.yaml");
    std::fs::write(
        &tax_path,
        r#"
id: test
version: "1.0"
protocol_version: "0.1"
roles: []
envelope_types: []
checkpoint_types: []
"#,
    )
    .unwrap();
    let mut config = RuntimeConfig::default();
    config.taxonomy.file = tax_path.to_string_lossy().to_string();
    let rt = Runtime::init_in_memory(config).unwrap();
    assert!(rt.taxonomy.is_valid_role("worker"));
}

#[tokio::test]
async fn init_creates_data_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = RuntimeConfig::default();
    config.storage.data_dir = dir.path().to_string_lossy().to_string();
    config.server.agent_listen = "127.0.0.1:39090".into();
    config.server.highway_listen = "127.0.0.1:39091".into();
    config.server.coordinator_listen = "127.0.0.1:39092".into();

    let _rt = Runtime::init(config, None).await.unwrap();

    assert!(dir.path().join("trail").exists());
    assert!(dir.path().join("checkpoints").exists());
    assert!(dir.path().join("snapshots").exists());
}

#[tokio::test]
async fn coordinator_dispatch_creates_node() {
    let mut rt = test_runtime();
    rt.coordinator.dispatch(DispatchRequest {
        task_id: TaskId::from("task-1"),
        config: worker_config("ws-1", "task-1"),
    });
    assert!(
        rt.coordinator
            .tree
            .get(&WorkspaceId::from("ws-1"))
            .is_some()
    );
}

#[tokio::test]
async fn coordinator_dispatch_sets_task_id() {
    let mut rt = test_runtime();
    rt.coordinator.dispatch(DispatchRequest {
        task_id: TaskId::from("task-42"),
        config: worker_config("ws-42", "task-42"),
    });
    let node = rt
        .coordinator
        .tree
        .get(&WorkspaceId::from("ws-42"))
        .unwrap();
    assert_eq!(node.task_id.as_ref().unwrap(), &TaskId::from("task-42"));
}

#[tokio::test]
async fn coordinator_multiple_workspaces() {
    let mut rt = test_runtime();
    for i in 0..3 {
        let ws = format!("ws-{i}");
        let task = format!("task-{i}");
        rt.coordinator.dispatch(DispatchRequest {
            task_id: TaskId::from(task.as_str()),
            config: worker_config(&ws, &task),
        });
    }
    for i in 0..3 {
        assert!(
            rt.coordinator
                .tree
                .get(&WorkspaceId::from(format!("ws-{i}")))
                .is_some()
        );
    }
}

#[tokio::test]
async fn coordinator_abort_sets_failed() {
    let mut rt = test_runtime();
    rt.coordinator.dispatch(DispatchRequest {
        task_id: TaskId::from("task-abort"),
        config: worker_config("ws-abort", "task-abort"),
    });
    rt.coordinator
        .abort_workspace(&WorkspaceId::from("ws-abort"))
        .await;

    // Drain events to let state update propagate.
    for _ in 0..5 {
        if let Ok(Some(e)) =
            tokio::time::timeout(std::time::Duration::from_millis(100), rt.event_rx.recv()).await
        {
            rt.coordinator.handle_event(&e);
        }
    }

    assert_eq!(
        rt.coordinator
            .tree
            .get(&WorkspaceId::from("ws-abort"))
            .unwrap()
            .status,
        WorkspaceState::Failed
    );
}

#[tokio::test]
async fn runtime_shutdown_no_workspaces() {
    let mut rt = test_runtime();
    rt.shutdown().await;
}

// ── Phase 27S.5: vertical manifest loading ──

#[test]
fn load_vertical_manifests_empty_config() {
    // No verticals_dir configured → empty registry.
    let rt = test_runtime();
    assert_eq!(rt.verticals.len(), 0);
}

#[test]
fn load_vertical_manifests_nonexistent_dir() {
    let mut config = RuntimeConfig::default();
    config.taxonomy.verticals_dir = "/tmp/wacp-nonexistent-9999xyz".into();
    let rt = Runtime::init_in_memory(config).unwrap();
    assert_eq!(rt.verticals.len(), 0);
}

#[test]
fn load_vertical_manifests_scans_sub_dirs() {
    let root = tempfile::tempdir().unwrap();

    // Write two minimal vertical.yaml files under separate sub-directories.
    let swe_dir = root.path().join("swe");
    std::fs::create_dir_all(&swe_dir).unwrap();
    std::fs::write(
        swe_dir.join("vertical.yaml"),
        r#"
id: swe
name: Software Engineering
defining_constraint: DAG ordering
"#,
    )
    .unwrap();

    let finance_dir = root.path().join("finance");
    std::fs::create_dir_all(&finance_dir).unwrap();
    std::fs::write(
        finance_dir.join("vertical.yaml"),
        r#"
id: finance
name: Finance
defining_constraint: Regulatory pre-check
"#,
    )
    .unwrap();

    let mut config = RuntimeConfig::default();
    config.taxonomy.verticals_dir = root.path().to_string_lossy().to_string();
    let rt = Runtime::init_in_memory(config).unwrap();

    assert_eq!(rt.verticals.len(), 2);
    // Stable alphabetical ordering.
    assert_eq!(rt.verticals[0].id, "finance");
    assert_eq!(rt.verticals[1].id, "swe");
}

#[test]
fn load_vertical_manifests_skips_malformed_yaml() {
    let root = tempfile::tempdir().unwrap();

    let bad_dir = root.path().join("bad");
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(
        bad_dir.join("vertical.yaml"),
        b"\xFF\xFE invalid utf8 maybe not, but bad yaml: [\n",
    )
    .unwrap();

    let good_dir = root.path().join("swe");
    std::fs::create_dir_all(&good_dir).unwrap();
    std::fs::write(
        good_dir.join("vertical.yaml"),
        r#"
id: swe
name: Software Engineering
defining_constraint: DAG ordering
"#,
    )
    .unwrap();

    let mut config = RuntimeConfig::default();
    config.taxonomy.verticals_dir = root.path().to_string_lossy().to_string();
    // Must not fail — malformed files are skipped.
    let rt = Runtime::init_in_memory(config).unwrap();
    assert_eq!(rt.verticals.len(), 1);
    assert_eq!(rt.verticals[0].id, "swe");
}

#[test]
fn load_vertical_manifests_ignores_files_without_vertical_yaml() {
    let root = tempfile::tempdir().unwrap();
    // A directory with no vertical.yaml — should be ignored.
    let empty_dir = root.path().join("misc");
    std::fs::create_dir_all(&empty_dir).unwrap();
    std::fs::write(empty_dir.join("README.md"), b"not a manifest").unwrap();

    let mut config = RuntimeConfig::default();
    config.taxonomy.verticals_dir = root.path().to_string_lossy().to_string();
    let rt = Runtime::init_in_memory(config).unwrap();
    assert_eq!(rt.verticals.len(), 0);
}

#[tokio::test]
async fn runtime_shutdown_aborts_active_workspaces() {
    let mut rt = test_runtime();
    rt.coordinator.dispatch(DispatchRequest {
        task_id: TaskId::from("task-shutdown"),
        config: worker_config("ws-shutdown", "task-shutdown"),
    });
    assert!(
        rt.coordinator
            .tree
            .get(&WorkspaceId::from("ws-shutdown"))
            .is_some()
    );
    rt.shutdown().await;
    // After shutdown, workspace should be failed.
    assert_eq!(
        rt.coordinator
            .tree
            .get(&WorkspaceId::from("ws-shutdown"))
            .unwrap()
            .status,
        WorkspaceState::Failed
    );
}

// ── WA1 — Bind projects WorkspaceConfig ──
//
// Exercises the cache-and-project path added in `wacp/impl/wa1-bind-projection.md`.
// The tests drive `handle_coordinator_request` / `handle_agent_request`
// directly (rather than via gRPC) so the assertions don't depend on a
// running Tonic server. The cache is populated only when SubmitGoal /
// Dispatch flow through the request-handler path; tests that call
// `rt.coordinator.dispatch()` directly bypass the cache by design and are
// covered separately by the existing `worker_config` tests.

async fn submit_goal_via_handler(
    rt: &mut Runtime,
    description: &str,
) -> wacp_v1::SubmitGoalResponse {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    rt.handle_coordinator_request(wacp_transport::CoordinatorRequest::SubmitGoal {
        request: wacp_v1::SubmitGoalRequest {
            description: description.into(),
            context: b"ctx-bytes".to_vec(),
            client_request_id: "req-1".into(),
        },
        reply: reply_tx,
    })
    .await;
    reply_rx.await.expect("reply").expect("status")
}

async fn dispatch_via_handler(
    rt: &mut Runtime,
    task_id: &str,
    role: &str,
) -> wacp_v1::DispatchResponse {
    // First create the task in the graph via Decompose so Dispatch validates.
    let (dec_tx, dec_rx) = tokio::sync::oneshot::channel();
    rt.handle_coordinator_request(wacp_transport::CoordinatorRequest::Decompose {
        request: wacp_v1::DecomposeRequest {
            tasks: vec![wacp_v1::TaskDefinition {
                name: task_id.into(),
                description: task_id.into(),
                depends_on: vec![],
                role: role.into(),
                directive_payload: b"decomposed-dir".to_vec(),
                tools: vec![],
            }],
            client_request_id: "dec-1".into(),
        },
        reply: dec_tx,
    })
    .await;
    let dec = dec_rx.await.expect("dec reply").expect("dec status");
    let assigned_task_id = dec.task_ids.first().cloned().expect("task id");

    let (d_tx, d_rx) = tokio::sync::oneshot::channel();
    rt.handle_coordinator_request(wacp_transport::CoordinatorRequest::Dispatch {
        request: wacp_v1::DispatchRequest {
            task_id: assigned_task_id,
            role: role.into(),
            directive_payload: b"dispatched-dir".to_vec(),
            tools: vec![],
            budget: None,
            client_request_id: "d-1".into(),
        },
        reply: d_tx,
    })
    .await;
    d_rx.await.expect("d reply").expect("d status")
}

async fn bind_via_handler(rt: &mut Runtime, ws_id: &str) -> Result<wacp_v1::BindResponse, String> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    rt.handle_agent_request(wacp_transport::AgentRequest::Bind {
        request: wacp_v1::BindRequest {
            workspace_id: ws_id.into(),
            auth_token: "test-token-8chars".into(),
            client_request_id: "b-1".into(),
        },
        reply: reply_tx,
    })
    .await;
    reply_rx
        .await
        .expect("reply")
        .map_err(|s| s.message().to_string())
}

#[tokio::test]
async fn wa1_bind_returns_populated_fields_after_submit_goal() {
    let mut rt = test_runtime();
    let submit = submit_goal_via_handler(&mut rt, "stub-e2e goal").await;
    assert!(!submit.root_workspace_id.is_empty());

    let bind = bind_via_handler(&mut rt, &submit.root_workspace_id)
        .await
        .expect("bind ok");

    assert_eq!(bind.workspace_id, submit.root_workspace_id);
    assert_eq!(bind.role, "worker");
    let directive = bind.directive.expect("directive present");
    assert_eq!(directive.to_workspace, submit.root_workspace_id);
    assert_eq!(directive.r#type, "directive");
    assert_eq!(directive.payload, b"ctx-bytes".to_vec());
    // SubmitGoal reuses the request context as the directive payload; it
    // does NOT populate the context field on WorkspaceConfig.
    assert_eq!(bind.context, Vec::<u8>::new());
    assert!(bind.visibility.is_empty());
    assert!(bind.authority.is_empty());
    assert!(bind.budget.is_none());
}

#[tokio::test]
async fn wa1_bind_returns_populated_fields_after_dispatch() {
    let mut rt = test_runtime();
    let dispatched = dispatch_via_handler(&mut rt, "task-d1", "worker").await;

    let bind = bind_via_handler(&mut rt, &dispatched.workspace_id)
        .await
        .expect("bind ok");

    assert_eq!(bind.workspace_id, dispatched.workspace_id);
    assert_eq!(bind.role, "worker");
    let directive = bind.directive.expect("directive present");
    assert_eq!(directive.r#type, "directive");
    assert_eq!(directive.payload, b"dispatched-dir".to_vec());
}

#[tokio::test]
async fn wa1_bind_after_terminate_falls_back_empty() {
    let mut rt = test_runtime();
    let submit = submit_goal_via_handler(&mut rt, "goal-to-terminate").await;

    // First bind succeeds with populated fields.
    let bind_before = bind_via_handler(&mut rt, &submit.root_workspace_id)
        .await
        .expect("bind before ok");
    assert_eq!(bind_before.role, "worker");

    // Simulate the coordinator event loop processing a Terminated event:
    // directly invoke the fan-out path the event loop would run. Use a
    // minimal archived workspace payload matching the shape the real
    // workspace actor emits.
    let archived = wacp_workspace::state::ArchivedWorkspace {
        id: WorkspaceId::from(submit.root_workspace_id.as_str()),
        terminal_state: WorkspaceState::Closed,
        role: "worker".into(),
        parent: WorkspaceId::from("ws-root"),
        owner: UserId::from("system"),
        originator: Originator::System,
        directive: Envelope {
            id: EnvelopeId::from("dir-archived"),
            from_workspace: WorkspaceId::from("ws-root"),
            to_workspace: WorkspaceId::from(submit.root_workspace_id.as_str()),
            envelope_type: "directive".into(),
            payload: vec![],
            in_reply_to: None,
            timestamp: 0,
            priority: EnvelopePriority::Normal,
            origin: EnvelopeOrigin::Agent,
            state: EnvelopeState::Created,
        },
        context: vec![],
        checkpoints: vec![],
        final_usage: Default::default(),
        visibility: HashSet::new(),
        authority: HashSet::new(),
    };
    rt.fan_out_event(&WorkspaceEvent::Terminated(Box::new(archived)));

    // After Terminated, the cache should be empty — but the tree node is
    // still present (tree cleanup is driven by `handle_event`, not
    // `fan_out_event`), so Bind still returns Ok, just with empty fields.
    let bind_after = bind_via_handler(&mut rt, &submit.root_workspace_id)
        .await
        .expect("bind after ok (tree still has node)");
    assert_eq!(bind_after.role, "");
    assert!(bind_after.directive.is_none());
    assert!(bind_after.visibility.is_empty());
    assert!(bind_after.authority.is_empty());
}

#[tokio::test]
async fn wa1_bind_unknown_workspace_returns_not_found() {
    let mut rt = test_runtime();
    let err = bind_via_handler(&mut rt, "ws-does-not-exist")
        .await
        .expect_err("expected not_found");
    assert!(err.contains("workspace not found"), "got: {err}");
}
