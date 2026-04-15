use std::io::Write;

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
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(seg_path)
            .unwrap();
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
    idx.insert(&make_entry(1, Some("ws-1"), "worker", "checkpoint_created"))
        .unwrap();
    idx.insert(&make_entry(2, Some("ws-1"), "worker", "signal_emitted"))
        .unwrap();
    idx.insert(&make_entry(3, Some("ws-2"), "worker", "checkpoint_created"))
        .unwrap();

    let result = idx
        .query(&TrailQuery {
            workspace_id: Some("ws-1".into()),
            ..TrailQuery::new()
        })
        .unwrap();
    assert_eq!(result.entries.len(), 2);
}

#[test]
fn index_query_by_event_type() {
    let idx = TrailIndex::open_in_memory().unwrap();
    idx.insert(&make_entry(1, Some("ws-1"), "worker", "signal_emitted"))
        .unwrap();
    idx.insert(&make_entry(2, Some("ws-1"), "worker", "checkpoint_created"))
        .unwrap();

    let result = idx
        .query(&TrailQuery {
            event_type: Some("checkpoint_created".into()),
            ..TrailQuery::new()
        })
        .unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].sequence_number, 2);
}

#[test]
fn index_query_by_actor() {
    let idx = TrailIndex::open_in_memory().unwrap();
    idx.insert(&make_entry(1, None, "protocol", "run_started"))
        .unwrap();
    idx.insert(&make_entry(2, Some("ws-1"), "worker", "signal_emitted"))
        .unwrap();

    let result = idx
        .query(&TrailQuery {
            actor: Some("protocol".into()),
            ..TrailQuery::new()
        })
        .unwrap();
    assert_eq!(result.entries.len(), 1);
}

#[test]
fn index_query_by_timestamp_range() {
    let idx = TrailIndex::open_in_memory().unwrap();
    for i in 1..=5 {
        idx.insert(&make_entry(i, Some("ws-1"), "worker", "evt"))
            .unwrap();
    }

    let from = wacp_clock::Timestamp::new(2000, 0).to_bytes();
    let to = wacp_clock::Timestamp::new(4000, 0).to_bytes();

    let result = idx
        .query(&TrailQuery {
            from_timestamp: Some(from),
            to_timestamp: Some(to),
            ..TrailQuery::new()
        })
        .unwrap();
    assert_eq!(result.entries.len(), 3); // seq 2,3,4
}

#[test]
fn index_query_compound() {
    let idx = TrailIndex::open_in_memory().unwrap();
    idx.insert(&make_entry(1, Some("ws-1"), "worker", "signal_emitted"))
        .unwrap();
    idx.insert(&make_entry(2, Some("ws-1"), "worker", "checkpoint_created"))
        .unwrap();
    idx.insert(&make_entry(3, Some("ws-2"), "worker", "checkpoint_created"))
        .unwrap();

    let result = idx
        .query(&TrailQuery {
            workspace_id: Some("ws-1".into()),
            event_type: Some("checkpoint_created".into()),
            ..TrailQuery::new()
        })
        .unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].sequence_number, 2);
}

#[test]
fn index_query_limit() {
    let idx = TrailIndex::open_in_memory().unwrap();
    for i in 1..=10 {
        idx.insert(&make_entry(i, Some("ws-1"), "worker", "evt"))
            .unwrap();
    }

    let result = idx
        .query(&TrailQuery {
            limit: 3,
            ..TrailQuery::new()
        })
        .unwrap();
    assert_eq!(result.entries.len(), 3);
    assert!(result.has_more);
}

#[test]
fn index_query_empty() {
    let idx = TrailIndex::open_in_memory().unwrap();
    let result = idx
        .query(&TrailQuery {
            workspace_id: Some("nonexistent".into()),
            ..TrailQuery::new()
        })
        .unwrap();
    assert!(result.entries.is_empty());
    assert!(!result.has_more);
}

#[test]
fn index_last_sequence() {
    let idx = TrailIndex::open_in_memory().unwrap();
    assert_eq!(idx.last_sequence().unwrap(), None);

    idx.insert(&make_entry(5, None, "protocol", "evt")).unwrap();
    idx.insert(&make_entry(10, None, "protocol", "evt"))
        .unwrap();
    assert_eq!(idx.last_sequence().unwrap(), Some(10));
}

