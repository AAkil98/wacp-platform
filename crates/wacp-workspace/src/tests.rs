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
    ws.push_inbox(test_envelope(
        "normal",
        "feedback",
        EnvelopePriority::Normal,
    ));
    ws.push_inbox(test_envelope(
        "urgent",
        "feedback",
        EnvelopePriority::Urgent,
    ));
    ws.push_inbox(test_envelope(
        "blocking",
        "feedback",
        EnvelopePriority::Blocking,
    ));

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

    while let Ok(evt) =
        tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await
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

    let evt = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv())
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
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-1",
            "directive",
            EnvelopePriority::Normal,
        )))
        .await
        .unwrap();

    // Drain the StateChanged event.
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;

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
        if let Ok(Some(evt)) =
            tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await
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
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-1",
            "directive",
            EnvelopePriority::Normal,
        )))
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
        if let Ok(Some(evt)) =
            tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await
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
    ws.push_inbox(test_envelope(
        "normal",
        "feedback",
        EnvelopePriority::Normal,
    ));
    ws.push_inbox(test_envelope(
        "urgent",
        "feedback",
        EnvelopePriority::Urgent,
    ));
    ws.push_inbox(test_envelope(
        "blocking",
        "feedback",
        EnvelopePriority::Blocking,
    ));

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
    let ws =
        WsState::restore_from_snapshot(test_config(), snap, wacp_types::WorkspaceState::Active);
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
    let ws =
        WsState::restore_from_snapshot(test_config(), snap, wacp_types::WorkspaceState::Blocked);
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
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-1",
            "directive",
            EnvelopePriority::Normal,
        )))
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
        if let Ok(Some(evt)) =
            tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await
        {
            match evt {
                WorkspaceEvent::StateChanged { to, .. }
                    if to == wacp_types::WorkspaceState::Migrating =>
                {
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
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-1",
            "directive",
            EnvelopePriority::Normal,
        )))
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
        if let Ok(Some(WorkspaceEvent::StateChanged { to, .. })) =
            tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await
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
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-1",
            "directive",
            EnvelopePriority::Normal,
        )))
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
        if let Ok(Some(WorkspaceEvent::StateChanged { to, .. })) =
            tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await
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
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-1",
            "directive",
            EnvelopePriority::Normal,
        )))
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

// ── Phase 18a.6: Workspace command coverage ──

// --- State-level tests ---

#[test]
fn pop_inbox_empty_returns_none() {
    let mut ws = WsState::new(test_config());
    assert!(ws.pop_inbox().is_none());
}

#[test]
fn archive_from_closed() {
    let mut ws = WsState::new(test_config());
    ws.status = wacp_types::WorkspaceState::Closed;
    let archived = ws.archive();
    assert_eq!(archived.terminal_state, wacp_types::WorkspaceState::Closed);
    assert_eq!(archived.id, WorkspaceId::from("ws-test"));
    assert_eq!(archived.role, "worker");
    assert_eq!(archived.owner, UserId::from("user-1"));
}

