# Task 15.3: Tiered Storage

## Scope

Add zstd-compressed warm tier and optional cold tier for trail segments. `TierManager` tracks segment tiers, compresses segments during hot→warm transition, and moves segments to cold storage. Enforces the recovery window invariant (segments needed for recovery from latest snapshot stay hot).

## Types

### `SegmentTier`

```rust
pub enum SegmentTier { Hot, Warm, Cold }
```

### `SegmentInfo`

```rust
pub struct SegmentInfo {
    pub id: u64,
    pub tier: SegmentTier,
    pub path: PathBuf,
    pub created: SystemTime,
    pub size_bytes: u64,
}
```

### `TierManager`

```rust
pub struct TierManager {
    trail_dir: PathBuf,
    warm_dir: PathBuf,
    cold_destination: Option<PathBuf>,
    hot_limit: u32,
    warm_retention_days: u32,
    segments: Vec<SegmentInfo>,
    snapshot_anchor: Option<u64>,
}
```

## Functions

- `compress_segment(src, dst)` — zstd compress a sealed segment file
- `decompress_segment(src, dst)` — zstd decompress for warm tier reads
- `transition_hot_to_warm()` — move oldest hot segments beyond limit to warm
- `transition_warm_to_cold()` — move aged warm segments to cold destination
- `set_snapshot_anchor(seq)` — update recovery window boundary

## Tests

| Test | Verifies |
|------|----------|
| `compress_decompress_roundtrip` | zstd compress then decompress produces original |
| `hot_to_warm_respects_limit` | Only segments beyond hot_limit are compressed |
| `recovery_window_prevents_transition` | Segments after snapshot anchor stay hot |
| `warm_to_cold_moves_file` | Aged segments move to cold destination |
| `no_cold_destination_skips` | When cold_destination is None, no cold transition |
| `segment_listing` | TierManager correctly lists segments by tier |

## Acceptance Criteria

- zstd compression produces valid compressed data that decompresses correctly.
- Hot→warm transition respects hot segment count limit.
- Recovery window invariant prevents premature warm transitions.
- Cold tier is optional (no-op when destination not configured).
- All existing trail tests pass.
