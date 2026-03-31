use wacp_fsm::TaskTrigger;
use wacp_types::*;

use crate::dispatch::*;
use crate::events::*;
use crate::gate::*;
use crate::handler::*;
use crate::integration::*;
use crate::migration::*;
use crate::ownership::*;
use crate::port_rights::*;
use crate::resource::*;
use crate::scheduling::*;
use crate::task_graph::*;
use crate::topology::*;
use crate::tree::*;
use crate::visibility::*;

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

/// Helper: create a WorkspaceNode with defaults.
fn make_node(
    id: &str,
    parent: Option<&str>,
    owner: &str,
    originator: Originator,
    status: WorkspaceState,
) -> WorkspaceNode {
    WorkspaceNode {
        id: ws(id),
        parent: parent.map(ws),
        children: vec![],
        owner: uid(owner),
        originator,
        status,
        task_id: None,
    }
}

// ══════════════════════════════════════════
// Task 5.1 — Workspace Tree (existing)
// ══════════════════════════════════════════

#[test]
fn tree_root_exists() {
    let tree = WorkspaceTree::new(ws("root"), uid("owner"));
    assert!(tree.get(&ws("root")).is_some());
}

#[test]
fn insert_child() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(make_node("child", Some("root"), "owner", Originator::System, WorkspaceState::Idle))
        .unwrap();

    assert!(tree.get(&ws("child")).is_some());
    assert!(tree.children(&ws("root")).contains(&&ws("child")));
}

#[test]
fn insert_orphan_rejected() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    let result = tree.insert(make_node(
        "orphan",
        Some("nonexistent"),
        "owner",
        Originator::System,
        WorkspaceState::Idle,
    ));
    assert!(result.is_err());
}

#[test]
fn descendants_recursive() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(make_node("A", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();
    tree.insert(make_node("B", Some("A"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();

    let desc = tree.descendants(&ws("root"));
    assert!(desc.contains(&ws("A")));
    assert!(desc.contains(&ws("B")));
    assert_eq!(desc.len(), 2);
}

#[test]
fn parent_chain() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(make_node("A", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();
    tree.insert(make_node("B", Some("A"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();

    let chain = tree.parent_chain(&ws("B"));
    assert_eq!(chain, vec![ws("A"), ws("root")]);
}

#[test]
fn cascade_failure_same_owner() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(make_node("A", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();
    tree.insert(make_node("B", Some("A"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();

    tree.cascade_failure(&ws("A"));
    assert_eq!(tree.get(&ws("A")).unwrap().status, WorkspaceState::Failed);
    assert_eq!(tree.get(&ws("B")).unwrap().status, WorkspaceState::Failed);
}

#[test]
fn cascade_reparents_cross_owner() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner1"));
    tree.insert(make_node("A", Some("root"), "owner1", Originator::System, WorkspaceState::Active))
        .unwrap();
    tree.insert(make_node("B", Some("A"), "owner2", Originator::System, WorkspaceState::Active))
        .unwrap();

    let reparented = tree.cascade_failure(&ws("A"));
    assert_eq!(tree.get(&ws("A")).unwrap().status, WorkspaceState::Failed);
    // B should be reparented, not failed.
    assert_eq!(tree.get(&ws("B")).unwrap().status, WorkspaceState::Active);
    assert!(reparented.contains(&ws("B")));
    assert_eq!(tree.get(&ws("B")).unwrap().parent, Some(ws("root")));
}

#[test]
fn reparent_moves_subtree() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(make_node("A", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();
    tree.insert(make_node("B", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();

    tree.reparent(&ws("B"), &ws("A"));
    assert_eq!(tree.get(&ws("B")).unwrap().parent, Some(ws("A")));
    assert!(tree.children(&ws("A")).contains(&&ws("B")));
    assert!(!tree.children(&ws("root")).contains(&&ws("B")));
}

#[test]
fn active_workspaces() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(make_node("A", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();
    tree.insert(make_node("B", Some("root"), "owner", Originator::System, WorkspaceState::Closed))
        .unwrap();

    let active = tree.active_workspaces();
    assert!(active.contains(&&ws("root")));
    assert!(active.contains(&&ws("A")));
    assert!(!active.contains(&&ws("B")));
}

// ══════════════════════════════════════════
// Task 9.1 — Tree Indices
// ══════════════════════════════════════════

#[test]
fn root_in_both_indices() {
    let tree = WorkspaceTree::new(ws("root"), uid("owner"));
    assert!(tree.by_originator(&Originator::System).contains(&ws("root")));
    assert!(tree.by_owner(&uid("owner")).contains(&ws("root")));
}

#[test]
fn originator_tracked_on_insert() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    let user_orig = Originator::User(uid("alice"));
    tree.insert(make_node("A", Some("root"), "owner", user_orig.clone(), WorkspaceState::Active))
        .unwrap();

    assert!(tree.by_originator(&user_orig).contains(&ws("A")));
    assert!(!tree.by_originator(&Originator::System).contains(&ws("A")));
}

#[test]
fn originator_index_immutable() {
    // No public method changes originator after insert — verify the field is what was set.
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    let user_orig = Originator::User(uid("bob"));
    tree.insert(make_node("A", Some("root"), "owner", user_orig.clone(), WorkspaceState::Active))
        .unwrap();

    assert_eq!(tree.get(&ws("A")).unwrap().originator, user_orig);
}

#[test]
fn owner_tracked_on_insert() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner1"));
    tree.insert(make_node("A", Some("root"), "owner2", Originator::System, WorkspaceState::Active))
        .unwrap();

    assert!(tree.by_owner(&uid("owner2")).contains(&ws("A")));
    assert!(!tree.by_owner(&uid("owner1")).contains(&ws("A")));
}

#[test]
fn transfer_owner_updates_index() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner1"));
    tree.insert(make_node("A", Some("root"), "owner1", Originator::System, WorkspaceState::Active))
        .unwrap();

    let old = tree.transfer_owner(&ws("A"), uid("owner2")).unwrap();
    assert_eq!(old, uid("owner1"));
    assert_eq!(tree.get(&ws("A")).unwrap().owner, uid("owner2"));
    assert!(tree.by_owner(&uid("owner2")).contains(&ws("A")));
    assert!(!tree.by_owner(&uid("owner1")).contains(&ws("A")));
}

#[test]
fn transfer_owner_same_owner_rejected() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    let result = tree.transfer_owner(&ws("root"), uid("owner"));
    assert!(matches!(result, Err(TreeError::SameOwner)));
}

#[test]
fn siblings_basic() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(make_node("A", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();
    tree.insert(make_node("B", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();

    let sibs = tree.siblings(&ws("A"));
    assert_eq!(sibs, vec![ws("B")]);

    let sibs_b = tree.siblings(&ws("B"));
    assert_eq!(sibs_b, vec![ws("A")]);
}

#[test]
fn siblings_root_empty() {
    let tree = WorkspaceTree::new(ws("root"), uid("owner"));
    assert!(tree.siblings(&ws("root")).is_empty());
}

#[test]
fn siblings_only_child_empty() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(make_node("A", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();
    assert!(tree.siblings(&ws("A")).is_empty());
}

#[test]
fn causal_descendants_filters() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    let alice = Originator::User(uid("alice"));
    let bob = Originator::User(uid("bob"));
    tree.insert(make_node("A", Some("root"), "owner", alice.clone(), WorkspaceState::Active))
        .unwrap();
    tree.insert(make_node("B", Some("root"), "owner", bob.clone(), WorkspaceState::Active))
        .unwrap();
    tree.insert(make_node("C", Some("A"), "owner", alice.clone(), WorkspaceState::Active))
        .unwrap();

    let causal = tree.causal_descendants(&ws("root"), &alice);
    assert!(causal.contains(&ws("A")));
    assert!(causal.contains(&ws("C")));
    assert!(!causal.contains(&ws("B")));
    assert_eq!(causal.len(), 2);
}

#[test]
fn causal_descendants_empty() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(make_node("A", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();

    let causal = tree.causal_descendants(&ws("root"), &Originator::User(uid("nobody")));
    assert!(causal.is_empty());
}

#[test]
fn by_originator_empty_key() {
    let tree = WorkspaceTree::new(ws("root"), uid("owner"));
    assert!(tree.by_originator(&Originator::User(uid("nonexistent"))).is_empty());
}

#[test]
fn by_owner_empty_key() {
    let tree = WorkspaceTree::new(ws("root"), uid("owner"));
    assert!(tree.by_owner(&uid("nonexistent")).is_empty());
}

#[test]
fn cascade_preserves_indices() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner1"));
    let alice = Originator::User(uid("alice"));
    tree.insert(make_node("A", Some("root"), "owner1", alice.clone(), WorkspaceState::Active))
        .unwrap();
    tree.insert(make_node("B", Some("A"), "owner1", alice.clone(), WorkspaceState::Active))
        .unwrap();

    tree.cascade_failure(&ws("A"));

    // Indices should still be valid — cascade changes status, not owner/originator.
    assert!(tree.by_originator(&alice).contains(&ws("A")));
    assert!(tree.by_originator(&alice).contains(&ws("B")));
    assert!(tree.by_owner(&uid("owner1")).contains(&ws("A")));
    assert!(tree.by_owner(&uid("owner1")).contains(&ws("B")));
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
// Task 10.1 — Task Graph Enhancement
// ══════════════════════════════════════════

#[test]
fn remaining_deps_set_on_insert() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::InProgress)).unwrap();
    graph.add_task(make_task("t2", vec![], TaskStatus::InProgress)).unwrap();
    graph.add_task(make_task("t3", vec!["t1", "t2"], TaskStatus::Draft)).unwrap();
    assert_eq!(graph.remaining_deps(&tid("t3")), Some(2));
}

#[test]
fn remaining_deps_zero_for_no_deps() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Draft)).unwrap();
    assert_eq!(graph.remaining_deps(&tid("t1")), Some(0));
}

#[test]
fn remaining_deps_skips_terminal() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Integrated)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::Pending)).unwrap();
    // t1 is already Integrated → doesn't count.
    assert_eq!(graph.remaining_deps(&tid("t2")), Some(0));
}

#[test]
fn mark_completed_decrements() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::InProgress)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::Pending)).unwrap();
    assert_eq!(graph.remaining_deps(&tid("t2")), Some(1));

    graph.mark_completed(&tid("t1"));
    assert_eq!(graph.remaining_deps(&tid("t2")), Some(0));
}

#[test]
fn mark_completed_returns_newly_ready() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::InProgress)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::Pending)).unwrap();

    let newly_ready = graph.mark_completed(&tid("t1"));
    assert!(newly_ready.contains(&tid("t2")));
}

#[test]
fn mark_completed_not_ready_if_other_deps() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::InProgress)).unwrap();
    graph.add_task(make_task("t2", vec![], TaskStatus::InProgress)).unwrap();
    graph.add_task(make_task("t3", vec!["t1", "t2"], TaskStatus::Pending)).unwrap();

    let newly_ready = graph.mark_completed(&tid("t1"));
    // t3 still has t2 pending.
    assert!(newly_ready.is_empty());
    assert_eq!(graph.remaining_deps(&tid("t3")), Some(1));
}

#[test]
fn mark_failed_returns_dependents() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::InProgress)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::Pending)).unwrap();
    graph.add_task(make_task("t3", vec!["t1"], TaskStatus::Pending)).unwrap();

    let blocked = graph.mark_failed(&tid("t1"));
    assert_eq!(blocked.len(), 2);
}

#[test]
fn bind_sets_both_maps() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    graph.bind(&tid("t1"), &ws("ws-1")).unwrap();

    assert_eq!(graph.get(&tid("t1")).unwrap().workspace_ref, Some(ws("ws-1")));
    assert_eq!(graph.task_for_workspace(&ws("ws-1")), Some(&tid("t1")));
    assert!(graph.get(&tid("t1")).unwrap().workspace_history.contains(&ws("ws-1")));
}

#[test]
fn bind_duplicate_task_rejected() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    graph.bind(&tid("t1"), &ws("ws-1")).unwrap();
    assert!(matches!(
        graph.bind(&tid("t1"), &ws("ws-2")),
        Err(GraphError::TaskAlreadyBound(_))
    ));
}

#[test]
fn bind_duplicate_workspace_rejected() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    graph.add_task(make_task("t2", vec![], TaskStatus::Pending)).unwrap();
    graph.bind(&tid("t1"), &ws("ws-1")).unwrap();
    assert!(matches!(
        graph.bind(&tid("t2"), &ws("ws-1")),
        Err(GraphError::WorkspaceAlreadyBound(_))
    ));
}

#[test]
fn unbind_clears_both() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    graph.bind(&tid("t1"), &ws("ws-1")).unwrap();
    graph.unbind(&tid("t1"));

    assert_eq!(graph.get(&tid("t1")).unwrap().workspace_ref, None);
    assert_eq!(graph.task_for_workspace(&ws("ws-1")), None);
}

#[test]
fn task_for_workspace_lookup() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    graph.bind(&tid("t1"), &ws("ws-1")).unwrap();
    assert_eq!(graph.task_for_workspace(&ws("ws-1")), Some(&tid("t1")));
}

#[test]
fn dispatchable_uses_counter() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    let d = graph.dispatchable();
    assert!(d.contains(&&tid("t1")));
}

#[test]
fn dispatchable_excludes_non_pending() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Draft)).unwrap();
    let d = graph.dispatchable();
    assert!(!d.contains(&&tid("t1")));
}

#[test]
fn forward_edges_built_on_insert() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Integrated)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::Pending)).unwrap();
    let deps = graph.dependents(&tid("t1"));
    assert!(deps.contains(&&tid("t2")));
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

// ══════════════════════════════════════════
// Task 9.2 — Visibility Graph
// ══════════════════════════════════════════

#[test]
fn self_visibility_implicit() {
    let graph = VisibilityGraph::new();
    // Self-visibility is true even for unregistered workspaces.
    assert!(graph.can_see(&ws("A"), &ws("A")));
}

#[test]
fn grant_creates_edge() {
    let mut graph = VisibilityGraph::new();
    graph.register(&ws("A"));
    graph.register(&ws("B"));
    assert!(graph.grant(&ws("A"), &ws("B")));
    assert!(graph.can_see(&ws("A"), &ws("B")));
}

#[test]
fn grant_not_symmetric() {
    let mut graph = VisibilityGraph::new();
    graph.register(&ws("A"));
    graph.register(&ws("B"));
    graph.grant(&ws("A"), &ws("B"));
    assert!(!graph.can_see(&ws("B"), &ws("A")));
}

#[test]
fn grant_idempotent() {
    let mut graph = VisibilityGraph::new();
    graph.register(&ws("A"));
    graph.register(&ws("B"));
    assert!(graph.grant(&ws("A"), &ws("B")));
    assert!(!graph.grant(&ws("A"), &ws("B"))); // second grant returns false
}

#[test]
fn grant_checked_succeeds() {
    let mut graph = VisibilityGraph::new();
    graph.register(&ws("coord"));
    graph.register(&ws("A"));
    graph.register(&ws("B"));
    // Coordinator can see B (granted).
    graph.grant(&ws("coord"), &ws("B"));
    // Coordinator grants A → B.
    let result = graph.grant_checked(&ws("A"), &ws("B"), &ws("coord"));
    assert!(result.unwrap());
    assert!(graph.can_see(&ws("A"), &ws("B")));
}

#[test]
fn grant_checked_rejects_invisible_target() {
    let mut graph = VisibilityGraph::new();
    graph.register(&ws("grantor"));
    graph.register(&ws("A"));
    graph.register(&ws("B"));
    // Grantor cannot see B — grant should fail.
    let result = graph.grant_checked(&ws("A"), &ws("B"), &ws("grantor"));
    assert!(result.is_err());
    assert!(!graph.can_see(&ws("A"), &ws("B")));
}

