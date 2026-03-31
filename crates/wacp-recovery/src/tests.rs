use wacp_trail::{compute_chain_hash, ChainHash, InMemoryTrailStorage, TrailStorage};

use crate::*;

/// Helper: append a JSON event to the trail with proper hash chain.
fn append_event(trail: &mut InMemoryTrailStorage, chain_head: &mut ChainHash, event: &serde_json::Value) {
    let bytes = serde_json::to_vec(event).unwrap();
    let hash = compute_chain_hash(chain_head, &bytes);
    trail.append(&bytes, hash.as_ref()).unwrap();
    *chain_head = hash;
}

#[test]
fn recover_empty_trail() {
    let trail = InMemoryTrailStorage::new();
    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.last_sequence, 0);
    assert_eq!(state.clock_initial, wacp_clock::Timestamp::ZERO);
    assert!(state.workspace_states.is_empty());
    assert!(state.in_flight_envelopes.is_empty());
}

#[test]
fn recover_single_entry() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    let event = serde_json::json!({
        "event_type": "workspace_created",
        "workspace_id": "ws-1",
        "new_state": "Idle",
        "timestamp": 1000
    });
    append_event(&mut trail, &mut head, &event);

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.last_sequence, 1);
    assert_eq!(
        state.workspace_states.get("ws-1"),
        Some(&WorkspaceState::Idle)
    );
    assert!(state.clock_initial > wacp_clock::Timestamp::ZERO);
}

#[test]
fn recover_chain_valid() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    for i in 0..5 {
        let event = serde_json::json!({
            "event_type": "workspace_state_changed",
            "workspace_id": format!("ws-{i}"),
            "new_state": "Active",
            "timestamp": 1000 + i
        });
        append_event(&mut trail, &mut head, &event);
    }

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.last_sequence, 5);
    assert_eq!(state.workspace_states.len(), 5);
}

#[test]
fn recover_chain_corrupt() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    let event1 = serde_json::json!({"event_type": "test", "timestamp": 1000});
    append_event(&mut trail, &mut head, &event1);

    // Write a second entry with a wrong hash (tampered).
    let event2_bytes = serde_json::to_vec(&serde_json::json!({"event_type": "test", "timestamp": 2000})).unwrap();
    let wrong_hash = [0xFFu8; 32];
    trail.append(&event2_bytes, &wrong_hash).unwrap();

    let result = RecoveryEngine::recover(&trail);
    assert!(matches!(result, Err(RecoveryError::TrailCorruption { .. })));
}

#[test]
fn recover_clock_advanced() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    let event = serde_json::json!({
        "event_type": "test",
        "timestamp": 5000
    });
    append_event(&mut trail, &mut head, &event);

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert!(state.clock_initial > wacp_clock::Timestamp::new(5000, 0));
}

#[test]
fn recover_in_flight_detected() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    // Envelope created but not delivered.
    let created = serde_json::json!({
        "event_type": "envelope_created",
        "envelope_id": "env-1",
        "timestamp": 1000
    });
    append_event(&mut trail, &mut head, &created);

    // Another envelope created AND delivered.
    let created2 = serde_json::json!({
        "event_type": "envelope_created",
        "envelope_id": "env-2",
        "timestamp": 2000
    });
    append_event(&mut trail, &mut head, &created2);

    let delivered = serde_json::json!({
        "event_type": "envelope_delivered",
        "envelope_id": "env-2",
        "timestamp": 3000
    });
    append_event(&mut trail, &mut head, &delivered);

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.in_flight_envelopes.len(), 1);
    assert!(state.in_flight_envelopes.contains(&"env-1".to_string()));
}

// ── Phase 18b.3: Recovery paths ──

use wacp_trail::InMemorySnapshotStorage;

fn make_snapshot(seq: u64, ws_states: &[(&str, &str)], task_statuses: &[(&str, &str)]) -> (u64, Vec<u8>) {
    let mut nodes = serde_json::Map::new();
    for (id, state) in ws_states {
        nodes.insert(
            id.to_string(),
            serde_json::json!({"status": state}),
        );
    }
    let mut tasks = serde_json::Map::new();
    for (id, status) in task_statuses {
        tasks.insert(
            id.to_string(),
            serde_json::json!({"status": status}),
        );
    }
    let snap = serde_json::json!({
        "sequence": seq,
        "tree": {"nodes": nodes},
        "task_graph": {"tasks": tasks}
    });
    (seq, serde_json::to_vec(&snap).unwrap())
}