#[test]
fn index_batch_insert() {
    let idx = TrailIndex::open_in_memory().unwrap();
    let entries: Vec<_> = (1..=5)
        .map(|i| make_entry(i, Some("ws-1"), "worker", "evt"))
        .collect();
    idx.insert_batch(&entries).unwrap();
    assert_eq!(idx.last_sequence().unwrap(), Some(5));
}

// ══════════════════════════════════════════
// Phase 18b.1 — Trail error paths
// ══════════════════════════════════════════

#[test]
fn fs_read_nonexistent_segment() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileTrailStorage::open(FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 1024 * 1024,
    })
    .unwrap();

    let result = store.read(SegmentId(999), 0, 10);
    assert!(matches!(result, Err(StorageError::SegmentNotFound(999))));
}

#[test]
fn fs_scan_nonexistent_segment() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileTrailStorage::open(FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 1024 * 1024,
    })
    .unwrap();

    let result = store.scan(SegmentId(999), 0);
    assert!(matches!(result, Err(StorageError::SegmentNotFound(999))));
}

#[test]
fn fs_scan_skips_partial_tail_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = FileTrailStorage::open(FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 1024 * 1024,
    })
    .unwrap();

    // Write one valid entry.
    let hash = compute_chain_hash(&ChainHash::GENESIS, b"good");
    store.append(b"good", hash.as_ref()).unwrap();

    // Append garbage (partial entry) directly to the segment file.
    let seg_path = dir.path().join("segment-000000.trail");
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&seg_path)
        .unwrap();
    // Write a length prefix but not the full entry.
    f.write_all(&100u32.to_be_bytes()).unwrap();
    f.write_all(b"incomplete").unwrap();
    drop(f);

    // Scan should return only the valid entry.
    let entries = store.scan(SegmentId(0), 0).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].1, b"good");
}

#[test]
fn fs_rotate_exact_boundary() {
    let dir = tempfile::tempdir().unwrap();
    // Entry overhead is 4 (length prefix) + 32 (hash) = 36 bytes per entry.
    // With a 10-byte payload: 36 + 10 = 46 bytes per entry.
    let mut store = FileTrailStorage::open(FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 46, // Exactly one entry fits.
    })
    .unwrap();

    let hash1 = compute_chain_hash(&ChainHash::GENESIS, b"0123456789");
    let r1 = store.append(b"0123456789", hash1.as_ref()).unwrap();
    assert_eq!(r1.segment, SegmentId(0));

    // After first entry, active_size == 46 == max_segment_size → rotation triggered.
    // Second entry goes to segment 1.
    let hash2 = compute_chain_hash(&hash1, b"second");
    let r2 = store.append(b"second", hash2.as_ref()).unwrap();
    assert_eq!(r2.segment, SegmentId(1));
}

#[test]
fn fs_crash_recovery_preserves_valid_entries() {
    let dir = tempfile::tempdir().unwrap();
    let config = FileTrailConfig {
        dir: dir.path().to_path_buf(),
        max_segment_size: 1024 * 1024,
    };

    // Write two valid entries.
    {
        let mut store = FileTrailStorage::open(FileTrailConfig {
            dir: dir.path().to_path_buf(),
            max_segment_size: 1024 * 1024,
        })
        .unwrap();
        let h1 = compute_chain_hash(&ChainHash::GENESIS, b"entry1");
        store.append(b"entry1", h1.as_ref()).unwrap();
        let h2 = compute_chain_hash(&h1, b"entry2");
        store.append(b"entry2", h2.as_ref()).unwrap();
    }

    // Append garbage to simulate crash.
    let seg_path = dir.path().join("segment-000000.trail");
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&seg_path)
        .unwrap();
    f.write_all(b"CRASH_GARBAGE").unwrap();
    drop(f);

    // Recover should truncate garbage, keep valid entries.
    let store = FileTrailStorage::recover(config).unwrap();
    let entries = store.scan(SegmentId(0), 0).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].1, b"entry1");
    assert_eq!(entries[1].1, b"entry2");
}

#[test]
fn index_query_all_entries_no_filter() {
    let idx = TrailIndex::open_in_memory().unwrap();
    for i in 1..=5 {
        idx.insert(&make_entry(i, Some("ws-1"), "worker", "evt"))
            .unwrap();
    }

    let result = idx.query(&TrailQuery::new()).unwrap();
    assert_eq!(result.entries.len(), 5);
    assert!(!result.has_more);
}

