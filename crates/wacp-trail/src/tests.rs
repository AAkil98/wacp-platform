use wacp_types::WorkspaceId;

use crate::*;

// ══════════════════════════════════════════
// Task 3.1 — In-memory backends
// ══════════════════════════════════════════

#[test]
fn trail_append_and_read() {
    let mut store = InMemoryTrailStorage::new();
    let hash = [1u8; 32];
    let receipt = store.append(b"hello", &hash).unwrap();
    let data = store.read(receipt.segment, receipt.offset, 5).unwrap();
    assert_eq!(data, b"hello");
}

#[test]
fn trail_sequence_increments() {
    let mut store = InMemoryTrailStorage::new();
    let r1 = store.append(b"a", &[0; 32]).unwrap();
    let r2 = store.append(b"b", &[0; 32]).unwrap();
    let r3 = store.append(b"c", &[0; 32]).unwrap();
    assert_eq!(r1.sequence, 1);
    assert_eq!(r2.sequence, 2);
    assert_eq!(r3.sequence, 3);
}

#[test]
fn trail_rotate() {
    let mut store = InMemoryTrailStorage::new();
    store.append(b"seg0", &[0; 32]).unwrap();
    let new_seg = store.rotate().unwrap();
    let r = store.append(b"seg1", &[0; 32]).unwrap();
    assert_eq!(r.segment, new_seg);

    // Old segment still readable.
    let data = store.read(SegmentId(0), 0, 4).unwrap();
    assert_eq!(data, b"seg0");
}

#[test]
fn trail_scan() {
    let mut store = InMemoryTrailStorage::new();
    store.append(b"a", &[1; 32]).unwrap();
    store.append(b"b", &[2; 32]).unwrap();
    store.append(b"c", &[3; 32]).unwrap();

    let entries = store.scan(SegmentId(0), 0).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].1, b"a");
    assert_eq!(entries[2].1, b"c");
}

#[test]
fn trail_metadata() {
    let mut store = InMemoryTrailStorage::new();
    let m = store.metadata();
    assert_eq!(m.total_entries, 0);

    store.append(b"x", &[42; 32]).unwrap();
    let m = store.metadata();
    assert_eq!(m.total_entries, 1);
    assert_eq!(m.chain_head, [42; 32]);
}

#[test]
fn checkpoint_store_and_read() {
    let store = InMemoryCheckpointStorage::new();
    let hash = [0xAA; 32];
    assert!(store.store(&hash, b"payload").unwrap());
    let data = store.read(&hash).unwrap().unwrap();
    assert_eq!(data, b"payload");
}

#[test]
fn checkpoint_dedup() {
    let store = InMemoryCheckpointStorage::new();
    let hash = [0xBB; 32];
    assert!(store.store(&hash, b"data").unwrap());
    assert!(!store.store(&hash, b"data").unwrap());
}

#[test]
fn checkpoint_exists() {
    let store = InMemoryCheckpointStorage::new();
    let hash = [0xCC; 32];
    assert!(!store.exists(&hash).unwrap());
    store.store(&hash, b"x").unwrap();
    assert!(store.exists(&hash).unwrap());
}

#[test]
fn checkpoint_delete() {
    let store = InMemoryCheckpointStorage::new();
    let hash = [0xDD; 32];
    store.store(&hash, b"x").unwrap();
    assert!(store.delete(&hash).unwrap());
    assert!(store.read(&hash).unwrap().is_none());
}

#[test]
fn checkpoint_not_found() {
    let store = InMemoryCheckpointStorage::new();
    assert!(store.read(&[0xFF; 32]).unwrap().is_none());
}

#[test]
fn snapshot_workspace_roundtrip() {
    let store = InMemorySnapshotStorage::new();
    let ws = WorkspaceId::from("ws-1");
    store.write_workspace(&ws, b"state").unwrap();
    let data = store.read_workspace(&ws).unwrap().unwrap();
    assert_eq!(data, b"state");
}