#[test]
fn recover_with_snapshot_valid() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    // 5 entries establishing ws-1 as Active.
    for i in 0..5 {
        let event = serde_json::json!({
            "event_type": "workspace_state_changed",
            "workspace_id": "ws-1",
            "new_state": if i == 0 { "Idle" } else { "Active" },
            "timestamp": 1000 + i
        });
        append_event(&mut trail, &mut head, &event);
    }

    // Snapshot at seq=3 captures ws-1 as Active.
    let snapshots = InMemorySnapshotStorage::new();
    let (seq, data) = make_snapshot(3, &[("ws-1", "Active")], &[]);
    snapshots.write_system(seq, &data).unwrap();

    // 2 more entries after snapshot: ws-1 → Blocked.
    let event = serde_json::json!({
        "event_type": "workspace_state_changed",
        "workspace_id": "ws-1",
        "new_state": "Blocked",
        "timestamp": 2000
    });
    append_event(&mut trail, &mut head, &event);
    let event = serde_json::json!({
        "event_type": "workspace_state_changed",
        "workspace_id": "ws-2",
        "new_state": "Active",
        "timestamp": 2001
    });
    append_event(&mut trail, &mut head, &event);

    let state = RecoveryEngine::recover_with_snapshot(&trail, &snapshots).unwrap();
    assert_eq!(state.snapshot_sequence, Some(3));
    // ws-1: snapshot says Active, delta says Blocked → Blocked wins.
    assert_eq!(state.workspace_states.get("ws-1"), Some(&WorkspaceState::Blocked));
    // ws-2: only in delta.
    assert_eq!(state.workspace_states.get("ws-2"), Some(&WorkspaceState::Active));
}

#[test]
fn recover_with_corrupted_snapshot_falls_back() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    let event = serde_json::json!({
        "event_type": "workspace_created",
        "workspace_id": "ws-1",
        "new_state": "Idle",
        "timestamp": 1000
    });
    append_event(&mut trail, &mut head, &event);

    // Write corrupted snapshot.
    let snapshots = InMemorySnapshotStorage::new();
    snapshots.write_system(1, b"NOT VALID JSON").unwrap();

    let with_snap = RecoveryEngine::recover_with_snapshot(&trail, &snapshots).unwrap();
    let without_snap = RecoveryEngine::recover(&trail).unwrap();

    // Should produce same result: fallback to full replay.
    assert_eq!(with_snap.snapshot_sequence, None);
    assert_eq!(with_snap.workspace_states, without_snap.workspace_states);
    assert_eq!(with_snap.last_sequence, without_snap.last_sequence);
}

#[test]
fn recover_empty_trail_with_valid_snapshot() {
    let trail = InMemoryTrailStorage::new();

    let snapshots = InMemorySnapshotStorage::new();
    let (seq, data) = make_snapshot(
        42,
        &[("ws-1", "Active"), ("ws-2", "Blocked")],
        &[("t-1", "InProgress")],
    );
    snapshots.write_system(seq, &data).unwrap();

    let state = RecoveryEngine::recover_with_snapshot(&trail, &snapshots).unwrap();
    assert_eq!(state.snapshot_sequence, Some(42));
    assert_eq!(state.last_sequence, 42);
    assert_eq!(state.workspace_states.get("ws-1"), Some(&WorkspaceState::Active));
    assert_eq!(state.workspace_states.get("ws-2"), Some(&WorkspaceState::Blocked));
    assert_eq!(state.task_statuses.get("t-1"), Some(&TaskStatus::InProgress));
    assert_eq!(state.clock_initial, wacp_clock::Timestamp::ZERO);
}

#[test]
fn recover_multiple_state_changes_last_wins() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    let states = ["Idle", "Active", "Blocked", "Active"];
    for (i, st) in states.iter().enumerate() {
        let event = serde_json::json!({
            "event_type": "workspace_state_changed",
            "workspace_id": "ws-1",
            "new_state": st,
            "timestamp": 1000 + i as u64
        });
        append_event(&mut trail, &mut head, &event);
    }

    let state = RecoveryEngine::recover(&trail).unwrap();
    // Last state wins.
    assert_eq!(state.workspace_states.get("ws-1"), Some(&WorkspaceState::Active));
    assert_eq!(state.last_sequence, 4);
}

#[test]
fn recover_task_status_changes() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    let statuses = ["Draft", "Pending", "Assigned", "InProgress", "Completed"];
    for (i, st) in statuses.iter().enumerate() {
        let event = serde_json::json!({
            "event_type": "task_status_changed",
            "task_id": "t-1",
            "new_status": st,
            "timestamp": 1000 + i as u64
        });
        append_event(&mut trail, &mut head, &event);
    }

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.task_statuses.get("t-1"), Some(&TaskStatus::Completed));
}

