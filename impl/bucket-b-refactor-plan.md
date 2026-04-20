---
id: wacp-bucket-b-refactor
type: impl
status: draft
created: 2026-04-20T01:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [refactor, file-splits, bucket-b, closeout-plan-P4]
depends_on: [tech-debt-2026-04-18, wacp-closeout-plan]
---

# Bucket B refactor — `refactor/file-splits` execution log

> **Sub-plan for `closeout-plan.md` P4.** Content source of truth is `tech-debt-2026-04-18.md` §3.2. This doc tracks incremental execution of the 9-file split across what may span multiple sessions, so progress is resumable.
> **Branch:** `refactor/file-splits`, branched from `dev` tip (post-2026-04-20 ff) at `b076711`.
> **Acceptance:** no Rust production file >800 lines in B.1–B.4 scope, B.5 files under ~800 lines; `cargo test --workspace` pass count unchanged; clippy + fmt clean; `.file-size-allowlist` Rust production section reduced to ≤1 entry.

## Baseline (pre-refactor, SEED.md-sourced)

| Crate | Test count |
|---|---:|
| `wacp-coordinator` | 387 |
| `wacp-workspace` | 65 |
| `wacp-runtime` | 109 |
| `wacp-types` | 45 |
| `console-integration` | 50 (0 ignored) |
| `wacp-local` | 86 |
| `wacp-cli` | 132 |
| `sdk-python` | 104 |

## Phases

### B.1 — `wacp/crates/wacp-runtime/src/init.rs` (2139 lines)

**Target split** (per tech-debt §3.2):
- `init.rs` (kept, shrunk to ~734): `Runtime` struct, `RuntimeError`, `CheckpointRecord`, `init/init_in_memory/run/fan_out_event/shutdown`.
- `agent_service.rs` (new, ~340): `impl Runtime { handle_agent_request }`.
- `highway_service.rs` (new, ~408): `impl Runtime { handle_highway_request }`.
- `coordinator_service.rs` (new, ~495): `impl Runtime { handle_coordinator_request }`.
- `conversions.rs` (new, ~162): the `*_to_proto` / `proto_to_*` helpers (lines 1978–2139).

**Deviation from tech-debt §3.2 letter**: the plan calls the surviving lifecycle module `runtime.rs`, but renaming `init.rs → runtime.rs` inside a crate named `wacp-runtime` creates `wacp_runtime::runtime::Runtime` which is awkward. Keeping `init.rs` means zero churn to `main.rs` / `tests.rs` re-exports and preserves module-level blame. The *structural* goal — one module per handler — is unchanged.

**Status:** landed 2026-04-20. init.rs 2139 → 819 lines (under warn threshold; removed from `.file-size-allowlist`). Test baseline holds: 109 wacp-runtime tests still pass across all four incremental commits.

### B.2 — `wacp-console/crates/console-core/src/session_monitor.rs` (2120 lines)

**Deviation from tech-debt §3.2 letter.** The file's production code is only ~787 lines; the remaining 1328 lines are a single `#[cfg(test)] mod tests { ... }` block. tech-debt §3.2 proposed a 5-way split (`trail_driver.rs` / `gate_driver.rs` / `escalation_driver.rs` / `workspace_driver.rs` / `monitor.rs`), but each driver function is only 22–42 lines — extraction would produce 25-line sibling files for each, which has little navigability payoff. The real cohesive block is `impl Monitor` (297 lines), already cohesive inside the main file.

**What landed:** the inline test module was extracted to a sibling `session_monitor_tests.rs` via `#[cfg(test)] #[path = "session_monitor_tests.rs"] mod tests;`. Same test-coverage, same visibility rules (`use super::*` resolves against `session_monitor`), one file-boundary. Production file drops from 2120 → 779 lines (under warn threshold; removed from `.file-size-allowlist`). Test monolith added to the test-file allowlist per the convention documented in the allowlist header.

**Status:** landed 2026-04-20. 179 console-core tests still pass; fmt + clippy clean.

### B.3 — `wacp-console/crates/console-core/src/session_launcher.rs` (1877 lines)

Split by launch stage per tech-debt §3.2: `submit.rs` + `decompose.rs` + `dispatch.rs` + `rollback.rs` + `launcher.rs`.

**Status:** pending.

### B.4 — `wacp-console/crates/console-api/src/routes/highway.rs` (1832 lines)

Split by REST resource per tech-debt §3.2: `routes/highway/gates.rs` + `escalations.rs` + `envelopes.rs` + `pending.rs` + `mod.rs`.

**Status:** pending.

### B.5 — `config.rs` / `recovery.rs` / `rest_gateway.rs` / `routes/sessions.rs` / `tools/execution.rs`

Each splits along natural internal boundaries. Target no file >800 lines.

**Status:** pending.

## Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| (plan scaffold) | — | 2026-04-20 | direct to `refactor/file-splits` branch |
| B.1a conversions | `b9b9eb6` | 2026-04-20 | extract 11 proto-conversion helpers; init.rs 2139 → 1974 |
| B.1b agent_service | `c5199dd` | 2026-04-20 | extract handle_agent_request; visibility widened; init.rs 1974 → 1640 |
| B.1c highway_service | `dc4b5f5` | 2026-04-20 | extract handle_highway_request; init.rs 1640 → 1231 |
| B.1d coordinator_service | `61afe2b` | 2026-04-20 | extract handle_coordinator_request; init.rs 1231 → 819; allowlist entry removed |
| B.2 test-extraction | _(this commit)_ | 2026-04-20 | inline `mod tests` → sibling `session_monitor_tests.rs` via `#[path]`; production file 2120 → 779; allowlist entry removed |

## Invariants

- **Behavior-preserving.** No logic changes. Only extraction, `pub(crate)` visibility tweaks, import fix-ups.
- **One commit per structural step** (e.g., "extract conversions", "extract agent_service"). Preserves blame and lets future bisects land at a specific extraction.
- **`cargo test` green after every commit.** Never commit a broken tree — bisect becomes useless.
- **`.file-size-allowlist` shrinks with the refactor.** Removing an entry when the file drops under threshold is part of the same commit.
- **ff to `dev` at the end** per `impl/git-strategy.md` §5.2 — preserve §X.Y anchors via multiple commits rather than one squash.

## References

| ID | Title | Relationship |
|----|-------|--------------|
| tech-debt-2026-04-18 | Tech-debt triage (§3.2 Bucket B) | content source of truth for the splits |
| wacp-closeout-plan | Closeout plan (§3.4 P4) | parent plan this doc sub-executes |
| wacp-git-strategy | Git strategy (§5.2, §9.3) | merge ceremony for this branch |

---

*Sub-plan scaffolded 2026-04-20 by Claude Opus 4.7 (1M context) at session resume after closeout-plan P2 + P3 landed and ff'd to main.*