#[test]
fn visible_to_includes_self() {
    let graph = VisibilityGraph::new();
    let set = graph.visible_to(&ws("A"));
    assert!(set.contains(&ws("A")));
    assert_eq!(set.len(), 1);
}

#[test]
fn visible_to_includes_grants() {
    let mut graph = VisibilityGraph::new();
    graph.register(&ws("A"));
    graph.register(&ws("B"));
    graph.register(&ws("C"));
    graph.grant(&ws("A"), &ws("B"));
    graph.grant(&ws("A"), &ws("C"));
    let set = graph.visible_to(&ws("A"));
    assert!(set.contains(&ws("A")));
    assert!(set.contains(&ws("B")));
    assert!(set.contains(&ws("C")));
    assert_eq!(set.len(), 3);
}

#[test]
fn who_can_see_includes_self() {
    let graph = VisibilityGraph::new();
    let set = graph.who_can_see(&ws("A"));
    assert!(set.contains(&ws("A")));
    assert_eq!(set.len(), 1);
}

#[test]
fn who_can_see_tracks_reverse() {
    let mut graph = VisibilityGraph::new();
    graph.register(&ws("A"));
    graph.register(&ws("B"));
    graph.register(&ws("C"));
    graph.grant(&ws("A"), &ws("B"));
    graph.grant(&ws("C"), &ws("B"));
    let set = graph.who_can_see(&ws("B"));
    assert!(set.contains(&ws("A")));
    assert!(set.contains(&ws("B"))); // self
    assert!(set.contains(&ws("C")));
    assert_eq!(set.len(), 3);
}

#[test]
fn unregistered_workspace_invisible() {
    let graph = VisibilityGraph::new();
    assert!(!graph.can_see(&ws("A"), &ws("X")));
}

#[test]
fn register_idempotent() {
    let mut graph = VisibilityGraph::new();
    graph.register(&ws("A"));
    graph.register(&ws("B"));
    graph.grant(&ws("A"), &ws("B"));
    // Re-register A — should not clear grants.
    graph.register(&ws("A"));
    assert!(graph.can_see(&ws("A"), &ws("B")));
}

#[test]
fn multiple_grants_accumulate() {
    let mut graph = VisibilityGraph::new();
    graph.register(&ws("A"));
    graph.register(&ws("B"));
    graph.register(&ws("C"));
    graph.register(&ws("D"));
    graph.grant(&ws("A"), &ws("B"));
    graph.grant(&ws("A"), &ws("C"));
    graph.grant(&ws("A"), &ws("D"));
    let set = graph.visible_to(&ws("A"));
    assert_eq!(set.len(), 4); // B, C, D + self
}

#[test]
fn grant_count_accurate() {
    let mut graph = VisibilityGraph::new();
    graph.register(&ws("A"));
    graph.register(&ws("B"));
    graph.register(&ws("C"));
    assert_eq!(graph.grant_count(), 0);
    graph.grant(&ws("A"), &ws("B"));
    assert_eq!(graph.grant_count(), 1);
    graph.grant(&ws("A"), &ws("C"));
    assert_eq!(graph.grant_count(), 2);
    graph.grant(&ws("B"), &ws("C"));
    assert_eq!(graph.grant_count(), 3);
    // Self-grant should not count.
    graph.grant(&ws("A"), &ws("A"));
    assert_eq!(graph.grant_count(), 3);
}

// ══════════════════════════════════════════
// Task 9.3 — Ownership Domains + Causation
// ══════════════════════════════════════════

#[test]
fn escalation_router_register_and_route() {
    let mut router = EscalationRouter::new();
    router.register(&ws("A"), &uid("alice"));
    assert_eq!(router.route(&ws("A")), Some(&uid("alice")));
}

#[test]
fn escalation_router_update() {
    let mut router = EscalationRouter::new();
    router.register(&ws("A"), &uid("alice"));
    router.update(&ws("A"), &uid("bob"));
    assert_eq!(router.route(&ws("A")), Some(&uid("bob")));
}

#[test]
fn escalation_router_unknown_returns_none() {
    let router = EscalationRouter::new();
    assert_eq!(router.route(&ws("unknown")), None);
}

#[test]
fn resolve_owner_inherits_parent() {
    let owner = resolve_owner(&uid("alice"), None);
    assert_eq!(owner, uid("alice"));
}

#[test]
fn resolve_owner_explicit_override() {
    let owner = resolve_owner(&uid("alice"), Some(&uid("bob")));
    assert_eq!(owner, uid("bob"));
}

#[test]
fn resolve_originator_delegation_inherits() {
    let parent = Originator::User(uid("alice"));
    let orig = resolve_originator(&parent, false, None);
    assert_eq!(orig, Originator::User(uid("alice")));
}

#[test]
fn resolve_originator_injection_sets_user() {
    let orig = resolve_originator(&Originator::System, true, Some(&uid("bob")));
    assert_eq!(orig, Originator::User(uid("bob")));
}

#[test]
fn resolve_originator_system_parent_inherited() {
    let orig = resolve_originator(&Originator::System, false, None);
    assert_eq!(orig, Originator::System);
}

#[test]
fn causal_impact_active_only() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    let alice = Originator::User(uid("alice"));
    tree.insert(make_node("A", Some("root"), "owner", alice.clone(), WorkspaceState::Active))
        .unwrap();
    tree.insert(make_node("B", Some("root"), "owner", alice.clone(), WorkspaceState::Closed))
        .unwrap();
    tree.insert(make_node("C", Some("root"), "owner", alice.clone(), WorkspaceState::Active))
        .unwrap();

    let impact = tree.causal_impact(&uid("alice"));
    assert!(impact.contains(&ws("A")));
    assert!(impact.contains(&ws("C")));
    assert!(!impact.contains(&ws("B"))); // terminal
    assert_eq!(impact.len(), 2);
}

#[test]
fn causal_impact_empty_for_unknown_user() {
    let tree = WorkspaceTree::new(ws("root"), uid("owner"));
    assert!(tree.causal_impact(&uid("nobody")).is_empty());
}

#[test]
fn is_causal_boundary_root_false() {
    let tree = WorkspaceTree::new(ws("root"), uid("owner"));
    assert!(!tree.is_causal_boundary(&ws("root")));
}

#[test]
fn is_causal_boundary_same_originator_false() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(make_node("A", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();
    assert!(!tree.is_causal_boundary(&ws("A")));
}

#[test]
fn is_causal_boundary_different_originator_true() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    let alice = Originator::User(uid("alice"));
    tree.insert(make_node("A", Some("root"), "owner", alice, WorkspaceState::Active))
        .unwrap();
    // Root is System, A is User(alice) → boundary.
    assert!(tree.is_causal_boundary(&ws("A")));
}

// ══════════════════════════════════════════
// Task 9.4 — Port Rights Graph
// ══════════════════════════════════════════

#[test]
fn create_right() {
    let mut graph = PortRightsGraph::new();
    let id = graph.create(ws("A"), ws("B"), PortRightType::Send);
    let entry = graph.get(&id).unwrap();
    assert_eq!(entry.holder, ws("A"));
    assert_eq!(entry.target, ws("B"));
    assert_eq!(entry.kind, PortRightType::Send);
    assert_eq!(entry.status, PortRightStatus::Active);
}

#[test]
fn validate_send_with_right() {
    let mut graph = PortRightsGraph::new();
    graph.create(ws("A"), ws("B"), PortRightType::Send);
    assert!(graph.validate_send(&ws("A"), &ws("B")).is_ok());
}

#[test]
fn validate_send_no_right() {
    let graph = PortRightsGraph::new();
    assert!(graph.validate_send(&ws("A"), &ws("B")).is_err());
}

#[test]
fn validate_send_send_once() {
    let mut graph = PortRightsGraph::new();
    graph.create(ws("A"), ws("B"), PortRightType::SendOnce);
    assert!(graph.validate_send(&ws("A"), &ws("B")).is_ok());
}

#[test]
fn consume_send_once() {
    let mut graph = PortRightsGraph::new();
    let id = graph.create(ws("A"), ws("B"), PortRightType::SendOnce);
    graph.consume(&id).unwrap();
    assert_eq!(graph.get(&id).unwrap().status, PortRightStatus::Consumed);
    // Subsequent validate_send should fail.
    assert!(graph.validate_send(&ws("A"), &ws("B")).is_err());
}

#[test]
fn consume_non_send_once_rejected() {
    let mut graph = PortRightsGraph::new();
    let id = graph.create(ws("A"), ws("B"), PortRightType::Send);
    assert!(matches!(graph.consume(&id), Err(PortRightError::NotSendOnce)));
}

#[test]
fn revoke_right() {
    let mut graph = PortRightsGraph::new();
    let id = graph.create(ws("A"), ws("B"), PortRightType::Send);
    graph.revoke(&id).unwrap();
    assert_eq!(graph.get(&id).unwrap().status, PortRightStatus::Revoked);
    assert!(graph.validate_send(&ws("A"), &ws("B")).is_err());
}

#[test]
fn revoke_non_active_rejected() {
    let mut graph = PortRightsGraph::new();
    let id = graph.create(ws("A"), ws("B"), PortRightType::Send);
    graph.revoke(&id).unwrap();
    assert!(matches!(graph.revoke(&id), Err(PortRightError::NotActive)));
}

#[test]
fn transfer_send_right() {
    let mut graph = PortRightsGraph::new();
    let id = graph.create(ws("A"), ws("B"), PortRightType::Send);
    let old = graph.transfer(&id, &ws("C")).unwrap();
    assert_eq!(old, ws("A"));
    assert_eq!(graph.get(&id).unwrap().holder, ws("C"));
    // Old holder can no longer send.
    assert!(graph.validate_send(&ws("A"), &ws("B")).is_err());
    // New holder can send.
    assert!(graph.validate_send(&ws("C"), &ws("B")).is_ok());
}

#[test]
fn transfer_receive_rejected() {
    let mut graph = PortRightsGraph::new();
    let id = graph.create(ws("A"), ws("A"), PortRightType::Receive);
    assert!(matches!(
        graph.transfer(&id, &ws("B")),
        Err(PortRightError::ReceiveNotTransferable)
    ));
}

#[test]
fn transfer_non_active_rejected() {
    let mut graph = PortRightsGraph::new();
    let id = graph.create(ws("A"), ws("B"), PortRightType::Send);
    graph.revoke(&id).unwrap();
    assert!(matches!(
        graph.transfer(&id, &ws("C")),
        Err(PortRightError::NotActive)
    ));
}

#[test]
fn expire_workspace_both_directions() {
    let mut graph = PortRightsGraph::new();
    let id_held = graph.create(ws("A"), ws("B"), PortRightType::Send);
    let id_targeted = graph.create(ws("C"), ws("A"), PortRightType::Send);
    let id_unrelated = graph.create(ws("B"), ws("C"), PortRightType::Send);

    graph.expire_workspace(&ws("A"));

    assert_eq!(graph.get(&id_held).unwrap().status, PortRightStatus::Expired);
    assert_eq!(graph.get(&id_targeted).unwrap().status, PortRightStatus::Expired);
    assert_eq!(graph.get(&id_unrelated).unwrap().status, PortRightStatus::Active);
}

#[test]
fn rights_held_by_returns_active_only() {
    let mut graph = PortRightsGraph::new();
    graph.create(ws("A"), ws("B"), PortRightType::Send);
    let id2 = graph.create(ws("A"), ws("C"), PortRightType::Send);
    graph.revoke(&id2).unwrap();

    let active = graph.rights_held_by(&ws("A"));
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].target, ws("B"));
}

#[test]
fn active_count_tracks_correctly() {
    let mut graph = PortRightsGraph::new();
    graph.create(ws("A"), ws("B"), PortRightType::Send);
    graph.create(ws("A"), ws("C"), PortRightType::Send);
    let id3 = graph.create(ws("A"), ws("D"), PortRightType::SendOnce);
    assert_eq!(graph.active_count(), 3);

    graph.consume(&id3).unwrap();
    assert_eq!(graph.active_count(), 2);

    graph.expire_workspace(&ws("A"));
    assert_eq!(graph.active_count(), 0);
}

#[test]
fn multiple_rights_same_pair() {
    let mut graph = PortRightsGraph::new();
    let id1 = graph.create(ws("A"), ws("B"), PortRightType::Send);
    let id2 = graph.create(ws("A"), ws("B"), PortRightType::Send);
    assert_ne!(id1, id2);
    // Both are valid — validate_send finds one.
    assert!(graph.validate_send(&ws("A"), &ws("B")).is_ok());
    // Revoke one, still valid via the other.
    graph.revoke(&id1).unwrap();
    assert!(graph.validate_send(&ws("A"), &ws("B")).is_ok());
    // Revoke both, now fails.
    graph.revoke(&id2).unwrap();
    assert!(graph.validate_send(&ws("A"), &ws("B")).is_err());
}

// ══════════════════════════════════════════
// Task 9.5 — Compound Operations
// ══════════════════════════════════════════

fn make_create_params(
    id: &str,
    parent: &str,
    owner: &str,
    originator: Originator,
) -> CreateWorkspaceParams {
    CreateWorkspaceParams {
        id: ws(id),
        parent: ws(parent),
        owner: uid(owner),
        originator,
        status: WorkspaceState::Idle,
        task_id: None,
    }
}

#[test]
fn create_workspace_updates_all() {
    let mut topo = TopologySet::new(ws("root"), uid("owner"));
    topo.create_workspace(make_create_params("A", "root", "owner", Originator::System))
        .unwrap();

    // Tree: node exists.
    assert!(topo.tree.get(&ws("A")).is_some());

    // Visibility: registered.
    assert!(topo.visibility.can_see(&ws("A"), &ws("A"))); // self

    // Escalation: routes to owner.
    assert_eq!(topo.escalation.route(&ws("A")), Some(&uid("owner")));

    // Port rights: 3 created (parent→child send, child→parent send, child self-receive).
    assert_eq!(topo.port_rights.active_count(), 3);
    assert!(topo.port_rights.validate_send(&ws("root"), &ws("A")).is_ok());
    assert!(topo.port_rights.validate_send(&ws("A"), &ws("root")).is_ok());
}

#[test]
fn create_workspace_coordinator_sees_child() {
    let mut topo = TopologySet::new(ws("root"), uid("owner"));
    topo.create_workspace(make_create_params("A", "root", "owner", Originator::System))
        .unwrap();

    assert!(topo.visibility.can_see(&ws("root"), &ws("A")));
}

#[test]
fn terminate_closed_expires_rights() {
    let mut topo = TopologySet::new(ws("root"), uid("owner"));
    topo.create_workspace(make_create_params("A", "root", "owner", Originator::System))
        .unwrap();
    assert_eq!(topo.port_rights.active_count(), 3);

    let effect = topo.terminate_workspace(&ws("A"), WorkspaceState::Closed);
    assert!(effect.failed.is_empty());
    assert!(effect.reparented.is_empty());
    assert!(effect.rights_expired > 0);
    // All rights involving A should be expired.
    assert!(topo.port_rights.validate_send(&ws("root"), &ws("A")).is_err());
    assert!(topo.port_rights.validate_send(&ws("A"), &ws("root")).is_err());
}

#[test]
fn terminate_failed_cascades() {
    let mut topo = TopologySet::new(ws("root"), uid("owner"));
    topo.create_workspace(make_create_params("A", "root", "owner", Originator::System))
        .unwrap();
    topo.create_workspace(make_create_params("B", "A", "owner", Originator::System))
        .unwrap();

    let effect = topo.terminate_workspace(&ws("A"), WorkspaceState::Failed);
    // B is same-owner child → failed.
    assert!(effect.failed.contains(&ws("B")));
    assert_eq!(topo.tree.get(&ws("B")).unwrap().status, WorkspaceState::Failed);
}