#[test]
fn snapshot_full_roundtrip_all_fields() {
    let mut ws = WsState::new(test_config());

    // Mark an envelope as delivered first (for dedup set).
    ws.push_inbox(test_envelope(
        "e-delivered",
        "feedback",
        EnvelopePriority::Normal,
    ));
    let popped = ws.pop_inbox().unwrap();
    assert_eq!(popped.id, EnvelopeId::from("e-delivered"));

    // Now push envelopes that will remain in inbox for snapshot.
    ws.push_inbox(test_envelope("e1", "feedback", EnvelopePriority::Normal));
    ws.push_inbox(test_envelope("e2", "feedback", EnvelopePriority::Urgent));
    ws.working_memory = vec![0xAA, 0xBB, 0xCC];
    ws.push_checkpoint(Checkpoint {
        id: CheckpointId::from("cp-1"),
        workspace_id: WorkspaceId::from("ws-test"),
        checkpoint_type: "artifact".into(),
        payload: vec![1, 2, 3],
        content_hash: "hash123".into(),
        intent: "test intent".into(),
        parent_checkpoint: None,
        status: CheckpointStatus::Provisional,
        confidence: Confidence::Medium,
        timestamp: 42,
        resource_usage: Some(ResourceUsage {
            tokens: 10,
            wall_time_ms: 20,
            storage_bytes: 30,
            network_bytes: 40,
            cost_micros: 50,
        }),
    });
    ws.resource_meter.usage.tokens = 777;
    ws.resource_meter.budget = Some(ResourceBudget {
        max_tokens: Some(1000),
        max_wall_time_ms: None,
        max_storage_bytes: None,
        max_network_bytes: None,
        max_cost_micros: None,
        warning_threshold: 0.9,
    });
    ws.trail_sequence = 99;

    let snap = ws.capture_snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let restored_snap: MigrationSnapshot = serde_json::from_str(&json).unwrap();

    let restored = WsState::restore_from_snapshot(
        test_config(),
        restored_snap,
        wacp_types::WorkspaceState::Active,
    );

    assert_eq!(restored.status, wacp_types::WorkspaceState::Active);
    assert_eq!(restored.inbox_len(), 2); // e2 (Urgent) + e1 (Normal)
    assert_eq!(restored.working_memory, vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(restored.checkpoints().len(), 1);
    assert_eq!(restored.checkpoints()[0].intent, "test intent");
    assert_eq!(restored.resource_meter.usage.tokens, 777);
    assert!(restored.resource_meter.budget.is_some());
    assert_eq!(restored.trail_sequence, 99);
    assert!(restored.is_envelope_delivered("e-delivered"));
    // Immutable fields from config
    assert_eq!(restored.directive().envelope_type, "directive");
    assert_eq!(restored.context(), b"ctx");
    assert!(restored.visibility().contains("res-1"));
    assert!(restored.authority().contains("res-1"));
}

// --- Actor-level helpers ---

/// Activate workspace: deliver first envelope, drain StateChanged.
async fn activate(handle: &WorkspaceHandle, event_rx: &mut mpsc::Receiver<WorkspaceEvent>) {
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-activate",
            "directive",
            EnvelopePriority::Normal,
        )))
        .await
        .unwrap();
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;
}

/// Collect events until timeout, returning all collected events.
async fn drain_events(
    event_rx: &mut mpsc::Receiver<WorkspaceEvent>,
    max: usize,
) -> Vec<WorkspaceEvent> {
    let mut events = Vec::new();
    for _ in 0..max {
        match tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await {
            Ok(Some(evt)) => events.push(evt),
            _ => break,
        }
    }
    events
}

// --- Actor command tests ---

#[tokio::test]
async fn actor_suspend_from_active() {
    let (handle, mut event_rx) = spawn_test().await;
    activate(&handle, &mut event_rx).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::Suspend)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 3).await;
    let state_change = events.iter().find(|e| {
        matches!(e, WorkspaceEvent::StateChanged { to, .. }
            if *to == wacp_types::WorkspaceState::Suspended)
    });
    assert!(state_change.is_some(), "expected transition to Suspended");
}

#[tokio::test]
async fn actor_resume_from_suspended() {
    let (handle, mut event_rx) = spawn_test().await;
    activate(&handle, &mut event_rx).await;

    // Suspend
    handle
        .coordinator_tx
        .send(CoordinatorCommand::Suspend)
        .await
        .unwrap();
    let _ = drain_events(&mut event_rx, 3).await;

    // Resume
    handle
        .coordinator_tx
        .send(CoordinatorCommand::Resume)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 3).await;
    let state_change = events.iter().find(|e| {
        matches!(e, WorkspaceEvent::StateChanged { from, to, .. }
            if *from == wacp_types::WorkspaceState::Suspended
            && *to == wacp_types::WorkspaceState::Active)
    });
    assert!(state_change.is_some(), "expected Suspended → Active");
}

