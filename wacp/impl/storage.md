# WACP Implementation: Storage Architecture

```yaml
id: wacp-impl-storage
type: implementation-spec
status: complete
created: 2026-03-18
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4.4 (checkpoint)
  - §4.5 (trail)
  - §6.1 (workspace internal model)
  - §9 (trail)
  - §10 (recovery and fault handling)
  - §11.5 (trail integrity)
depends_on:
  - wacp-impl-runtime
  - wacp-spec-trail
  - wacp-spec-checkpoint
  - wacp-spec-workspace
  - wacp-spec-recovery
  - wacp-spec-security
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, storage, trail, checkpoint, persistence, recovery]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Storage Domains](#2-storage-domains)
3. [Trail Backend](#3-trail-backend)
4. [Trail Indexing and Queries](#4-trail-indexing-and-queries)
5. [Checkpoint Store](#5-checkpoint-store)
6. [Workspace State Persistence](#6-workspace-state-persistence)
7. [Snapshots](#7-snapshots)
8. [Tiered Storage](#8-tiered-storage)
9. [Retention and Compaction](#9-retention-and-compaction)
10. [Durability Guarantees](#10-durability-guarantees)
11. [Storage Backend Trait](#11-storage-backend-trait)
12. [References](#12-references)

## 1. Purpose

This spec defines how the WACP runtime persists data. It answers "where does state live on disk" — not "what state exists" (that's the protocol's job) or "how is state managed in memory" (that's the runtime spec's job).

The runtime spec (§6) establishes that the trail write is the commit point — synchronous, durable, write-ahead. This spec defines what "durable" means: the storage backends, their guarantees, and their interfaces. Three kinds of data require persistence: trail entries, checkpoint payloads, and workspace state for recovery. Each has different access patterns, different size profiles, and different durability requirements.

**Scope.** Storage backends for the three data domains. On-disk formats. Indexing for trail queries. Snapshot mechanism for recovery acceleration. Tiered storage for trail lifecycle management. The storage backend trait that the runtime's `wacp-trail` crate programs against.

**Not in scope.** In-memory state management (runtime spec). Protobuf serialization of messages crossing process boundaries (protocol-interface spec). Distributed storage or replication (future concern).

**Design constraint.** All storage is local to the runtime process — files on the same machine. The storage layer is not an external service. This keeps the trust boundary tight: the runtime controls its own persistence directly, with no network round-trips on the write-ahead path.

---

## 2. Storage Domains

The runtime persists three categories of data. Each has distinct access patterns, size characteristics, and durability requirements. They are stored separately — no single database holds everything.

**Domain 1: Trail entries.** The append-only audit log. Every protocol event produces one entry. Entries are small (hundreds of bytes to low kilobytes), written frequently (every protocol operation), and read for queries, recovery, and streaming to the highway. Access pattern: append-heavy, sequential write, random read by index or time range, sequential scan for recovery. Durability requirement: **absolute** — the trail write is the commit point. A lost trail entry means a lost protocol guarantee.

**Domain 2: Checkpoint payloads.** The work products agents create. Checkpoints carry metadata (id, workspace, type, intent, confidence — stored as trail entries) and payloads (code, prose, files, structured data — stored separately). Payloads range from kilobytes to megabytes. Access pattern: write-once, read at integration time and for review. Durability requirement: **high** — a lost checkpoint payload means lost work, but the trail entry recording its existence survives.

**Domain 3: Workspace state.** The nine internal components of each active workspace (runtime spec, §8). Persisted for recovery — if the runtime crashes, workspace state is reconstructed from the trail. Access pattern: periodic persistence (not on every operation — the trail is the authoritative recovery source), read on recovery. Durability requirement: **advisory** — workspace state is derivable from the trail. Persisting it is an optimization that accelerates recovery, not a correctness requirement.

**Separation principle.** Each domain has its own storage backend. Trail entries go to the trail store. Checkpoint payloads go to the checkpoint store. Workspace state goes to the snapshot store. The three stores share a data directory but operate independently. A failure in the checkpoint store does not affect trail writes. A failure in the snapshot store does not affect anything — recovery falls back to full trail replay.

| Domain | Write frequency | Entry size | Durability | Recovery role |
|--------|----------------|------------|------------|---------------|
| Trail entries | Every protocol operation | 100B – 4KB | Absolute | Primary source of truth |
| Checkpoint payloads | Per checkpoint | 1KB – 10MB | High | Referenced by trail entries |
| Workspace state | Periodic | 10KB – 1MB per workspace | Advisory | Accelerates recovery |

## 3. Trail Backend

The trail store is the most critical storage component. Every protocol operation writes to it synchronously. Its performance sets the runtime's throughput ceiling. Its durability is the foundation of every recovery guarantee.

**Choice: Custom append-only log.** The trail store is a purpose-built append-only log — not SQLite, not RocksDB, not an off-the-shelf embedded database. The reasoning:

- **SQLite** adds transaction overhead that the trail doesn't need. Trail writes are always appends — never updates, never deletes. A B-tree index is wasted on sequential writes. SQLite's WAL mode adds a second write-ahead layer on top of the protocol's own write-ahead, doubling the fsync cost for no benefit.
- **RocksDB** is an LSM-tree designed for key-value workloads with updates. Trail entries are never updated. The compaction machinery (merging SSTables, garbage collecting tombstones) does work the trail will never generate. The write amplification from compaction is pure overhead.
- A custom append-only log does exactly one thing: append an entry and fsync. No index maintenance on the write path. No compaction. No transaction overhead. The write path is: serialize → append to file → fsync → return. This is the minimum possible work for a durable write.

**On-disk format.** The trail is stored as a sequence of segment files in a dedicated directory. Each segment is a flat file containing a contiguous sequence of entries.

```
trail/
├── segment-000000.trail        # entries 0 – N
├── segment-000001.trail        # entries N+1 – M
├── segment-000002.trail        # entries M+1 – current (active)
└── trail.meta                  # current segment, global sequence counter, chain head hash
```

**Segment structure.** Each segment file is a sequence of length-prefixed entries:

```
[4 bytes: entry length][entry bytes][32 bytes: SHA-256 chain hash]
[4 bytes: entry length][entry bytes][32 bytes: SHA-256 chain hash]
...
```

The length prefix enables forward scanning without parsing entry contents. The chain hash at the end of each entry links it to the previous entry (runtime spec, §6). Reading an entry requires: read length → read that many bytes → read 32-byte hash → verify hash against previous.

**Segment rotation.** The active segment rotates when it reaches a configurable size threshold (default: 64 MB). Rotation creates a new segment file. The previous segment becomes read-only. Rotation is the only structural operation on the write path — it adds one file create and one metadata update. The trail writer holds a file handle to the active segment; rotation closes the old handle and opens a new one.

**Write path detail.** The trail writer (runtime spec, §6) calls into the trail store with a serialized entry:

1. Compute chain hash: `SHA-256(previous_hash || entry_bytes)`.
2. Write to active segment: length prefix + entry bytes + chain hash.
3. Call `fsync()` on the segment file descriptor.
4. Update in-memory metadata: increment global sequence counter, update chain head hash.
5. Periodically flush `trail.meta` to disk (not on every write — metadata is reconstructable from the segments on recovery).

Step 3 is the durability point. After fsync returns, the entry is on disk. The runtime proceeds with the operation.

**Crash safety.** A crash during write can produce a partial entry at the end of the active segment. On recovery, the trail integrity check (runtime spec, §13) detects this: the last entry's length prefix does not match the remaining bytes, or the chain hash does not verify. The recovery engine truncates the partial entry — it represents an operation that was never committed. The protocol's write-ahead guarantee holds: if the fsync didn't complete, the operation didn't happen.

## 4. Trail Indexing and Queries

The trail backend (§3) is optimized for writes — append-only, no index maintenance on the write path. But the protocol requires the trail to be queryable during a run (§9.4), not only after completion. This section defines how queries are served without degrading write performance.

**Separate index.** The index is a secondary data structure built alongside the trail, not embedded in it. The trail segments are the source of truth; the index is a derived, rebuildable acceleration structure. If the index is lost or corrupted, it is rebuilt by scanning the trail segments. Index corruption does not affect trail integrity.

**Choice: SQLite for the index.** The index is a SQLite database. This is the opposite of the trail backend decision — and deliberately so. The trail backend rejects SQLite because trail writes are on the critical path and must be minimal. The index is *off* the critical path. Index updates happen asynchronously after the trail write completes. SQLite's query capabilities (SQL, B-tree indexes, range scans) are exactly what trail queries need.

```
trail/
├── segment-*.trail             # append-only trail data
├── trail.meta                  # trail metadata
└── trail-index.db              # SQLite index (derived, rebuildable)
```

**Index schema.** One table mapping trail entry metadata to its physical location:

```sql
CREATE TABLE trail_index (
    sequence_number INTEGER PRIMARY KEY,  -- global sequence (total order)
    timestamp       BLOB NOT NULL,        -- HLC timestamp (10 bytes, lexicographic ordering)
    workspace_id    TEXT,                  -- NULL for system-level events
    actor           TEXT NOT NULL,         -- role name, "protocol", or user_id
    event_type      TEXT NOT NULL,         -- from the closed event registry
    segment_id      INTEGER NOT NULL,     -- which segment file
    offset          INTEGER NOT NULL,     -- byte offset within segment
    length          INTEGER NOT NULL      -- entry byte length (for direct read)
);