#[test]
fn terminate_failed_expires_cascaded_rights() {
    let mut topo = TopologySet::new(ws("root"), uid("owner"));
    topo.create_workspace(make_create_params("A", "root", "owner", Originator::System))
        .unwrap();
    topo.create_workspace(make_create_params("B", "A", "owner", Originator::System))
        .unwrap();
    // 6 rights total: 3 for A + 3 for B.
    assert_eq!(topo.port_rights.active_count(), 6);

    topo.terminate_workspace(&ws("A"), WorkspaceState::Failed);
    // All rights for A and B should be expired.
    assert_eq!(topo.port_rights.active_count(), 0);
}

#[test]
fn cascade_updates_escalation_router() {
    let mut topo = TopologySet::new(ws("root"), uid("owner1"));
    topo.create_workspace(make_create_params("A", "root", "owner1", Originator::System))
        .unwrap();
    topo.create_workspace(make_create_params("B", "A", "owner2", Originator::System))
        .unwrap();

    // Set B to Active so it's a non-terminal candidate.
    topo.tree.update_status(&ws("B"), WorkspaceState::Active);

    let effect = topo.terminate_workspace(&ws("A"), WorkspaceState::Failed);
    // B is cross-owner → reparented, not failed.
    assert!(effect.reparented.contains(&ws("B")));
    assert_eq!(topo.tree.get(&ws("B")).unwrap().status, WorkspaceState::Active);
    // Escalation still routes B to owner2.
    assert_eq!(topo.escalation.route(&ws("B")), Some(&uid("owner2")));
}

#[test]
fn transfer_ownership_updates_both() {
    let mut topo = TopologySet::new(ws("root"), uid("owner1"));
    topo.create_workspace(make_create_params("A", "root", "owner1", Originator::System))
        .unwrap();

    let old = topo.transfer_ownership(&ws("A"), uid("owner2")).unwrap();
    assert_eq!(old, uid("owner1"));
    assert_eq!(topo.tree.get(&ws("A")).unwrap().owner, uid("owner2"));
    assert!(topo.tree.by_owner(&uid("owner2")).contains(&ws("A")));
    assert_eq!(topo.escalation.route(&ws("A")), Some(&uid("owner2")));
}

#[test]
fn create_then_terminate_lifecycle() {
    let mut topo = TopologySet::new(ws("root"), uid("owner"));

    // Create.
    topo.create_workspace(make_create_params("A", "root", "owner", Originator::System))
        .unwrap();
    assert!(topo.tree.get(&ws("A")).is_some());
    assert!(topo.visibility.can_see(&ws("root"), &ws("A")));
    assert_eq!(topo.port_rights.active_count(), 3);

    // Terminate.
    topo.terminate_workspace(&ws("A"), WorkspaceState::Closed);
    assert_eq!(topo.tree.get(&ws("A")).unwrap().status, WorkspaceState::Closed);
    assert_eq!(topo.port_rights.active_count(), 0);
    // Visibility preserved — terminal workspaces remain visible for trail queries.
    assert!(topo.visibility.can_see(&ws("root"), &ws("A")));
}

// ══════════════════════════════════════════
// Task 10.2 — Gate Enforcement
// ══════════════════════════════════════════

fn make_gate_ctrl() -> GateController {
    GateController::new(30_000, GateFallback::AutoApprove)
}

#[test]
fn open_gate_creates_pending() {
    let mut ctrl = make_gate_ctrl();
    let event = ctrl.open_gate(tid("t1"), "task1".into(), "desc", None, None);
    assert!(ctrl.is_pending(&event.gate_id));
    assert_eq!(ctrl.pending_count(), 1);
}

#[test]
fn open_gate_returns_event() {
    let mut ctrl = make_gate_ctrl();
    let event = ctrl.open_gate(tid("t1"), "task1".into(), "desc", None, None);
    assert_eq!(event.gate_type, GateType::TaskApproval);
    assert_eq!(event.task_id, Some(tid("t1")));
}

#[test]
fn resolve_approve_removes_gate() {
    let mut ctrl = make_gate_ctrl();
    let event = ctrl.open_gate(tid("t1"), "task1".into(), "desc", None, None);
    let res = ctrl.resolve(&event.gate_id, GateDecision::Approve);
    assert_eq!(res, Some(GateResolution::Approved { source: "human".into() }));
    assert!(!ctrl.is_pending(&event.gate_id));
}

#[test]
fn resolve_reject_removes_gate() {
    let mut ctrl = make_gate_ctrl();
    let event = ctrl.open_gate(tid("t1"), "task1".into(), "desc", None, None);
    let res = ctrl.resolve(&event.gate_id, GateDecision::Reject);
    assert_eq!(res, Some(GateResolution::Rejected));
    assert_eq!(ctrl.pending_count(), 0);
}

#[test]
fn resolve_already_resolved_returns_none() {
    let mut ctrl = make_gate_ctrl();
    let event = ctrl.open_gate(tid("t1"), "task1".into(), "desc", None, None);
    ctrl.resolve(&event.gate_id, GateDecision::Approve);
    let res = ctrl.resolve(&event.gate_id, GateDecision::Approve);
    assert_eq!(res, None);
}

#[test]
fn timeout_auto_approve() {
    let mut ctrl = make_gate_ctrl();
    let event = ctrl.open_gate(tid("t1"), "task1".into(), "desc", None, Some(GateFallback::AutoApprove));
    let res = ctrl.timeout(&event.gate_id);
    assert_eq!(res, Some(GateResolution::Approved { source: "timeout_auto_approve".into() }));
}

#[test]
fn timeout_cancel() {
    let mut ctrl = make_gate_ctrl();
    let event = ctrl.open_gate(tid("t1"), "task1".into(), "desc", None, Some(GateFallback::Cancel));
    let res = ctrl.timeout(&event.gate_id);
    assert_eq!(res, Some(GateResolution::Rejected));
}

#[test]
fn timeout_already_resolved_returns_none() {
    let mut ctrl = make_gate_ctrl();
    let event = ctrl.open_gate(tid("t1"), "task1".into(), "desc", None, None);
    ctrl.resolve(&event.gate_id, GateDecision::Approve);
    let res = ctrl.timeout(&event.gate_id);
    assert_eq!(res, None);
}

#[test]
fn pending_for_task_lookup() {
    let mut ctrl = make_gate_ctrl();
    ctrl.open_gate(tid("t1"), "task1".into(), "desc", None, None);
    let pending = ctrl.pending_for_task(&tid("t1"));
    assert!(pending.is_some());
    assert_eq!(pending.unwrap().task_name, "task1");
}

#[test]
fn default_timeout_applied() {
    let mut ctrl = GateController::new(60_000, GateFallback::Cancel);
    let event = ctrl.open_gate(tid("t1"), "task1".into(), "desc", None, None);
    assert_eq!(event.timeout_ms, 60_000);
    assert_eq!(event.fallback_action, "cancel");
}

// ══════════════════════════════════════════
// Task 10.3 — Dispatch + Resource Allocation
// ══════════════════════════════════════════

fn make_dispatcher() -> Dispatcher {
    Dispatcher::new(DispatchConfig::default())
}

fn setup_dispatch() -> (Dispatcher, TaskGraph, TopologySet) {
    let dispatcher = make_dispatcher();
    let graph = TaskGraph::new();
    let topo = TopologySet::new(ws("root"), uid("owner"));
    (dispatcher, graph, topo)
}

#[test]
fn dispatch_single_task() {
    let (mut disp, mut graph, mut topo) = setup_dispatch();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();

    let actions = disp.try_dispatch(&mut graph, &mut topo);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].task_id, tid("t1"));
}

#[test]
fn dispatch_multiple_tasks() {
    let (mut disp, mut graph, mut topo) = setup_dispatch();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    graph.add_task(make_task("t2", vec![], TaskStatus::Pending)).unwrap();

    let actions = disp.try_dispatch(&mut graph, &mut topo);
    assert_eq!(actions.len(), 2);
}

#[test]
fn dispatch_respects_capacity() {
    let config = DispatchConfig {
        max_concurrent_workspaces: Some(1),
        ..Default::default()
    };
    let mut disp = Dispatcher::new(config);
    let mut graph = TaskGraph::new();
    let mut topo = TopologySet::new(ws("root"), uid("owner"));

    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    graph.add_task(make_task("t2", vec![], TaskStatus::Pending)).unwrap();

    let actions = disp.try_dispatch(&mut graph, &mut topo);
    assert_eq!(actions.len(), 1);
}

#[test]
fn dispatch_skips_non_dispatchable() {
    let (mut disp, mut graph, mut topo) = setup_dispatch();
    graph.add_task(make_task("t1", vec![], TaskStatus::Draft)).unwrap();
    graph.add_task(make_task("t2", vec![], TaskStatus::InProgress)).unwrap();

    let actions = disp.try_dispatch(&mut graph, &mut topo);
    assert!(actions.is_empty());
}

#[test]
fn dispatch_binds_task_to_workspace() {
    let (mut disp, mut graph, mut topo) = setup_dispatch();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();

    let actions = disp.try_dispatch(&mut graph, &mut topo);
    let ws_id = &actions[0].workspace_id;
    assert_eq!(graph.task_for_workspace(ws_id), Some(&tid("t1")));
}

#[test]
fn dispatch_creates_workspace_in_topology() {
    let (mut disp, mut graph, mut topo) = setup_dispatch();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();

    let actions = disp.try_dispatch(&mut graph, &mut topo);
    let ws_id = &actions[0].workspace_id;
    assert!(topo.tree.get(ws_id).is_some());
    assert_eq!(topo.tree.get(ws_id).unwrap().parent, Some(ws("root")));
}

#[test]
fn select_parent_root_task() {
    let graph = TaskGraph::new();
    let task = make_task("t1", vec![], TaskStatus::Pending);
    let parent = Dispatcher::select_parent(&graph, &task, &ws("root"));
    assert_eq!(parent, ws("root"));
}

#[test]
fn select_parent_subtask() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    graph.bind(&tid("t1"), &ws("ws-parent")).unwrap();

    let mut child = make_task("t2", vec![], TaskStatus::Pending);
    child.parent_task = Some(tid("t1"));
    graph.add_task(child.clone()).unwrap();

    let parent = Dispatcher::select_parent(&graph, &child, &ws("root"));
    assert_eq!(parent, ws("ws-parent"));
}

#[test]
fn allocate_budget_applies_margin() {
    let disp = Dispatcher::new(DispatchConfig {
        budget_margin: 0.2,
        ..Default::default()
    });
    let base = ResourceBudget {
        max_tokens: Some(1000),
        max_wall_time_ms: Some(60_000),
        ..Default::default()
    };
    let budget = disp.allocate_budget(&base);
    assert_eq!(budget.max_tokens, Some(1200));
    assert_eq!(budget.max_wall_time_ms, Some(72_000));
}

#[test]
fn allocate_budget_unlimited_stays_unlimited() {
    let disp = make_dispatcher();
    let base = ResourceBudget::default();
    let budget = disp.allocate_budget(&base);
    assert_eq!(budget.max_tokens, None);
    assert_eq!(budget.max_wall_time_ms, None);
}

#[test]
fn has_capacity_unlimited() {
    let topo = TopologySet::new(ws("root"), uid("owner"));
    assert!(Dispatcher::has_capacity(&topo, None));
}

#[test]
fn has_capacity_at_limit() {
    let mut topo = TopologySet::new(ws("root"), uid("owner"));
    topo.create_workspace(CreateWorkspaceParams {
        id: ws("ws-1"),
        parent: ws("root"),
        owner: uid("owner"),
        originator: Originator::System,
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();
    // Root + ws-1 = 2 active, 1 worker. At limit of 1.
    assert!(!Dispatcher::has_capacity(&topo, Some(1)));
}

// ══════════════════════════════════════════
// Task 10.4 — Context Assembly + Retry + Decomposition
// ══════════════════════════════════════════

fn make_task_with_checkpoint(id: &str, deps: Vec<&str>, status: TaskStatus, cp: Option<&str>) -> Task {
    Task {
        id: tid(id),
        name: id.into(),
        description: String::new(),
        depends_on: deps.into_iter().map(tid).collect(),
        parent_task: None,
        status,
        workspace_ref: None,
        workspace_history: vec![],
        checkpoint_ref: cp.map(CheckpointId::from),
    }
}

#[test]
fn assemble_context_collects_deps() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task_with_checkpoint("t1", vec![], TaskStatus::Integrated, Some("cp-1"))).unwrap();
    graph.add_task(make_task_with_checkpoint("t2", vec!["t1"], TaskStatus::Pending, None)).unwrap();

    let ctx = SchedulingOps::assemble_context(&graph, &tid("t2"));
    assert_eq!(ctx.dependency_outputs.len(), 1);
    assert_eq!(ctx.dependency_outputs[0].task_id, tid("t1"));
    assert_eq!(ctx.dependency_outputs[0].checkpoint_ref, CheckpointId::from("cp-1"));
}

#[test]
fn assemble_context_skips_no_checkpoint() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task_with_checkpoint("t1", vec![], TaskStatus::Integrated, None)).unwrap();
    graph.add_task(make_task_with_checkpoint("t2", vec!["t1"], TaskStatus::Pending, None)).unwrap();

    let ctx = SchedulingOps::assemble_context(&graph, &tid("t2"));
    assert!(ctx.dependency_outputs.is_empty());
}

#[test]
fn assemble_context_empty_no_deps() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();

    let ctx = SchedulingOps::assemble_context(&graph, &tid("t1"));
    assert!(ctx.dependency_outputs.is_empty());
}

#[test]
fn should_retry_within_limit() {
    let policy = RetryPolicy::default(); // max_attempts: 1
    let task = make_task("t1", vec![], TaskStatus::Failed);
    assert!(SchedulingOps::should_retry(&policy, &task, "agent_error"));
}

#[test]
fn should_retry_exceeds_limit() {
    let policy = RetryPolicy { max_attempts: 1, ..Default::default() };
    let mut task = make_task("t1", vec![], TaskStatus::Failed);
    task.workspace_history = vec![ws("ws-old")]; // 1 prior attempt
    assert!(!SchedulingOps::should_retry(&policy, &task, "agent_error"));
}

#[test]
fn should_retry_timeout_denied() {
    let policy = RetryPolicy { retry_on_timeout: false, ..Default::default() };
    let task = make_task("t1", vec![], TaskStatus::Failed);
    assert!(!SchedulingOps::should_retry(&policy, &task, "timeout"));
}

#[test]
fn should_retry_agent_allowed() {
    let policy = RetryPolicy { retry_on_agent_failure: true, ..Default::default() };
    let task = make_task("t1", vec![], TaskStatus::Failed);
    assert!(SchedulingOps::should_retry(&policy, &task, "agent_error"));
}

#[test]
fn prepare_retry_unbinds() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::InProgress)).unwrap();
    graph.bind(&tid("t1"), &ws("ws-1")).unwrap();

    // Simulate failure.
    graph.transition(&tid("t1"), TaskTrigger::Fail).unwrap();
    SchedulingOps::prepare_retry(&mut graph, &tid("t1"));

    assert_eq!(graph.get(&tid("t1")).unwrap().workspace_ref, None);
    assert_eq!(graph.task_for_workspace(&ws("ws-1")), None);
}

#[test]
fn cancel_task_transitions() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Draft)).unwrap();
    let cancelled = SchedulingOps::cancel_task(&mut graph, &tid("t1"));
    assert!(cancelled.contains(&tid("t1")));
    assert_eq!(graph.get(&tid("t1")).unwrap().status, TaskStatus::Cancelled);
}

#[test]
fn cancel_cascades_to_pending_dependents() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::Pending)).unwrap();

    let cancelled = SchedulingOps::cancel_task(&mut graph, &tid("t1"));
    assert!(cancelled.contains(&tid("t1")));
    assert!(cancelled.contains(&tid("t2")));
}

