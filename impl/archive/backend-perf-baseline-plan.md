---
id: wacp-backend-perf-baseline
type: impl
status: final
created: 2026-04-20T07:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, perf, baseline, benchmarks, v0.1.0]
depends_on: [HEALTH-LOG-8, HEALTH-LOG-9.3, HEALTH-LOG-10]
---

# Backend perf baseline

> **Split from `v0.1.0-readiness-plan.md` Track C** after Tracks A + B landed on 2026-04-20. The 7 bench-writing phases need ~5–8 h of dedicated work (harness scaffolding + three hot-path benches + two stub optimizations + one measure-and-decide), too much to fold into the same branch as the user-facing cleanup.
> **Target branch:** `refactor/backend-perf-baseline` from `dev` (post-cut-point-2 ff). Seven phases. ff to dev at end.
> **Rough effort:** ~5–8 h.
> **Not in scope:** anything front-end (handled by `v0.1.0-readiness-plan` Tracks A + B); tokio-console integration beyond what's needed to sanity-check the harness (separate future plan if needed).

## 1. Goal & Motivation

HEALTH-LOG §8 has been explicit since initial authoring that the backend side has not produced runtime-perf signals in any session — which means we also have no tripwires. Shipping v0.1.0 without bench baselines means the first post-v0.1.0 perf regression has no reference point to compare against.

Three hot paths deserve baselines:
- **`session_monitor` broadcast fan-out** — the oversight-dashboard data-plane workhorse; every active session has N subscribers consuming ~M frames/s.
- **`session_launcher` coordinator sequence** — the SubmitGoal → Decompose → Dispatch path that every new session walks; O(N) in task-count but the constant factor matters for 16+-task sessions.
- **`console-api/middleware`** — auth + CSRF per request; the Argon2id cost-factor calibration is a security / latency trade-off that should be measured, not assumed.

Two micro-optimizations from HEALTH-LOG §10 fit the same session naturally:
- **Stub `serialize_for_match` single-call** (§10.1) — one allocation per `complete()` instead of two.
- **Stub lazy streaming** (§10.2) — `async_stream!` replaces eager `Vec<StreamEvent>`; peak memory drops from O(n events) to O(1).

Plus one measurement from §9.3:
- **`console-db` migration amortization revisit** — measure current `create_test_pool()` cost; only act if >10 ms mean.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| **C1** | Criterion bench harness scaffold — add `criterion` dev-dep to `console-core` / `console-api` / `wacp-llm`; `benches/` dir + `[[bench]]` entries; top-level `scripts/bench-baseline.sh` emitting a `docs/perf-baseline-2026-04-20.md` summary | ~1–2 h | — | `cargo bench --workspace` runs green; report generated |
| **C2** | `session_monitor` broadcast fan-out bench — N subscribers × M frames, p50/p95/p99 | ~1 h | C1 | bench at N=16 / M=1000; results in summary doc |
| **C3** | `session_launcher` coordinator sequence bench — SubmitGoal → Decompose(N) → Dispatch(N) at N=1/4/16/64 | ~1 h | C1 | scaling curve documented; anomaly if ≠ O(N) |
| **C4** | `console-api/middleware` auth + CSRF bench | ~1 h | C1 | Argon2id p99 < 100 ms; CSRF + rate-limit combined < 1 ms |
| **C5** | `wacp-llm` stub `serialize_for_match` single-call opt (§10.1) | ~30–60 min | C1 | `complete()` walltime drops by the serialization cost |
| **C6** | `wacp-llm` stub lazy streaming (§10.2) | ~1 h | C5 | RSS peak on 1000-event fixture drops from O(n) to O(1) |
| **C7** | `console-db` migration amortization revisit (§9.3) | ~30 min | C1 | current cost measured; optimization scaffolded iff >10 ms mean |

## 3. Deliverables — per phase

Deliverables inherited verbatim from `v0.1.0-readiness-plan.md` §3.3 (C1–C7). Content moves when the successor plan lands; until then, see that doc for each phase's mechanics.