CREATE INDEX idx_workspace ON trail_index(workspace_id, sequence_number);
CREATE INDEX idx_event_type ON trail_index(event_type, sequence_number);
CREATE INDEX idx_timestamp ON trail_index(timestamp);
CREATE INDEX idx_actor ON trail_index(actor, sequence_number);
```

**Index update path.** After the trail writer completes a durable write (fsync), it sends the entry's metadata and physical location to the index writer — a separate component running on a background `tokio` task. The index writer batches inserts into SQLite transactions (configurable batch size, default: 100 entries or 50ms, whichever comes first). Index updates are asynchronous — a query issued immediately after a trail write may not see the latest entry. This is acceptable because:

- The trail writer returns the entry id to the caller. The caller knows the entry exists.
- Queries that must see the absolute latest state can fall back to a tail scan of the active segment.
- Recovery replays from the trail segments, not the index. The index is never on the correctness path.

**Query interface.** The `wacp-trail` crate exposes a query API that translates structured queries into SQL:

- By workspace: `SELECT ... WHERE workspace_id = ? ORDER BY sequence_number`
- By time range: `SELECT ... WHERE timestamp BETWEEN ? AND ? ORDER BY sequence_number`
- By event type: `SELECT ... WHERE event_type = ? ORDER BY sequence_number`
- By actor: `SELECT ... WHERE actor = ? ORDER BY sequence_number`
- Compound: any combination of the above with `AND`

Each result row provides the segment id and byte offset. The query engine reads the full entry from the trail segment at that location. This two-step process (index lookup → segment read) keeps the index small and the trail segments as the single source of entry data.

**Access control.** The query API enforces the protocol's access rules (§9.2). A worker querying outside its workspace scope sees an empty result, not an error. The query layer filters results by the caller's visibility set before returning. Access control is applied in the query engine, not in SQLite — the index itself has no access restrictions.

**Index rebuild.** On startup, if the index is missing or its sequence counter lags behind the trail's, the recovery engine rebuilds the index by scanning trail segments from the gap point forward. Full rebuild from scratch is O(n) in trail size — acceptable as a rare recovery operation, not a routine cost.

## 5. Checkpoint Store

Checkpoint payloads are the work products agents create — code files, prose, structured data, analysis results. They are write-once, read-occasionally, and range from kilobytes to megabytes. The checkpoint store holds payloads separately from their metadata (which lives in the trail as `checkpoint_created` entries).

**Choice: Content-addressable filesystem store.** Checkpoint payloads are stored as files named by the SHA-256 hash of their content. This provides three properties for free:

1. **Deduplication.** If two checkpoints produce identical payloads (e.g., a retry that produces the same output), only one copy is stored. The hash is the same; the file already exists; no write needed.
2. **Integrity verification.** Reading a payload and hashing it verifies that the content has not been modified since creation. The hash is recorded in the trail entry — any tampering is detectable by comparing the stored hash against a fresh computation.
3. **Immutability.** Files are named by their content. Modifying a file changes its hash, which changes its name. The original name still points to the original content (or to nothing, if deleted). Content-addressing makes mutation semantically incoherent.

**On-disk layout.** Two-level directory sharding by hash prefix to avoid filesystem performance degradation with many files in a single directory:

```
checkpoints/
├── a3/
│   ├── a3f2b1c4d5e6...7890.blob    # raw payload bytes
│   └── a3e9d8c7b6a5...1234.blob
├── b7/
│   └── b7c4a1d2e3f4...5678.blob
└── checkpoints.meta                  # statistics, last compaction timestamp
```

The first two hex characters of the hash form the subdirectory. The full hash is the filename. The `.blob` extension is conventional — the content is opaque bytes.

**Write path.** When a workspace actor creates a checkpoint:

1. The workspace actor serializes the payload and computes its SHA-256 hash.
2. The `checkpoint_created` trail entry is written (write-ahead), including the payload hash as the `content_hash` field.
3. After the trail write succeeds, the payload is written to the checkpoint store. The write checks if the file already exists (deduplication) — if so, the write is skipped.
4. The payload file is fsynced.

The trail write (step 2) is the commit point, not the payload write (step 4). If the runtime crashes between steps 2 and 4, recovery detects the orphaned trail entry (a `checkpoint_created` with no corresponding payload file). The recovery engine marks the checkpoint as `payload_missing` — the trail records its existence, but the payload must be recreated (via workspace retry or salvage).

**Read path.** At integration time or for review, the coordinator reads a checkpoint payload by its content hash: construct the file path from the hash, read the file, verify the hash against the content. Hash mismatch means corruption — the coordinator rejects the checkpoint and records a `security_event` trail entry.

**Multi-file payloads.** A checkpoint may contain multiple artifacts (e.g., several source files). The payload is a tar archive — a single blob containing all files with their relative paths preserved. The checkpoint store does not interpret payload contents. Unpacking and interpretation happen at integration time, in the coordinator actor.

## 6. Workspace State Persistence

Workspace state is the nine internal components held in memory by each workspace actor (runtime spec, §8). Persisting workspace state is an optimization — it accelerates recovery by allowing the recovery engine to load a recent snapshot instead of replaying the entire trail from the beginning.

**Not on the critical path.** Workspace state persistence is never synchronous with protocol operations. The trail is the authoritative recovery source. Workspace state snapshots are a cache — stale is acceptable, missing is acceptable, corrupt is acceptable. In all three cases, recovery falls back to trail replay.

**Persistence trigger.** The workspace actor persists its state on two occasions:

1. **Periodic.** Every N checkpoints (configurable, default: 5) or every T seconds of wall time (configurable, default: 60), whichever comes first. The workspace actor serializes its `WorkspaceState` struct and writes it to the snapshot store.
2. **Terminal.** When a workspace reaches `Closed` or `Failed`, the workspace actor writes a final snapshot — the `ArchivedWorkspace` (runtime spec, §8). This is the definitive record of the workspace's terminal state.

**On-disk layout.** One file per workspace, named by workspace id. Only the most recent snapshot is kept — previous snapshots are overwritten.

```
snapshots/
├── ws-abc123.snapshot          # latest state for workspace abc123
├── ws-def456.snapshot          # latest state for workspace def456
└── ws-ghi789.snapshot          # archived (terminal) state
```

**Serialization format.** Workspace state is serialized using the same binary encoding as protobuf messages (protocol-interface spec). This reuses the code generation pipeline and ensures the snapshot format is versioned alongside the protocol definitions. A version field in the snapshot header enables forward compatibility — the recovery engine can detect and reject snapshots from incompatible protocol versions rather than silently loading corrupt state.

**Recovery integration.** The recovery engine (runtime spec, §13) uses snapshots as an acceleration layer:

1. For each workspace with a snapshot file, load the snapshot and read its `last_trail_sequence` field — the global sequence number of the most recent trail entry reflected in the snapshot.
2. Replay trail entries starting from `last_trail_sequence + 1` instead of from the beginning.
3. If the snapshot is missing, corrupt, or from an incompatible version, skip it and replay from the beginning for that workspace.

The savings are proportional to how much of the workspace's trail history the snapshot covers. For a workspace with 10,000 trail entries and a snapshot taken at entry 9,500, recovery replays only the last 500 entries.

**No consistency requirement with the trail.** The snapshot may be behind the trail — entries have been written since the last snapshot. This is normal and handled by step 2 above. The snapshot may be slightly ahead of the trail in a corruption scenario — the recovery engine detects this (snapshot references a sequence number beyond the trail's end) and discards the snapshot.

## 7. Snapshots

Snapshots are system-level recovery acceleration points — distinct from workspace state snapshots (§6). A snapshot captures the entire runtime's reconstructed state at a specific trail sequence number, enabling recovery to skip replaying the trail from the beginning.

**Distinction from workspace snapshots.** Workspace snapshots (§6) are per-workspace and written by workspace actors during normal operation. System snapshots are global and written by the coordinator actor at deliberate intervals. A system snapshot includes:

- The state of every active workspace at the snapshot point.
- The coordinator's state: workspace tree, task graph, port rights table.
- The global trail sequence number and chain head hash at the snapshot point.
- The clock's HLC state.
- Resource meters for all active workspaces.

**When snapshots are taken.** The coordinator actor takes a system snapshot on a configurable schedule — by default, every S trail entries (default: 10,000) or every T minutes (default: 30), whichever comes first. Snapshots are also taken at clean shutdown — the coordinator drains all workspaces and writes a final snapshot before exiting.

**Snapshot procedure.** The coordinator actor initiates a snapshot by:

1. Recording a `snapshot_started` trail entry with the current global sequence number. This is the snapshot's anchor point.
2. Collecting workspace state from all active workspace actors. Each workspace actor serializes its current state and sends it to the coordinator through the normal message channel. This is not a freeze — workspaces continue operating. The snapshot captures a consistent-enough view (each workspace's state is internally consistent, but different workspaces may reflect different trail points). The recovery engine handles this by replaying trail entries after the snapshot's anchor point for all workspaces.
3. Serializing the coordinator's own state (workspace tree, task graph, port rights).
4. Writing the combined snapshot to a single file.
5. Recording a `snapshot_completed` trail entry.

**On-disk layout.** System snapshots are numbered by their anchor sequence number:

```
snapshots/
├── ws-*.snapshot                # per-workspace snapshots (§6)
├── system-000010000.snapshot   # system snapshot at sequence 10,000
├── system-000020000.snapshot   # system snapshot at sequence 20,000
└── system-latest.snapshot      # symlink to most recent
```

**Recovery with snapshots.** The recovery engine (runtime spec, §13) checks for system snapshots before beginning trail replay:

1. Find the most recent valid system snapshot (verify its internal checksum).
2. Load the snapshot — reconstruct coordinator state, workspace states, clock, resource meters.
3. Replay trail entries from `snapshot_sequence + 1` forward.
4. If no valid snapshot exists, fall back to full trail replay from the beginning.

**Snapshot retention.** Old system snapshots are retained for a configurable window (default: 3 snapshots). Beyond that, they are deleted. The retention policy runs after each new snapshot is written. Workspace snapshots (§6) are not subject to this policy — they are single-file, overwrite-in-place, and cleaned up when the workspace reaches a terminal state.

## 8. Tiered Storage

The trail grows monotonically (§9.6). A long-running system produces gigabytes of trail data. Not all of it needs to be on fast storage. The protocol defines three storage tiers: hot, warm, and cold. This section defines how tiers are implemented.

**Tier definitions:**

| Tier | Contains | Storage medium | Access latency | Purpose |
|------|----------|---------------|----------------|---------|
| **Hot** | Active segment + recent sealed segments | Local filesystem (SSD) | Microseconds | Write path, live queries, recovery |
| **Warm** | Older sealed segments | Local filesystem (HDD or compressed on SSD) | Milliseconds | Historical queries, audit |
| **Cold** | Archived segments | External storage (optional: S3, NFS, tape) | Seconds to minutes | Compliance retention, long-term audit |

**Hot tier.** The active segment (currently being written to) and the N most recent sealed segments (configurable, default: 10). These segments are uncompressed on fast storage. The trail index (§4) covers all hot-tier segments. Live queries are served from the hot tier. Recovery reads primarily from the hot tier (plus a system snapshot if available). The hot tier holds the working set — everything needed for normal runtime operation.

**Warm tier.** Sealed segments that have aged out of the hot tier. Warm segments are compressed using `zstd` (high compression ratio, fast decompression). The trail index still covers warm segments — queries can locate entries by sequence number and decompress the relevant segment on demand. Reads from the warm tier are slower (decompression overhead) but still local.

**Cold tier.** Segments that have aged out of the warm tier. Cold storage is optional — deployments that don't need long-term retention can skip it. Cold segments are moved to external storage (the destination is configurable). The trail index retains metadata for cold entries (sequence number, timestamp, workspace, event type) but does not store physical offsets — retrieving a cold entry requires fetching the segment from external storage first.

**Tier transition.** A background task in the coordinator actor manages tier transitions:

1. **Hot → warm.** When the number of sealed segments in the hot tier exceeds the configured limit, the oldest sealed segment is compressed and moved to the warm directory. The index is updated to reflect the new location.
2. **Warm → cold.** When the total warm-tier size exceeds the configured limit or a segment's age exceeds the warm retention period, the segment is moved to cold storage. The index entry is updated to mark it as cold (physical offset cleared, cold location recorded).
3. Transitions are recorded in the trail as `trail_tier_transition` entries.

**Invariant: hot tier always covers the recovery window.** The hot tier must contain all segments needed for recovery from the most recent system snapshot. If the snapshot anchor is at sequence N, every segment containing entries from N onward must be in the hot tier. The tier transition logic enforces this — it will not move a segment to warm if doing so would break the recovery window.

## 9. Retention and Compaction

The protocol requires that trail entries are never deleted within the retention window (§9.6). Compaction relocates entries — it never destroys them. This section defines the retention policy and what compaction means for an append-only log.

**Retention policy.** Configurable per deployment. Three parameters:

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `hot_retention` | 10 segments | Number of sealed segments kept uncompressed in the hot tier |
| `warm_retention` | 90 days | Maximum age for warm-tier segments before transition to cold |
| `cold_retention` | indefinite | How long cold-tier segments are kept. `indefinite` means never deleted. |

Entries within the retention window are guaranteed to exist in some tier. Entries beyond the cold retention window may be deleted — but only entire segments, never individual entries. Deletion of a segment is recorded in the trail as a `trail_segment_deleted` entry (in the current active segment, which is always retained).

**What compaction means here.** In traditional databases, compaction merges and garbage-collects. The trail has no garbage — entries are never updated or logically deleted. Compaction for the trail means:

1. **Compression.** Moving segments from hot to warm involves compressing with `zstd`. This is the primary space-saving operation.
2. **Segment merging (optional).** Multiple small warm-tier segments can be merged into a single larger segment to reduce file count. The entries are concatenated in sequence order. The hash chain is preserved — entries retain their original hashes. The index is updated with new physical locations.
3. **Checkpoint payload cleanup.** Checkpoint payloads referenced only by trail entries that have left the cold tier can be deleted. The checkpoint store scans for payload files whose content hashes are no longer referenced by any trail entry in any tier. This is the only case where stored data is actually destroyed — and only after the referencing trail entries themselves have been deleted.

**Compaction schedule.** A background task runs periodically (configurable, default: every hour). It checks tier thresholds and executes transitions and merges as needed. Compaction never runs on the write path — it operates on sealed segments only, never on the active segment. Compaction is interruptible — if the runtime shuts down during compaction, the next startup resumes from where it left off (the trail is the source of truth for which segments exist and where they are).

**Workspace snapshot cleanup.** When a workspace reaches a terminal state and its final snapshot is written (§6), the snapshot file is retained until the workspace's trail entries have all left the hot tier. After that, the snapshot is deleted — recovery no longer needs it because the trail entries for that workspace are in warm or cold storage and the workspace is immutable.

**Invariant: no silent deletion.** Every deletion — segment, snapshot, checkpoint payload — is recorded in the trail before the deletion is executed. If the deletion fails, the trail entry still records the intent. The trail is the last thing deleted in any retention cascade — the audit record of a deletion outlives the deleted data.

## 10. Durability Guarantees

This section consolidates the durability guarantees across all three storage domains and makes them explicit. Each guarantee traces back to a protocol requirement.

**Trail entries: fsync-durable, single-entry granularity.**

Every trail entry is individually fsynced before the write is acknowledged. This is the strongest durability guarantee in the system. It exists because the trail write is the commit point for every protocol operation (runtime spec, §6). The cost is one fsync per protocol operation. The benefit is that no committed operation can be lost to a crash.

Guarantee: if the trail writer returns success, the entry is on stable storage. A power failure immediately after the return cannot lose it. This follows from the POSIX fsync semantics — after fsync returns, the data has been transferred to the storage device (or the device has acknowledged receipt for battery-backed caches).

**Checkpoint payloads: fsync-durable, eventual.**

Checkpoint payloads are fsynced after write, but the fsync happens after the trail entry is committed — not before. A crash between the trail write and the payload fsync produces a trail entry referencing a missing payload. This is a detectable, recoverable state (§5: `payload_missing`). The protocol's correctness does not depend on checkpoint payloads being durable at the exact moment of the trail write — it depends on the trail entry being durable.

Guarantee: if the checkpoint store's write returns success, the payload is on stable storage. If the runtime crashes before the write returns, the payload may be lost but the trail entry survives — the loss is detectable and the work can be re-requested.

**Workspace snapshots: best-effort.**

Snapshots are not fsynced individually — they use buffered writes with periodic fsync. A crash may produce a corrupt or truncated snapshot. This is acceptable because snapshots are an optimization, not a correctness requirement (§6). The recovery engine validates snapshots before using them and falls back to trail replay on any inconsistency.

Guarantee: none beyond the filesystem's default behavior. Snapshot loss or corruption is handled gracefully.

**Summary table:**

| Domain | Durability | Fsync granularity | Loss impact | Recovery |
|--------|-----------|-------------------|-------------|----------|
| Trail entries | Absolute | Per entry | Protocol guarantee violated | Not recoverable — prevented by design |
| Checkpoint payloads | High | Per payload (after trail commit) | Lost work product, detectable | Re-request from agent |
| Workspace snapshots | Advisory | Periodic / best-effort | Slower recovery | Full trail replay |
| Trail index (SQLite) | Advisory | SQLite-managed | Slower queries until rebuilt | Rebuild from trail segments |
| System snapshots | Advisory | Per snapshot file | Slower recovery | Full trail replay |

**Filesystem requirements.** The runtime assumes the underlying filesystem honors fsync — when fsync returns, data is durable. This is true for ext4 (with `data=ordered` or `data=journal`), XFS, ZFS, and most production filesystems. It is not true for some NFS configurations or for filesystems mounted with `nobarrier`. The runtime does not verify filesystem behavior — deploying on a filesystem that lies about fsync silently voids the durability guarantee. This is documented as a deployment requirement, not enforced at runtime.

---

## 11. Storage Backend Trait

The `wacp-trail` crate programs against a trait, not a concrete implementation. This enables testing with in-memory backends, alternative storage engines, and future distributed backends — all without changing the trail writer or query engine.

**Trail storage trait.**

```rust
trait TrailStorage: Send + Sync {
    /// Append an entry to the active segment. Returns the byte offset
    /// at which the entry was written. The implementation MUST fsync
    /// before returning.
    fn append(&mut self, entry: &[u8], chain_hash: &[u8; 32]) -> Result<WriteReceipt, StorageError>;

