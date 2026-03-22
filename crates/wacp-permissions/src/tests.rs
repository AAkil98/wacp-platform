use wacp_taxonomy::Taxonomy;
use wacp_types::*;

use crate::*;

const PV: &str = "0.1";

fn base_engine() -> PermissionEngine {
    let t = Taxonomy::empty(PV);
    PermissionEngine::new(&t)
}

fn reviewer_engine() -> PermissionEngine {
    let yaml = r#"
id: test
version: "1.0"
protocol_version: "0.1"
roles:
  - name: reviewer
    extends: worker
    add:
      - "send: report → coordinator"
    remove:
      - "send: query → coordinator"
    checkpoint_types:
      - review
envelope_types:
  - name: report
    permissions:
      - sender_role: reviewer
        receiver_role: coordinator
checkpoint_types:
  - name: review
    permitted_roles:
      - reviewer
"#;
    let t = Taxonomy::load_yaml(yaml, PV).unwrap();
    PermissionEngine::new(&t)
}

fn ws(id: &str) -> WorkspaceId {
    WorkspaceId::from(id)
}

// --- Permission matrix tests ---

#[test]
fn base_matrix_coordinator_directive() {
    let mut e = base_engine();
    e.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws("coord"),
        target: ws("w1"),
    });
    let result = e.evaluate(&Action::SendEnvelope {
        sender_role: "coordinator",
        envelope_type: "directive",
        receiver_role: "worker",
        sender_workspace: &ws("coord"),
        receiver_workspace: &ws("w1"),
        origin: EnvelopeOrigin::Agent,
    });
    assert!(result.is_ok());
}

#[test]
fn base_matrix_coordinator_feedback() {
    let mut e = base_engine();
    e.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws("coord"),
        target: ws("w1"),
    });
    let result = e.evaluate(&Action::SendEnvelope {
        sender_role: "coordinator",
        envelope_type: "feedback",
        receiver_role: "worker",
        sender_workspace: &ws("coord"),
        receiver_workspace: &ws("w1"),
        origin: EnvelopeOrigin::Agent,
    });
    assert!(result.is_ok());
}

#[test]
fn base_matrix_worker_query() {
    let mut e = base_engine();
    e.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws("w1"),
        target: ws("coord"),
    });
    let result = e.evaluate(&Action::SendEnvelope {
        sender_role: "worker",
        envelope_type: "query",
        receiver_role: "coordinator",
        sender_workspace: &ws("w1"),
        receiver_workspace: &ws("coord"),
        origin: EnvelopeOrigin::Agent,
    });
    assert!(result.is_ok());
}

#[test]
fn base_matrix_deny_worker_directive() {
    let mut e = base_engine();
    e.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws("w1"),
        target: ws("w2"),
    });
    let result = e.evaluate(&Action::SendEnvelope {
        sender_role: "worker",
        envelope_type: "directive",
        receiver_role: "worker",
        sender_workspace: &ws("w1"),
        receiver_workspace: &ws("w2"),
        origin: EnvelopeOrigin::Agent,
    });
    assert!(matches!(
        result,
        Err(PermissionDenied::NotInPermissionMatrix { .. })
    ));
}

#[test]
fn base_matrix_deny_observer_send() {
    let mut e = base_engine();
    e.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws("obs"),
        target: ws("w1"),
    });
    let result = e.evaluate(&Action::SendEnvelope {
        sender_role: "observer",
        envelope_type: "query",
        receiver_role: "coordinator",
        sender_workspace: &ws("obs"),
        receiver_workspace: &ws("w1"),
        origin: EnvelopeOrigin::Agent,
    });
    assert!(matches!(
        result,
        Err(PermissionDenied::NotInPermissionMatrix { .. })
    ));
}

#[test]
fn custom_type_permission() {
    let mut e = reviewer_engine();
    e.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws("rev"),
        target: ws("coord"),
    });
    let result = e.evaluate(&Action::SendEnvelope {
        sender_role: "reviewer",
        envelope_type: "report",
        receiver_role: "coordinator",
        sender_workspace: &ws("rev"),
        receiver_workspace: &ws("coord"),
        origin: EnvelopeOrigin::Agent,
    });
    assert!(result.is_ok());
}

// --- Checkpoint tests ---

#[test]
fn checkpoint_worker_artifact() {
    let e = base_engine();
    let result = e.evaluate(&Action::CreateCheckpoint {
        role: "worker",
        checkpoint_type: "artifact",
    });
    assert!(result.is_ok());
}

#[test]
fn checkpoint_observer_observation() {
    let e = base_engine();
    let result = e.evaluate(&Action::CreateCheckpoint {
        role: "observer",
        checkpoint_type: "observation",
    });
    assert!(result.is_ok());
}

#[test]
fn checkpoint_deny_worker_observation() {
    let e = base_engine();
    let result = e.evaluate(&Action::CreateCheckpoint {
        role: "worker",
        checkpoint_type: "observation",
    });
    assert!(matches!(
        result,
        Err(PermissionDenied::CheckpointTypeNotPermitted { .. })
    ));
}

