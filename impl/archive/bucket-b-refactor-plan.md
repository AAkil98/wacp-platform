---
id: wacp-bucket-b-refactor
type: impl
status: final
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

**Deviation from tech-debt §3.2 letter.** File was 1877 lines; 1308 in a single `#[cfg(test)] mod tests { ... }` block. Production-only is 568 lines — already cohesive and under threshold. Extracting to `submit/decompose/dispatch/rollback/launcher` siblings as the plan proposed would have required widening 30+ internal helpers from `fn` to `pub(crate) fn` + refactoring private state access. The stage orchestration flow reads top-to-bottom naturally today, so keeping it unified preserves that anchor.

**What landed:** test module extracted via `#[path]` pattern. Production file 1877 → 572 lines.

**Status:** landed 2026-04-20 (commit `747a991`). 179 console-core tests still pass.

### B.4 — `wacp-console/crates/console-api/src/routes/highway.rs` (1832 lines)

**Partial deviation.** The plan proposed splitting into `gates.rs` / `escalations.rs` / `envelopes.rs` / `pending.rs` / `mod.rs`. The `pending` submodule **already exists** at line 503 (`pub(crate) mod pending { ... }`) — so the pending→mod split is essentially already done. What's left is the outer file containing gate + escalation + envelope handlers plus shared helpers.

**What landed in this session:** top-level `#[cfg(test)] mod tests` (line 1301, 530 lines) extracted to a sibling via `#[path]`. The nested `#[cfg(test)] mod tests` inside `pub(crate) mod pending` at line 753 stays in place — scoped to that submodule and not reachable with the same outer `#[path]` trick.