    /// Read an entry by segment id and byte offset.
    fn read(&self, segment: SegmentId, offset: u64, length: u32) -> Result<Vec<u8>, StorageError>;

    /// Rotate the active segment. The current segment becomes sealed
    /// (read-only). A new active segment is created.
    fn rotate(&mut self) -> Result<SegmentId, StorageError>;

    /// Scan a segment sequentially from a byte offset, yielding entries.
    /// Used for recovery and index rebuild.
    fn scan(&self, segment: SegmentId, from_offset: u64) -> Result<SegmentScanner, StorageError>;

    /// Return metadata: current segment id, total entry count,
    /// chain head hash, total bytes across all segments.
    fn metadata(&self) -> StorageMetadata;
}

struct WriteReceipt {
    segment: SegmentId,
    offset: u64,
    sequence: u64,
}
```

**Checkpoint storage trait.**

```rust
trait CheckpointStorage: Send + Sync {
    /// Store a payload by its content hash. Returns true if newly stored,
    /// false if deduplicated (already exists). MUST fsync before returning.
    fn store(&self, content_hash: &[u8; 32], payload: &[u8]) -> Result<bool, StorageError>;

    /// Read a payload by content hash. Returns None if not found.
    fn read(&self, content_hash: &[u8; 32]) -> Result<Option<Vec<u8>>, StorageError>;