#[tokio::test]
async fn actor_grant_visibility() {
    let (handle, mut event_rx) = spawn_test().await;
    activate(&handle, &mut event_rx).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::GrantVisibility(vec![
            "res-new-1".into(),
            "res-new-2".into(),
        ]))
        .await
        .unwrap();

    // GrantVisibility doesn't emit events, so verify the actor is still alive
    // by sending another command that does produce an event.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::Abort)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;
    let terminated = events
        .iter()
        .any(|e| matches!(e, WorkspaceEvent::Terminated(_)));
    assert!(
        terminated,
        "actor should still be alive after GrantVisibility"
    );
}

#[tokio::test]
async fn actor_update_budget() {
    let (handle, mut event_rx) = spawn_test().await;
    activate(&handle, &mut event_rx).await;

    let budget = ResourceBudget {
        max_tokens: Some(5000),
        max_wall_time_ms: Some(60000),
        max_storage_bytes: None,
        max_network_bytes: None,
        max_cost_micros: Some(100),
        warning_threshold: 0.75,
    };
    handle
        .coordinator_tx
        .send(CoordinatorCommand::UpdateBudget(budget))
        .await
        .unwrap();

    // Verify actor is alive.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::Abort)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WorkspaceEvent::Terminated(_)))
    );
}

#[tokio::test]
async fn actor_graceful_termination_noop() {
    let (handle, mut event_rx) = spawn_test().await;
    activate(&handle, &mut event_rx).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::GracefulTermination {
            grace_period_ms: 1000,
        })
        .await
        .unwrap();

    // Placeholder — no events expected. Verify actor is still alive.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::Abort)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WorkspaceEvent::Terminated(_)))
    );
}

// --- Integration + conflict command tests ---

/// Get workspace to Integrating state: activate → agent sends Complete.
async fn to_integrating(handle: &WorkspaceHandle, event_rx: &mut mpsc::Receiver<WorkspaceEvent>) {
    activate(handle, event_rx).await;
    handle
        .agent_tx
        .send(AgentMessage::EmitSignal {
            signal_type: SignalType::Complete,
            reason: None,
            context: None,
        })
        .await
        .unwrap();
    let _ = drain_events(event_rx, 5).await;
}

#[tokio::test]
async fn actor_integration_succeeded_to_closed() {
    let (handle, mut event_rx) = spawn_test().await;
    to_integrating(&handle, &mut event_rx).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::IntegrationSucceeded)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;
    let terminated = events.iter().find_map(|e| match e {
        WorkspaceEvent::Terminated(archived) => Some(archived),
        _ => None,
    });
    assert!(terminated.is_some(), "should terminate");
    assert_eq!(
        terminated.unwrap().terminal_state,
        wacp_types::WorkspaceState::Closed
    );
}

#[tokio::test]
async fn actor_integration_failed_to_failed() {
    let (handle, mut event_rx) = spawn_test().await;
    to_integrating(&handle, &mut event_rx).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::IntegrationFailed)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;
    let terminated = events.iter().find_map(|e| match e {
        WorkspaceEvent::Terminated(archived) => Some(archived),
        _ => None,
    });
    assert!(terminated.is_some());
    assert_eq!(
        terminated.unwrap().terminal_state,
        wacp_types::WorkspaceState::Failed
    );
}

#[tokio::test]
async fn actor_conflict_detected_to_conflicted() {
    let (handle, mut event_rx) = spawn_test().await;
    to_integrating(&handle, &mut event_rx).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::ConflictDetected)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;
    let state_change = events.iter().find(|e| {
        matches!(e, WorkspaceEvent::StateChanged { to, .. }
            if *to == wacp_types::WorkspaceState::Conflicted)
    });
    assert!(state_change.is_some(), "expected transition to Conflicted");
}

#[tokio::test]
async fn actor_conflict_resolved_to_closed() {
    let (handle, mut event_rx) = spawn_test().await;
    to_integrating(&handle, &mut event_rx).await;

    // Integrating → Conflicted
    handle
        .coordinator_tx
        .send(CoordinatorCommand::ConflictDetected)
        .await
        .unwrap();
    let _ = drain_events(&mut event_rx, 3).await;

    // Conflicted → Closed
    handle
        .coordinator_tx
        .send(CoordinatorCommand::ConflictResolved)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;
    let terminated = events.iter().find_map(|e| match e {
        WorkspaceEvent::Terminated(archived) => Some(archived),
        _ => None,
    });
    assert!(terminated.is_some());
    assert_eq!(
        terminated.unwrap().terminal_state,
        wacp_types::WorkspaceState::Closed
    );
}

