use wacp_fsm::TaskTrigger;
use wacp_types::*;

use crate::integration::*;
use crate::task_graph::*;
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
