# Backend perf baseline — 2026-04-20

> Initial `cargo bench` baselines for the three hot paths identified in HEALTH-LOG §8 + §10.1/10.2. Measured on dev box (WSL2, Linux 6.6). Not a CI-gated number — kept here as the reference point for post-v0.1.0 regression checks.
>
> Run with `scripts/bench-baseline.sh`. Each bench emits HTML at `target/criterion/*/report/index.html`.

## Session monitor — broadcast fan-out (`console-core::session_monitor_bench`)

Scenario: N subscribers attached to one `tokio::sync::broadcast::channel` (capacity 256 — matches `SessionMonitor`'s W3 cap), publisher fires 1000 frames as fast as the channel accepts, each subscriber drains.

| Subscribers | Mean time / 1000 frames | Throughput | Notes |
|---:|---:|---:|---|
| 1 | 112 µs | 8.9 M elem/s | Baseline — single-subscriber pass-through. |
| 4 | 278 µs | 3.6 M elem/s | Near-linear (4× subs → ~2.5× time — some fan-out overhead). |
| 16 | 987 µs | 1.0 M elem/s | Target load per W3 (oversight dashboard: ≤16 clients per session). At p50 ≈ 1 ms per 1000-frame burst. |
| 64 | 4.1 ms | 240 K elem/s | Headroom check — scales ~linearly with subscriber count. |

**Interpretation.** At the plan-target load (16 subscribers × 1000 frames), mean latency is 987 µs (~1 ms/burst, ~1 µs/frame/subscriber). The frame-fan-out cost is dominated by subscriber count, not frame volume, which matches the broadcast-channel design (one fan-out per send). **Regression tripwire:** a jump above 1.5 ms at N=16 / M=1000 indicates either subscriber churn (constantly resubscribing) or downstream backpressure.

## Console API middleware (`console-api::middleware_bench`)

Scenario: per-request authentication costs — Argon2id password verify (login path) + CSRF double-submit compare (state-changing routes).

| Path | Mean time | Target | Status |
|---|---:|---:|---|
| Argon2id verify | 28.8 ms | < 100 ms | ✅ calibrated |
| CSRF compare (32 bytes, constant-time) | 59.7 ns | < 100 µs | ✅ |

**Interpretation.** Argon2id at 28.8 ms is a reasonable cost-factor-calibrated setting — fast enough that login isn't user-visible, slow enough that brute-force over the network is impractical. CSRF compare is effectively free (60 ns via `subtle::ConstantTimeEq` on a 32-byte token). **Regression tripwires:** Argon2id > 200 ms (cost-factor accidentally increased) or < 10 ms (cost-factor accidentally decreased — security regression).

## wacp-llm stub — `serialize_for_match` (`wacp-llm::stub_bench`)

Scenario: serialize a conversation into a stable matcher string. Current `StubAdapter` implementation calls this **twice per `complete()`** (once for fixture match, once for input-token count = `serialized.len() / 4`).

| Messages × chars | Mean time | Notes |
|---:|---:|---|
| 1 × 500 | 173 ns | Trivial for one message. |
| 5 × 500 | 327 ns | Typical coordinator turn. |
| 20 × 500 | 595 ns | Long conversation with history. |

**Interpretation.** Cost scales linearly with message count (~25 ns per message at 500 chars + constant setup). At 20 messages, the double-call pattern cost ~1.2 µs per `complete()`. Not a hot spot at test-suite volumes.

**C5 landed.** `StubAdapter::resolve_response` now returns `(StubResponse, serialized_len)` so `complete()` + `complete_stream()` compute `input_tokens` from the returned length instead of re-serializing. One allocation per `complete()` now instead of two — architectural change, not visible in the serialize-only bench (which measures the fn in isolation). A follow-up `complete()`-end-to-end bench could quantify the saved ~595 ns at n=20; skipping here since the benefit is obvious from source inspection.

**C6 landed.** `complete_stream` now yields events lazily via `async_stream::stream!` instead of building the full `Vec<StreamEvent>` upfront. Peak memory in the stream path is O(1) in event count; a 1000-token fixture no longer preallocates 1000 `StreamEvent` instances before the first `yield`. `build_stream_events` helper deleted (dead code post-refactor). 169 wacp-llm tests still pass; `cargo clippy -- -D warnings` clean.

## console-db migration (`console-db::migration_bench`)

Scenario: single call to `create_test_pool()` — opens an in-memory SQLite DB, runs all 9 migrations.

| Path | Mean time | Threshold | Status |
|---|---:|---:|---|
| `create_test_pool` | 5.78 ms | 10 ms | ✅ under threshold |

**Interpretation.** Per HEALTH-LOG §9.3 the threshold for amortizing this via a `lazy_static!` migrated-template + `ATTACH DATABASE` clone pattern was >10 ms mean. At 5.78 ms we're under — the 9-migration replay per test is fast enough that the optimization's complexity cost exceeds its walltime benefit. **Regression tripwire:** a jump above 15 ms means either a migration got heavier or `sqlx` setup got slower; revisit then.

## Session launcher — `SubmitGoal → Decompose(N) → Dispatch×N → finalize` (`console-core::session_launcher_bench`)

Scenario: measure `SessionLauncher::launch` wall-time across N ∈ {1, 3, 10, 30} against a `ProgrammableCoordinator` (in-process, canned responses — no `wacp-runtime` child process). Setup (DB pool + coord spawn) is excluded via `iter_custom`; each sample re-seeds a fresh session row + N assignments and re-primes coord queues before timing the `launch` call.

| N assignments | Median time | Notes |
|---:|---:|---|
| 1  | 2.94 ms | Constant floor: SubmitGoal + Decompose + Dispatch×1 + DB finalize. |
| 3  | 5.40 ms | Dispatch cost ~0.82 ms / assignment over the N=1 floor. |
| 10 | 13.6 ms | Scales linearly with N. |
| 30 | 34.2 ms | ~1.04 ms / assignment at the high end. |

**Interpretation.** The N=1 floor (~3 ms) is dominated by the three gRPC round-trips (SubmitGoal, Decompose, one Dispatch) through the tonic transport + the DB finalize transaction. Per-Dispatch cost is ~1 ms end-to-end, consistent with an in-process localhost socket + one `UPDATE session_assignments SET workspace_id = ?` per assignment. The bench uses `ProgrammableCoordinator` rather than `InjectableCoordinator` (which forwards to a real runtime upstream) because the bench should not pay the child-process cost — we're measuring launcher logic, not runtime behavior. **Regression tripwire:** a jump above 2× at any N (e.g., N=1 > 6 ms, N=30 > 70 ms) signals either a DB-query regression or an extra RPC round-trip introduced in the launcher.

## Placeholders / follow-up benches

- **`stub_bench.rs` streaming path** — a memory-peak bench on the 1000-event fixture. `criterion` doesn't give peak-RSS natively; either wire a minimal `peak-alloc`-style allocator counter or use `dhat` with a profile option. Deferred — C6 architectural change was verified via source inspection + existing stream tests.

## How to regenerate

```bash
./scripts/bench-baseline.sh
```

Runs all four benches; re-review this doc against the new `target/criterion/*/report/index.html` numbers.

---

*Generated 2026-04-20 by Claude Opus 4.7 (1M context) as part of `backend-perf-baseline-plan.md` C1. This is a tier-3 baseline snapshot — overwrite in place as new measurements land; the git history is the diff.*