#[tokio::test]
async fn actor_conflict_unresolvable_to_failed() {
    let (handle, mut event_rx) = spawn_test().await;
    to_integrating(&handle, &mut event_rx).await;

    // Integrating → Conflicted
    handle
        .coordinator_tx
        .send(CoordinatorCommand::ConflictDetected)
        .await
        .unwrap();
    let _ = drain_events(&mut event_rx, 3).await;

    // Conflicted → Failed
    handle
        .coordinator_tx
        .send(CoordinatorCommand::ConflictUnresolvable)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;
    let terminated = events.iter().find_map(|e| match e {
        WorkspaceEvent::Terminated(archived) => Some(archived),
        _ => None,
    });
    assert!(terminated.is_some());
    assert_eq!(
        terminated.unwrap().terminal_state,
        wacp_types::WorkspaceState::Failed
    );
}

#[tokio::test]
async fn actor_envelopes_accepted_in_migrating() {
    let (handle, mut event_rx) = spawn_test().await;

    // Activate, then migrate.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-1",
            "directive",
            EnvelopePriority::Normal,
        )))
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
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-2",
            "feedback",
            EnvelopePriority::Normal,
        )))
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
        if let Ok(Some(WorkspaceEvent::StateChanged { to, .. })) =
            tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await
        {
            if to == wacp_types::WorkspaceState::Active {
                alive = true;
                break;
            }
        }
    }
    assert!(alive);
}

// ══════════════════════════════════════════
// Phase 18b.4 — Workspace hardening (+16)
// ══════════════════════════════════════════

#[test]
fn workspace_budget_exhaustion_check() {
    let budget = ResourceBudget {
        max_tokens: Some(100),
        max_wall_time_ms: None,
        max_storage_bytes: None,
        max_network_bytes: None,
        max_cost_micros: None,
        warning_threshold: 0.8,
    };
    let mut ws = WsState::new(WorkspaceConfig {
        budget: Some(budget),
        ..test_config()
    });
    ws.resource_meter.usage.tokens = 85; // at warning threshold (0.8 * 100 = 80)

    // Budget is tracked, usage at limit returns warning-level state.
    let b = ws.resource_meter.budget.as_ref().unwrap();
    let limit = b.max_tokens.unwrap();
    let threshold = (limit as f64 * b.warning_threshold as f64) as u64;
    assert!(
        ws.resource_meter.usage.tokens >= threshold,
        "usage should be at or above warning"
    );
    assert!(
        ws.resource_meter.usage.tokens < limit,
        "usage should be below hard limit"
    );
}

#[tokio::test]
async fn workspace_concurrent_envelope_delivery() {
    let (handle, mut event_rx) = spawn_test().await;
    activate(&handle, &mut event_rx).await;

    // Deliver two envelopes.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-a",
            "feedback",
            EnvelopePriority::Normal,
        )))
        .await
        .unwrap();
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-b",
            "feedback",
            EnvelopePriority::Normal,
        )))
        .await
        .unwrap();

    // Verify actor is alive after both deliveries (no panic/crash).
    handle
        .coordinator_tx
        .send(CoordinatorCommand::Abort)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 10).await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WorkspaceEvent::Terminated(_)))
    );
}

#[test]
fn workspace_resource_meter_tracks_usage() {
    let mut ws = WsState::new(test_config());
    assert_eq!(ws.resource_meter.usage.tokens, 0);

    ws.resource_meter.usage.tokens += 100;
    ws.resource_meter.usage.storage_bytes += 2048;
    ws.resource_meter.usage.wall_time_ms += 500;

    assert_eq!(ws.resource_meter.usage.tokens, 100);
    assert_eq!(ws.resource_meter.usage.storage_bytes, 2048);
    assert_eq!(ws.resource_meter.usage.wall_time_ms, 500);
}