#[test]
fn snapshot_workspace_overwrite() {
    let store = InMemorySnapshotStorage::new();
    let ws = WorkspaceId::from("ws-1");
    store.write_workspace(&ws, b"v1").unwrap();
    store.write_workspace(&ws, b"v2").unwrap();
    assert_eq!(store.read_workspace(&ws).unwrap().unwrap(), b"v2");
}

#[test]
fn snapshot_workspace_delete() {
    let store = InMemorySnapshotStorage::new();
    let ws = WorkspaceId::from("ws-1");
    store.write_workspace(&ws, b"x").unwrap();
    store.delete_workspace(&ws).unwrap();
    assert!(store.read_workspace(&ws).unwrap().is_none());
}

#[test]
fn snapshot_system_roundtrip() {
    let store = InMemorySnapshotStorage::new();
    store.write_system(100, b"snap").unwrap();
    let (seq, data) = store.read_latest_system().unwrap().unwrap();
    assert_eq!(seq, 100);
    assert_eq!(data, b"snap");
}

#[test]
fn snapshot_system_latest() {
    let store = InMemorySnapshotStorage::new();
    store.write_system(100, b"first").unwrap();
    store.write_system(200, b"second").unwrap();
    let (seq, data) = store.read_latest_system().unwrap().unwrap();
    assert_eq!(seq, 200);
    assert_eq!(data, b"second");
}

// ══════════════════════════════════════════
// Task 3.5 — Hash chain
// ══════════════════════════════════════════

#[test]
fn genesis_hash() {
    let h = compute_chain_hash(&ChainHash::GENESIS, b"first entry");
    assert_ne!(h, ChainHash::GENESIS);
}

#[test]
fn chain_link_deterministic() {
    let prev = ChainHash::GENESIS;
    let h1 = compute_chain_hash(&prev, b"data");
    let h2 = compute_chain_hash(&prev, b"data");
    assert_eq!(h1, h2);
}

#[test]
fn chain_link_different_entries() {
    let prev = ChainHash::GENESIS;
    let h1 = compute_chain_hash(&prev, b"aaa");
    let h2 = compute_chain_hash(&prev, b"bbb");
    assert_ne!(h1, h2);
}

#[test]
fn chain_link_different_previous() {
    let h1 = compute_chain_hash(&ChainHash::GENESIS, b"data");
    let h2 = compute_chain_hash(&ChainHash([1u8; 32]), b"data");
    assert_ne!(h1, h2);
}

#[test]
fn verify_valid_link() {
    let prev = ChainHash::GENESIS;
    let h = compute_chain_hash(&prev, b"entry");
    assert!(verify_chain_link(&prev, b"entry", &h));
}

#[test]
fn verify_invalid_link() {
    let prev = ChainHash::GENESIS;
    let h = compute_chain_hash(&prev, b"entry");
    assert!(!verify_chain_link(&prev, b"tampered", &h));
}

#[test]
fn verify_chain_valid() {
    let mut chain = Vec::new();
    let mut prev = ChainHash::GENESIS;
    for i in 0..10 {
        let entry = format!("entry-{i}");
        let h = compute_chain_hash(&prev, entry.as_bytes());
        chain.push((entry.into_bytes(), h));
        prev = h;
    }
    assert!(verify_chain(&chain).is_ok());
}

#[test]
fn verify_chain_tampered() {
    let mut chain = Vec::new();
    let mut prev = ChainHash::GENESIS;
    for i in 0..5 {
        let entry = format!("entry-{i}");
        let h = compute_chain_hash(&prev, entry.as_bytes());
        chain.push((entry.into_bytes(), h));
        prev = h;
    }
    // Tamper entry 2.
    chain[2].0 = b"tampered".to_vec();
    let err = verify_chain(&chain).unwrap_err();
    assert_eq!(err.index, 2);
}

#[test]
fn verify_chain_empty() {
    assert!(verify_chain(&[]).is_ok());
}

// ══════════════════════════════════════════
// Task 3.2 — Filesystem trail backend
// ══════════════════════════════════════════