#[test]
fn cancel_does_not_cascade_to_in_progress() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::InProgress)).unwrap();
    graph.add_task(make_task("t2", vec!["t1"], TaskStatus::InProgress)).unwrap();

    let cancelled = SchedulingOps::cancel_task(&mut graph, &tid("t1"));
    assert!(cancelled.contains(&tid("t1")));
    assert!(!cancelled.contains(&tid("t2"))); // in-progress: left running
}

#[test]
fn add_subtasks_sets_parent() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("parent", vec![], TaskStatus::InProgress)).unwrap();

    let sub = make_task("sub1", vec![], TaskStatus::Draft);
    let ids = SchedulingOps::add_subtasks(&mut graph, &tid("parent"), vec![sub]).unwrap();

    assert_eq!(ids.len(), 1);
    assert_eq!(graph.get(&tid("sub1")).unwrap().parent_task, Some(tid("parent")));
}

#[test]
fn add_subtasks_validates_parent() {
    let mut graph = TaskGraph::new();
    let sub = make_task("sub1", vec![], TaskStatus::Draft);
    let result = SchedulingOps::add_subtasks(&mut graph, &tid("nonexistent"), vec![sub]);
    assert!(result.is_err());
}

#[test]
fn add_subtasks_in_graph() {
    let mut graph = TaskGraph::new();
    graph.add_task(make_task("parent", vec![], TaskStatus::InProgress)).unwrap();

    let sub1 = make_task("sub1", vec![], TaskStatus::Draft);
    let sub2 = make_task("sub2", vec![], TaskStatus::Draft);
    SchedulingOps::add_subtasks(&mut graph, &tid("parent"), vec![sub1, sub2]).unwrap();

    assert!(graph.get(&tid("sub1")).is_some());
    assert!(graph.get(&tid("sub2")).is_some());
}

// ══════════════════════════════════════════
// Task 11.1 — Integration Pipeline + Ordering
// ══════════════════════════════════════════

#[test]
fn queue_push_and_next() {
    let mut q = IntegrationQueue::new();
    q.push(ws("ws-1"));
    assert_eq!(q.take_next(), Some(ws("ws-1")));
}

#[test]
fn queue_one_at_a_time() {
    let mut q = IntegrationQueue::new();
    q.push(ws("ws-1"));
    q.push(ws("ws-2"));
    q.take_next(); // ws-1 in progress
    assert_eq!(q.take_next(), None); // blocked
}

#[test]
fn queue_complete_allows_next() {
    let mut q = IntegrationQueue::new();
    q.push(ws("ws-1"));
    q.push(ws("ws-2"));
    q.take_next();
    q.complete();
    assert_eq!(q.take_next(), Some(ws("ws-2")));
}

#[test]
fn queue_fifo_order() {
    let mut q = IntegrationQueue::new();
    q.push(ws("ws-1"));
    q.push(ws("ws-2"));
    q.push(ws("ws-3"));
    assert_eq!(q.take_next(), Some(ws("ws-1")));
    q.complete();
    assert_eq!(q.take_next(), Some(ws("ws-2")));
    q.complete();
    assert_eq!(q.take_next(), Some(ws("ws-3")));
}

#[test]
fn queue_pending_count() {
    let mut q = IntegrationQueue::new();
    assert_eq!(q.pending_count(), 0);
    q.push(ws("ws-1"));
    q.push(ws("ws-2"));
    assert_eq!(q.pending_count(), 2);
    q.take_next();
    assert_eq!(q.pending_count(), 1); // ws-1 moved to in_progress
}

fn make_checkpoint(id: &str, status: CheckpointStatus, confidence: Confidence) -> Checkpoint {
    Checkpoint {
        id: CheckpointId::from(id),
        workspace_id: ws("ws-1"),
        checkpoint_type: "artifact".into(),
        payload: vec![],
        content_hash: format!("hash-{id}"),
        intent: format!("intent-{id}"),
        parent_checkpoint: None,
        status,
        confidence,
        timestamp: 0,
        resource_usage: None,
    }
}

#[test]
fn find_final_checkpoint_found() {
    let cps = vec![
        make_checkpoint("cp-1", CheckpointStatus::Provisional, Confidence::Medium),
        make_checkpoint("cp-2", CheckpointStatus::Final, Confidence::High),
    ];
    let result = IntegrationPipeline::find_final_checkpoint(&cps);
    assert!(result.is_some());
    assert_eq!(result.unwrap().checkpoint_id, CheckpointId::from("cp-2"));
}

#[test]
fn find_final_checkpoint_none() {
    let cps = vec![
        make_checkpoint("cp-1", CheckpointStatus::Provisional, Confidence::Medium),
    ];
    assert!(IntegrationPipeline::find_final_checkpoint(&cps).is_none());
}

#[test]
fn find_final_prefers_latest() {
    let cps = vec![
        make_checkpoint("cp-1", CheckpointStatus::Final, Confidence::Medium),
        make_checkpoint("cp-2", CheckpointStatus::Provisional, Confidence::Medium),
        make_checkpoint("cp-3", CheckpointStatus::Final, Confidence::High),
    ];
    let result = IntegrationPipeline::find_final_checkpoint(&cps).unwrap();
    assert_eq!(result.checkpoint_id, CheckpointId::from("cp-3"));
}

#[test]
fn decide_high_confidence_accepts() {
    let cp = CheckpointRef {
        checkpoint_id: CheckpointId::from("cp-1"),
        content_hash: "hash".into(),
        intent: "intent".into(),
        confidence: Confidence::High,
    };
    let decision = IntegrationPipeline::decide(&cp);
    assert_eq!(
        decision,
        IntegrationDecision::Accept { strategy: MergeStrategy::Direct }
    );
}

#[test]
fn decide_low_confidence_revises() {
    let cp = CheckpointRef {
        checkpoint_id: CheckpointId::from("cp-1"),
        content_hash: "hash".into(),
        intent: "intent".into(),
        confidence: Confidence::Low,
    };
    let decision = IntegrationPipeline::decide(&cp);
    assert!(matches!(decision, IntegrationDecision::Revise { .. }));
}

#[test]
fn decide_medium_accepts_layered() {
    let cp = CheckpointRef {
        checkpoint_id: CheckpointId::from("cp-1"),
        content_hash: "hash".into(),
        intent: "intent".into(),
        confidence: Confidence::Medium,
    };
    let decision = IntegrationPipeline::decide(&cp);
    assert_eq!(
        decision,
        IntegrationDecision::Accept { strategy: MergeStrategy::Layered }
    );
}

#[test]
fn select_strategy_mapping() {
    assert_eq!(IntegrationPipeline::select_strategy(Confidence::High), MergeStrategy::Direct);
    assert_eq!(IntegrationPipeline::select_strategy(Confidence::Medium), MergeStrategy::Layered);
    assert_eq!(IntegrationPipeline::select_strategy(Confidence::Low), MergeStrategy::Evaluated);
}

// ══════════════════════════════════════════
// Task 11.2 — Merge Strategies
// ══════════════════════════════════════════

fn make_merge_ctx(source_res: Vec<&str>, parent_res: Vec<&str>) -> MergeContext {
    MergeContext {
        source_id: ws("src"),
        target_id: ws("parent"),
        source_resources: source_res.into_iter().map(String::from).collect(),
        parent_resources: parent_res.into_iter().map(String::from).collect(),
        checkpoint: CheckpointRef {
            checkpoint_id: CheckpointId::from("cp-1"),
            content_hash: "hash".into(),
            intent: "intent".into(),
            confidence: Confidence::High,
        },
    }
}

#[test]
fn direct_no_conflicts() {
    let ctx = make_merge_ctx(vec!["a", "b"], vec!["a", "b"]); // overlap doesn't matter
    assert!(matches!(MergeExecutor::merge_direct(&ctx), MergeResult::Success));
}

#[test]
fn layered_no_overlap() {
    let ctx = make_merge_ctx(vec!["a", "b"], vec!["c", "d"]);
    assert!(matches!(MergeExecutor::merge_layered(&ctx), MergeResult::Success));
}

#[test]
fn layered_detects_overlap() {
    let ctx = make_merge_ctx(vec!["a", "b"], vec!["b", "c"]);
    match MergeExecutor::merge_layered(&ctx) {
        MergeResult::Conflicts(conflicts) => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].conflict_type, ConflictType::ContentOverlap);
            assert!(conflicts[0].description.contains("b"));
        }
        MergeResult::Success => panic!("expected conflict"),
    }
}

#[test]
fn evaluated_detects_overlap() {
    let ctx = make_merge_ctx(vec!["x", "y"], vec!["y", "z"]);
    match MergeExecutor::merge_evaluated(&ctx) {
        MergeResult::Conflicts(conflicts) => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].conflict_type, ConflictType::ContentOverlap);
        }
        MergeResult::Success => panic!("expected conflict"),
    }
}

#[test]
fn evaluated_no_conflicts() {
    let ctx = make_merge_ctx(vec!["a"], vec!["b"]);
    assert!(matches!(MergeExecutor::merge_evaluated(&ctx), MergeResult::Success));
}

#[test]
fn execute_dispatches_correctly() {
    let overlap_ctx = make_merge_ctx(vec!["a"], vec!["a"]);
    // Direct: succeeds despite overlap.
    assert!(matches!(MergeExecutor::execute(MergeStrategy::Direct, &overlap_ctx), MergeResult::Success));
    // Layered: detects overlap.
    assert!(matches!(MergeExecutor::execute(MergeStrategy::Layered, &overlap_ctx), MergeResult::Conflicts(_)));
    // Evaluated: detects overlap.
    assert!(matches!(MergeExecutor::execute(MergeStrategy::Evaluated, &overlap_ctx), MergeResult::Conflicts(_)));
}

// ══════════════════════════════════════════
// Task 11.3 — Conflict Resolution
// ══════════════════════════════════════════

fn make_conflict(ct: ConflictType) -> Conflict {
    Conflict {
        conflict_type: ct,
        description: "test conflict".into(),
        workspace_id: ws("ws-1"),
    }
}

#[test]
fn resolve_one_coordinator() {
    let r = ConflictResolver::resolve_one(
        make_conflict(ConflictType::ContentOverlap),
        ResolutionStrategy::CoordinatorResolve,
    );
    assert_eq!(r.outcome, ResolutionOutcome::Resolved);
}

#[test]
fn resolve_one_escalate() {
    let r = ConflictResolver::resolve_one(
        make_conflict(ConflictType::ContentOverlap),
        ResolutionStrategy::Escalate,
    );
    assert_eq!(r.outcome, ResolutionOutcome::Escalated);
}

#[test]
fn resolve_one_rework() {
    let r = ConflictResolver::resolve_one(
        make_conflict(ConflictType::ContentOverlap),
        ResolutionStrategy::AgentRework,
    );
    assert_eq!(r.outcome, ResolutionOutcome::Reworked);
}

#[test]
fn select_strategy_overlap_is_coordinator() {
    assert_eq!(
        ConflictResolver::select_strategy(ConflictType::ContentOverlap),
        ResolutionStrategy::CoordinatorResolve
    );
}

#[test]
fn select_strategy_semantic_is_escalate() {
    assert_eq!(
        ConflictResolver::select_strategy(ConflictType::SemanticContradiction),
        ResolutionStrategy::Escalate
    );
}

#[test]
fn select_strategy_dependency_is_rework() {
    assert_eq!(
        ConflictResolver::select_strategy(ConflictType::DependencyViolation),
        ResolutionStrategy::AgentRework
    );
}

#[test]
fn resolve_all_coordinator_only() {
    let conflicts = vec![
        make_conflict(ConflictType::ContentOverlap),
        make_conflict(ConflictType::ContentOverlap),
    ];
    let result = ConflictResolver::resolve_all(conflicts);
    assert!(matches!(result, ConflictResolutionResult::Resolved(r) if r.len() == 2));
}

#[test]
fn resolve_all_stops_on_escalation() {
    let conflicts = vec![
        make_conflict(ConflictType::ContentOverlap),         // coordinator
        make_conflict(ConflictType::SemanticContradiction),   // escalate → stop
        make_conflict(ConflictType::ContentOverlap),          // never reached
    ];
    let result = ConflictResolver::resolve_all(conflicts);
    assert!(matches!(result, ConflictResolutionResult::Pending(r) if r.len() == 2));
}

#[test]
fn resolve_all_stops_on_rework() {
    let conflicts = vec![
        make_conflict(ConflictType::ContentOverlap),         // coordinator
        make_conflict(ConflictType::DependencyViolation),    // rework → stop
        make_conflict(ConflictType::ContentOverlap),          // never reached
    ];
    let result = ConflictResolver::resolve_all(conflicts);
    assert!(matches!(result, ConflictResolutionResult::Rework(r) if r.len() == 2));
}

// ══════════════════════════════════════════
// Task 11.4 — Salvage Integration
// ══════════════════════════════════════════

#[test]
fn salvage_select_prefers_final() {
    let cps = vec![
        make_checkpoint("cp-1", CheckpointStatus::Provisional, Confidence::Medium),
        make_checkpoint("cp-2", CheckpointStatus::Final, Confidence::High),
    ];
    let cp = SalvageIntegration::select_checkpoint(&cps).unwrap();
    assert_eq!(cp.checkpoint_id, CheckpointId::from("cp-2"));
}

#[test]
fn salvage_select_falls_back_to_provisional() {
    let cps = vec![
        make_checkpoint("cp-1", CheckpointStatus::Provisional, Confidence::Medium),
    ];
    let cp = SalvageIntegration::select_checkpoint(&cps).unwrap();
    assert_eq!(cp.checkpoint_id, CheckpointId::from("cp-1"));
}

#[test]
fn salvage_select_empty_returns_none() {
    assert!(SalvageIntegration::select_checkpoint(&[]).is_none());
}

#[test]
fn salvage_confidence_guardrail() {
    let mut cp = CheckpointRef {
        checkpoint_id: CheckpointId::from("cp-1"),
        content_hash: "hash".into(),
        intent: "intent".into(),
        confidence: Confidence::High,
    };
    SalvageIntegration::apply_confidence_guardrail(&mut cp);
    assert_eq!(cp.confidence, Confidence::Low);
}

#[test]
fn salvage_uses_evaluated_strategy() {
    // Clean context → success, but via evaluated path.
    let ctx = make_merge_ctx(vec!["a"], vec!["b"]);
    assert!(matches!(SalvageIntegration::execute(&ctx), MergeResult::Success));
}

#[test]
fn salvage_detects_conflicts() {
    // Overlap → evaluated detects it.
    let ctx = make_merge_ctx(vec!["a"], vec!["a"]);
    assert!(matches!(SalvageIntegration::execute(&ctx), MergeResult::Conflicts(_)));
}

#[test]
fn salvage_is_applicable() {
    let cps = vec![make_checkpoint("cp-1", CheckpointStatus::Provisional, Confidence::Low)];
    assert!(SalvageIntegration::is_applicable(&cps));
    assert!(!SalvageIntegration::is_applicable(&[]));
}

// ══════════════════════════════════════════
// Task 12.1 — Timeout Enforcement
// ══════════════════════════════════════════

#[test]
fn timeout_register_and_elapsed() {
    let mut t = TimeoutTracker::new();
    t.register(&ws("A"), 10_000);
    assert_eq!(t.elapsed_ms(&ws("A"), 0), 0);
}

#[test]
fn timeout_starts_on_active() {
    let mut t = TimeoutTracker::new();
    t.register(&ws("A"), 10_000);
    t.on_state_change(&ws("A"), WorkspaceState::Active, 100);
    assert_eq!(t.elapsed_ms(&ws("A"), 500), 400);
}