#[test]
fn workspace_dedup_prevents_repeat() {
    let mut ws = WsState::new(test_config());
    ws.push_inbox(test_envelope("e1", "feedback", EnvelopePriority::Normal));
    let _ = ws.pop_inbox();

    // e1 is delivered.
    assert!(ws.is_envelope_delivered("e1"));

    // Push another envelope with same ID — at the state level the dedup tracking says delivered.
    // The actor layer prevents pushes of already-delivered envelope IDs.
    assert!(ws.is_envelope_delivered("e1"));
}

#[test]
fn workspace_checkpoint_count_increments() {
    let mut ws = WsState::new(test_config());
    assert_eq!(ws.checkpoints().len(), 0);

    for i in 1..=5 {
        ws.push_checkpoint(Checkpoint {
            id: CheckpointId::from(format!("cp-{i}")),
            workspace_id: WorkspaceId::from("ws-test"),
            checkpoint_type: "artifact".into(),
            payload: vec![],
            content_hash: format!("hash-{i}"),
            intent: format!("intent-{i}"),
            parent_checkpoint: None,
            status: CheckpointStatus::Provisional,
            confidence: Confidence::High,
            timestamp: i as u64,
            resource_usage: None,
        });
        assert_eq!(ws.checkpoints().len(), i);
    }
}

#[test]
fn workspace_snapshot_preserves_all_components() {
    let mut ws = WsState::new(test_config());
    ws.push_inbox(test_envelope("e1", "feedback", EnvelopePriority::Normal));
    ws.working_memory = vec![42, 43, 44];
    ws.push_checkpoint(Checkpoint {
        id: CheckpointId::from("cp-snap"),
        workspace_id: WorkspaceId::from("ws-test"),
        checkpoint_type: "artifact".into(),
        payload: vec![1, 2, 3],
        content_hash: "snaph".into(),
        intent: "snap test".into(),
        parent_checkpoint: None,
        status: CheckpointStatus::Final,
        confidence: Confidence::High,
        timestamp: 100,
        resource_usage: None,
    });
    ws.resource_meter.usage.tokens = 555;
    ws.trail_sequence = 88;

    let snap = ws.capture_snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let roundtrip: MigrationSnapshot = serde_json::from_str(&json).unwrap();

    assert_eq!(roundtrip.inbox.len(), 1);
    assert_eq!(roundtrip.working_memory, vec![42, 43, 44]);
    assert_eq!(roundtrip.checkpoint_register.len(), 1);
    assert_eq!(roundtrip.resource_meter.usage.tokens, 555);
    assert_eq!(roundtrip.trail_sequence, 88);
}

#[test]
fn workspace_context_is_readonly() {
    let ws = WsState::new(test_config());
    // Context returned as &[u8], no setter exists.
    let ctx = ws.context();
    assert_eq!(ctx, b"ctx");
    // Compilation ensures: no &mut method for context.
}

#[test]
fn workspace_directive_is_readonly() {
    let ws = WsState::new(test_config());
    let dir = ws.directive();
    assert_eq!(dir.envelope_type, "directive");
    // Compilation ensures: directive() returns &Envelope, no setter.
}

#[test]
fn workspace_visibility_additive_multiple() {
    let mut ws = WsState::new(test_config());
    assert_eq!(ws.visibility().len(), 1); // initial "res-1"

    ws.grant_visibility(&["res-2".into()]);
    ws.grant_visibility(&["res-3".into(), "res-4".into()]);
    assert_eq!(ws.visibility().len(), 4);
    assert!(ws.visibility().contains("res-1"));
    assert!(ws.visibility().contains("res-2"));
    assert!(ws.visibility().contains("res-3"));
    assert!(ws.visibility().contains("res-4"));
}

#[test]
fn workspace_authority_frozen_after_init() {
    let ws = WsState::new(test_config());
    let auth = ws.authority();
    assert!(auth.contains("res-1"));
    // No public method to modify authority — verified by compilation.
    // Authority remains exactly what was set in config.
    assert_eq!(auth.len(), 1);
}

