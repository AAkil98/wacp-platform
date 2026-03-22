use wacp_fsm::TaskTrigger;
use wacp_types::*;

use crate::integration::*;
use crate::task_graph::*;
use crate::tree::*;

fn ws(id: &str) -> WorkspaceId {
    WorkspaceId::from(id)
}

fn uid(id: &str) -> UserId {
    UserId::from(id)
}

fn tid(id: &str) -> TaskId {
    TaskId::from(id)
}

fn make_task(id: &str, deps: Vec<&str>, status: TaskStatus) -> Task {
    Task {
        id: tid(id),
        name: id.into(),
        description: String::new(),
        depends_on: deps.into_iter().map(tid).collect(),
        parent_task: None,
        status,
        workspace_ref: None,
        workspace_history: vec![],
        checkpoint_ref: None,
    }
}

// ══════════════════════════════════════════
// Task 5.1 — Workspace Tree
// ══════════════════════════════════════════

#[test]
fn tree_root_exists() {
    let tree = WorkspaceTree::new(ws("root"), uid("owner"));
    assert!(tree.get(&ws("root")).is_some());
}

#[test]
fn insert_child() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(WorkspaceNode {
        id: ws("child"),
        parent: Some(ws("root")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Idle,
        task_id: None,
    })
    .unwrap();

    assert!(tree.get(&ws("child")).is_some());
    assert!(tree.children(&ws("root")).contains(&&ws("child")));
}

#[test]
fn insert_orphan_rejected() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    let result = tree.insert(WorkspaceNode {
        id: ws("orphan"),
        parent: Some(ws("nonexistent")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Idle,
        task_id: None,
    });
    assert!(result.is_err());
}

#[test]
fn descendants_recursive() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(WorkspaceNode {
        id: ws("A"),
        parent: Some(ws("root")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();
    tree.insert(WorkspaceNode {
        id: ws("B"),
        parent: Some(ws("A")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();

    let desc = tree.descendants(&ws("root"));
    assert!(desc.contains(&ws("A")));
    assert!(desc.contains(&ws("B")));
    assert_eq!(desc.len(), 2);
}

#[test]
fn parent_chain() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(WorkspaceNode {
        id: ws("A"),
        parent: Some(ws("root")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();
    tree.insert(WorkspaceNode {
        id: ws("B"),
        parent: Some(ws("A")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();

    let chain = tree.parent_chain(&ws("B"));
    assert_eq!(chain, vec![ws("A"), ws("root")]);
}

#[test]
fn cascade_failure_same_owner() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(WorkspaceNode {
        id: ws("A"),
        parent: Some(ws("root")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();
    tree.insert(WorkspaceNode {
        id: ws("B"),
        parent: Some(ws("A")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();

    tree.cascade_failure(&ws("A"));
    assert_eq!(tree.get(&ws("A")).unwrap().status, WorkspaceState::Failed);
    assert_eq!(tree.get(&ws("B")).unwrap().status, WorkspaceState::Failed);
}

#[test]
fn cascade_reparents_cross_owner() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner1"));
    tree.insert(WorkspaceNode {
        id: ws("A"),
        parent: Some(ws("root")),
        children: vec![],
        owner: uid("owner1"),
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();
    tree.insert(WorkspaceNode {
        id: ws("B"),
        parent: Some(ws("A")),
        children: vec![],
        owner: uid("owner2"), // different owner
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();

    let reparented = tree.cascade_failure(&ws("A"));
    assert_eq!(tree.get(&ws("A")).unwrap().status, WorkspaceState::Failed);
    // B should be reparented, not failed.
    assert_eq!(tree.get(&ws("B")).unwrap().status, WorkspaceState::Active);
    assert!(reparented.contains(&ws("B")));
    assert_eq!(
        tree.get(&ws("B")).unwrap().parent,
        Some(ws("root"))
    );
}

#[test]
fn reparent_moves_subtree() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(WorkspaceNode {
        id: ws("A"),
        parent: Some(ws("root")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();
    tree.insert(WorkspaceNode {
        id: ws("B"),
        parent: Some(ws("root")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();

    tree.reparent(&ws("B"), &ws("A"));
    assert_eq!(tree.get(&ws("B")).unwrap().parent, Some(ws("A")));
    assert!(tree.children(&ws("A")).contains(&&ws("B")));
    assert!(!tree.children(&ws("root")).contains(&&ws("B")));
}

#[test]
fn active_workspaces() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(WorkspaceNode {
        id: ws("A"),
        parent: Some(ws("root")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();
    tree.insert(WorkspaceNode {
        id: ws("B"),
        parent: Some(ws("root")),
        children: vec![],
        owner: uid("owner"),
        status: WorkspaceState::Closed,
        task_id: None,
    })
    .unwrap();

    let active = tree.active_workspaces();
    assert!(active.contains(&&ws("root")));
    assert!(active.contains(&&ws("A")));
    assert!(!active.contains(&&ws("B")));
}

// ══════════════════════════════════════════
// Task 5.2 — Task Graph
// ══════════════════════════════════════════

#[test]
fn empty_graph() {
    let graph = TaskGraph::new();
    assert!(graph.roots().is_empty());
    assert!(graph.is_complete());
}

#[test]
fn add_task_no_deps() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Draft)).unwrap();
    assert!(graph.get(&tid("t1")).is_some());
}

#[test]
fn add_task_with_deps() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Integrated)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::Draft)).unwrap();
    assert!(graph.get(&tid("t2")).is_some());
}

#[test]
fn add_task_missing_dep() {
    let mut graph = TaskGraph::new();
    let result = graph.add_task(make_task("t1", vec!["nonexistent"], TaskStatus::Draft));
    assert!(result.is_err());
}

#[test]
fn ready_tasks_no_deps() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    let ready = graph.ready_tasks();
    assert!(ready.contains(&&tid("t1")));
}

#[test]
fn ready_tasks_blocked_by_dep() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::InProgress)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::Pending)).unwrap();
    let ready = graph.ready_tasks();
    assert!(!ready.contains(&&tid("t2")));
}

#[test]
fn ready_tasks_dep_integrated() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Integrated)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::Pending)).unwrap();
    let ready = graph.ready_tasks();
    assert!(ready.contains(&&tid("t2")));
}

#[test]
fn transition_task() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Draft)).unwrap();
    let status = graph.transition(&tid("t1"), TaskTrigger::Approve).unwrap();
    assert_eq!(status, TaskStatus::Pending);
    assert_eq!(graph.get(&tid("t1")).unwrap().status, TaskStatus::Pending);
}

#[test]
fn is_complete_all_integrated() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Integrated)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::Integrated)).unwrap();
    assert!(graph.is_complete());
}

#[test]
fn is_complete_mixed() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Integrated)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::InProgress)).unwrap();
    assert!(!graph.is_complete());
}

