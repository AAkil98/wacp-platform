---
id: wcon-merge-execution-log
type: impl
status: in-progress
created: 2026-04-15T03:00:00
authors: [AAkil98, Claude Opus 4.6]
tags: [monorepo, merge, execution, resumption]
depends_on: [wcon-merge-plan, wcon-wiring-strategy]
---

# Merge Execution Log — `wacp-platform/` assembly

> Live execution record of `wacp-console/impl/merge-plan.md` (M0 → M7). Read alongside the plan — this file captures actual commit SHAs, deviations from the procedure, and the precise next action. If resuming a cold session, start here, not at the top of the plan.

## Table of Contents

- 1. TL;DR — Where We Are Right Now
- 2. Three-Repo Layout (Current State)
- 3. Execution Log (M0 → M2)
- 4. Deviations from the Plan
- 5. Next Step — M3
- 6. Outstanding Plan Corrections

---

## 1. TL;DR — Where We Are Right Now

**Completed:** M0 (pre-flight) + M1 (subtree import) + M2 (workspace unification).

**Working umbrella:** `/home/aakil98/mada/wacp-platform/` — on branch `main`, commit `d8b3d4d`. 23 workspace members compile against a unified `Cargo.toml`; both primary binaries (`wacp-runtime`, `wacp-console`) check cleanly.

**Source repos:** untouched since the pre-monorepo tags. Ready to archive after W-phase work lands (30-day fallback per §7 of merge-plan).

**Next:** M3 — extract `wacp-proto` crate (shared `tonic_build` codegen). Plan §5.4 calls for one atomic commit.

## 2. Three-Repo Layout (Current State)

| Repo | Path | Branch | HEAD | Role |
|------|------|--------|------|------|
| `wacp` (source) | `/home/aakil98/mada/wacp/` | `dev` | `d010336` | Frozen at `pre-monorepo-wacp` tag. Subtree source. |
| `wacp-console` (source) | `/home/aakil98/mada/wacp-console/` | `dev` | `6c19eb0` | Frozen at `pre-monorepo-console` tag. Subtree source. |
| `wacp-platform` (umbrella) | `/home/aakil98/mada/wacp-platform/` | `main` | `d8b3d4d` | Active work. Not yet pushed to any remote. |

**To resume merge execution:** `cd /home/aakil98/mada/wacp-platform && git log --oneline -5` should show `d8b3d4d` at HEAD.

## 3. Execution Log (M0 → M2)

### M0 — Pre-flight (§5.1 of merge-plan)

| Step | Action | Anchor |
|------|--------|--------|
| Inspect `wacp/?? .cargo/` | Confirmed legitimate WSL2 OOM mitigation (26 lines, `jobs=1` + `mold` linker). Committed on wacp `dev`. | `d010336` — wacp |
| Re-sync `wacp-console/Cargo.lock` | Phase 2 dep additions (arc-swap, reqwest, console-runtime, serde_yaml_ng, tempfile) had never been re-committed. | `40457e5` — console |
| Land finalized merge-plan + cross-link from SEED + wiring-strategy | `status: final`, 12 decisions + 6 §10 items resolved. SEED resumption-point sync'd. | `6c19eb0` — console |
| Rollback-anchor tags | `pre-monorepo-wacp` at `d010336` (wacp), `pre-monorepo-console` at `6c19eb0` (console). | Local-only, no push. |

### M1 — Subtree import (§5.2)

Umbrella initialized on `main` with empty root commit `ec22702`. Local-path remotes (`../wacp` → `wacp-origin`, `../wacp-console` → `console-origin`). Two subtree adds:

| Prefix | Source tip | Merge commit | Parents |
|--------|-----------|--------------|---------|
| `wacp/` | `d010336` | `e83442d` | `ec22702` + `d010336` |
| `wacp-console/` | `6c19eb0` | `fabc758` | `e83442d` + `6c19eb0` |

