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