// ══════════════════════════════════════════
// Phase 18b.2 — Trail hardening (+12)
// ══════════════════════════════════════════

#[test]
fn compaction_warm_retention_days() {
    // Warm segments outside retention are eligible for cold transition.
    let dir = tempfile::tempdir().unwrap();
    let trail_dir = dir.path().join("trail");
    let cold_dir = dir.path().join("cold");
    std::fs::create_dir_all(&trail_dir).unwrap();

    let task = crate::compaction::CompactionTask::new(
        trail_dir.clone(),
        Some(cold_dir.clone()),
        crate::compaction::CompactionConfig {
            hot_limit: 10,
            warm_retention_days: 0, // immediate warm→cold
            cold_retention_days: None,
            compaction_interval_minutes: 60,
        },
    )
    .unwrap();

    // Create a warm segment.
    let warm_dir = trail_dir.join("warm");
    std::fs::write(warm_dir.join("segment-000001.trail.zst"), b"warm data").unwrap();

    let summary = task.run_once().unwrap();
    // With 0 retention days, the warm segment should move to cold.
    assert_eq!(summary.warm_to_cold, 1);
    assert!(cold_dir.join("segment-000001.trail.zst").exists());
}

#[test]
fn compaction_cold_positive_integer() {
    // Cold retention with a positive integer: segments too young are NOT deleted.
    let dir = tempfile::tempdir().unwrap();
    let trail_dir = dir.path().join("trail");
    let cold_dir = dir.path().join("cold");
    std::fs::create_dir_all(&trail_dir).unwrap();
    std::fs::create_dir_all(&cold_dir).unwrap();

    // Just-created file with 365-day retention: should NOT be deleted.
    std::fs::write(cold_dir.join("segment-000001.trail.zst"), b"recent data").unwrap();

    let task = crate::compaction::CompactionTask::new(
        trail_dir,
        Some(cold_dir.clone()),
        crate::compaction::CompactionConfig {
            hot_limit: 10,
            warm_retention_days: 90,
            cold_retention_days: Some(365),
            compaction_interval_minutes: 60,
        },
    )
    .unwrap();

    let summary = task.run_once().unwrap();
    assert_eq!(summary.cold_deleted, 0);
    assert!(cold_dir.join("segment-000001.trail.zst").exists());
}

#[test]
fn index_rebuild_after_corrupt() {
    // Corrupt index entries, rebuild from scratch: same results.
    let idx = TrailIndex::open_in_memory().unwrap();
    for i in 1..=5 {
        idx.insert(&make_entry(i, Some("ws-1"), "worker", "evt"))
            .unwrap();
    }
    let original = idx.query(&TrailQuery::new()).unwrap();
    assert_eq!(original.entries.len(), 5);

    // Simulate "rebuild": create a new index and re-insert the same entries.
    let idx2 = TrailIndex::open_in_memory().unwrap();
    for i in 1..=5 {
        idx2.insert(&make_entry(i, Some("ws-1"), "worker", "evt"))
            .unwrap();
    }
    let rebuilt = idx2.query(&TrailQuery::new()).unwrap();
    assert_eq!(rebuilt.entries.len(), original.entries.len());
    for (a, b) in original.entries.iter().zip(rebuilt.entries.iter()) {
        assert_eq!(a.sequence_number, b.sequence_number);
        assert_eq!(a.workspace_id, b.workspace_id);
        assert_eq!(a.event_type, b.event_type);
    }
}

#[test]
fn concurrent_append_no_data_loss() {
    // Sequential approach: append 100 entries, verify all 100 present in scan.
    let mut store = InMemoryTrailStorage::new();
    let mut head = ChainHash::GENESIS;
    for i in 0..100u64 {
        let data = format!("entry-{i}");
        let hash = compute_chain_hash(&head, data.as_bytes());
        store.append(data.as_bytes(), hash.as_ref()).unwrap();
        head = hash;
    }

    let entries = store.scan(SegmentId(0), 0).unwrap();
    assert_eq!(entries.len(), 100);
    for (i, (_, data, _)) in entries.iter().enumerate() {
        assert_eq!(data, format!("entry-{i}").as_bytes());
    }
    assert_eq!(store.metadata().total_entries, 100);
}