**What still landed as a follow-up on `refactor/config-highway-splits`:** the `pub(crate) mod pending` inline submodule (769 lines — ~60% of the file) extracted to a sibling `routes/highway_pending.rs` via `#[path = "highway_pending.rs"] pub(crate) mod pending;`. `pending` was already a cohesive submodule with its own `use super::*;` prelude, so extraction was mechanical — zero logic change, no visibility widening needed (pending's contents were always `pub(super)` within the mod; the new file keeps that semantics since `use super::*;` resolves against `routes/highway`). The other three proposed modules (gates / escalations / envelopes) are 90–110 line handlers each and don't warrant their own files now that the big pending block is gone.

**Status:** fully landed 2026-04-20 (B.4 initial test-extract `8b9df1f`; pending-submodule extract `27789ff..`). 143 console-api tests still pass; fmt + clippy clean. `routes/highway.rs` final: 536 lines. `routes/highway_pending.rs`: 765 lines. Entry removed from `.file-size-allowlist`.

### B.5 — `config.rs` / `recovery.rs` / `rest_gateway.rs` / `routes/sessions.rs` / `tools/execution.rs`

**Same test-extraction pattern for all five.** Each splits the inline `#[cfg(test)] mod tests` to a sibling `*_tests.rs` via `#[path]`. Effect per file:

| File | Before | After | Test file | Allowlist |
|---|---:|---:|---|---|
| `recovery.rs` | 1485 | 251 | `recovery_tests.rs` (1234) | production removed; test monolith added |
| `rest_gateway.rs` | 1202 | 772 | `rest_gateway_tests.rs` (429) | production removed; test file under 1000, no entry |
| `routes/sessions.rs` | 1181 | 851 | `sessions_tests.rs` (331) | production removed; test file under 1000, no entry |
| `execution.rs` | 1137 | 165 | `execution_tests.rs` (970) | production removed; test file under 1000, no entry |
| `config.rs` | 1748 | 1038 | `config_tests.rs` (710) | production moved "oversized" → "warn-only"; test file under 1000, no entry |

**config.rs extraction quirk.** First attempt corrupted the YAML fragments inside `r#"..."#` raw strings — the naïve `sed 's/^    //'` de-indent (applied per-line before rustfmt) stripped 4 spaces from each line, including the ones inside YAML literals, breaking the `parse_full_config` test. Fixed by extracting *without* de-indent and letting rustfmt re-indent the Rust code on AST lines (it doesn't touch string-literal contents). Lesson captured in the B.5e commit message.

**config.rs follow-up landed.** Rather than a four-way `server/storage/resources/mod` split, the pragmatic cut was extracting the `WACP_*` env-variable override block — `apply_env_overrides`, `apply_overrides_from`, `apply_single_override`, `parse_{bool,u32,u64,f32}_env` — to a sibling `config_env.rs`. `config.rs` re-exports `apply_env_overrides` (pub) and `apply_overrides_from` (pub(crate) #[cfg(test)]) so existing callers and the inline test module's `use super::*;` resolve without modification. Final: config.rs 860 lines; config_env.rs 193 lines. Both under the 1000-line warn threshold. The four-way struct-grouped split wasn't needed once env-overrides moved out.

**Status:** landed 2026-04-20 (commits `34f29c0`, `b045e60`, `e2d5e42`, `4d5e348`, `0e286e3` for B.5a–d + test-extract on config; `27789ff` for the env-override follow-up). Test counts: 179 console-core + 209 wacp-transport + 143 console-api + 135 wacp-tools + 109 wacp-runtime — all green.

## Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| (plan scaffold) | — | 2026-04-20 | direct to `refactor/file-splits` branch |
| B.1a conversions | `b9b9eb6` | 2026-04-20 | extract 11 proto-conversion helpers; init.rs 2139 → 1974 |
| B.1b agent_service | `c5199dd` | 2026-04-20 | extract handle_agent_request; visibility widened; init.rs 1974 → 1640 |
| B.1c highway_service | `dc4b5f5` | 2026-04-20 | extract handle_highway_request; init.rs 1640 → 1231 |
| B.1d coordinator_service | `61afe2b` | 2026-04-20 | extract handle_coordinator_request; init.rs 1231 → 819; allowlist entry removed |
| B.2 test-extraction | `a6af818` | 2026-04-20 | inline `mod tests` → sibling `session_monitor_tests.rs` via `#[path]`; production file 2120 → 779; allowlist entry removed |
| B.3 test-extraction | `747a991` | 2026-04-20 | session_launcher test monolith → sibling file; 1877 → 572 |
| B.5a recovery | `34f29c0` | 2026-04-20 | recovery test monolith → sibling; 1485 → 251 |
| B.5b rest_gateway | `b045e60` | 2026-04-20 | rest_gateway test monolith → sibling (`pub(crate)` preserved); 1202 → 772 |
| B.5c routes/sessions | `e2d5e42` | 2026-04-20 | cancel_tests module → sibling; 1181 → 851 |
| B.5d execution | `4d5e348` | 2026-04-20 | execution test monolith → sibling; 1137 → 165 |
| B.4 routes/highway | `8b9df1f` | 2026-04-20 | top-level tests → sibling; nested `pending::tests` left in place; 1832 → 1302 (still warn-only) |
| B.5e config | `0e286e3` | 2026-04-20 | config test monolith → sibling (YAML-in-raw-string preserved by skipping sed de-indent); 1748 → 1038 (still warn-only) |
| closeout + fmt | `55c29ab` | 2026-04-20 | bucket-b plan refresh + rustfmt cleanup of config_tests.rs; 12 commits total on `refactor/file-splits`, 1939 workspace tests green |
| B.5e follow-up config_env | `27789ff` | 2026-04-20 | on `refactor/config-highway-splits`; config.rs 1038 → 860 via env-override extract; allowlist entry removed |
| B.4 follow-up highway_pending | _(this commit)_ | 2026-04-20 | on `refactor/config-highway-splits`; routes/highway.rs 1302 → 536 via `pending` submodule extract; allowlist entry removed. `.file-size-allowlist` Rust production section now empty. |

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
