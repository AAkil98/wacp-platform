//! Integration: wacp-coordinator + wacp-trail (4 tests)

use wacp_integration_tests::*;

use wacp_coordinator::GateController;
use wacp_trail::{ChainHash, InMemoryTrailStorage, TrailStorage, compute_chain_hash};
use wacp_types::*;

fn append_event(trail: &mut InMemoryTrailStorage, json: &str, prev: &ChainHash) -> ChainHash {
    let bytes = json.as_bytes();
    let hash = compute_chain_hash(prev, bytes);
    trail.append(bytes, hash.as_ref()).unwrap();
    hash
}

#[tokio::test]
async fn event_logging_through_coordinator() {
    let (mut coord, mut rx) = make_coordinator();
    let mut trail = InMemoryTrailStorage::new();
    let mut chain = ChainHash::GENESIS;

    dispatch(&mut coord, "ws-1", "task-1");

    // Activate workspace
    let envelope = make_envelope("dir-ws-1", "ws-root", "ws-1", "directive");
    coord
        .route_envelope(&WorkspaceId::from("ws-1"), envelope)
        .await;

    // Collect events and log them
    let mut event_count = 0;
    for _ in 0..5 {
        if let Ok(Some(event)) =
            tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await
        {
            let json = format!(r#"{{"event":"{event:?}"}}"#);
            chain = append_event(&mut trail, &json, &chain);
            event_count += 1;
            coord.handle_event(&event).await;
        }
    }

    assert!(event_count > 0);
    assert_eq!(trail.metadata().total_entries, event_count);
}

#[tokio::test]
async fn trail_entries_match_state_changes() {
    let (mut coord, mut rx) = make_coordinator();
    let mut trail = InMemoryTrailStorage::new();
    let mut chain = ChainHash::GENESIS;

    dispatch(&mut coord, "ws-1", "task-1");

    // Abort to get a clear state change
    coord.abort_workspace(&WorkspaceId::from("ws-1")).await;

    let mut logged_events = vec![];
    for _ in 0..10 {
        if let Ok(Some(event)) =
            tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await
        {
            let json = serde_json::to_string(&format!("{event:?}")).unwrap();
            chain = append_event(&mut trail, &json, &chain);
            logged_events.push(format!("{event:?}"));
            coord.handle_event(&event).await;
        }
    }

    assert!(!logged_events.is_empty());
    // Last state should be Failed
    assert!(logged_events.iter().any(|e| e.contains("Failed")));
}

#[test]
fn gate_events_recorded_in_trail() {
    let mut gc = GateController::new(30_000, wacp_coordinator::GateFallback::Cancel);
    let mut trail = InMemoryTrailStorage::new();
    let mut chain = ChainHash::GENESIS;

    let event = gc.open_gate(
        TaskId::from("task-1"),
        "Test Task".into(),
        "test",
        None,
        None,
    );
    let gate_id = event.gate_id.clone();
    chain = append_event(
        &mut trail,
        r#"{"event_type":"gate_opened","gate_id":"gate-0"}"#,
        &chain,
    );

    assert!(gc.is_pending(&gate_id));

    gc.resolve(&gate_id, GateDecision::Approve);
    let _chain = append_event(
        &mut trail,
        r#"{"event_type":"gate_resolved","decision":"approve"}"#,
        &chain,
    );

    assert!(!gc.is_pending(&gate_id));
    assert_eq!(trail.metadata().total_entries, 2);
}

#[test]
fn gate_timeout_fallback() {
    let mut gc = GateController::new(30_000, wacp_coordinator::GateFallback::AutoApprove);

    let event = gc.open_gate(TaskId::from("task-1"), "Task".into(), "desc", None, None);

    assert!(gc.is_pending(&event.gate_id));

    let resolution = gc.timeout(&event.gate_id);
    assert!(resolution.is_some());
    assert!(!gc.is_pending(&event.gate_id));
}