    /// Check if a payload exists without reading it.
    fn exists(&self, content_hash: &[u8; 32]) -> Result<bool, StorageError>;

    /// Delete a payload by content hash. Used by retention cleanup.
    fn delete(&self, content_hash: &[u8; 32]) -> Result<bool, StorageError>;
}
```

**Snapshot storage trait.**

```rust
trait SnapshotStorage: Send + Sync {
    /// Write a workspace snapshot. Overwrites any existing snapshot
    /// for this workspace.
    fn write_workspace(&self, workspace_id: &WorkspaceId, data: &[u8]) -> Result<(), StorageError>;

    /// Read a workspace snapshot. Returns None if not found or corrupt.
    fn read_workspace(&self, workspace_id: &WorkspaceId) -> Result<Option<Vec<u8>>, StorageError>;

    /// Delete a workspace snapshot.
    fn delete_workspace(&self, workspace_id: &WorkspaceId) -> Result<(), StorageError>;

    /// Write a system snapshot.
    fn write_system(&self, sequence: u64, data: &[u8]) -> Result<(), StorageError>;

    /// Read the most recent valid system snapshot.
    fn read_latest_system(&self) -> Result<Option<(u64, Vec<u8>)>, StorageError>;
}
```

**Implementations.** The production implementations are the filesystem-based stores defined in §3 (trail), §5 (checkpoints), and §6/§7 (snapshots). The test implementation is an in-memory store — entries held in `Vec<Vec<u8>>`, checkpoint payloads in `HashMap<[u8; 32], Vec<u8>>`, snapshots in `HashMap<WorkspaceId, Vec<u8>>`. The in-memory backend satisfies the same trait with the same semantics, minus fsync (which is meaningless for in-memory data). This enables unit testing of the trail writer, query engine, and recovery engine without touching the filesystem.

**Error type.** `StorageError` is an enum covering I/O errors, corruption detected, capacity exceeded, and segment not found. It does not carry protocol semantics — the trail writer translates storage errors into protocol actions (degraded mode, operation rejection) as defined in the runtime spec (§6, §15).

## 12. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §4.4 (checkpoint) | §5 | Checkpoint structure, immutability, content integrity |
| §4.5 (trail) | §2, §3, §4 | Trail as append-only audit log, scopes, queryability |
| §6.1 (workspace internal model) | §6 | Nine components requiring persistence |
| §9.1 (trail entry schema) | §3, §4 | Entry structure, write-ahead rule |
| §9.3 (trail integrity) | §3, §10 | Append-only, immutable, no gaps |
| §9.4 (querying) | §4 | Trail queryable during run, access rules |
| §9.6 (storage and retention) | §8, §9 | Tiered storage, compaction, retention |
| §10.2 (recovery model) | §6, §7 | Trail as recovery source |
| §10.3 (partial failures) | §3, §10 | Trail write failures, degraded mode |
| §11.5 (trail integrity) | §3 | Hash chain, tamper evidence |

### Implementation Specs

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Runtime spec | §6 (trail write-ahead) | §1, §2, §3, §10 | Synchronous write, commit point, fsync requirement |
| Runtime spec | §8 (workspace isolation) | §6 | Nine components, `ArchivedWorkspace`, terminal state freezing |
| Runtime spec | §13 (recovery engine) | §6, §7 | Snapshot integration into recovery, trail replay |
| Runtime spec | §15 (error model) | §11 | Storage errors translated to protocol actions |
| Protocol interface spec | — | §6 | Protobuf encoding for snapshot serialization |

### Constituent Specs

| Spec | Referenced in | Topic |
|------|--------------|-------|
| Trail spec | §3, §4, §8, §9 | Trail structure, scopes, tiered storage, retention |
| Checkpoint spec | §5 | Checkpoint types, immutability, content integrity |
| Recovery spec | §6, §7 | Trail as recovery source, snapshot acceleration |
| Security spec | §3, §5 | Hash chains, content-addressable integrity |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/PROTOCOL.md) | Implementation Journal: [IMPLEMENTATION.md](../IMPLEMENTATION.md)*
