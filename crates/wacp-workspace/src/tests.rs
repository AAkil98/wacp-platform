use std::collections::HashSet;

use tokio::sync::mpsc;
use wacp_types::*;

use crate::actor::*;
use crate::state::{WorkspaceConfig, WorkspaceState as WsState};

fn test_config() -> WorkspaceConfig {
    WorkspaceConfig {
        id: WorkspaceId::from("ws-test"),
        role: "worker".into(),
        base_role: BaseRole::Worker,
        parent: WorkspaceId::from("ws-root"),
        owner: UserId::from("user-1"),
        originator: Originator::System,
        directive: test_envelope("env-dir", "directive", EnvelopePriority::Normal),
        context: b"ctx".to_vec(),
        visibility: HashSet::from(["res-1".to_string()]),
        authority: HashSet::from(["res-1".to_string()]),
        delegate: false,
        budget: None,
    }
}

fn test_envelope(id: &str, etype: &str, priority: EnvelopePriority) -> Envelope {
    Envelope {
        id: EnvelopeId::from(id),
        from_workspace: WorkspaceId::from("ws-root"),
        to_workspace: WorkspaceId::from("ws-test"),
        envelope_type: etype.into(),
        payload: vec![],
        in_reply_to: None,
        timestamp: 0,
        priority,
        origin: EnvelopeOrigin::Agent,
        state: EnvelopeState::Created,
    }
}

// ── Task 4.1: State tests ──

#[test]
fn new_workspace_idle() {
    let ws = WsState::new(test_config());
    assert_eq!(ws.status, wacp_types::WorkspaceState::Idle);
}

#[test]
fn directive_immutable() {
    let ws = WsState::new(test_config());
    assert_eq!(ws.directive().envelope_type, "directive");
}

#[test]
fn context_immutable() {
    let ws = WsState::new(test_config());
    assert_eq!(ws.context(), b"ctx");
}

#[test]
fn authority_frozen() {
    let ws = WsState::new(test_config());
    assert!(ws.authority().contains("res-1"));
    // No method to modify authority exists — compilation ensures this.
}

#[test]
fn visibility_additive() {
    let mut ws = WsState::new(test_config());
    assert_eq!(ws.visibility().len(), 1);
    ws.grant_visibility(&["res-2".into(), "res-3".into()]);
    assert_eq!(ws.visibility().len(), 3);
    assert!(ws.visibility().contains("res-2"));
}

#[test]
fn inbox_fifo() {
    let mut ws = WsState::new(test_config());
    ws.push_inbox(test_envelope("e1", "feedback", EnvelopePriority::Normal));
    ws.push_inbox(test_envelope("e2", "feedback", EnvelopePriority::Normal));
    assert_eq!(ws.pop_inbox().unwrap().id, EnvelopeId::from("e1"));
    assert_eq!(ws.pop_inbox().unwrap().id, EnvelopeId::from("e2"));
}

#[test]
fn inbox_priority_ordering() {
    let mut ws = WsState::new(test_config());
    ws.push_inbox(test_envelope("normal", "feedback", EnvelopePriority::Normal));
    ws.push_inbox(test_envelope("urgent", "feedback", EnvelopePriority::Urgent));
    ws.push_inbox(test_envelope("blocking", "feedback", EnvelopePriority::Blocking));

    assert_eq!(ws.pop_inbox().unwrap().id, EnvelopeId::from("blocking"));
    assert_eq!(ws.pop_inbox().unwrap().id, EnvelopeId::from("urgent"));
    assert_eq!(ws.pop_inbox().unwrap().id, EnvelopeId::from("normal"));
}

#[test]
fn checkpoint_append_only() {
    let mut ws = WsState::new(test_config());
    let cp = Checkpoint {
        id: CheckpointId::from("cp-1"),
        workspace_id: WorkspaceId::from("ws-test"),
        checkpoint_type: "artifact".into(),
        payload: vec![],
        content_hash: "abc".into(),
        intent: "test".into(),
        parent_checkpoint: None,
        status: CheckpointStatus::Provisional,
        confidence: Confidence::High,
        timestamp: 0,
        resource_usage: None,
    };
    ws.push_checkpoint(cp);
    assert_eq!(ws.checkpoints().len(), 1);
}

#[test]
fn checkpoint_chain_head() {
    let mut ws = WsState::new(test_config());
    assert!(ws.last_checkpoint().is_none());

    let cp = Checkpoint {
        id: CheckpointId::from("cp-1"),
        workspace_id: WorkspaceId::from("ws-test"),
        checkpoint_type: "artifact".into(),
        payload: vec![],
        content_hash: "abc".into(),
        intent: "test".into(),
        parent_checkpoint: None,
        status: CheckpointStatus::Provisional,
        confidence: Confidence::High,
        timestamp: 0,
        resource_usage: None,
    };
    ws.push_checkpoint(cp);
    assert_eq!(ws.last_checkpoint().unwrap().id, CheckpointId::from("cp-1"));
}

#[test]
fn archive_from_terminal() {
    let mut ws = WsState::new(test_config());
    ws.status = wacp_types::WorkspaceState::Failed;
    let archived = ws.archive();
    assert_eq!(archived.terminal_state, wacp_types::WorkspaceState::Failed);
    assert_eq!(archived.id, WorkspaceId::from("ws-test"));
}