#[test]
fn fs_append_and_read() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = FileTrailStorage::open(FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 1024 * 1024,
    })
    .unwrap();

    let hash = compute_chain_hash(&ChainHash::GENESIS, b"hello");
    let receipt = store.append(b"hello", hash.as_ref()).unwrap();
    let data = store.read(receipt.segment, receipt.offset, 5).unwrap();
    assert_eq!(data, b"hello");
}

#[test]
fn fs_sequence_increments() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = FileTrailStorage::open(FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 1024 * 1024,
    })
    .unwrap();

    let r1 = store.append(b"a", &[0; 32]).unwrap();
    let r2 = store.append(b"b", &[0; 32]).unwrap();
    assert_eq!(r1.sequence, 1);
    assert_eq!(r2.sequence, 2);
}

#[test]
fn fs_rotate_at_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = FileTrailStorage::open(FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 100, // tiny threshold
    })
    .unwrap();

    // Each entry is 4 + payload + 32 = 86 bytes. First entry (86) < 100, no rotation yet.
    let big_entry = vec![0xAA; 50];
    let r1 = store.append(&big_entry, &[0; 32]).unwrap();
    assert_eq!(r1.segment, SegmentId(0));

    // Second entry pushes active_size to 172 > 100, rotation happens after write.
    // Receipt still shows segment 0 (written before rotation).
    let r2 = store.append(&big_entry, &[0; 32]).unwrap();
    assert_eq!(r2.segment, SegmentId(0));

    // Third entry lands in the new segment after rotation.
    let r3 = store.append(&big_entry, &[0; 32]).unwrap();
    assert_eq!(r3.segment, SegmentId(1));
}

#[test]
fn fs_scan_segment() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = FileTrailStorage::open(FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 1024 * 1024,
    })
    .unwrap();

    store.append(b"aaa", &[1; 32]).unwrap();
    store.append(b"bbb", &[2; 32]).unwrap();
    store.append(b"ccc", &[3; 32]).unwrap();

    let entries = store.scan(SegmentId(0), 0).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].1, b"aaa");
    assert_eq!(entries[1].1, b"bbb");
    assert_eq!(entries[2].1, b"ccc");
}

#[test]
fn fs_metadata_accurate() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = FileTrailStorage::open(FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 1024 * 1024,
    })
    .unwrap();

    assert_eq!(store.metadata().total_entries, 0);
    store.append(b"x", &[9; 32]).unwrap();
    let m = store.metadata();
    assert_eq!(m.total_entries, 1);
    assert_eq!(m.chain_head, [9; 32]);
}

#[test]
fn fs_reopen_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let config = FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 1024 * 1024,
    };

    {
        let mut store = FileTrailStorage::open(FileTrailConfig {
            dir: config.dir.clone(),
            max_segment_size: config.max_segment_size,
        })
        .unwrap();
        store.append(b"entry1", &[1; 32]).unwrap();
        store.append(b"entry2", &[2; 32]).unwrap();
    }

    // Reopen.
    let mut store = FileTrailStorage::open(config).unwrap();
    assert_eq!(store.metadata().total_entries, 2);

    let r = store.append(b"entry3", &[3; 32]).unwrap();
    assert_eq!(r.sequence, 3);
}

#[test]
fn fs_crash_recovery_truncate() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().to_path_buf();

    // Write some valid entries.
    {
        let mut store = FileTrailStorage::open(FileTrailConfig {
            dir: config_dir.clone(),
            max_segment_size: 1024 * 1024,
        })
        .unwrap();
        store.append(b"good1", &[1; 32]).unwrap();
        store.append(b"good2", &[2; 32]).unwrap();
    }

    // Append garbage bytes to simulate a partial write.
    {
        use std::io::Write;
        let seg_path = config_dir.join("segment-000000.trail");
        let mut f = std::fs::OpenOptions::new().append(true).open(seg_path).unwrap();
        f.write_all(&[0xFF; 10]).unwrap(); // partial/corrupt bytes
    }

    // Recovery should truncate the garbage.
    let store = FileTrailStorage::recover(FileTrailConfig {
        dir: config_dir,
        max_segment_size: 1024 * 1024,
    })
    .unwrap();
    assert_eq!(store.metadata().total_entries, 2);
}