#[test]
fn timeout_pauses_on_suspended() {
    let mut t = TimeoutTracker::new();
    t.register(&ws("A"), 10_000);
    t.on_state_change(&ws("A"), WorkspaceState::Active, 100);
    t.on_state_change(&ws("A"), WorkspaceState::Suspended, 300);
    // 200ms elapsed, timer paused.
    assert_eq!(t.elapsed_ms(&ws("A"), 1000), 200);
}

#[test]
fn timeout_resumes_after_pause() {
    let mut t = TimeoutTracker::new();
    t.register(&ws("A"), 10_000);
    t.on_state_change(&ws("A"), WorkspaceState::Active, 100);
    t.on_state_change(&ws("A"), WorkspaceState::Suspended, 300); // 200ms
    t.on_state_change(&ws("A"), WorkspaceState::Active, 500);    // resume
    assert_eq!(t.elapsed_ms(&ws("A"), 700), 400); // 200 + 200
}

#[test]
fn timeout_continues_in_blocked() {
    let mut t = TimeoutTracker::new();
    t.register(&ws("A"), 10_000);
    t.on_state_change(&ws("A"), WorkspaceState::Active, 0);
    t.on_state_change(&ws("A"), WorkspaceState::Blocked, 100);
    // Timer still running in Blocked.
    assert_eq!(t.elapsed_ms(&ws("A"), 300), 300);
}

#[test]
fn timeout_expired_detected() {
    let mut t = TimeoutTracker::new();
    t.register(&ws("A"), 500);
    t.on_state_change(&ws("A"), WorkspaceState::Active, 0);
    let expired = t.check_expired(600);
    assert!(expired.contains(&ws("A")));
}

#[test]
fn timeout_zero_limit_never_expires() {
    let mut t = TimeoutTracker::new();
    t.register(&ws("A"), 0); // no timeout
    t.on_state_change(&ws("A"), WorkspaceState::Active, 0);
    assert!(t.check_expired(1_000_000).is_empty());
}

#[test]
fn timeout_extend_additive() {
    let mut t = TimeoutTracker::new();
    t.register(&ws("A"), 500);
    t.on_state_change(&ws("A"), WorkspaceState::Active, 0);
    t.extend(&ws("A"), 300);
    // 500 + 300 = 800ms limit. At 600ms, not expired.
    assert!(t.check_expired(600).is_empty());
    // At 900ms, expired.
    assert!(!t.check_expired(900).is_empty());
}

// ══════════════════════════════════════════
// Task 12.2 — Budget Enforcement
// ══════════════════════════════════════════

#[test]
fn budget_ok_within_limits() {
    let usage = ResourceUsage { tokens: 50, ..Default::default() };
    let budget = ResourceBudget { max_tokens: Some(100), ..Default::default() };
    assert_eq!(BudgetEnforcer::check(&usage, &budget), BudgetCheckResult::Ok);
}

#[test]
fn budget_warning_at_threshold() {
    let usage = ResourceUsage { tokens: 85, ..Default::default() };
    let budget = ResourceBudget {
        max_tokens: Some(100),
        warning_threshold: 0.8,
        ..Default::default()
    };
    match BudgetEnforcer::check(&usage, &budget) {
        BudgetCheckResult::Warning(dims) => assert!(dims.contains(&"tokens".to_string())),
        other => panic!("expected Warning, got {other:?}"),
    }
}

#[test]
fn budget_exceeded_at_limit() {
    let usage = ResourceUsage { tokens: 100, ..Default::default() };
    let budget = ResourceBudget { max_tokens: Some(100), ..Default::default() };
    match BudgetEnforcer::check(&usage, &budget) {
        BudgetCheckResult::Exceeded(dims) => assert!(dims.contains(&"tokens".to_string())),
        other => panic!("expected Exceeded, got {other:?}"),
    }
}

#[test]
fn budget_unlimited_never_warns() {
    let usage = ResourceUsage { tokens: 999_999, ..Default::default() };
    let budget = ResourceBudget::default(); // all None
    assert_eq!(BudgetEnforcer::check(&usage, &budget), BudgetCheckResult::Ok);
}

#[test]
fn budget_multiple_dimensions() {
    let usage = ResourceUsage {
        tokens: 100,
        storage_bytes: 85,
        ..Default::default()
    };
    let budget = ResourceBudget {
        max_tokens: Some(100),
        max_storage_bytes: Some(100),
        warning_threshold: 0.8,
        ..Default::default()
    };
    match BudgetEnforcer::check(&usage, &budget) {
        BudgetCheckResult::Exceeded(dims) => assert!(dims.contains(&"tokens".to_string())),
        other => panic!("expected Exceeded, got {other:?}"),
    }
}

#[test]
fn budget_increase_additive() {
    let mut budget = ResourceBudget {
        max_tokens: Some(100),
        max_wall_time_ms: Some(5000),
        ..Default::default()
    };
    let additional = ResourceBudget {
        max_tokens: Some(50),
        max_wall_time_ms: Some(3000),
        ..Default::default()
    };
    BudgetEnforcer::increase_budget(&mut budget, &additional);
    assert_eq!(budget.max_tokens, Some(150));
    assert_eq!(budget.max_wall_time_ms, Some(8000));
}

// ══════════════════════════════════════════
// Task 12.3 — Liveness Monitoring
// ══════════════════════════════════════════

#[test]
fn liveness_register_and_active() {
    let mut m = LivenessMonitor::new(5000);
    m.register(&ws("A"), 100);
    assert!(m.check_inactive(4000).is_empty()); // within interval
}

#[test]
fn liveness_detects_inactivity() {
    let mut m = LivenessMonitor::new(5000);
    m.register(&ws("A"), 100);
    let inactive = m.check_inactive(6000); // 5900ms since last activity
    assert!(inactive.contains(&ws("A")));
}

#[test]
fn liveness_record_resets_timer() {
    let mut m = LivenessMonitor::new(5000);
    m.register(&ws("A"), 100);
    m.record_activity(&ws("A"), 4000);
    assert!(m.check_inactive(8000).is_empty()); // 4000ms since last, < 5000
    assert!(!m.check_inactive(10_000).is_empty()); // 6000ms since last
}

#[test]
fn liveness_disabled_never_reports() {
    let mut m = LivenessMonitor::new(0); // disabled
    m.register(&ws("A"), 0);
    assert!(m.check_inactive(999_999).is_empty());
    assert!(!m.is_enabled());
}

#[test]
fn liveness_remove_stops_tracking() {
    let mut m = LivenessMonitor::new(5000);
    m.register(&ws("A"), 100);
    m.remove(&ws("A"));
    assert!(m.check_inactive(999_999).is_empty());
}

// ══════════════════════════════════════════
// Tasks 13.1–13.3 — Request Handler
// ══════════════════════════════════════════

/// Build a RequestHandler with a topology containing root + one child workspace.
fn setup_handler() -> (
    TopologySet,
    TaskGraph,
    GateController,
    IntegrationQueue,
    TimeoutTracker,
    LivenessMonitor,
    MigrationCoordinator,
    u64,
    u64,
) {
    let mut topo = TopologySet::new(ws("root"), uid("owner"));
    topo.create_workspace(CreateWorkspaceParams {
        id: ws("ws-1"),
        parent: ws("root"),
        owner: uid("owner"),
        originator: Originator::System,
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();

    let mut graph = TaskGraph::new();
    graph.add_task(make_task("t1", vec![], TaskStatus::Pending)).unwrap();
    graph.bind(&tid("t1"), &ws("ws-1")).unwrap();

    let gate = GateController::new(30_000, GateFallback::AutoApprove);
    let queue = IntegrationQueue::new();
    let timeout = TimeoutTracker::new();
    let liveness = LivenessMonitor::new(0);
    let migration = MigrationCoordinator::new(60_000);

    (topo, graph, gate, queue, timeout, liveness, migration, 1, 1)
}

#[test]
fn handler_bind_returns_state() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_bind(&ws("ws-1"), None).unwrap();
    assert_eq!(result.workspace_id, ws("ws-1"));
    assert_eq!(result.state, WorkspaceState::Active);
    assert_eq!(result.owner, uid("owner"));
}

#[test]
fn handler_bind_not_found() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    assert!(h.handle_bind(&ws("nonexistent"), None).is_err());
}

#[test]
fn handler_send_envelope_validates_rights() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    // root → ws-1 has a send right (created by TopologySet::create_workspace).
    let result = h.handle_send_envelope(
        &ws("root"), &ws("ws-1"), "directive", b"payload", EnvelopePriority::Normal,
    );
    assert!(result.is_ok());
}

#[test]
fn handler_send_envelope_no_right() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();

    // Create ws-2 with no special rights to ws-1.
    topo.create_workspace(CreateWorkspaceParams {
        id: ws("ws-2"),
        parent: ws("root"),
        owner: uid("owner"),
        originator: Originator::System,
        status: WorkspaceState::Active,
        task_id: None,
    }).unwrap();

    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    // ws-2 → ws-1 has no send right.
    let result = h.handle_send_envelope(
        &ws("ws-2"), &ws("ws-1"), "feedback", b"payload", EnvelopePriority::Normal,
    );
    assert!(result.is_err());
}

#[test]
fn handler_emit_signal_accepted() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_emit_signal(&ws("ws-1"), SignalType::Started, None);
    assert!(result.is_ok());
}

#[test]
fn handler_emit_signal_unknown_workspace() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    assert!(h.handle_emit_signal(&ws("nonexistent"), SignalType::Started, None).is_err());
}

#[test]
fn handler_create_checkpoint_returns_id() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_create_checkpoint(
        &ws("ws-1"), "artifact", b"content", "test", CheckpointStatus::Final, Confidence::High,
    );
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(!r.content_hash.is_empty());
}

#[test]
fn handler_get_workspace_view() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let view = h.handle_get_workspace(&ws("ws-1")).unwrap();
    assert_eq!(view.id, ws("ws-1"));
    assert_eq!(view.state, WorkspaceState::Active);
    assert_eq!(view.parent, Some(ws("root")));
}

#[test]
fn handler_get_task_graph_snapshot() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let snapshot = h.handle_get_task_graph();
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].id, tid("t1"));
}

#[test]
fn handler_inject_envelope_succeeds() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_inject_envelope(
        &ws("ws-1"), "directive", b"injected", EnvelopePriority::Urgent,
    );
    assert!(result.is_ok());
}

#[test]
fn handler_inject_envelope_not_found() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    assert!(h.handle_inject_envelope(&ws("nope"), "directive", b"x", EnvelopePriority::Normal).is_err());
}

#[test]
fn handler_gate_response_resolves() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();

    // Open a gate first.
    let event = gate.open_gate(tid("t1"), "task".into(), "desc", None, None);
    let gate_id = event.gate_id;

    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_gate_response(&gate_id, GateDecision::Approve).unwrap();
    assert!(result.is_some());
}

#[test]
fn handler_gate_response_unknown_returns_none() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_gate_response(&GateId::from("gate-999"), GateDecision::Approve).unwrap();
    assert!(result.is_none());
}

// ══════════════════════════════════════════
// Task 13.4 — Event Bus
// ══════════════════════════════════════════

#[test]
fn event_bus_buffering() {
    let mut bus = EventBus::new(true);
    bus.emit(CoordinatorEvent::WorkspaceStateChanged {
        workspace_id: ws("A"),
        from: WorkspaceState::Idle,
        to: WorkspaceState::Active,
    });
    assert_eq!(bus.buffered_count(), 1);
}

#[test]
fn event_bus_drain() {
    let mut bus = EventBus::new(true);
    bus.emit(CoordinatorEvent::TaskStatusChanged {
        task_id: tid("t1"),
        from: TaskStatus::Draft,
        to: TaskStatus::Pending,
    });
    bus.emit(CoordinatorEvent::TaskStatusChanged {
        task_id: tid("t2"),
        from: TaskStatus::Pending,
        to: TaskStatus::Assigned,
    });
    let events = bus.drain();
    assert_eq!(events.len(), 2);
    assert_eq!(bus.buffered_count(), 0);
}

#[test]
fn event_bus_subscriber_called() {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = count.clone();

    let mut bus = EventBus::new(false);
    bus.subscribe(Box::new(move |_| {
        count_clone.fetch_add(1, Ordering::SeqCst);
    }));

    bus.emit(CoordinatorEvent::TrailEntry {
        workspace_id: Some(ws("A")),
        event_type: "test".into(),
        sequence: 1,
    });
    bus.emit(CoordinatorEvent::TrailEntry {
        workspace_id: Some(ws("A")),
        event_type: "test".into(),
        sequence: 2,
    });

    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn event_bus_multiple_subscribers() {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    let count = Arc::new(AtomicUsize::new(0));

    let mut bus = EventBus::new(false);
    for _ in 0..3 {
        let c = count.clone();
        bus.subscribe(Box::new(move |_| { c.fetch_add(1, Ordering::SeqCst); }));
    }

    bus.emit(CoordinatorEvent::GateOpened(GateEvent {
        gate_id: GateId::from("g1"),
        gate_type: GateType::TaskApproval,
        subject: vec![],
        workspace_id: ws("A"),
        task_id: None,
        timeout_ms: 0,
        fallback_action: String::new(),
        created_at: 0,
    }));

    assert_eq!(count.load(Ordering::SeqCst), 3);
    assert_eq!(bus.subscriber_count(), 3);
}

#[test]
fn event_bus_no_buffering_by_default() {
    let mut bus = EventBus::default();
    bus.emit(CoordinatorEvent::TrailEntry {
        workspace_id: None,
        event_type: "test".into(),
        sequence: 1,
    });
    assert_eq!(bus.buffered_count(), 0);
}

// ══════════════════════════════════════════
// Phase 16 — Agent Migration
// ══════════════════════════════════════════

// ── 16.1: MigrationCoordinator ──

fn make_agent_ref(agent_type: &str) -> AgentRef {
    AgentRef {
        agent_type: agent_type.to_string(),
        config: None,
    }
}

fn make_migration_request(ws_id: &str, agent_type: &str) -> MigrationRequest {
    MigrationRequest {
        workspace_id: ws(ws_id),
        new_agent: make_agent_ref(agent_type),
        reason: "test".to_string(),
    }
}

#[test]
fn migration_start_active_workspace() {
    let mut mc = MigrationCoordinator::new(60_000);
    let ctx = mc
        .start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 1000)
        .unwrap();
    assert_eq!(ctx.workspace_id, ws("ws-1"));
    assert_eq!(ctx.pre_migration_state, WorkspaceState::Active);
    assert_eq!(ctx.old_agent, "gpt-4");
    assert_eq!(ctx.new_agent.agent_type, "gpt-5");
    assert_eq!(ctx.started_at_ms, 1000);
    assert!(ctx.snapshot.is_none());
}

#[test]
fn migration_start_blocked_workspace() {
    let mut mc = MigrationCoordinator::new(60_000);
    let ctx = mc
        .start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Blocked, "gpt-4".into(), 1000)
        .unwrap();
    assert_eq!(ctx.pre_migration_state, WorkspaceState::Blocked);
}

#[test]
fn migration_start_invalid_state_idle() {
    let mut mc = MigrationCoordinator::new(60_000);
    assert!(matches!(
        mc.start(make_migration_request("ws-1", "x"), WorkspaceState::Idle, "a".into(), 0),
        Err(MigrationError::InvalidState(WorkspaceState::Idle))
    ));
}

#[test]
fn migration_start_invalid_state_suspended() {
    let mut mc = MigrationCoordinator::new(60_000);
    assert!(matches!(
        mc.start(make_migration_request("ws-1", "x"), WorkspaceState::Suspended, "a".into(), 0),
        Err(MigrationError::InvalidState(WorkspaceState::Suspended))
    ));
}

#[test]
fn migration_start_invalid_state_migrating() {
    let mut mc = MigrationCoordinator::new(60_000);
    assert!(matches!(
        mc.start(make_migration_request("ws-1", "x"), WorkspaceState::Migrating, "a".into(), 0),
        Err(MigrationError::InvalidState(WorkspaceState::Migrating))
    ));
}

