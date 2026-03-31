//! Integration: wacp-taxonomy + wacp-permissions (4 tests)

use wacp_permissions::{Action, PermissionEngine};
use wacp_taxonomy::Taxonomy;
use wacp_types::*;

fn empty_engine() -> PermissionEngine {
    let tax = Taxonomy::empty("0.1");
    PermissionEngine::new(&tax)
}

fn send_action<'a>(
    sender: &'a str,
    etype: &'a str,
    receiver: &'a str,
    from: &'a WorkspaceId,
    to: &'a WorkspaceId,
) -> Action<'a> {
    Action::SendEnvelope {
        sender_role: sender,
        envelope_type: etype,
        receiver_role: receiver,
        sender_workspace: from,
        receiver_workspace: to,
        origin: EnvelopeOrigin::Agent,
    }
}

#[test]
fn base_role_signal_permissions() {
    let engine = empty_engine();
    assert!(engine
        .evaluate(&Action::EmitSignal {
            role: "worker",
            signal_type: SignalType::Ready,
        })
        .is_ok());
    assert!(engine
        .evaluate(&Action::EmitSignal {
            role: "worker",
            signal_type: SignalType::Blocked,
        })
        .is_ok());
}

#[test]
fn base_role_envelope_permissions() {
    let mut engine = empty_engine();
    let ws_root = WorkspaceId::from("ws-root");
    let ws_1 = WorkspaceId::from("ws-1");

    // Grant port right for ws-root → ws-1
    engine.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws_root.clone(),
        target: ws_1.clone(),
    });

    // Coordinator can send directive to worker (with port right)
    assert!(engine
        .evaluate(&send_action("coordinator", "directive", "worker", &ws_root, &ws_1))
        .is_ok());
    // Worker cannot send directive (not in permission matrix)
    engine.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws_1.clone(),
        target: ws_root.clone(),
    });
    assert!(engine
        .evaluate(&send_action("worker", "directive", "coordinator", &ws_1, &ws_root))
        .is_err());
}

#[test]
fn base_role_checkpoint_permissions() {
    let engine = empty_engine();
    assert!(engine
        .evaluate(&Action::CreateCheckpoint {
            role: "worker",
            checkpoint_type: "artifact",
        })
        .is_ok());
    assert!(engine
        .evaluate(&Action::CreateCheckpoint {
            role: "observer",
            checkpoint_type: "artifact",
        })
        .is_err());
}

#[test]
fn derived_role_taxonomy_permissions() {
    let yaml = r#"
id: test
version: "1.0"
protocol_version: "0.1"
roles:
  - name: analyst
    extends: worker
    add_capabilities: []
    remove_capabilities: []
envelope_types:
  - name: analysis_report
    permissions:
      - sender_role: analyst
        receiver_role: coordinator
checkpoint_types:
  - name: analysis_result
    permitted_roles: [analyst]
"#;
    let tax = Taxonomy::load_yaml(yaml, "0.1").unwrap();
    let mut engine = PermissionEngine::new(&tax);

    // Derived role inherits worker signal permissions
    assert!(engine
        .evaluate(&Action::EmitSignal {
            role: "analyst",
            signal_type: SignalType::Complete,
        })
        .is_ok());

    // Custom envelope type (needs port right)
    let ws_a = WorkspaceId::from("ws-analyst");
    let ws_c = WorkspaceId::from("ws-coordinator");
    engine.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws_a.clone(),
        target: ws_c.clone(),
    });
    assert!(engine
        .evaluate(&send_action("analyst", "analysis_report", "coordinator", &ws_a, &ws_c))
        .is_ok());

    // Custom checkpoint type
    assert!(engine
        .evaluate(&Action::CreateCheckpoint {
            role: "analyst",
            checkpoint_type: "analysis_result",
        })
        .is_ok());

    // Worker cannot use analyst-only checkpoint type
    assert!(engine
        .evaluate(&Action::CreateCheckpoint {
            role: "worker",
            checkpoint_type: "analysis_result",
        })
        .is_err());
}