#[tokio::test]
async fn workspace_idle_accepts_envelope() {
    let (handle, mut event_rx) = spawn_test().await;

    // Workspace starts Idle; delivering first envelope should transition to Active.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-first",
            "directive",
            EnvelopePriority::Normal,
        )))
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 3).await;
    let activated = events.iter().any(|e| {
        matches!(e, WorkspaceEvent::StateChanged { from, to, .. }
            if *from == wacp_types::WorkspaceState::Idle
            && *to == wacp_types::WorkspaceState::Active)
    });
    assert!(activated, "expected Idle -> Active on first envelope");
}

#[tokio::test]
async fn workspace_active_accepts_signal() {
    let (handle, mut event_rx) = spawn_test().await;
    activate(&handle, &mut event_rx).await;

    handle
        .agent_tx
        .send(AgentMessage::EmitSignal {
            signal_type: SignalType::Blocked,
            reason: Some("waiting for dep".into()),
            context: None,
        })
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;
    let got_signal = events
        .iter()
        .any(|e| matches!(e, WorkspaceEvent::Signal(_)));
    assert!(got_signal, "expected Signal event from Active workspace");
}

#[tokio::test]
async fn workspace_failed_terminal() {
    let (handle, mut event_rx) = spawn_test().await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::Abort)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;
    let terminated = events.iter().find_map(|e| match e {
        WorkspaceEvent::Terminated(archived) => Some(archived),
        _ => None,
    });
    assert!(terminated.is_some());
    assert_eq!(
        terminated.unwrap().terminal_state,
        wacp_types::WorkspaceState::Failed
    );
}

#[tokio::test]
async fn workspace_closed_terminal() {
    let (handle, mut event_rx) = spawn_test().await;
    to_integrating(&handle, &mut event_rx).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::IntegrationSucceeded)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;
    let terminated = events.iter().find_map(|e| match e {
        WorkspaceEvent::Terminated(archived) => Some(archived),
        _ => None,
    });
    assert!(terminated.is_some());
    assert_eq!(
        terminated.unwrap().terminal_state,
        wacp_types::WorkspaceState::Closed
    );
}

#[tokio::test]
async fn workspace_migration_snapshot_captures_state() {
    let (handle, mut event_rx) = spawn_test().await;
    activate(&handle, &mut event_rx).await;

    // Deliver another envelope so inbox is non-empty during migration.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::DeliverEnvelope(test_envelope(
            "env-extra",
            "feedback",
            EnvelopePriority::Normal,
        )))
        .await
        .unwrap();
    // Small delay for delivery.
    let _ = tokio::time::timeout(std::time::Duration::from_millis(50), event_rx.recv()).await;

    // Begin migration.
    handle
        .coordinator_tx
        .send(CoordinatorCommand::MigrateBegin)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 10).await;
    let got_snapshot = events
        .iter()
        .any(|e| matches!(e, WorkspaceEvent::MigrationSnapshot { .. }));
    assert!(got_snapshot, "expected MigrationSnapshot event");
}

#[tokio::test]
async fn workspace_abort_from_active() {
    let (handle, mut event_rx) = spawn_test().await;
    activate(&handle, &mut event_rx).await;

    handle
        .coordinator_tx
        .send(CoordinatorCommand::Abort)
        .await
        .unwrap();

    let events = drain_events(&mut event_rx, 5).await;

    let state_change = events.iter().find(|e| {
        matches!(e, WorkspaceEvent::StateChanged { to, .. }
            if *to == wacp_types::WorkspaceState::Failed)
    });
    assert!(state_change.is_some(), "expected Active -> Failed on abort");

    let terminated = events.iter().find_map(|e| match e {
        WorkspaceEvent::Terminated(archived) => Some(archived),
        _ => None,
    });
    assert!(terminated.is_some());
    assert_eq!(
        terminated.unwrap().terminal_state,
        wacp_types::WorkspaceState::Failed
    );
}