#[test]
fn fs_multiple_segments() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = FileTrailStorage::open(FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 80, // very small
    })
    .unwrap();

    for i in 0..10u8 {
        store.append(&[i; 20], &[i; 32]).unwrap();
    }
    assert_eq!(store.metadata().total_entries, 10);
    // Should have created multiple segments.
    assert!(store.metadata().current_segment.0 > 0);
}

// ══════════════════════════════════════════
// Task 3.4 — Filesystem checkpoint store
// ══════════════════════════════════════════

#[test]
fn fs_checkpoint_store_read() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let store = FileCheckpointStorage::open(dir.path().join("ckpt")).unwrap();

    let payload = b"checkpoint data";
    let hash: [u8; 32] = Sha256::digest(payload).into();
    assert!(store.store(&hash, payload).unwrap());

    let data = store.read(&hash).unwrap().unwrap();
    assert_eq!(data, payload);
}

#[test]
fn fs_checkpoint_dedup() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let store = FileCheckpointStorage::open(dir.path().join("ckpt")).unwrap();

    let payload = b"same data";
    let hash: [u8; 32] = Sha256::digest(payload).into();
    assert!(store.store(&hash, payload).unwrap());
    assert!(!store.store(&hash, payload).unwrap());
}

#[test]
fn fs_checkpoint_integrity() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileCheckpointStorage::open(dir.path().join("ckpt")).unwrap();

    // Store with a wrong hash — read should detect corruption.
    let fake_hash = [0xAA; 32];
    store.store(&fake_hash, b"real data").unwrap();

    let result = store.read(&fake_hash);
    assert!(result.is_err());
}

#[test]
fn fs_checkpoint_exists() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let store = FileCheckpointStorage::open(dir.path().join("ckpt")).unwrap();

    let hash: [u8; 32] = Sha256::digest(b"x").into();
    assert!(!store.exists(&hash).unwrap());
    store.store(&hash, b"x").unwrap();
    assert!(store.exists(&hash).unwrap());
}

#[test]
fn fs_checkpoint_delete() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let store = FileCheckpointStorage::open(dir.path().join("ckpt")).unwrap();

    let hash: [u8; 32] = Sha256::digest(b"del").into();
    store.store(&hash, b"del").unwrap();
    assert!(store.delete(&hash).unwrap());
    assert!(store.read(&hash).unwrap().is_none());
}

#[test]
fn fs_checkpoint_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileCheckpointStorage::open(dir.path().join("ckpt")).unwrap();
    assert!(store.read(&[0xFF; 32]).unwrap().is_none());
}

#[test]
fn fs_checkpoint_shard_dirs() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("ckpt");
    let store = FileCheckpointStorage::open(base.clone()).unwrap();

    let hash: [u8; 32] = Sha256::digest(b"shard test").into();
    store.store(&hash, b"shard test").unwrap();

    // Check that the shard directory exists.
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    let shard_dir = base.join(&hex[..2]);
    assert!(shard_dir.exists());
}

// ══════════════════════════════════════════
// Task 3.3 — SQLite trail index
// ══════════════════════════════════════════

fn make_entry(seq: u64, ws: Option<&str>, actor: &str, event_type: &str) -> IndexEntry {
    IndexEntry {
        sequence_number: seq,
        timestamp_bytes: wacp_clock::Timestamp::new(seq * 1000, 0).to_bytes(),
        workspace_id: ws.map(String::from),
        actor: actor.into(),
        event_type: event_type.into(),
        segment_id: 0,
        offset: seq * 100,
        length: 50,
    }
}

#[test]
fn index_insert_and_query() {
    let idx = TrailIndex::open_in_memory().unwrap();
    idx.insert(&make_entry(1, Some("ws-1"), "worker", "checkpoint_created")).unwrap();
    idx.insert(&make_entry(2, Some("ws-1"), "worker", "signal_emitted")).unwrap();
    idx.insert(&make_entry(3, Some("ws-2"), "worker", "checkpoint_created")).unwrap();

    let result = idx.query(&TrailQuery {
        workspace_id: Some("ws-1".into()),
        ..TrailQuery::new()
    }).unwrap();
    assert_eq!(result.entries.len(), 2);
}