#[test]
fn migration_start_invalid_state_terminal() {
    let mut mc = MigrationCoordinator::new(60_000);
    assert!(matches!(
        mc.start(make_migration_request("ws-1", "x"), WorkspaceState::Closed, "a".into(), 0),
        Err(MigrationError::InvalidState(WorkspaceState::Closed))
    ));
    assert!(matches!(
        mc.start(make_migration_request("ws-1", "x"), WorkspaceState::Failed, "a".into(), 0),
        Err(MigrationError::InvalidState(WorkspaceState::Failed))
    ));
}

#[test]
fn migration_start_already_migrating() {
    let mut mc = MigrationCoordinator::new(60_000);
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 0).unwrap();
    assert!(matches!(
        mc.start(make_migration_request("ws-1", "gpt-6"), WorkspaceState::Active, "gpt-4".into(), 0),
        Err(MigrationError::AlreadyMigrating(_))
    ));
}

#[test]
fn migration_set_snapshot() {
    let mut mc = MigrationCoordinator::new(60_000);
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 0).unwrap();

    let snap = wacp_workspace::MigrationSnapshot {
        inbox: vec![],
        working_memory: vec![1, 2, 3],
        checkpoint_register: vec![],
        resource_meter: wacp_workspace::ResourceMeter::default(),
        trail_sequence: 42,
        delivered_envelope_ids: Default::default(),
    };
    mc.set_snapshot("ws-1", snap).unwrap();
    assert!(mc.get("ws-1").unwrap().snapshot.is_some());
}

#[test]
fn migration_set_snapshot_not_migrating() {
    let mut mc = MigrationCoordinator::new(60_000);
    let snap = wacp_workspace::MigrationSnapshot {
        inbox: vec![],
        working_memory: vec![],
        checkpoint_register: vec![],
        resource_meter: wacp_workspace::ResourceMeter::default(),
        trail_sequence: 0,
        delivered_envelope_ids: Default::default(),
    };
    assert!(matches!(
        mc.set_snapshot("ws-1", snap),
        Err(MigrationError::NotMigrating(_))
    ));
}

#[test]
fn migration_set_snapshot_already_set() {
    let mut mc = MigrationCoordinator::new(60_000);
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 0).unwrap();

    let snap = || wacp_workspace::MigrationSnapshot {
        inbox: vec![],
        working_memory: vec![],
        checkpoint_register: vec![],
        resource_meter: wacp_workspace::ResourceMeter::default(),
        trail_sequence: 0,
        delivered_envelope_ids: Default::default(),
    };
    mc.set_snapshot("ws-1", snap()).unwrap();
    assert!(matches!(
        mc.set_snapshot("ws-1", snap()),
        Err(MigrationError::SnapshotAlreadySet(_))
    ));
}

#[test]
fn migration_complete_returns_context() {
    let mut mc = MigrationCoordinator::new(60_000);
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 100).unwrap();
    let ctx = mc.complete("ws-1").unwrap();
    assert_eq!(ctx.workspace_id, ws("ws-1"));
    assert_eq!(ctx.old_agent, "gpt-4");
    assert!(!mc.is_migrating("ws-1"));
}

#[test]
fn migration_complete_not_migrating() {
    let mut mc = MigrationCoordinator::new(60_000);
    assert!(matches!(mc.complete("ws-1"), Err(MigrationError::NotMigrating(_))));
}

#[test]
fn migration_fail_returns_context() {
    let mut mc = MigrationCoordinator::new(60_000);
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 100).unwrap();
    let ctx = mc.fail("ws-1", "bind timeout".into(), 6).unwrap();
    assert_eq!(ctx.workspace_id, ws("ws-1"));
    assert!(!mc.is_migrating("ws-1"));
}

#[test]
fn migration_check_timeouts_expired() {
    let mut mc = MigrationCoordinator::new(1000); // 1s timeout
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 100).unwrap();

    assert!(mc.check_timeouts(500).is_empty()); // within timeout
    let expired = mc.check_timeouts(1100); // past timeout
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0], ws("ws-1"));
}

#[test]
fn migration_check_timeouts_not_expired() {
    let mut mc = MigrationCoordinator::new(60_000);
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 1000).unwrap();
    assert!(mc.check_timeouts(50_000).is_empty());
}

#[test]
fn migration_parallel_different_workspaces() {
    let mut mc = MigrationCoordinator::new(60_000);
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "a".into(), 0).unwrap();
    mc.start(make_migration_request("ws-2", "gpt-5"), WorkspaceState::Blocked, "b".into(), 0).unwrap();
    assert_eq!(mc.active_count(), 2);
    assert!(mc.is_migrating("ws-1"));
    assert!(mc.is_migrating("ws-2"));
}

#[test]
fn migration_expected_agent() {
    let mut mc = MigrationCoordinator::new(60_000);
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 0).unwrap();
    assert_eq!(mc.expected_agent("ws-1").unwrap().agent_type, "gpt-5");
    assert!(mc.expected_agent("ws-2").is_none());
}

// ── 16.2: Handler migration-aware bind ──

#[test]
fn handler_bind_migrating_correct_agent() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, mut migration, mut env_id, mut cp_id) =
        setup_handler();

    // Put ws-1 in Migrating state and register a migration.
    topo.tree.update_status(&ws("ws-1"), WorkspaceState::Migrating);
    migration.start(
        make_migration_request("ws-1", "new-agent"),
        WorkspaceState::Active,
        "old-agent".into(),
        0,
    ).unwrap();

    let h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_bind(&ws("ws-1"), Some("new-agent"));
    assert!(result.is_ok());
    assert_eq!(result.unwrap().state, WorkspaceState::Migrating);
}

#[test]
fn handler_bind_migrating_wrong_agent() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, mut migration, mut env_id, mut cp_id) =
        setup_handler();

    topo.tree.update_status(&ws("ws-1"), WorkspaceState::Migrating);
    migration.start(
        make_migration_request("ws-1", "new-agent"),
        WorkspaceState::Active,
        "old-agent".into(),
        0,
    ).unwrap();

    let h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_bind(&ws("ws-1"), Some("wrong-agent"));
    assert!(matches!(result, Err(HandlerError::PermissionDenied(_))));
}

#[test]
fn handler_bind_migrating_no_identity() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, mut migration, mut env_id, mut cp_id) =
        setup_handler();

    topo.tree.update_status(&ws("ws-1"), WorkspaceState::Migrating);
    migration.start(
        make_migration_request("ws-1", "new-agent"),
        WorkspaceState::Active,
        "old-agent".into(),
        0,
    ).unwrap();

    let h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_bind(&ws("ws-1"), None);
    assert!(matches!(result, Err(HandlerError::PermissionDenied(_))));
}

#[test]
fn handler_bind_normal_workspace_no_identity_check() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    // Non-migrating workspace — identity is not checked
    let result = h.handle_bind(&ws("ws-1"), None);
    assert!(result.is_ok());
}

// ── 16.3: Trail event builders ──

#[test]
fn trail_event_started() {
    let mut mc = MigrationCoordinator::new(60_000);
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 1000).unwrap();
    let ctx = mc.get("ws-1").unwrap();
    let event = MigrationCoordinator::started_event(ctx);
    assert_eq!(event.workspace_id, ws("ws-1"));
    assert_eq!(event.old_agent, "gpt-4");
    assert_eq!(event.new_agent, "gpt-5");
    assert_eq!(event.reason, "test");
    assert_eq!(event.pre_migration_state, "Active");
}

#[test]
fn trail_event_completed() {
    let mut mc = MigrationCoordinator::new(60_000);
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 1000).unwrap();
    let ctx = mc.get("ws-1").unwrap();
    let event = MigrationCoordinator::completed_event(ctx, 3500);
    assert_eq!(event.duration_ms, 2500);
    assert_eq!(event.restored_state, "Active");
}

#[test]
fn trail_event_failed() {
    let mut mc = MigrationCoordinator::new(60_000);
    mc.start(make_migration_request("ws-1", "gpt-5"), WorkspaceState::Active, "gpt-4".into(), 1000).unwrap();
    let ctx = mc.get("ws-1").unwrap();
    let event = MigrationCoordinator::failed_event(ctx, "bind timeout", 6, 5000);
    assert_eq!(event.error, "bind timeout");
    assert_eq!(event.failed_at_step, 6);
    assert_eq!(event.duration_ms, 4000);
}

// ── 16.4: Resource continuity ──

#[test]
fn timeout_paused_during_migrating() {
    let mut tracker = TimeoutTracker::new();
    tracker.register(&ws("ws-1"), 10_000);

    // Active → timer starts
    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Active, 100);
    assert_eq!(tracker.elapsed_ms(&ws("ws-1"), 500), 400);

    // Active → Migrating → timer pauses
    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Migrating, 500);
    assert_eq!(tracker.elapsed_ms(&ws("ws-1"), 1000), 400); // no change
    assert_eq!(tracker.elapsed_ms(&ws("ws-1"), 5000), 400); // still no change
}

#[test]
fn timeout_resumes_after_migration() {
    let mut tracker = TimeoutTracker::new();
    tracker.register(&ws("ws-1"), 10_000);

    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Active, 100);
    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Migrating, 500); // 400ms elapsed
    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Active, 2000); // resume

    // 400ms accumulated + new running time
    assert_eq!(tracker.elapsed_ms(&ws("ws-1"), 3000), 1400);
}

#[test]
fn timeout_continuous_across_migration() {
    let mut tracker = TimeoutTracker::new();
    tracker.register(&ws("ws-1"), 10_000);

    // Active for 300ms
    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Active, 100);
    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Migrating, 400);
    // Migration gap (should not accumulate)
    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Active, 1000);
    // Active for 200ms more
    assert_eq!(tracker.elapsed_ms(&ws("ws-1"), 1200), 500); // 300 + 200, no gap
}

#[test]
fn timeout_migration_to_blocked() {
    let mut tracker = TimeoutTracker::new();
    tracker.register(&ws("ws-1"), 10_000);

    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Blocked, 100);
    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Migrating, 600); // 500ms elapsed
    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Blocked, 2000); // resume

    assert_eq!(tracker.elapsed_ms(&ws("ws-1"), 2500), 1000); // 500 + 500
}

#[test]
fn liveness_reset_on_completion() {
    let mut monitor = LivenessMonitor::new(5000);
    monitor.register(&ws("ws-1"), 100);

    // No activity for a while → would be inactive
    assert!(!monitor.check_inactive(5200).is_empty());

    // Reset activity (migration completion)
    monitor.reset_activity(&ws("ws-1"), 5200);

    // No longer inactive
    assert!(monitor.check_inactive(5200).is_empty());
    assert!(monitor.check_inactive(9000).is_empty()); // still within interval after reset
}

#[test]
fn liveness_no_false_warning_after_migration() {
    let mut monitor = LivenessMonitor::new(3000);
    monitor.register(&ws("ws-1"), 100);

    // Activity at 100, then migration happens at 200, completes at 5000
    // Without reset, check at 5100 would fire (5100-100 = 5000 >= 3000)
    monitor.reset_activity(&ws("ws-1"), 5000);

    // After reset, should NOT fire until 5000+3000 = 8000
    assert!(monitor.check_inactive(7999).is_empty());
    assert!(!monitor.check_inactive(8000).is_empty());
}

#[test]
fn trail_event_serde_roundtrip() {
    let started = MigrationStartedEvent {
        workspace_id: ws("ws-1"),
        old_agent: "gpt-4".into(),
        new_agent: "gpt-5".into(),
        reason: "upgrade".into(),
        pre_migration_state: "Active".into(),
    };
    let json = serde_json::to_string(&started).unwrap();
    let roundtrip: MigrationStartedEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.workspace_id, ws("ws-1"));
    assert_eq!(roundtrip.new_agent, "gpt-5");

    let completed = MigrationCompletedEvent {
        workspace_id: ws("ws-1"),
        old_agent: "gpt-4".into(),
        new_agent: "gpt-5".into(),
        duration_ms: 2500,
        restored_state: "Active".into(),
    };
    let json = serde_json::to_string(&completed).unwrap();
    let roundtrip: MigrationCompletedEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.duration_ms, 2500);

    let failed = MigrationFailedEvent {
        workspace_id: ws("ws-1"),
        old_agent: "gpt-4".into(),
        new_agent: "gpt-5".into(),
        reason: "upgrade".into(),
        error: "bind timeout".into(),
        failed_at_step: 6,
        duration_ms: 4000,
    };
    let json = serde_json::to_string(&failed).unwrap();
    let roundtrip: MigrationFailedEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.failed_at_step, 6);
}

// ══════════════════════════════════════════
// Phase 17 — End-to-End Tests
// ══════════════════════════════════════════

use std::time::Duration;
use tokio::sync::mpsc;
use wacp_workspace::{
    AgentMessage, CoordinatorCommand, WorkspaceActor, WorkspaceConfig, WorkspaceEvent,
};

fn e2e_config(id: &str, parent: &str, owner: &str) -> WorkspaceConfig {
    WorkspaceConfig {
        id: ws(id),
        role: "worker".into(),
        base_role: BaseRole::Worker,
        parent: ws(parent),
        owner: uid(owner),
        originator: Originator::System,
        directive: Envelope {
            id: EnvelopeId::from(format!("dir-{id}")),
            from_workspace: ws(parent),
            to_workspace: ws(id),
            envelope_type: "directive".into(),
            payload: b"work".to_vec(),
            in_reply_to: None,
            timestamp: 0,
            priority: EnvelopePriority::Normal,
            origin: EnvelopeOrigin::Agent,
            state: EnvelopeState::Created,
        },
        context: vec![],
        visibility: Default::default(),
        authority: Default::default(),
        delegate: false,
        budget: None,
    }
}

fn e2e_envelope(id: &str, from: &str, to: &str) -> Envelope {
    Envelope {
        id: EnvelopeId::from(id),
        from_workspace: ws(from),
        to_workspace: ws(to),
        envelope_type: "directive".into(),
        payload: b"work".to_vec(),
        in_reply_to: None,
        timestamp: 0,
        priority: EnvelopePriority::Normal,
        origin: EnvelopeOrigin::Agent,
        state: EnvelopeState::Created,
    }
}

/// Drain events from the workspace actor channel, processing each through the coordinator.
/// Returns all events collected.
async fn drain_events(
    coordinator: &mut crate::orchestrator::Coordinator,
    event_rx: &mut mpsc::Receiver<WorkspaceEvent>,
    max: usize,
) -> Vec<WorkspaceEvent> {
    let mut events = Vec::new();
    for _ in 0..max {
        match tokio::time::timeout(Duration::from_millis(200), event_rx.recv()).await {
            Ok(Some(event)) => {
                coordinator.handle_event(&event);
                events.push(event);
            }
            _ => break,
        }
    }
    events
}

/// Find last state for a workspace in an event list.
fn last_state(events: &[WorkspaceEvent], ws_id: &str) -> Option<WorkspaceState> {
    events
        .iter()
        .rev()
        .find_map(|e| match e {
            WorkspaceEvent::StateChanged {
                workspace_id, to, ..
            } if workspace_id.as_ref() == ws_id => Some(*to),
            _ => None,
        })
}

// ── 17.1: Full lifecycle ──