**225 commits total in umbrella** (full history preserved via subtree grafts, reachable by walking parent SHAs).

### M2 — Workspace unification (§5.3)

Umbrella `Cargo.toml` unions both sides' `[workspace.dependencies]`. Three superset picks (all additive, safe): `tokio-util` features `["rt"]`, `reqwest` features `+["stream"]`, `uuid` features `+["serde"]`.

Internal workspace members declared with umbrella-relative paths so existing `{ workspace = true }` inheritance across all 22 crates continues to resolve: `wacp-taxonomy`, `wacp-types`, `console-api`, `console-core`, `console-db`, `console-runtime`, `console-test-support`.

Validation passed:
- `cargo metadata` → 23 workspace members resolve (plan D4's "22" missed the `wacp/tests` integration crate)
- `cargo check -p wacp-console` → 2m39s ✓
- `cargo check -p wacp-runtime` → 1m33s ✓
- `tonic v0.13.1`, `prost v0.13.5`, `tokio v1.52.0`, `serde v1.0.228` — all single-version
- SQLite exclusivity: `wacp-console` → `sqlx-sqlite` only; `wacp-runtime` → `rusqlite` only; shared `libsqlite3-sys v0.30.1`

M2 commit: `d8b3d4d` (`feat(m2): unify workspace + umbrella tooling hoisted early`).

## 4. Deviations from the Plan

### 4.1 Three files hoisted from M5 → M2 (safety-driven forward-port)

Plan §5.6 M5 creates umbrella root tooling. Three of those artifacts were needed earlier to make M2 validation safe and clean:

| File | Why hoisted at M2 | What M5 must now do |
|------|-------------------|---------------------|
| `.cargo/config.toml` | Cargo only walks *up* from cwd — without this at umbrella root every `cargo check/build` from root would use default parallelism and risk WSL2 OOM (wacp config header documents `jobs=2` confirmed to crash). | **Deduplicate.** Source-side `wacp/.cargo/config.toml` still exists; remove it so there's one canonical copy. |
| `rust-toolchain.toml` | Simple hoist — only wacp-console had one; copied verbatim. | **No-op.** Already in the right place. |
| `.gitignore` | M2 `git add -A` accidentally staged 281KB of `target/` artifacts; umbrella needed an explicit gitignore immediately. Applied §10.5 corrections inline (no `Cargo.lock` rule; explicit `**/node_modules/`). | **Deduplicate / audit.** `wacp/.gitignore` still has the now-inert `Cargo.lock` rule and `wacp/`-specific patterns; `wacp-console/.gitignore` has its own subset. Decide whether to keep subdir gitignores as augmentations or fold everything into umbrella. |

### 4.2 §6 validation checklist correction — `git log --follow` vs subtree

Plan §6 lists:
```
git log --follow wacp/crates/wacp-runtime/src/lib.rs shows pre-merge history
git log --follow wacp-console/crates/console-api/src/lib.rs shows pre-merge history
```

Both return empty. `--follow`'s rename-tracking doesn't cooperate with `git subtree add`'s path-rewrites. History IS fully present in the graph (verified: `git log d010336 -3` surfaces pre-subtree commits; both subtree merge commits have correct parents). The correct validation incantation is `git log <subtree-tip-SHA> -- <path>` or `git log --all -- <path>`. **§6 needs this correction.**

### 4.3 Plan D4 member count off-by-one

Plan §2 D4 says "22 crates as members." Actual count is 23 (16 wacp-* + `wacp/tests` integration crate + 6 console-*). Minor; noted here so no one wastes time hunting for the missing crate.

## 5. Next Step — M3

**Goal:** Extract `wacp-proto` crate that owns `tonic_build` codegen. Both `wacp-runtime` (which currently codegens via `wacp-transport`'s build.rs) and `console-runtime` (which codegens via its own build.rs reading `../../../wacp/proto/*.proto`) become consumers.

**Per §5.4 of `wacp-console/impl/merge-plan.md`:**

```
mkdir -p wacp/crates/wacp-proto/src
# wacp-proto/Cargo.toml: tonic, prost workspace deps; tonic-build as build-dep
# wacp-proto/build.rs: compile_protos(&["../../proto/*.proto"], &["../../proto"])
# wacp-proto/src/lib.rs: pub mod agent, coordinator, highway, primitives, taxonomy
```

**Then:**
1. Add `wacp-proto = { path = "wacp/crates/wacp-proto" }` to umbrella `Cargo.toml` `[workspace.dependencies]`.
2. Update wacp's existing proto-consuming crate (likely `wacp-transport`) to depend on `wacp-proto` instead of running its own `tonic_build`.
3. Update `console-runtime/Cargo.toml` to use `wacp-proto` via `{ workspace = true }`; **delete `console-runtime/build.rs`**.
4. Verify with `cargo check -p wacp-console` and `cargo check -p wacp-runtime` — both must still pass.
5. Atomic single commit. If partial, CI breaks on both sides simultaneously.

**Before starting:** inspect `wacp/proto/*.proto` (5 files: `agent.proto`, `coordinator.proto`, `highway.proto`, `primitives.proto`, `taxonomy.proto`) and find the current codegen location. Grep targets:
```
grep -rn 'tonic_build\|tonic::include_proto' wacp/crates/
grep -rn 'tonic_build' wacp-console/crates/console-runtime/
```

Plus: find who currently re-exports the generated modules. Likely `wacp-transport` has something like `pub mod agent { tonic::include_proto!("agent"); }` — the new `wacp-proto` crate subsumes this.

## 6. Outstanding Plan Corrections

These corrections accumulated from §10.1–§10.6 resolutions and from M0–M2 execution. The plan body (§§1–9) still reads as drafted; executors should cross-reference §10.x and this log before acting on any §§1–9 step.

Tagged by milestone they affect:

- **§1.1** — lists `.cargo/` under `wacp-console/` (doesn't exist there; just hoist from wacp).
- **§1.2** — `.cargo/` collision row should say "hoist" not "union."
- **§2 D4** — member count 22 should be 23.
- **§5.3** — tokio-util/reqwest/uuid superset picks explicit (already done at M2 but plan body still says "union features explicitly" without specifics).
- **§5.6 M5** — reframe as dedup/audit rather than first-creation for `.cargo/`, `rust-toolchain.toml`, `.gitignore`. Add rust-embed integration scope per §10.3. Add env-var clap attributes per §10.2. Add console Dockerfile path alignment per §10.3.
- **§5.7 M6** — per-job hybrid (body-verbatim + wiring-rewrite) per §10.6; post-M4 `ci-console.yml` loses cross-repo checkout; `paths:` filter includes `.github/workflows/ci-*.yml`; `-test`-tag guards on `release-runtime.yml` per §10.4.
- **§6** — `git log --follow` entries must become `git log --all -- <path>` (see §4.2 above).
- **§6** — add "`cargo build --release -p wacp-console` succeeds only when `frontend/dist/` is present" per §10.3 safety-net.
- **§8 risk row** — rust-embed impact Medium → Low (compile-time failure is loud, not silent).
- **§9** — add: "Execute §10.4 cleanup runbook after M7 validation observes green." Add: "Consider unifying pnpm 9/10 and node 22/24 between `ci-wacp.yml` and `ci-console.yml` — deferred from M6." Add: "Remove stale `Cargo.lock` line from `wacp/.gitignore` (inert since M2)."

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-merge-plan | Monorepo Merge Plan | implements (this log records actual execution of that plan) |
| wcon-wiring-strategy | Wiring Strategy | informs (W0 = this merge; W1+ begins post-M7) |

*WACP Platform — authored by AKIL Abderrahim and Claude Opus 4.6*