#[test]
fn checkpoint_custom_type() {
    let e = reviewer_engine();
    let result = e.evaluate(&Action::CreateCheckpoint {
        role: "reviewer",
        checkpoint_type: "review",
    });
    assert!(result.is_ok());
}

// --- Signal tests ---

#[test]
fn signal_worker_emit_set() {
    let e = base_engine();
    let allowed = [
        SignalType::Ready,
        SignalType::Started,
        SignalType::Blocked,
        SignalType::Checkpoint,
        SignalType::Complete,
        SignalType::Failed,
        SignalType::Escalation,
    ];
    for st in allowed {
        assert!(
            e.evaluate(&Action::EmitSignal {
                role: "worker",
                signal_type: st,
            })
            .is_ok(),
            "worker should be able to emit {st:?}"
        );
    }
}

#[test]
fn signal_deny_worker_integrate() {
    let e = base_engine();
    let result = e.evaluate(&Action::EmitSignal {
        role: "worker",
        signal_type: SignalType::Integrate,
    });
    assert!(matches!(
        result,
        Err(PermissionDenied::SignalTypeNotPermitted { .. })
    ));
}

#[test]
fn signal_coordinator_emit_set() {
    let e = base_engine();
    let allowed = [
        SignalType::Ready,
        SignalType::Started,
        SignalType::Failed,
        SignalType::Integrate,
        SignalType::Acknowledged,
    ];
    for st in allowed {
        assert!(
            e.evaluate(&Action::EmitSignal {
                role: "coordinator",
                signal_type: st,
            })
            .is_ok(),
            "coordinator should be able to emit {st:?}"
        );
    }
}

// --- Port rights tests ---

#[test]
fn port_right_grant_and_check() {
    let mut e = base_engine();
    assert!(!e.has_send_right(&ws("w1"), &ws("w2")));

    e.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws("w1"),
        target: ws("w2"),
    });
    assert!(e.has_send_right(&ws("w1"), &ws("w2")));
}

#[test]
fn port_right_revoke() {
    let mut e = base_engine();
    e.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws("w1"),
        target: ws("w2"),
    });
    assert!(e.has_send_right(&ws("w1"), &ws("w2")));

    e.revoke_port_right(&ws("w1"), &ws("w2"), PortRightType::Send);
    assert!(!e.has_send_right(&ws("w1"), &ws("w2")));
}

#[test]
fn port_right_send_once_consumed() {
    let mut e = base_engine();
    e.grant_port_right(PortRight {
        right_type: PortRightType::SendOnce,
        holder: ws("w1"),
        target: ws("w2"),
    });
    assert!(e.has_send_right(&ws("w1"), &ws("w2")));
    assert!(e.consume_send_once(&ws("w1"), &ws("w2")));
    assert!(!e.has_send_right(&ws("w1"), &ws("w2")));
}

#[test]
fn port_right_send_once_double_consume() {
    let mut e = base_engine();
    e.grant_port_right(PortRight {
        right_type: PortRightType::SendOnce,
        holder: ws("w1"),
        target: ws("w2"),
    });
    assert!(e.consume_send_once(&ws("w1"), &ws("w2")));
    assert!(!e.consume_send_once(&ws("w1"), &ws("w2")));
}

#[test]
fn envelope_send_requires_port_right() {
    let e = base_engine();
    // Valid matrix entry but no port right.
    let result = e.evaluate(&Action::SendEnvelope {
        sender_role: "coordinator",
        envelope_type: "directive",
        receiver_role: "worker",
        sender_workspace: &ws("coord"),
        receiver_workspace: &ws("w1"),
        origin: EnvelopeOrigin::Agent,
    });
    assert!(matches!(
        result,
        Err(PermissionDenied::NoPortRight { .. })
    ));
}

#[test]
fn highway_override_skips_matrix() {
    let e = base_engine();
    // No matrix entry, no port right, but human origin.
    let result = e.evaluate(&Action::SendEnvelope {
        sender_role: "observer",
        envelope_type: "directive",
        receiver_role: "worker",
        sender_workspace: &ws("obs"),
        receiver_workspace: &ws("w1"),
        origin: EnvelopeOrigin::Human,
    });
    assert!(result.is_ok());
}

#[test]
fn default_deny() {
    let mut e = base_engine();
    e.grant_port_right(PortRight {
        right_type: PortRightType::Send,
        holder: ws("w1"),
        target: ws("w2"),
    });
    let result = e.evaluate(&Action::SendEnvelope {
        sender_role: "worker",
        envelope_type: "unknown_type",
        receiver_role: "worker",
        sender_workspace: &ws("w1"),
        receiver_workspace: &ws("w2"),
        origin: EnvelopeOrigin::Agent,
    });
    assert!(matches!(
        result,
        Err(PermissionDenied::NotInPermissionMatrix { .. })
    ));
}