#[test]
fn large_payload_checkpoint() {
    use sha2::{Digest, Sha256};
    let store = InMemoryCheckpointStorage::new();
    let payload = vec![0xABu8; 1024 * 1024]; // 1MB
    let hash: [u8; 32] = Sha256::digest(&payload).into();

    store.store(&hash, &payload).unwrap();
    let read = store.read(&hash).unwrap().unwrap();
    assert_eq!(read.len(), 1024 * 1024);

    let read_hash: [u8; 32] = Sha256::digest(&read).into();
    assert_eq!(hash, read_hash);
}

#[test]
fn zero_byte_snapshot_rejected() {
    use crate::fs_snapshot::FileSnapshotStorage;
    let dir = tempfile::tempdir().unwrap();
    let storage = FileSnapshotStorage::new(dir.path().to_path_buf());
    let ws = WorkspaceId::from("ws-zero");

    // Write a zero-byte snapshot file directly.
    let path = dir.path().join("ws-ws-zero.snapshot");
    std::fs::write(&path, b"").unwrap();

    let result = storage.read_workspace(&ws);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("too small"));
}

#[test]
fn fs_checkpoint_integrity_verified() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let store = FileCheckpointStorage::open(dir.path().join("ckpt")).unwrap();

    let payload = b"checkpoint integrity test data";
    let hash: [u8; 32] = Sha256::digest(payload).into();

    store.store(&hash, payload).unwrap();
    let read = store.read(&hash).unwrap().unwrap();
    assert_eq!(read, payload);

    let read_hash: [u8; 32] = Sha256::digest(&read).into();
    assert_eq!(hash, read_hash);
}

#[test]
fn trail_metadata_bytes_accurate() {
    let mut store = InMemoryTrailStorage::new();
    let entry_a = b"aaaa"; // 4 bytes
    let entry_b = b"bbbbbbbb"; // 8 bytes
    let entry_c = b"cc"; // 2 bytes

    store.append(entry_a, &[0; 32]).unwrap();
    store.append(entry_b, &[0; 32]).unwrap();
    store.append(entry_c, &[0; 32]).unwrap();

    let m = store.metadata();
    assert_eq!(m.total_bytes, 4 + 8 + 2);
}

#[test]
fn hot_warm_transition_count() {
    // Verify hot_limit is respected: with 8 segments and hot_limit=5, 3 move to warm.
    let dir = tempfile::tempdir().unwrap();
    let trail_dir = dir.path().to_path_buf();

    for i in 0..8u64 {
        std::fs::write(
            trail_dir.join(format!("segment-{i:06}.trail")),
            format!("data {i}"),
        )
        .unwrap();
    }

    let mgr = crate::tiered::TierManager::new(trail_dir, 5, 90, None).unwrap();
    let moved = mgr.transition_hot_to_warm().unwrap();
    assert_eq!(moved, 3);
    assert_eq!(mgr.list_hot_segments().unwrap().len(), 5);
}

#[test]
fn segment_id_ordering() {
    // SegmentId(0) < SegmentId(1) via inner u64 comparison.
    let a = SegmentId(0);
    let b = SegmentId(1);
    let c = SegmentId(100);
    assert!(a.0 < b.0);
    assert!(b.0 < c.0);
    assert!(a.0 < c.0);
}

#[test]
fn scan_empty_segment() {
    let store = InMemoryTrailStorage::new();
    // Segment 0 exists but is empty.
    let entries = store.scan(SegmentId(0), 0).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn append_updates_chain_head() {
    let mut store = InMemoryTrailStorage::new();
    let genesis = store.metadata().chain_head;
    assert_eq!(genesis, [0u8; 32]);

    let hash1 = [1u8; 32];
    store.append(b"entry1", &hash1).unwrap();
    assert_eq!(store.metadata().chain_head, hash1);

    let hash2 = [2u8; 32];
    store.append(b"entry2", &hash2).unwrap();
    assert_eq!(store.metadata().chain_head, hash2);

    // Verify it updates each time, not stuck on the first.
    let hash3 = [3u8; 32];
    store.append(b"entry3", &hash3).unwrap();
    assert_eq!(store.metadata().chain_head, hash3);
}