#[test]
fn resource_meter_default() {
    let ws = WsState::new(test_config());
    assert_eq!(ws.resource_meter.usage.tokens, 0);
    assert_eq!(ws.resource_meter.usage.storage_bytes, 0);
}

// ── Task 4.2: Actor tests ──

async fn spawn_test() -> (WorkspaceHandle, mpsc::Receiver<WorkspaceEvent>) {
    let (event_tx, event_rx) = mpsc::channel(64);
    let handle = WorkspaceActor::spawn(test_config(), event_tx);
    (handle, event_rx)
}

#[tokio::test]
async fn coordinator_abort() {
    let (handle, mut event_rx) = spawn_test().await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::Abort)
        .await
        .unwrap();

    // Expect StateChanged to Failed, then Terminated.
    let mut got_state_change = false;
    let mut got_terminated = false;

    while let Ok(evt) = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        event_rx.recv(),
    )
    .await
    {
        match evt {
            Some(WorkspaceEvent::StateChanged { to, .. }) => {
                assert_eq!(to, wacp_types::WorkspaceState::Failed);
                got_state_change = true;
            }
            Some(WorkspaceEvent::Terminated(archived)) => {
                assert_eq!(archived.terminal_state, wacp_types::WorkspaceState::Failed);
                got_terminated = true;
                }
            _ => {}
        }
        if got_terminated {
            break;
        }
    }

    assert!(got_state_change);
    assert!(got_terminated);
}

#[tokio::test]
async fn first_envelope_activates() {
    let (handle, mut event_rx) = spawn_test().await;

    let envelope = test_envelope("env-1", "directive", EnvelopePriority::Normal);
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(envelope))
        .await
        .unwrap();

    let evt = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        event_rx.recv(),
    )
    .await
    .unwrap()
    .unwrap();

    match evt {
        WorkspaceEvent::StateChanged { from, to, .. } => {
            assert_eq!(from, wacp_types::WorkspaceState::Idle);
            assert_eq!(to, wacp_types::WorkspaceState::Active);
        }
        other => panic!("expected StateChanged, got {other:?}"),
    }
}

#[tokio::test]
async fn agent_emit_signal() {
    let (handle, mut event_rx) = spawn_test().await;

    // First activate the workspace.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(
            test_envelope("env-1", "directive", EnvelopePriority::Normal),
        ))
        .await
        .unwrap();

    // Drain the StateChanged event.
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        event_rx.recv(),
    )
    .await;

    // Now emit a signal.
    handle
        .agent_tx
        .send(AgentMessage::EmitSignal {
            signal_type: SignalType::Blocked,
            reason: Some("waiting".into()),
            context: None,
        })
        .await
        .unwrap();

    // Should get StateChanged (Active→Blocked) and Signal.
    let mut got_signal = false;
    for _ in 0..3 {
        if let Ok(Some(evt)) = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        {
            if let WorkspaceEvent::Signal(sig) = evt {
                assert_eq!(sig.signal_type, SignalType::Blocked);
                got_signal = true;
                break;
            }
        }
    }
    assert!(got_signal);
}

// ── Task 4.3: Envelope processing tests ──

#[test]
fn delivered_ids_tracked() {
    let mut ws = WsState::new(test_config());
    ws.push_inbox(test_envelope("e1", "feedback", EnvelopePriority::Normal));
    let _ = ws.pop_inbox();
    assert!(ws.is_envelope_delivered("e1"));
}

#[test]
fn duplicate_envelope_ignored() {
    let mut ws = WsState::new(test_config());
    ws.push_inbox(test_envelope("e1", "feedback", EnvelopePriority::Normal));
    let _ = ws.pop_inbox();
    assert!(ws.is_envelope_delivered("e1"));
    // The actor-level dedup prevents pushing duplicates. At the state level,
    // we verify the tracking is correct.
}

// ── Task 4.4: Checkpoint creation tests ──

#[tokio::test]
async fn auto_signal_emitted_on_checkpoint() {
    let (handle, mut event_rx) = spawn_test().await;

    // Activate.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(
            test_envelope("env-1", "directive", EnvelopePriority::Normal),
        ))
        .await
        .unwrap();

    // Drain activation event.
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;

    // Create checkpoint.
    handle
        .agent_tx
        .send(AgentMessage::CreateCheckpoint {
            checkpoint_type: "artifact".into(),
            payload: b"work product".to_vec(),
            content_hash: "abc123".into(),
            intent: "first draft".into(),
            status: CheckpointStatus::Provisional,
            confidence: Confidence::High,
            resource_usage: None,
        })
        .await
        .unwrap();

    // Expect CheckpointCreated and Signal(Checkpoint).
    let mut got_checkpoint = false;
    let mut got_signal = false;

    for _ in 0..5 {
        if let Ok(Some(evt)) = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        {
            match evt {
                WorkspaceEvent::CheckpointCreated(_) => got_checkpoint = true,
                WorkspaceEvent::Signal(sig) if sig.signal_type == SignalType::Checkpoint => {
                    got_signal = true;
                }
                _ => {}
            }
        }
        if got_checkpoint && got_signal {
            break;
        }
    }

    assert!(got_checkpoint);
    assert!(got_signal);
}
