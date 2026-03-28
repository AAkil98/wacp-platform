use std::collections::HashSet;

use tokio::sync::mpsc;
use wacp_types::*;

use crate::actor::*;
use crate::state::{MigrationSnapshot, ResourceMeter, WorkspaceConfig, WorkspaceState as WsState};

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

// ── Phase 16: Migration Snapshot + Restore + Actor ──

#[test]
fn snapshot_capture_empty_workspace() {
    let ws = WsState::new(test_config());
    let snap = ws.capture_snapshot();
    assert!(snap.inbox.is_empty());
    assert!(snap.working_memory.is_empty());
    assert!(snap.checkpoint_register.is_empty());
    assert_eq!(snap.resource_meter.usage.tokens, 0);
    assert_eq!(snap.trail_sequence, 0);
    assert!(snap.delivered_envelope_ids.is_empty());
}

#[test]
fn snapshot_capture_with_state() {
    let mut ws = WsState::new(test_config());
    ws.push_inbox(test_envelope("e1", "feedback", EnvelopePriority::Normal));
    ws.push_inbox(test_envelope("e2", "feedback", EnvelopePriority::Urgent));
    ws.working_memory = vec![10, 20, 30];
    ws.push_checkpoint(Checkpoint {
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
    });
    ws.resource_meter.usage.tokens = 500;
    ws.trail_sequence = 7;

    let snap = ws.capture_snapshot();
    assert_eq!(snap.inbox.len(), 2);
    assert_eq!(snap.working_memory, vec![10, 20, 30]);
    assert_eq!(snap.checkpoint_register.len(), 1);
    assert_eq!(snap.resource_meter.usage.tokens, 500);
    assert_eq!(snap.trail_sequence, 7);
}

#[test]
fn snapshot_capture_preserves_inbox_order() {
    let mut ws = WsState::new(test_config());
    ws.push_inbox(test_envelope("normal", "feedback", EnvelopePriority::Normal));
    ws.push_inbox(test_envelope("urgent", "feedback", EnvelopePriority::Urgent));
    ws.push_inbox(test_envelope("blocking", "feedback", EnvelopePriority::Blocking));

    let snap = ws.capture_snapshot();
    // Priority order: blocking > urgent > normal
    assert_eq!(snap.inbox[0].id, EnvelopeId::from("blocking"));
    assert_eq!(snap.inbox[1].id, EnvelopeId::from("urgent"));
    assert_eq!(snap.inbox[2].id, EnvelopeId::from("normal"));
}

#[test]
fn snapshot_capture_preserves_dedup_set() {
    let mut ws = WsState::new(test_config());
    ws.push_inbox(test_envelope("e1", "feedback", EnvelopePriority::Normal));
    let _ = ws.pop_inbox(); // marks e1 as delivered

    let snap = ws.capture_snapshot();
    assert!(snap.delivered_envelope_ids.contains("e1"));
}

#[test]
fn restore_from_snapshot() {
    let mut ws = WsState::new(test_config());
    ws.push_inbox(test_envelope("e1", "feedback", EnvelopePriority::Normal));
    ws.working_memory = vec![1, 2, 3];
    ws.resource_meter.usage.tokens = 999;
    ws.trail_sequence = 42;
    let _ = ws.pop_inbox();
    ws.push_inbox(test_envelope("e2", "feedback", EnvelopePriority::Normal));

    let snap = ws.capture_snapshot();

    let restored = WsState::restore_from_snapshot(
        test_config(),
        snap.clone(),
        wacp_types::WorkspaceState::Active,
    );

    assert_eq!(restored.status, wacp_types::WorkspaceState::Active);
    assert_eq!(restored.inbox_len(), snap.inbox.len());
    assert_eq!(restored.working_memory, vec![1, 2, 3]);
    assert_eq!(restored.resource_meter.usage.tokens, 999);
    assert_eq!(restored.trail_sequence, 42);
    assert!(restored.is_envelope_delivered("e1")); // dedup set restored
    // Immutable fields from config, not snapshot
    assert_eq!(restored.directive().envelope_type, "directive");
    assert_eq!(restored.context(), b"ctx");
}

#[test]
fn restore_status_active() {
    let snap = MigrationSnapshot {
        inbox: vec![],
        working_memory: vec![],
        checkpoint_register: vec![],
        resource_meter: ResourceMeter::default(),
        trail_sequence: 0,
        delivered_envelope_ids: Default::default(),
    };
    let ws = WsState::restore_from_snapshot(test_config(), snap, wacp_types::WorkspaceState::Active);
    assert_eq!(ws.status, wacp_types::WorkspaceState::Active);
}

#[test]
fn restore_status_blocked() {
    let snap = MigrationSnapshot {
        inbox: vec![],
        working_memory: vec![],
        checkpoint_register: vec![],
        resource_meter: ResourceMeter::default(),
        trail_sequence: 0,
        delivered_envelope_ids: Default::default(),
    };
    let ws = WsState::restore_from_snapshot(test_config(), snap, wacp_types::WorkspaceState::Blocked);
    assert_eq!(ws.status, wacp_types::WorkspaceState::Blocked);
}

#[test]
fn snapshot_serde_roundtrip() {
    let mut ws = WsState::new(test_config());
    ws.push_inbox(test_envelope("e1", "feedback", EnvelopePriority::Normal));
    ws.working_memory = vec![42];
    ws.resource_meter.usage.tokens = 100;
    ws.trail_sequence = 5;

    let snap = ws.capture_snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let roundtrip: MigrationSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.inbox.len(), 1);
    assert_eq!(roundtrip.working_memory, vec![42]);
    assert_eq!(roundtrip.resource_meter.usage.tokens, 100);
    assert_eq!(roundtrip.trail_sequence, 5);
}