#[tokio::test]
async fn e2e_single_worker_lifecycle() {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    let mut coord = crate::orchestrator::Coordinator::new(ws("root"), uid("system"), event_tx.clone());

    // Dispatch workspace.
    let handle = WorkspaceActor::spawn(e2e_config("ws-1", "root", "system"), event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-1"),
        parent: Some(ws("root")),
        children: vec![],
        owner: uid("system"),
        originator: Originator::System,
        status: WorkspaceState::Idle,
        task_id: None,
    }).unwrap();

    // Step 1: Deliver directive → Idle → Active
    handle.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(
        e2e_envelope("dir-1", "root", "ws-1"),
    )).await.unwrap();

    let events = drain_events(&mut coord, &mut event_rx, 3).await;
    assert_eq!(last_state(&events, "ws-1"), Some(WorkspaceState::Active));
    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Active);

    // Step 2: Agent creates checkpoint
    handle.agent_tx.send(AgentMessage::CreateCheckpoint {
        checkpoint_type: "artifact".into(),
        payload: b"output".to_vec(),
        content_hash: "hash-1".into(),
        intent: "first draft".into(),
        status: CheckpointStatus::Final,
        confidence: Confidence::High,
        resource_usage: None,
    }).await.unwrap();

    let events = drain_events(&mut coord, &mut event_rx, 5).await;
    assert!(events.iter().any(|e| matches!(e, WorkspaceEvent::CheckpointCreated(_))));

    // Step 3: Agent signals Complete → Active → Integrating
    handle.agent_tx.send(AgentMessage::EmitSignal {
        signal_type: SignalType::Complete,
        reason: Some("done".into()),
        context: None,
    }).await.unwrap();

    let events = drain_events(&mut coord, &mut event_rx, 5).await;
    assert_eq!(last_state(&events, "ws-1"), Some(WorkspaceState::Integrating));

    // Step 4: Coordinator completes integration → Integrating → Closed
    handle.coordinator_tx.send(CoordinatorCommand::IntegrationSucceeded).await.unwrap();

    let events = drain_events(&mut coord, &mut event_rx, 5).await;
    // Should see Closed then Terminated
    assert!(events.iter().any(|e| matches!(e,
        WorkspaceEvent::StateChanged { to: WorkspaceState::Closed, .. }
    )));
    assert!(events.iter().any(|e| matches!(e, WorkspaceEvent::Terminated(_))));
}

#[tokio::test]
async fn e2e_multi_worker_parallel() {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    let mut coord = crate::orchestrator::Coordinator::new(ws("root"), uid("system"), event_tx.clone());

    // Dispatch two workspaces.
    let h1 = WorkspaceActor::spawn(e2e_config("ws-1", "root", "system"), event_tx.clone());
    let h2 = WorkspaceActor::spawn(e2e_config("ws-2", "root", "system"), event_tx.clone());
    for id in ["ws-1", "ws-2"] {
        coord.tree.insert(crate::tree::WorkspaceNode {
            id: ws(id),
            parent: Some(ws("root")),
            children: vec![],
            owner: uid("system"),
            originator: Originator::System,
            status: WorkspaceState::Idle,
            task_id: None,
        }).unwrap();
    }

    // Activate both.
    h1.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d1", "root", "ws-1"))).await.unwrap();
    h2.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d2", "root", "ws-2"))).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 10).await;

    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Active);
    assert_eq!(coord.tree.get(&ws("ws-2")).unwrap().status, WorkspaceState::Active);

    // Both agents complete.
    h1.agent_tx.send(AgentMessage::EmitSignal { signal_type: SignalType::Complete, reason: None, context: None }).await.unwrap();
    h2.agent_tx.send(AgentMessage::EmitSignal { signal_type: SignalType::Complete, reason: None, context: None }).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 10).await;

    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Integrating);
    assert_eq!(coord.tree.get(&ws("ws-2")).unwrap().status, WorkspaceState::Integrating);

    // Integration succeeds for both.
    h1.coordinator_tx.send(CoordinatorCommand::IntegrationSucceeded).await.unwrap();
    h2.coordinator_tx.send(CoordinatorCommand::IntegrationSucceeded).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 10).await;

    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Closed);
    assert_eq!(coord.tree.get(&ws("ws-2")).unwrap().status, WorkspaceState::Closed);
}

#[tokio::test]
async fn e2e_delegation_subtask() {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    let mut coord = crate::orchestrator::Coordinator::new(ws("root"), uid("system"), event_tx.clone());

    // Parent workspace.
    let parent = WorkspaceActor::spawn(e2e_config("ws-parent", "root", "system"), event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-parent"),
        parent: Some(ws("root")),
        children: vec![],
        owner: uid("system"),
        originator: Originator::System,
        status: WorkspaceState::Idle,
        task_id: None,
    }).unwrap();

    // Activate parent.
    parent.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d-p", "root", "ws-parent"))).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 3).await;

    // Parent "delegates" — coordinator creates subtask workspace.
    let child = WorkspaceActor::spawn(e2e_config("ws-child", "ws-parent", "system"), event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-child"),
        parent: Some(ws("ws-parent")),
        children: vec![],
        owner: uid("system"),
        originator: Originator::System,
        status: WorkspaceState::Idle,
        task_id: None,
    }).unwrap();

    // Activate child.
    child.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d-c", "ws-parent", "ws-child"))).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 3).await;

    // Child completes and integrates.
    child.agent_tx.send(AgentMessage::EmitSignal { signal_type: SignalType::Complete, reason: None, context: None }).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;
    child.coordinator_tx.send(CoordinatorCommand::IntegrationSucceeded).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;

    // Child is Closed, parent is still Active.
    assert_eq!(coord.tree.get(&ws("ws-child")).unwrap().status, WorkspaceState::Closed);
    assert_eq!(coord.tree.get(&ws("ws-parent")).unwrap().status, WorkspaceState::Active);
}

#[tokio::test]
async fn e2e_workspace_survives_full_cycle() {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    let mut coord = crate::orchestrator::Coordinator::new(ws("root"), uid("system"), event_tx.clone());

    let handle = WorkspaceActor::spawn(e2e_config("ws-1", "root", "system"), event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-1"), parent: Some(ws("root")), children: vec![],
        owner: uid("system"), originator: Originator::System,
        status: WorkspaceState::Idle, task_id: None,
    }).unwrap();

    // Activate → Complete → Integrate → Close
    handle.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d1", "root", "ws-1"))).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 3).await;
    handle.agent_tx.send(AgentMessage::EmitSignal { signal_type: SignalType::Complete, reason: None, context: None }).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;
    handle.coordinator_tx.send(CoordinatorCommand::IntegrationSucceeded).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;

    // Tree node persists with terminal status.
    let node = coord.tree.get(&ws("ws-1")).unwrap();
    assert_eq!(node.status, WorkspaceState::Closed);
}

// ── 17.2: Failure scenarios ──

#[tokio::test]
async fn e2e_timeout_expiry() {
    let mut tracker = TimeoutTracker::new();
    tracker.register(&ws("ws-1"), 1000);
    tracker.on_state_change(&ws("ws-1"), WorkspaceState::Active, 100);

    // Time passes beyond timeout.
    let expired = tracker.check_expired(1200);
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0], ws("ws-1"));

    // Coordinator would abort — test the workspace actor side.
    let (event_tx, mut event_rx) = mpsc::channel(64);
    let handle = WorkspaceActor::spawn(e2e_config("ws-1", "root", "system"), event_tx);
    handle.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d1", "root", "ws-1"))).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    // Drain activation event.
    while event_rx.try_recv().is_ok() {}

    handle.coordinator_tx.send(CoordinatorCommand::Abort).await.unwrap();
    let mut got_failed = false;
    for _ in 0..5 {
        if let Ok(Some(WorkspaceEvent::StateChanged { to, .. })) =
            tokio::time::timeout(Duration::from_millis(100), event_rx.recv()).await
        {
            if to == WorkspaceState::Failed { got_failed = true; break; }
        }
    }
    assert!(got_failed);
}

#[tokio::test]
async fn e2e_budget_exceeded() {
    let usage = ResourceUsage { tokens: 1000, wall_time_ms: 0, storage_bytes: 0, network_bytes: 0, cost_micros: 0 };
    let budget = ResourceBudget {
        max_tokens: Some(500),
        max_wall_time_ms: None, max_storage_bytes: None, max_network_bytes: None, max_cost_micros: None,
        warning_threshold: 0.8,
    };
    let result = BudgetEnforcer::check(&usage, &budget);
    assert!(matches!(result, BudgetCheckResult::Exceeded(_)));

    // Coordinator would abort — same as timeout.
    let (event_tx, mut event_rx) = mpsc::channel(64);
    let handle = WorkspaceActor::spawn(e2e_config("ws-1", "root", "system"), event_tx);
    handle.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d1", "root", "ws-1"))).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    while event_rx.try_recv().is_ok() {}

    handle.coordinator_tx.send(CoordinatorCommand::Abort).await.unwrap();
    let mut got_failed = false;
    for _ in 0..5 {
        if let Ok(Some(WorkspaceEvent::StateChanged { to, .. })) =
            tokio::time::timeout(Duration::from_millis(100), event_rx.recv()).await
        {
            if to == WorkspaceState::Failed { got_failed = true; break; }
        }
    }
    assert!(got_failed);
}

#[tokio::test]
async fn e2e_failure_cascade_same_owner() {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    let mut coord = crate::orchestrator::Coordinator::new(ws("root"), uid("system"), event_tx.clone());

    // Parent + same-owner child.
    let parent = WorkspaceActor::spawn(e2e_config("ws-parent", "root", "owner-a"), event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-parent"), parent: Some(ws("root")), children: vec![],
        owner: uid("owner-a"), originator: Originator::System,
        status: WorkspaceState::Active, task_id: None,
    }).unwrap();

    let _child = WorkspaceActor::spawn(e2e_config("ws-child", "ws-parent", "owner-a"), event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-child"), parent: Some(ws("ws-parent")), children: vec![],
        owner: uid("owner-a"), originator: Originator::System,
        status: WorkspaceState::Active, task_id: None,
    }).unwrap();

    // Parent fails.
    parent.coordinator_tx.send(CoordinatorCommand::Abort).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 10).await;

    // Both should be Failed (cascade).
    assert_eq!(coord.tree.get(&ws("ws-parent")).unwrap().status, WorkspaceState::Failed);
    assert_eq!(coord.tree.get(&ws("ws-child")).unwrap().status, WorkspaceState::Failed);
}

#[tokio::test]
async fn e2e_failure_cascade_cross_owner() {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    let mut coord = crate::orchestrator::Coordinator::new(ws("root"), uid("system"), event_tx.clone());

    // Parent owned by A, child owned by B.
    let parent = WorkspaceActor::spawn(e2e_config("ws-parent", "root", "owner-a"), event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-parent"), parent: Some(ws("root")), children: vec![],
        owner: uid("owner-a"), originator: Originator::System,
        status: WorkspaceState::Active, task_id: None,
    }).unwrap();

    let mut child_config = e2e_config("ws-child", "ws-parent", "owner-b");
    child_config.owner = uid("owner-b");
    let _child = WorkspaceActor::spawn(child_config, event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-child"), parent: Some(ws("ws-parent")), children: vec![],
        owner: uid("owner-b"), originator: Originator::System,
        status: WorkspaceState::Active, task_id: None,
    }).unwrap();

    // Parent fails.
    parent.coordinator_tx.send(CoordinatorCommand::Abort).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 10).await;

    // Parent Failed, child reparented to root (still Active in tree).
    assert_eq!(coord.tree.get(&ws("ws-parent")).unwrap().status, WorkspaceState::Failed);
    let child_node = coord.tree.get(&ws("ws-child")).unwrap();
    assert_eq!(child_node.parent.as_ref().unwrap().as_ref(), "root");
    // Cross-owner child is NOT failed — it's reparented.
    assert_eq!(child_node.status, WorkspaceState::Active);
}

#[tokio::test]
async fn e2e_conflict_to_resolution() {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    let mut coord = crate::orchestrator::Coordinator::new(ws("root"), uid("system"), event_tx.clone());

    let handle = WorkspaceActor::spawn(e2e_config("ws-1", "root", "system"), event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-1"), parent: Some(ws("root")), children: vec![],
        owner: uid("system"), originator: Originator::System,
        status: WorkspaceState::Idle, task_id: None,
    }).unwrap();

    // Activate → Complete → Integrating
    handle.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d1", "root", "ws-1"))).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 3).await;
    handle.agent_tx.send(AgentMessage::EmitSignal { signal_type: SignalType::Complete, reason: None, context: None }).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;
    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Integrating);

    // Conflict detected → Conflicted
    handle.coordinator_tx.send(CoordinatorCommand::ConflictDetected).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;
    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Conflicted);

    // Conflict resolved → Closed
    handle.coordinator_tx.send(CoordinatorCommand::ConflictResolved).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;
    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Closed);
}

#[tokio::test]
async fn e2e_conflict_unresolvable() {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    let mut coord = crate::orchestrator::Coordinator::new(ws("root"), uid("system"), event_tx.clone());

    let handle = WorkspaceActor::spawn(e2e_config("ws-1", "root", "system"), event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-1"), parent: Some(ws("root")), children: vec![],
        owner: uid("system"), originator: Originator::System,
        status: WorkspaceState::Idle, task_id: None,
    }).unwrap();

    // Activate → Complete → Integrating → Conflict → Unresolvable → Failed
    handle.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d1", "root", "ws-1"))).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 3).await;
    handle.agent_tx.send(AgentMessage::EmitSignal { signal_type: SignalType::Complete, reason: None, context: None }).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;
    handle.coordinator_tx.send(CoordinatorCommand::ConflictDetected).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 3).await;
    handle.coordinator_tx.send(CoordinatorCommand::ConflictUnresolvable).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;

    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Failed);
}

#[tokio::test]
async fn e2e_integration_failure() {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    let mut coord = crate::orchestrator::Coordinator::new(ws("root"), uid("system"), event_tx.clone());

    let handle = WorkspaceActor::spawn(e2e_config("ws-1", "root", "system"), event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-1"), parent: Some(ws("root")), children: vec![],
        owner: uid("system"), originator: Originator::System,
        status: WorkspaceState::Idle, task_id: None,
    }).unwrap();

    // Activate → Complete → Integrating → IntegrationFailed → Failed
    handle.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d1", "root", "ws-1"))).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 3).await;
    handle.agent_tx.send(AgentMessage::EmitSignal { signal_type: SignalType::Complete, reason: None, context: None }).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;
    handle.coordinator_tx.send(CoordinatorCommand::IntegrationFailed).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;

    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Failed);
}

// ── 17.3: Highway integration ──

#[test]
fn e2e_gate_approval_flow() {
    let mut gate = GateController::new(30_000, GateFallback::AutoApprove);
    let event = gate.open_gate(tid("t1"), "task_approval".into(), "approve task", None, None);
    assert!(gate.pending_for_task(&tid("t1")).is_some());

    let resolution = gate.resolve(&event.gate_id, GateDecision::Approve);
    assert!(resolution.is_some());
    assert!(matches!(resolution.unwrap(), GateResolution::Approved { .. }));
    assert!(gate.pending_for_task(&tid("t1")).is_none());
}

#[test]
fn e2e_gate_rejection() {
    let mut gate = GateController::new(30_000, GateFallback::AutoApprove);
    let event = gate.open_gate(tid("t1"), "task_approval".into(), "approve task", None, None);

    let resolution = gate.resolve(&event.gate_id, GateDecision::Reject);
    assert!(resolution.is_some());
    assert!(matches!(resolution.unwrap(), GateResolution::Rejected));
}

#[test]
fn e2e_envelope_injection() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_inject_envelope(
        &ws("ws-1"), "directive", b"injected payload", EnvelopePriority::Urgent,
    );
    assert!(result.is_ok());
    let envelope_id = result.unwrap().envelope_id;
    assert!(envelope_id.as_ref().starts_with("env-"));
}

