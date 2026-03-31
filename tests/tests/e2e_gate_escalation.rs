//! E2E: Gate + escalation flows (6 tests — E7–E12)

use wacp_integration_tests::e2e::E2eHarness;
use wacp_transport::wacp_v1;

// ── E7: Gate approval ──

#[tokio::test]
async fn e7_gate_approval() {
    let harness = E2eHarness::start().await;
    let mut highway = harness.highway_client().await;

    highway
        .authenticate(wacp_v1::AuthenticateRequest {
            auth_token: "admin".into(),
        })
        .await
        .unwrap();

    let ack = highway
        .respond_to_gate(wacp_v1::GateResponse {
            gate_id: "gate-e7".into(),
            decision: wacp_v1::GateDecision::Approve.into(),
            modifications: vec![],
            client_request_id: "r7".into(),
        })
        .await
        .unwrap();

    assert!(ack.get_ref().applied);
    harness.shutdown().await;
}

// ── E8: Gate rejection ──

#[tokio::test]
async fn e8_gate_rejection() {
    let harness = E2eHarness::start().await;
    let mut highway = harness.highway_client().await;

    highway
        .authenticate(wacp_v1::AuthenticateRequest {
            auth_token: "admin".into(),
        })
        .await
        .unwrap();

    let ack = highway
        .respond_to_gate(wacp_v1::GateResponse {
            gate_id: "gate-e8".into(),
            decision: wacp_v1::GateDecision::Reject.into(),
            modifications: vec![],
            client_request_id: "r8".into(),
        })
        .await
        .unwrap();

    assert!(ack.get_ref().applied);
    harness.shutdown().await;
}

// ── E9: Gate with modify decision ──

#[tokio::test]
async fn e9_gate_modify() {
    let harness = E2eHarness::start().await;
    let mut highway = harness.highway_client().await;

    highway
        .authenticate(wacp_v1::AuthenticateRequest {
            auth_token: "admin".into(),
        })
        .await
        .unwrap();

    let ack = highway
        .respond_to_gate(wacp_v1::GateResponse {
            gate_id: "gate-e9".into(),
            decision: wacp_v1::GateDecision::Modify.into(),
            modifications: b"adjusted params".to_vec(),
            client_request_id: "r9".into(),
        })
        .await
        .unwrap();

    assert!(ack.get_ref().applied);
    harness.shutdown().await;
}

// ── E10: Escalation feedback ──

#[tokio::test]
async fn e10_escalation_feedback() {
    let harness = E2eHarness::start().await;
    let mut highway = harness.highway_client().await;

    highway
        .authenticate(wacp_v1::AuthenticateRequest {
            auth_token: "admin".into(),
        })
        .await
        .unwrap();

    let ack = highway
        .respond_to_escalation(wacp_v1::EscalationResponse {
            escalation_id: "esc-e10".into(),
            action: Some(wacp_v1::escalation_response::Action::Feedback(
                wacp_v1::Envelope {
                    r#type: "feedback".into(),
                    payload: b"try this approach".to_vec(),
                    ..Default::default()
                },
            )),
            client_request_id: "r10".into(),
        })
        .await
        .unwrap();

    assert!(ack.get_ref().applied);
    harness.shutdown().await;
}

// ── E11: Escalation abort ──

#[tokio::test]
async fn e11_escalation_abort() {
    let harness = E2eHarness::start().await;
    let mut highway = harness.highway_client().await;

    highway
        .authenticate(wacp_v1::AuthenticateRequest {
            auth_token: "admin".into(),
        })
        .await
        .unwrap();

    let ack = highway
        .respond_to_escalation(wacp_v1::EscalationResponse {
            escalation_id: "esc-e11".into(),
            action: Some(wacp_v1::escalation_response::Action::Abort(true)),
            client_request_id: "r11".into(),
        })
        .await
        .unwrap();

    assert!(ack.get_ref().applied);
    harness.shutdown().await;
}

// ── E12: Escalation delegate ──

#[tokio::test]
async fn e12_escalation_delegate() {
    let harness = E2eHarness::start().await;
    let mut highway = harness.highway_client().await;

    highway
        .authenticate(wacp_v1::AuthenticateRequest {
            auth_token: "admin".into(),
        })
        .await
        .unwrap();

    let ack = highway
        .respond_to_escalation(wacp_v1::EscalationResponse {
            escalation_id: "esc-e12".into(),
            action: Some(wacp_v1::escalation_response::Action::DelegateToCoordinator(
                true,
            )),
            client_request_id: "r12".into(),
        })
        .await
        .unwrap();

    assert!(ack.get_ref().applied);
    harness.shutdown().await;
}