#[test]
fn recover_snapshot_at_middle_replays_delta() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    // 10 entries: ws-1 through ws-10.
    for i in 1..=10 {
        let event = serde_json::json!({
            "event_type": "workspace_created",
            "workspace_id": format!("ws-{i}"),
            "new_state": "Active",
            "timestamp": 1000 + i
        });
        append_event(&mut trail, &mut head, &event);
    }

    // Snapshot at seq=5 with ws-1..ws-5.
    let snap_ws: Vec<(&str, &str)> = vec![
        ("ws-1", "Active"),
        ("ws-2", "Active"),
        ("ws-3", "Active"),
        ("ws-4", "Active"),
        ("ws-5", "Active"),
    ];
    let snapshots = InMemorySnapshotStorage::new();
    let (seq, data) = make_snapshot(5, &snap_ws, &[]);
    snapshots.write_system(seq, &data).unwrap();

    let state = RecoveryEngine::recover_with_snapshot(&trail, &snapshots).unwrap();
    assert_eq!(state.snapshot_sequence, Some(5));
    // Should have all 10 workspaces: 5 from snapshot + 5 from delta replay.
    assert_eq!(state.workspace_states.len(), 10);
    for i in 1..=10 {
        assert!(
            state.workspace_states.contains_key(&format!("ws-{i}")),
            "missing ws-{i}"
        );
    }
}

#[test]
fn recover_migration_interrupted() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    // ws-1 created as Active.
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "workspace_created",
        "workspace_id": "ws-1",
        "new_state": "Active",
        "timestamp": 1000
    }));

    // ws-1 transitions to Migrating (migration_started).
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "workspace_state_changed",
        "workspace_id": "ws-1",
        "new_state": "Migrating",
        "timestamp": 2000
    }));

    // No migration_completed event — interrupted.

    let state = RecoveryEngine::recover(&trail).unwrap();
    // The reconstructed state reflects the last recorded state: Migrating.
    assert_eq!(state.workspace_states.get("ws-1"), Some(&WorkspaceState::Migrating));
}

#[test]
fn recover_unrecognized_event_type_skipped() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "unknown_event",
        "data": "something",
        "timestamp": 1000
    }));

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert!(state.workspace_states.is_empty());
    assert_eq!(state.last_sequence, 1);
}

// ══════════════════════════════════════════
// Phase 18b.3 — Recovery hardening (+11)
// ══════════════════════════════════════════

#[test]
fn recover_task_created_and_completed() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "task_status_changed",
        "task_id": "t-1",
        "new_status": "Pending",
        "timestamp": 1000
    }));
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "task_status_changed",
        "task_id": "t-1",
        "new_status": "InProgress",
        "timestamp": 2000
    }));
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "task_status_changed",
        "task_id": "t-1",
        "new_status": "Completed",
        "timestamp": 3000
    }));

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.task_statuses.get("t-1"), Some(&TaskStatus::Completed));
    assert_eq!(state.last_sequence, 3);
}

#[test]
fn recover_multiple_in_flight_some_delivered() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    // Create 3 envelopes.
    for i in 1..=3 {
        append_event(&mut trail, &mut head, &serde_json::json!({
            "event_type": "envelope_created",
            "envelope_id": format!("env-{i}"),
            "timestamp": 1000 + i
        }));
    }

    // Deliver only env-2.
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "envelope_delivered",
        "envelope_id": "env-2",
        "timestamp": 5000
    }));

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.in_flight_envelopes.len(), 2);
    assert!(state.in_flight_envelopes.contains(&"env-1".to_string()));
    assert!(state.in_flight_envelopes.contains(&"env-3".to_string()));
    assert!(!state.in_flight_envelopes.contains(&"env-2".to_string()));
}

#[test]
fn recover_workspace_created_then_closed() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "workspace_created",
        "workspace_id": "ws-1",
        "new_state": "Idle",
        "timestamp": 1000
    }));
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "workspace_state_changed",
        "workspace_id": "ws-1",
        "new_state": "Active",
        "timestamp": 2000
    }));
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "workspace_state_changed",
        "workspace_id": "ws-1",
        "new_state": "Closed",
        "timestamp": 3000
    }));

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.workspace_states.get("ws-1"), Some(&WorkspaceState::Closed));
}