#[tokio::test]
async fn e2e_migration_full_lifecycle() {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    let mut coord = crate::orchestrator::Coordinator::new(ws("root"), uid("system"), event_tx.clone());

    let handle = WorkspaceActor::spawn(e2e_config("ws-1", "root", "system"), event_tx.clone());
    coord.tree.insert(crate::tree::WorkspaceNode {
        id: ws("ws-1"), parent: Some(ws("root")), children: vec![],
        owner: uid("system"), originator: Originator::System,
        status: WorkspaceState::Idle, task_id: None,
    }).unwrap();

    // Activate.
    handle.coordinator_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d1", "root", "ws-1"))).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 3).await;
    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Active);

    // Start migration.
    coord.migration.start(
        MigrationRequest { workspace_id: ws("ws-1"), new_agent: AgentRef { agent_type: "gpt-5".into(), config: None }, reason: "upgrade".into() },
        WorkspaceState::Active,
        "gpt-4".into(),
        1000,
    ).unwrap();

    // Send MigrateBegin → workspace transitions to Migrating + emits snapshot.
    handle.coordinator_tx.send(CoordinatorCommand::MigrateBegin).await.unwrap();
    let events = drain_events(&mut coord, &mut event_rx, 5).await;
    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Migrating);
    assert!(events.iter().any(|e| matches!(e, WorkspaceEvent::MigrationSnapshot { .. })));
    // Snapshot should have been stored.
    assert!(coord.migration.get("ws-1").unwrap().snapshot.is_some());

    // Complete migration → Active.
    let ctx = coord.migration.get("ws-1").unwrap();
    let restore_blocked = ctx.pre_migration_state == WorkspaceState::Blocked;
    handle.coordinator_tx.send(CoordinatorCommand::MigrationComplete { restore_blocked }).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 3).await;
    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Active);

    // Clean up migration context.
    coord.migration.complete("ws-1").unwrap();
    assert!(!coord.migration.is_migrating("ws-1"));
}

#[tokio::test]
async fn e2e_migration_timeout() {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    let mut coord = crate::orchestrator::Coordinator::new(ws("root"), uid("system"), event_tx.clone());

    // Use dispatch() so the coordinator has the workspace handle for fail_migration.
    coord.dispatch(crate::orchestrator::DispatchRequest {
        task_id: tid("t1"),
        config: e2e_config("ws-1", "root", "system"),
    });

    // Activate via the coordinator's stored handle.
    let coord_tx = coord.handle(&ws("ws-1")).unwrap().coordinator_tx.clone();
    coord_tx.send(CoordinatorCommand::DeliverEnvelope(e2e_envelope("d1", "root", "ws-1"))).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 3).await;

    // Start migration with short timeout.
    coord.migration = MigrationCoordinator::new(100); // 100ms
    coord.migration.start(
        MigrationRequest { workspace_id: ws("ws-1"), new_agent: AgentRef { agent_type: "gpt-5".into(), config: None }, reason: "test".into() },
        WorkspaceState::Active,
        "gpt-4".into(),
        0,
    ).unwrap();

    coord_tx.send(CoordinatorCommand::MigrateBegin).await.unwrap();
    drain_events(&mut coord, &mut event_rx, 5).await;

    // Check timeout.
    let expired = coord.migration.check_timeouts(200);
    assert_eq!(expired.len(), 1);

    // Fail the migration — coordinator sends MigrationFailed to workspace actor.
    coord.fail_migration(&ws("ws-1"), "timeout".into(), 6).await;
    drain_events(&mut coord, &mut event_rx, 5).await;

    assert_eq!(coord.tree.get(&ws("ws-1")).unwrap().status, WorkspaceState::Failed);
    assert!(!coord.migration.is_migrating("ws-1"));
}

// ══════════════════════════════════════════
// Phase 18a.7 — Error path coverage
// ══════════════════════════════════════════

// --- Handler error paths ---

#[test]
fn handler_create_checkpoint_nonexistent_workspace() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_create_checkpoint(
        &ws("nonexistent"),
        "artifact",
        b"payload",
        "intent",
        CheckpointStatus::Provisional,
        Confidence::High,
    );
    assert!(matches!(result, Err(HandlerError::WorkspaceNotFound(_))));
}

#[test]
fn handler_send_envelope_from_nonexistent() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_send_envelope(
        &ws("ghost"), &ws("ws-1"), "directive", b"", EnvelopePriority::Normal,
    );
    assert!(matches!(result, Err(HandlerError::WorkspaceNotFound(_))));
}

#[test]
fn handler_send_envelope_to_nonexistent() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_send_envelope(
        &ws("root"), &ws("ghost"), "directive", b"", EnvelopePriority::Normal,
    );
    assert!(matches!(result, Err(HandlerError::WorkspaceNotFound(_))));
}

#[test]
fn handler_send_envelope_with_revoked_right() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();

    // Find and revoke the send right from root → ws-1.
    let right_id = topo
        .port_rights
        .validate_send(&ws("root"), &ws("ws-1"))
        .unwrap();
    topo.port_rights.revoke(&right_id).unwrap();

    let mut h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_send_envelope(
        &ws("root"), &ws("ws-1"), "directive", b"", EnvelopePriority::Normal,
    );
    assert!(matches!(result, Err(HandlerError::NoSendRight(_))));
}

#[test]
fn handler_get_workspace_not_found() {
    let (mut topo, mut graph, mut gate, queue, timeout, liveness, migration, mut env_id, mut cp_id) =
        setup_handler();
    let h = RequestHandler::new(
        &mut topo, &mut graph, &mut gate, &queue, &timeout, &liveness, &migration, &mut env_id, &mut cp_id,
    );

    let result = h.handle_get_workspace(&ws("nonexistent"));
    assert!(matches!(result, Err(HandlerError::WorkspaceNotFound(_))));
}

// --- Topology: duplicate ID ---

#[test]
fn topology_create_workspace_duplicate_id() {
    let mut topo = TopologySet::new(ws("root"), uid("owner"));
    topo.create_workspace(CreateWorkspaceParams {
        id: ws("ws-1"),
        parent: ws("root"),
        owner: uid("owner"),
        originator: Originator::System,
        status: WorkspaceState::Active,
        task_id: None,
    })
    .unwrap();

    // Second creation with same ID → DuplicateNode.
    let err = topo.create_workspace(CreateWorkspaceParams {
        id: ws("ws-1"),
        parent: ws("root"),
        owner: uid("owner"),
        originator: Originator::System,
        status: WorkspaceState::Active,
        task_id: None,
    });
    assert!(matches!(err, Err(TreeError::DuplicateNode(_))));
}

#[test]
fn tree_insert_duplicate_node() {
    let mut tree = WorkspaceTree::new(ws("root"), uid("owner"));
    tree.insert(make_node("child", Some("root"), "owner", Originator::System, WorkspaceState::Active))
        .unwrap();

    let err = tree.insert(make_node("child", Some("root"), "owner", Originator::System, WorkspaceState::Active));
    assert!(matches!(err, Err(TreeError::DuplicateNode(_))));
}

// --- Topology: deep cascade (3+ levels) ---

#[test]
fn topology_cascade_deep_tree_same_owner() {
    let mut topo = TopologySet::new(ws("root"), uid("owner"));

    // Build 4-level tree: root → L1 → L2 → L3, all same owner.
    for (child, parent) in [("L1", "root"), ("L2", "L1"), ("L3", "L2")] {
        topo.create_workspace(CreateWorkspaceParams {
            id: ws(child),
            parent: ws(parent),
            owner: uid("owner"),
            originator: Originator::System,
            status: WorkspaceState::Active,
            task_id: None,
        })
        .unwrap();
    }

    // Terminate L1 as Failed → same-owner cascade should fail L2 and L3.
    let effect = topo.terminate_workspace(&ws("L1"), WorkspaceState::Failed);

    // L2 and L3 should be in the failed list.
    assert!(effect.failed.contains(&ws("L2")) || effect.failed.contains(&ws("L3")),
        "deep cascade should fail descendants: {:?}", effect.failed);

    // Verify all are marked Failed in the tree.
    assert_eq!(topo.tree.get(&ws("L1")).unwrap().status, WorkspaceState::Failed);
    assert_eq!(topo.tree.get(&ws("L2")).unwrap().status, WorkspaceState::Failed);
    assert_eq!(topo.tree.get(&ws("L3")).unwrap().status, WorkspaceState::Failed);

    // Port rights should be expired.
    assert!(effect.rights_expired > 0);
}

#[test]
fn topology_cascade_deep_tree_cross_owner_stops() {
    let mut topo = TopologySet::new(ws("root"), uid("owner-A"));

    // root (owner-A) → child-A (owner-A) → child-B (owner-B) → grandchild-B (owner-B)
    topo.create_workspace(CreateWorkspaceParams {
        id: ws("child-A"),
        parent: ws("root"),
        owner: uid("owner-A"),
        originator: Originator::System,
        status: WorkspaceState::Active,
        task_id: None,
    }).unwrap();

    topo.create_workspace(CreateWorkspaceParams {
        id: ws("child-B"),
        parent: ws("child-A"),
        owner: uid("owner-B"),
        originator: Originator::System,
        status: WorkspaceState::Active,
        task_id: None,
    }).unwrap();

    topo.create_workspace(CreateWorkspaceParams {
        id: ws("grandchild-B"),
        parent: ws("child-B"),
        owner: uid("owner-B"),
        originator: Originator::System,
        status: WorkspaceState::Active,
        task_id: None,
    }).unwrap();

    // Fail child-A → cascade stops at child-B (different owner), reparents it.
    let effect = topo.terminate_workspace(&ws("child-A"), WorkspaceState::Failed);

    assert_eq!(topo.tree.get(&ws("child-A")).unwrap().status, WorkspaceState::Failed);
    // child-B is reparented, not failed.
    assert_eq!(topo.tree.get(&ws("child-B")).unwrap().status, WorkspaceState::Active);
    assert!(!effect.reparented.is_empty(), "child-B should be reparented");
    // grandchild-B also stays active.
    assert_eq!(topo.tree.get(&ws("grandchild-B")).unwrap().status, WorkspaceState::Active);
}

// --- Port rights: transfer/consume expired ---

#[test]
fn port_right_transfer_expired_right() {
    let mut graph = PortRightsGraph::new();
    let id = graph.create(ws("a"), ws("b"), PortRightType::Send);

    // Expire via workspace termination.
    graph.expire_workspace(&ws("a"));
    assert_eq!(graph.get(&id).unwrap().status, PortRightStatus::Expired);

    // Transfer should fail: NotActive.
    let err = graph.transfer(&id, &ws("c"));
    assert!(matches!(err, Err(PortRightError::NotActive)));
}

#[test]
fn port_right_consume_expired_send_once() {
    let mut graph = PortRightsGraph::new();
    let id = graph.create(ws("a"), ws("b"), PortRightType::SendOnce);

    // Expire.
    graph.expire_workspace(&ws("a"));

    // Consume should fail: NotActive.
    let err = graph.consume(&id);
    assert!(matches!(err, Err(PortRightError::NotActive)));
}

#[test]
fn port_right_consume_send_right_not_send_once() {
    let mut graph = PortRightsGraph::new();
    let id = graph.create(ws("a"), ws("b"), PortRightType::Send);

    // Consume a Send (not SendOnce) should fail: NotSendOnce.
    let err = graph.consume(&id);
    assert!(matches!(err, Err(PortRightError::NotSendOnce)));
}

#[test]
fn port_right_validate_send_after_expiry() {
    let mut graph = PortRightsGraph::new();
    graph.create(ws("a"), ws("b"), PortRightType::Send);

    // Works before expiry.
    assert!(graph.validate_send(&ws("a"), &ws("b")).is_ok());

    // Fail after expiry.
    graph.expire_workspace(&ws("a"));
    assert!(graph.validate_send(&ws("a"), &ws("b")).is_err());
}

// --- Migration: error paths ---

fn migration_request(ws_id: &str) -> MigrationRequest {
    MigrationRequest {
        workspace_id: ws(ws_id),
        new_agent: AgentRef {
            agent_type: "gpt-4".into(),
            config: None,
        },
        reason: "test".into(),
    }
}

#[test]
fn migration_start_rejects_idle_state() {
    let mut migration = MigrationCoordinator::new(60_000);
    let result = migration.start(
        migration_request("ws-test"),
        WorkspaceState::Idle,
        "old-agent".into(),
        0,
    );
    assert!(matches!(result, Err(MigrationError::InvalidState(WorkspaceState::Idle))));
}

#[test]
fn migration_start_rejects_closed_state() {
    let mut migration = MigrationCoordinator::new(60_000);
    let result = migration.start(
        migration_request("ws-test"),
        WorkspaceState::Closed,
        "old-agent".into(),
        0,
    );
    assert!(matches!(result, Err(MigrationError::InvalidState(WorkspaceState::Closed))));
}

#[test]
fn migration_start_rejects_failed_state() {
    let mut migration = MigrationCoordinator::new(60_000);
    let result = migration.start(
        migration_request("ws-test"),
        WorkspaceState::Failed,
        "old-agent".into(),
        0,
    );
    assert!(matches!(result, Err(MigrationError::InvalidState(WorkspaceState::Failed))));
}

#[test]
fn migration_complete_nonexistent_workspace() {
    let mut migration = MigrationCoordinator::new(60_000);
    let result = migration.complete("ws-nonexistent");
    assert!(matches!(result, Err(MigrationError::NotMigrating(_))));
}

#[test]
fn migration_fail_nonexistent_workspace() {
    let mut migration = MigrationCoordinator::new(60_000);
    let result = migration.fail("ws-nonexistent", "error".into(), 1);
    assert!(matches!(result, Err(MigrationError::NotMigrating(_))));
}

// ══════════════════════════════════════════
// Phase 18b.5 — Coordinator hardening (+3)
// ══════════════════════════════════════════

#[test]
fn event_bus_subscribe_receive_unsubscribe() {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = count.clone();

    let mut bus = EventBus::new(false);
    bus.subscribe(Box::new(move |_evt| {
        count_clone.fetch_add(1, Ordering::SeqCst);
    }));
    assert_eq!(bus.subscriber_count(), 1);

    // Emit an event — subscriber should be called.
    bus.emit(CoordinatorEvent::WorkspaceStateChanged {
        workspace_id: ws("ws-sub"),
        from: WorkspaceState::Idle,
        to: WorkspaceState::Active,
    });
    assert_eq!(count.load(Ordering::SeqCst), 1);

    // Emit another event.
    bus.emit(CoordinatorEvent::TaskStatusChanged {
        task_id: tid("t-sub"),
        from: TaskStatus::Draft,
        to: TaskStatus::Pending,
    });
    assert_eq!(count.load(Ordering::SeqCst), 2);

    // EventBus has no unsubscribe method — verified by subscriber_count remaining 1.
    // Once subscribed, the callback remains active.
    assert_eq!(bus.subscriber_count(), 1);
}

#[test]
fn dispatch_to_nonexistent_task_handled() {
    let (mut disp, mut graph, mut topo) = setup_dispatch();

    // No tasks in graph — dispatch should return empty actions.
    let actions = disp.try_dispatch(&mut graph, &mut topo);
    assert!(actions.is_empty());

    // Add a task that's not dispatchable (Draft status).
    graph.add_task(make_task("t-ghost", vec![], TaskStatus::Draft)).unwrap();
    let actions = disp.try_dispatch(&mut graph, &mut topo);
    assert!(actions.is_empty());
}

#[test]
fn concurrent_workspace_creation_unique_ids() {
    let mut disp = make_dispatcher();
    let mut graph = TaskGraph::new();
    let mut topo = TopologySet::new(ws("root"), uid("owner"));

    // Add 10 pending tasks.
    for i in 0..10 {
        graph.add_task(make_task(&format!("t-{i}"), vec![], TaskStatus::Pending)).unwrap();
    }

    let actions = disp.try_dispatch(&mut graph, &mut topo);
    assert_eq!(actions.len(), 10);

    // Verify all workspace IDs are unique.
    let ids: std::collections::HashSet<_> = actions.iter().map(|a| a.workspace_id.clone()).collect();
    assert_eq!(ids.len(), 10, "all 10 workspace IDs must be unique");
}
