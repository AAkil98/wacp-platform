//! Integration: full runtime assembly from individual crates (4 tests)

use wacp_integration_tests::*;

use wacp_coordinator::Coordinator;
use wacp_permissions::PermissionEngine;
use wacp_recovery::RecoveryEngine;
use wacp_taxonomy::Taxonomy;
use wacp_trail::{compute_chain_hash, ChainHash, InMemoryTrailStorage, TrailStorage};
use wacp_types::*;


/// Replicate Runtime::init_in_memory from individual crates.
fn assemble_runtime() -> (
    Coordinator,
    Taxonomy,
    PermissionEngine,
    tokio::sync::mpsc::Receiver<wacp_workspace::WorkspaceEvent>,
) {
    let taxonomy = Taxonomy::empty("0.1");
    let trail = InMemoryTrailStorage::new();
    let _recovered = RecoveryEngine::recover(&trail).unwrap();
    let permissions = PermissionEngine::new(&taxonomy);

    let (tx, rx) = tokio::sync::mpsc::channel(256);
    let coordinator = Coordinator::new(
        WorkspaceId::from("ws-root"),
        UserId::from("system"),
        tx,
    );

    (coordinator, taxonomy, permissions, rx)
}

#[tokio::test]
async fn full_initialization_sequence() {
    let (coord, taxonomy, engine, _rx) = assemble_runtime();

    // Taxonomy has base roles
    assert!(taxonomy.is_valid_role("worker"));
    assert!(taxonomy.is_valid_role("coordinator"));
    assert!(taxonomy.is_valid_role("observer"));

    // Tree has root
    assert!(coord.tree.get(&WorkspaceId::from("ws-root")).is_some());

    // Permission engine evaluates base actions
    assert!(engine
        .evaluate(&wacp_permissions::Action::EmitSignal {
            role: "worker".into(),
            signal_type: SignalType::Ready,
        })
        .is_ok());
}

#[tokio::test]
async fn init_dispatch_activate_shutdown() {
    let (mut coord, _taxonomy, _engine, mut rx) = assemble_runtime();

    dispatch(&mut coord, "ws-1", "task-1");
    dispatch(&mut coord, "ws-2", "task-2");

    // Activate ws-1
    let envelope = make_envelope("dir-1", "ws-root", "ws-1", "directive");
    coord.route_envelope(&WorkspaceId::from("ws-1"), envelope).await;
    drain_events(&mut coord, &mut rx, 5, 200).await;

    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-1")).unwrap().status,
        WorkspaceState::Active
    );

    // Shutdown: abort all non-root
    for ws_id in ["ws-1", "ws-2"] {
        coord.abort_workspace(&WorkspaceId::from(ws_id)).await;
    }
    drain_events(&mut coord, &mut rx, 20, 200).await;

    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-1")).unwrap().status,
        WorkspaceState::Failed
    );
    assert_eq!(
        coord.tree.get(&WorkspaceId::from("ws-2")).unwrap().status,
        WorkspaceState::Failed
    );
}

#[test]
fn recovery_roundtrip_with_trail() {
    let mut trail = InMemoryTrailStorage::new();
    let genesis = ChainHash::GENESIS;
    let h1 = {
        let bytes = br#"{"event_type":"workspace_created","workspace_id":"ws-1","new_state":"idle"}"#;
        let h = compute_chain_hash(&genesis, bytes);
        trail.append(bytes, h.as_ref()).unwrap();
        h
    };
    let _h2 = {
        let bytes = br#"{"event_type":"workspace_state_changed","workspace_id":"ws-1","new_state":"active"}"#;
        let h = compute_chain_hash(&h1, bytes);
        trail.append(bytes, h.as_ref()).unwrap();
        h
    };

    let recovered = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(recovered.last_sequence, 2);
    // Recovery parsed both entries; workspace state depends on the
    // engine recognizing the JSON schema, which is tested in trail_recovery suite.
}

#[tokio::test]
async fn taxonomy_drives_permission_evaluation() {
    let yaml = r#"
id: test
version: "1.0"
protocol_version: "0.1"
roles:
  - name: reviewer
    extends: observer
    add_capabilities: []
    remove_capabilities: []
envelope_types:
  - name: review
    permissions:
      - sender_role: reviewer
        receiver_role: worker
checkpoint_types: []
"#;
    let taxonomy = Taxonomy::load_yaml(yaml, "0.1").unwrap();
    let engine = PermissionEngine::new(&taxonomy);

    // Reviewer inherits observer; verify role is valid
    assert!(taxonomy.is_valid_role("reviewer"));

    // Reviewer can emit signals inherited from observer
    assert!(engine
        .evaluate(&wacp_permissions::Action::EmitSignal {
            role: "reviewer",
            signal_type: SignalType::Ready,
        })
        .is_ok());

    // "review" is a registered envelope type
    assert!(taxonomy.is_valid_envelope_type("review"));
}