## 4. Acceptance Criteria

### Per-phase
- [ ] C1: `criterion` in 3 target crates' dev-deps; `scripts/bench-baseline.sh` runs green; report at `docs/perf-baseline-2026-04-20.md`.
- [ ] C2: p50/p95/p99 captured for 16 subs × 1000 frames.
- [ ] C3: curve at N=1/4/16/64 captured; scaling annotation.
- [ ] C4: Argon2id + CSRF + rate-limit per-request overhead measured.
- [ ] C5: before/after `complete()` walltime measured; stub unit tests still green.
- [ ] C6: before/after RSS peak measured; stream tests still green.
- [ ] C7: measurement recorded; optimization landed iff baseline >10 ms mean.

### Plan-level
- [ ] Branch `refactor/backend-perf-baseline` ff'd to dev.
- [ ] HEALTH-LOG §8, §9.3, §10.1/10.2 updated with landed status + commit SHAs.
- [ ] `docs/perf-baseline-2026-04-20.md` committed as the numerical reference.
- [ ] `v0.1.0-readiness-plan.md` execution-log Track C rows updated with "landed via backend-perf-baseline-plan" pointers.
- [ ] Both plans archived together (v0.1.0-readiness first, then this one — citing the completion in its own §7).

## 5. Risks / Open Questions

Inherited from `v0.1.0-readiness-plan.md` §5 C-items (C1 harness choice criterion-vs-divan; C4 Argon2id cost-factor surprise; C6 `async_stream` license check). No new risks specific to the split.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| HEALTH-LOG-8 | §8 Backend — not yet investigated | motivation |
| HEALTH-LOG-9.3 | §9.3 console-db perf signals | C7 source |
| HEALTH-LOG-10.1 | §10.1 Per-call full-input serialization | C5 source |
| HEALTH-LOG-10.2 | §10.2 Streaming events materialized eagerly | C6 source |
| wacp-v0.1.0-readiness | `impl/v0.1.0-readiness-plan.md` | parent plan; held at cut-point #2 until this plan lands |

## 7. Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| (plan scaffold) | `c2e5df0` | 2026-04-20 | direct-to-dev; split from v0.1.0-readiness Track C |
| C1 harness scaffold | `c5149af` | 2026-04-20 | criterion + 4 bench files across `console-core`/`console-api`/`wacp-llm` + `scripts/bench-baseline.sh` + baseline doc |
| C2 session_monitor broadcast | _(landed with C1)_ | 2026-04-20 | measured @ 16 subs × 1000 frames: mean 987 µs (~1 ms/burst) |
| C3 session_launcher sequence | _(placeholder)_ | 2026-04-20 | bench file scaffolded; real benchmark deferred pending `InjectableCoordinator` mock relocation from `wacp-console/integration` → `console-test-support`. Documented as follow-up in baseline doc. |
| C4 console-api middleware | _(landed with C1)_ | 2026-04-20 | argon2_verify 28.8 ms (target <100 ms ✓); csrf_compare_32b 59.7 ns |
| C5 stub single-call serialize | `4b735b0` | 2026-04-20 | `resolve_response` returns `(StubResponse, serialized_len)`; `complete()`/`complete_stream()` compute `input_tokens` from returned length. 169 wacp-llm tests pass. |
| C6 stub lazy streaming | `4b735b0` | 2026-04-20 | `complete_stream` uses `async_stream::stream!` instead of eager `Vec<StreamEvent>`; peak memory now O(1) in event count. `build_stream_events` deleted. |
| C7 console-db migration | `8e117e4` | 2026-04-20 | measured `create_test_pool()` at 5.78 ms mean — under the 10 ms threshold, amortization optimization not justified. Regression tripwire: 15 ms. |

---

*Scaffolded 2026-04-20 by Claude Opus 4.7 (1M context) after `v0.1.0-readiness-plan.md` reached its §8 cut-point #2 (Tracks A + B complete). Carves Track C out so its 5–8 h of bench-writing can land in a dedicated future session without blocking the user-facing ship.*