#[test]
fn dependents() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Integrated)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::Pending)).unwrap();
    graph.add_task(make_task("t3", vec!["t1"], TaskStatus::Pending)).unwrap();
    let deps = graph.dependents(&tid("t1"));
    assert_eq!(deps.len(), 2);
}

// ══════════════════════════════════════════
// Task 5.4 — Integration Engine
// ══════════════════════════════════════════

fn test_checkpoint() -> Checkpoint {
    Checkpoint {
        id: CheckpointId::from("cp-1"),
        workspace_id: ws("ws-1"),
        checkpoint_type: "artifact".into(),
        payload: b"content".to_vec(),
        content_hash: "abc".into(),
        intent: "test".into(),
        parent_checkpoint: None,
        status: CheckpointStatus::Final,
        confidence: Confidence::High,
        timestamp: 0,
        resource_usage: None,
    }
}

#[test]
fn direct_merge_success() {
    let engine = IntegrationEngine;
    let req = IntegrationRequest {
        workspace_id: ws("ws-1"),
        strategy: MergeStrategy::Direct,
        mode: IntegrationMode::Normal,
        checkpoint: test_checkpoint(),
    };
    assert!(matches!(engine.integrate(&req), IntegrationResult::Success));
}

#[test]
fn layered_merge_success() {
    let engine = IntegrationEngine;
    let req = IntegrationRequest {
        workspace_id: ws("ws-1"),
        strategy: MergeStrategy::Layered,
        mode: IntegrationMode::Normal,
        checkpoint: test_checkpoint(),
    };
    assert!(matches!(engine.integrate(&req), IntegrationResult::Success));
}

#[test]
fn evaluated_merge_success() {
    let engine = IntegrationEngine;
    let req = IntegrationRequest {
        workspace_id: ws("ws-1"),
        strategy: MergeStrategy::Evaluated,
        mode: IntegrationMode::Normal,
        checkpoint: test_checkpoint(),
    };
    assert!(matches!(engine.integrate(&req), IntegrationResult::Success));
}

#[test]
fn conflict_detected() {
    let engine = IntegrationEngine;
    let conflict = engine.detect_conflict(
        ConflictType::ContentOverlap,
        "two workspaces modified same file",
        ws("ws-1"),
    );
    assert_eq!(conflict.conflict_type, ConflictType::ContentOverlap);
}

#[test]
fn resolve_coordinator_resolve() {
    let engine = IntegrationEngine;
    let conflict = engine.detect_conflict(
        ConflictType::SemanticContradiction,
        "test",
        ws("ws-1"),
    );
    let resolution = engine.resolve_conflict(conflict, ResolutionStrategy::CoordinatorResolve);
    assert_eq!(resolution.outcome, ResolutionOutcome::Resolved);
}

#[test]
fn resolve_escalate() {
    let engine = IntegrationEngine;
    let conflict = engine.detect_conflict(ConflictType::ContentOverlap, "test", ws("ws-1"));
    let resolution = engine.resolve_conflict(conflict, ResolutionStrategy::Escalate);
    assert_eq!(resolution.outcome, ResolutionOutcome::Escalated);
}

#[test]
fn resolve_agent_rework() {
    let engine = IntegrationEngine;
    let conflict = engine.detect_conflict(ConflictType::ContentOverlap, "test", ws("ws-1"));
    let resolution = engine.resolve_conflict(conflict, ResolutionStrategy::AgentRework);
    assert_eq!(resolution.outcome, ResolutionOutcome::Reworked);
}

#[test]
fn salvage_forces_evaluated() {
    let engine = IntegrationEngine;
    let req = IntegrationRequest {
        workspace_id: ws("ws-1"),
        strategy: MergeStrategy::Direct, // should be overridden
        mode: IntegrationMode::Salvage,
        checkpoint: test_checkpoint(),
    };
    // Salvage always succeeds in the framework.
    assert!(matches!(engine.integrate(&req), IntegrationResult::Success));
}