#[test]
fn recover_workspace_multiple_transitions() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    let transitions = ["Idle", "Active", "Blocked", "Active", "Closed"];
    for (i, st) in transitions.iter().enumerate() {
        let event_type = if i == 0 { "workspace_created" } else { "workspace_state_changed" };
        append_event(&mut trail, &mut head, &serde_json::json!({
            "event_type": event_type,
            "workspace_id": "ws-1",
            "new_state": st,
            "timestamp": 1000 + i as u64
        }));
    }

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.workspace_states.get("ws-1"), Some(&WorkspaceState::Closed));
    assert_eq!(state.last_sequence, 5);
}

#[test]
fn recover_clock_from_timestamps() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "test",
        "timestamp": 1000
    }));
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "test",
        "timestamp": 5000
    }));
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "test",
        "timestamp": 9999
    }));

    let state = RecoveryEngine::recover(&trail).unwrap();
    // clock_initial should be last_entry_timestamp.succ()
    assert!(state.clock_initial > wacp_clock::Timestamp::new(9999, 0));
}

#[test]
fn recover_empty_events_skipped() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    // Entry with empty event_type — should be skipped.
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "",
        "workspace_id": "ws-1",
        "new_state": "Active",
        "timestamp": 1000
    }));

    // Normal entry.
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "workspace_created",
        "workspace_id": "ws-2",
        "new_state": "Idle",
        "timestamp": 2000
    }));

    let state = RecoveryEngine::recover(&trail).unwrap();
    // Empty event_type doesn't match any known event, so ws-1 not added.
    assert!(!state.workspace_states.contains_key("ws-1"));
    assert_eq!(state.workspace_states.get("ws-2"), Some(&WorkspaceState::Idle));
    assert_eq!(state.last_sequence, 2);
}

#[test]
fn recover_large_trail_performance() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    for i in 0..1000u64 {
        append_event(&mut trail, &mut head, &serde_json::json!({
            "event_type": "workspace_state_changed",
            "workspace_id": format!("ws-{}", i % 10),
            "new_state": "Active",
            "timestamp": 1000 + i
        }));
    }

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.last_sequence, 1000);
    assert_eq!(state.workspace_states.len(), 10);
}

#[test]
fn recover_interleaved_workspaces() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    // 3 workspaces interleaved.
    let events = [
        ("ws-1", "Idle"),
        ("ws-2", "Idle"),
        ("ws-3", "Idle"),
        ("ws-1", "Active"),
        ("ws-2", "Active"),
        ("ws-3", "Active"),
        ("ws-1", "Blocked"),
        ("ws-2", "Closed"),
        ("ws-3", "Failed"),
    ];
    for (i, (ws_id, state)) in events.iter().enumerate() {
        let event_type = if state == &"Idle" { "workspace_created" } else { "workspace_state_changed" };
        append_event(&mut trail, &mut head, &serde_json::json!({
            "event_type": event_type,
            "workspace_id": ws_id,
            "new_state": state,
            "timestamp": 1000 + i as u64
        }));
    }

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.workspace_states.get("ws-1"), Some(&WorkspaceState::Blocked));
    assert_eq!(state.workspace_states.get("ws-2"), Some(&WorkspaceState::Closed));
    assert_eq!(state.workspace_states.get("ws-3"), Some(&WorkspaceState::Failed));
}

#[test]
fn recover_envelope_created_then_rejected() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    // Create envelope.
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "envelope_created",
        "envelope_id": "env-1",
        "timestamp": 1000
    }));

    // Deliver (reject = delivery in protocol terms).
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "envelope_delivered",
        "envelope_id": "env-1",
        "timestamp": 2000
    }));

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert!(state.in_flight_envelopes.is_empty());
}

#[test]
fn recover_last_sequence_matches_count() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    for i in 0..7u64 {
        append_event(&mut trail, &mut head, &serde_json::json!({
            "event_type": "test",
            "timestamp": 1000 + i
        }));
    }

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.last_sequence, 7);
}

#[test]
fn recover_unknown_fields_ignored() {
    let mut trail = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;

    // JSON with extra fields.
    append_event(&mut trail, &mut head, &serde_json::json!({
        "event_type": "workspace_created",
        "workspace_id": "ws-1",
        "new_state": "Idle",
        "timestamp": 1000,
        "extra_field": "should_be_ignored",
        "nested": {"deeply": "ignored"}
    }));

    let state = RecoveryEngine::recover(&trail).unwrap();
    assert_eq!(state.workspace_states.get("ws-1"), Some(&WorkspaceState::Idle));
    assert_eq!(state.last_sequence, 1);
}