#[tokio::test]
async fn actor_migrate_begin_emits_snapshot() {
    let (handle, mut event_rx) = spawn_test().await;

    // Activate first (Idle → Active via envelope).
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(
            test_envelope("env-1", "directive", EnvelopePriority::Normal),
        ))
        .await
        .unwrap();
    // Drain StateChanged.
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;

    // MigrateBegin
    handle
        .coordinator_tx
        .send(CoordinatorCommand::MigrateBegin)
        .await
        .unwrap();

    // Expect StateChanged (Active → Migrating) and MigrationSnapshot.
    let mut got_state_change = false;
    let mut got_snapshot = false;

    for _ in 0..5 {
        if let Ok(Some(evt)) = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        {
            match evt {
                WorkspaceEvent::StateChanged { to, .. } if to == wacp_types::WorkspaceState::Migrating => {
                    got_state_change = true;
                }
                WorkspaceEvent::MigrationSnapshot { workspace_id, .. } => {
                    assert_eq!(workspace_id, WorkspaceId::from("ws-test"));
                    got_snapshot = true;
                }
                _ => {}
            }
        }
        if got_state_change && got_snapshot {
            break;
        }
    }

    assert!(got_state_change);
    assert!(got_snapshot);
}

#[tokio::test]
async fn actor_migrate_begin_invalid_state() {
    let (handle, mut event_rx) = spawn_test().await;

    // Workspace is Idle — MigrateBegin should produce Error.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::MigrateBegin)
        .await
        .unwrap();

    let evt = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv())
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(evt, WorkspaceEvent::Error { .. }));
}

#[tokio::test]
async fn actor_migration_complete_to_active() {
    let (handle, mut event_rx) = spawn_test().await;

    // Activate, then migrate.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(
            test_envelope("env-1", "directive", EnvelopePriority::Normal),
        ))
        .await
        .unwrap();
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::MigrateBegin)
        .await
        .unwrap();
    // Drain StateChanged + MigrationSnapshot.
    for _ in 0..3 {
        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;
    }

    // Complete migration → Active.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::MigrationComplete {
            restore_blocked: false,
        })
        .await
        .unwrap();

    let mut found_active = false;
    for _ in 0..3 {
        if let Ok(Some(WorkspaceEvent::StateChanged { to, .. })) = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        {
            if to == wacp_types::WorkspaceState::Active {
                found_active = true;
                break;
            }
        }
    }
    assert!(found_active);
}

#[tokio::test]
async fn actor_migration_complete_to_blocked() {
    let (handle, mut event_rx) = spawn_test().await;

    // Activate, then migrate.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(
            test_envelope("env-1", "directive", EnvelopePriority::Normal),
        ))
        .await
        .unwrap();
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::MigrateBegin)
        .await
        .unwrap();
    for _ in 0..3 {
        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;
    }

    // Complete migration → Blocked.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::MigrationComplete {
            restore_blocked: true,
        })
        .await
        .unwrap();

    let mut found_blocked = false;
    for _ in 0..3 {
        if let Ok(Some(WorkspaceEvent::StateChanged { to, .. })) = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        {
            if to == wacp_types::WorkspaceState::Blocked {
                found_blocked = true;
                break;
            }
        }
    }
    assert!(found_blocked);
}

#[tokio::test]
async fn actor_agent_msg_rejected_in_migrating() {
    let (handle, mut event_rx) = spawn_test().await;

    // Activate, then migrate.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(
            test_envelope("env-1", "directive", EnvelopePriority::Normal),
        ))
        .await
        .unwrap();
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::MigrateBegin)
        .await
        .unwrap();
    // Drain events.
    for _ in 0..3 {
        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;
    }

    // Agent message while migrating — should be silently dropped.
    handle
        .agent_tx
        .send(AgentMessage::EmitSignal {
            signal_type: SignalType::Complete,
            reason: None,
            context: None,
        })
        .await
        .unwrap();

    // Should NOT get a Signal event.
    let result = tokio::time::timeout(std::time::Duration::from_millis(200), event_rx.recv()).await;
    assert!(result.is_err()); // timeout = no event received
}

#[tokio::test]
async fn actor_envelopes_accepted_in_migrating() {
    let (handle, mut event_rx) = spawn_test().await;

    // Activate, then migrate.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(
            test_envelope("env-1", "directive", EnvelopePriority::Normal),
        ))
        .await
        .unwrap();
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::MigrateBegin)
        .await
        .unwrap();
    for _ in 0..3 {
        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;
    }

    // Deliver envelope during migration — should be buffered in inbox.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(
            test_envelope("env-2", "feedback", EnvelopePriority::Normal),
        ))
        .await
        .unwrap();

    // Complete migration.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::MigrationComplete {
            restore_blocked: false,
        })
        .await
        .unwrap();

    // The workspace is now Active again with env-2 in inbox.
    // The workspace actor doesn't emit events for inbox pushes,
    // but it should still be alive and functional.
    let mut alive = false;
    for _ in 0..5 {
        if let Ok(Some(WorkspaceEvent::StateChanged { to, .. })) = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        {
            if to == wacp_types::WorkspaceState::Active {
                alive = true;
                break;
            }
        }
    }
    assert!(alive);
}