#[test]
fn index_query_by_event_type() {
    let idx = TrailIndex::open_in_memory().unwrap();
    idx.insert(&make_entry(1, Some("ws-1"), "worker", "signal_emitted")).unwrap();
    idx.insert(&make_entry(2, Some("ws-1"), "worker", "checkpoint_created")).unwrap();

    let result = idx.query(&TrailQuery {
        event_type: Some("checkpoint_created".into()),
        ..TrailQuery::new()
    }).unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].sequence_number, 2);
}

#[test]
fn index_query_by_actor() {
    let idx = TrailIndex::open_in_memory().unwrap();
    idx.insert(&make_entry(1, None, "protocol", "run_started")).unwrap();
    idx.insert(&make_entry(2, Some("ws-1"), "worker", "signal_emitted")).unwrap();

    let result = idx.query(&TrailQuery {
        actor: Some("protocol".into()),
        ..TrailQuery::new()
    }).unwrap();
    assert_eq!(result.entries.len(), 1);
}

#[test]
fn index_query_by_timestamp_range() {
    let idx = TrailIndex::open_in_memory().unwrap();
    for i in 1..=5 {
        idx.insert(&make_entry(i, Some("ws-1"), "worker", "evt")).unwrap();
    }

    let from = wacp_clock::Timestamp::new(2000, 0).to_bytes();
    let to = wacp_clock::Timestamp::new(4000, 0).to_bytes();

    let result = idx.query(&TrailQuery {
        from_timestamp: Some(from),
        to_timestamp: Some(to),
        ..TrailQuery::new()
    }).unwrap();
    assert_eq!(result.entries.len(), 3); // seq 2,3,4
}

#[test]
fn index_query_compound() {
    let idx = TrailIndex::open_in_memory().unwrap();
    idx.insert(&make_entry(1, Some("ws-1"), "worker", "signal_emitted")).unwrap();
    idx.insert(&make_entry(2, Some("ws-1"), "worker", "checkpoint_created")).unwrap();
    idx.insert(&make_entry(3, Some("ws-2"), "worker", "checkpoint_created")).unwrap();

    let result = idx.query(&TrailQuery {
        workspace_id: Some("ws-1".into()),
        event_type: Some("checkpoint_created".into()),
        ..TrailQuery::new()
    }).unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].sequence_number, 2);
}

#[test]
fn index_query_limit() {
    let idx = TrailIndex::open_in_memory().unwrap();
    for i in 1..=10 {
        idx.insert(&make_entry(i, Some("ws-1"), "worker", "evt")).unwrap();
    }

    let result = idx.query(&TrailQuery {
        limit: 3,
        ..TrailQuery::new()
    }).unwrap();
    assert_eq!(result.entries.len(), 3);
    assert!(result.has_more);
}

#[test]
fn index_query_empty() {
    let idx = TrailIndex::open_in_memory().unwrap();
    let result = idx.query(&TrailQuery {
        workspace_id: Some("nonexistent".into()),
        ..TrailQuery::new()
    }).unwrap();
    assert!(result.entries.is_empty());
    assert!(!result.has_more);
}

#[test]
fn index_last_sequence() {
    let idx = TrailIndex::open_in_memory().unwrap();
    assert_eq!(idx.last_sequence().unwrap(), None);

    idx.insert(&make_entry(5, None, "protocol", "evt")).unwrap();
    idx.insert(&make_entry(10, None, "protocol", "evt")).unwrap();
    assert_eq!(idx.last_sequence().unwrap(), Some(10));
}

#[test]
fn index_batch_insert() {
    let idx = TrailIndex::open_in_memory().unwrap();
    let entries: Vec<_> = (1..=5).map(|i| make_entry(i, Some("ws-1"), "worker", "evt")).collect();
    idx.insert_batch(&entries).unwrap();
    assert_eq!(idx.last_sequence().unwrap(), Some(5));
}
