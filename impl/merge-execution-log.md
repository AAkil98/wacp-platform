---
id: wcon-merge-execution-log
type: impl
status: in-progress
created: 2026-04-15T03:00:00
revised: 2026-04-15T04:10:00
authors: [AAkil98, Claude Opus 4.6]
tags: [monorepo, merge, execution, resumption]
depends_on: [wcon-merge-plan, wcon-wiring-strategy]
---

# Merge Execution Log — `wacp-platform/` assembly

> Live execution record of `wacp-console/impl/merge-plan.md` (M0 → M7). Read alongside the plan — this file captures actual commit SHAs, deviations from the procedure, and the precise next action. If resuming a cold session, start here, not at the top of the plan.

## Table of Contents

- 1. TL;DR — Where We Are Right Now
- 2. Three-Repo Layout (Current State)
- 3. Execution Log (M0 → M7)
- 4. Deviations from the Plan
- 5. Next Step — Tag & Push
- 6. Outstanding Plan Corrections

---

## 1. TL;DR — Where We Are Right Now

**Completed:** M0 → M7. The merge plan is fully executed; §6 validation checklist is green modulo three documented pre-existing deviations (§4.5) and the docker/UI items that require out-of-band environments (§4.6).

**Working umbrella:** `/home/aakil98/mada/wacp-platform/` — on branch `main`, commit `76237a9`. 23 workspace members build cleanly; `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and `cargo test --workspace` all green (1442 tests passed, 0 failed). Both binaries start end-to-end: `wacp-runtime serve` loads 7 verticals and serves REST on `[::1]:9093`; `wacp-console serve` loads taxonomy via REST (roles=37, tools=68, verticals=7) and serves the rust-embed'd SPA on `[::1]:8080`.

**Source repos:** untouched since the pre-monorepo tags. Ready to archive after W-phase work lands (30-day fallback per §7 of merge-plan).

**Next:** Tag `monorepo-v0` locally, then coordinate the remote push + source-repo archive per §5.8. Docker/OCI image validation and the UI login/profile/session smoke are the last two §6 items deferred until docker and a browser session are available.

## 2. Three-Repo Layout (Current State)

| Repo | Path | Branch | HEAD | Role |
|------|------|--------|------|------|
| `wacp` (source) | `/home/aakil98/mada/wacp/` | `dev` | `d010336` | Frozen at `pre-monorepo-wacp` tag. Subtree source. |
| `wacp-console` (source) | `/home/aakil98/mada/wacp-console/` | `dev` | `6c19eb0` | Frozen at `pre-monorepo-console` tag. Subtree source. |
| `wacp-platform` (umbrella) | `/home/aakil98/mada/wacp-platform/` | `main` | `76237a9` | Active work. Not yet pushed to any remote. |

**To resume:** `cd /home/aakil98/mada/wacp-platform && git log --oneline -10` should show the M3–M7 chain topped by `76237a9` (`build(m7): split highway-ui production build from test typecheck`).

## 3. Execution Log (M0 → M7)

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

### M3 — Extract `wacp-proto` (§5.4)

Atomic single-commit extraction — per §5.4, partial state would break CI on both sides. New crate `wacp/crates/wacp-proto/` owns one `build.rs` that compiles all 5 proto files (`agent`, `coordinator`, `highway`, `primitives`, `taxonomy`) under the `wacp.v1` package with both server and client stubs. Three consumers drop their own `tonic-build` build-dep + build.rs:

| Consumer | Before | After |
|----------|--------|-------|
| `wacp-transport` | local build.rs + `tonic::include_proto!("wacp.v1")` | depends on `wacp-proto`; `src/proto.rs` re-exports `wacp_v1::*` (server + client) |
| `console-runtime` | local build.rs reading `../../../wacp/proto/*.proto` (cross-repo path) | depends on `wacp-proto` via `{ workspace = true }`; build.rs deleted |
| `console-test-support` | local build.rs for mock server stubs | same — deleted build.rs, now uses `wacp-proto` |

Public module paths preserved via re-exports so no call-site edits were needed: `wacp_transport::proto::wacp_v1::*`, `console_runtime::proto::*`, `console_test_support::mock_grpc::proto`.

Validated: `cargo check --workspace` clean in 1m26s; `wacp-runtime` + `wacp-console` both build.

M3 commit: `c713fcc` (`feat(m3): extract wacp-proto for shared tonic_build codegen`).

### M4 — Flip path deps to workspace inheritance (§5.5)

Every per-crate `Cargo.toml` in `wacp/` now references its sibling internal crates via `{ workspace = true }` instead of `{ path = "../wacp-foo" }`. Umbrella `[workspace.dependencies]` owns the paths — one canonical declaration per internal crate. Added the 11 runtime-side crates that hadn't yet been hoisted into umbrella workspace deps: `wacp-clock`, `-coordinator`, `-coordinator-sdk`, `-fsm`, `-permissions`, `-recovery`, `-sdk`, `-tools`, `-trail`, `-transport`, `-workspace`.

**Plan correction (§4.4 below):** §5.5 directs "remove explicit paths" from workspace deps on `wacp-taxonomy`/`wacp-types`/`wacp-proto`. Cargo rejects bare `{ version = "0.1.0" }` for unpublished crates (falls through to crates.io). Paths must stay at the workspace level; the "flip" is achieved at the consumer level — which matches the plan's intended outcome ("consumers reference via `{ workspace = true }`").

M4 commit: `c6a421b` (`refactor(m4): flip internal path deps to workspace inheritance`).

### M5 — Dedup / docs / env-vars / rust-embed / Dockerfiles (§5.6, §10.2, §10.3)

Split into three commits; each covers a distinct concern so a bisect lands on the right file.

**§5.6 dedup + doc reorganization** — commit `d3f5689` (`chore(m5): dedup hoisted tooling + umbrella doc reorganization`):
- Delete the M2-hoisted duplicates: `wacp/.cargo/config.toml`, `wacp-console/rust-toolchain.toml`, `wacp/.gitignore`, `wacp-console/.gitignore`, `wacp-console/.claude/CLAUDE.md`.
- `.gitignore`: add `.claude/` so per-session context directories stay local.
- Rename `wacp/SEED-CONTEXT.md` → `wacp/SEED.md` and `wacp-console/SEED_CONTEXT.md` → `wacp-console/SEED.md`; standardize on `SEED.md` at umbrella root.
- Relocate `wacp-console/impl/wiring-strategy.md` → `impl/wiring-strategy.md` (cross-cutting per D7); merge-plan stays in `wacp-console/impl/` since it's under the `wcon-*` namespace.
- Relocate `merge-execution-log.md` from umbrella root into `impl/`.

**§10.2 env-var clap overrides + §10.3 rust-embed** — commit `9703138` (`feat(m5): env-var flag overrides + rust-embed frontend integration`):
- clap `env = "..."` fallbacks on five Serve flags so runtime addresses + console listen address can be overridden in docker-compose: `--listen` / `WACP_CONSOLE_LISTEN`, `--agent-address` / `WACP_RUNTIME_AGENT_ADDRESS`, `--highway-address` / `WACP_RUNTIME_HIGHWAY_ADDRESS`, `--coordinator-address` / `WACP_RUNTIME_COORDINATOR_ADDRESS`, `--rest-address` / `WACP_RUNTIME_REST_ADDRESS`. Precedence: CLI flag > env var > default.
- New `wacp-console/crates/console/src/assets.rs` — `FrontendAssets` embedded from `$CARGO_MANIFEST_DIR/../../frontend/dist/` via `rust-embed`. Wired into the Axum router in `console-api` as a `.fallback_service`. `--frontend-path` dev-mode fallback (previously declared but discarded at `main.rs:84`) now properly threads through.

**§5.6 Dockerfile + docker-compose** — commit `47ebf04` (`build(m5): wacp-console Dockerfile + umbrella docker-compose`):
- `wacp-console/Dockerfile` — three-stage OCI build with umbrella-root context: (1) `node:22-alpine` pnpm install + build → `frontend/dist/`; (2) `rust:1.85-bookworm` `cargo build --release -p wacp-console` (rust-embed picks up `dist/`); (3) `debian:bookworm-slim` stripped binary + non-root `console` user + volume at `/var/lib/wacp-console` for sqlite.
- `wacp/Dockerfile` — rewrite for umbrella-root context (pre-merge it copied `Cargo.toml crates/ proto/` from its own subtree; those manifests no longer exist there post-M2). Now copies umbrella `Cargo.toml Cargo.lock rust-toolchain.toml .cargo/ wacp/ wacp-console/` and builds `-p wacp-runtime`.
- `docker-compose.yml` at umbrella root — one service per binary. Compose DNS: the `wacp-runtime` hostname resolves to the runtime container; console reaches it on the four runtime ports via the env-vars added above.
- `.dockerignore` at umbrella root — excludes `target/`, `node_modules/`, `.wacp-data/`, etc.

### M6 — CI rewrite (§5.7, §10.4, §10.6)

Commit `49c4f98` (`ci(m6): rewrite workflows for umbrella monorepo`).

Deletions (GitHub Actions only reads `.github/workflows/` at repo root; subdir copies would be dead weight):
- `wacp/.github/workflows/ci.yml`, `wacp/.github/workflows/release.yml`
- `wacp-console/.github/workflows/ci.yml`

New umbrella workflows at `.github/workflows/`:
- `ci-wacp.yml` — paths filter: `wacp/**`, `Cargo.toml`, `Cargo.lock`, self. Job bodies ported verbatim from `wacp/.github/workflows/ci.yml` with mechanical `wacp/` path-prefix updates; `rust` job rewritten to a positive `-p` list over the 17 wacp crates plus `wacp-integration-tests` so new crates break CI loudly.
- `ci-console.yml` — paths filter: `wacp-console/**`, `Cargo.toml`, `Cargo.lock`, self. Post-M4 the `console-runtime` build no longer needs a cross-repo checkout (was ~100 lines of pre-M4 scaffolding — gone).
- `ci-lint.yml` — workspace-wide `cargo fmt --check` + `cargo clippy --workspace -- -D warnings` on every push (not per-subtree).
- `release-console.yml` — new, fires on `wacp-console-v*` tags.
- `release-runtime.yml` — adapted from `wacp/.github/workflows/release.yml`; `-test`-tag fast-path per §10.4 (runtime: docker job only, skipping native matrix) so scratch-tag validation doesn't spend 20 min rebuilding for N targets.

### M7 — Validate (§5.8, §6)

Three commits:
- `c1bb3f6` (`style(m7): apply cargo fmt --all across unified workspace`) — landed all `rustfmt.toml`-driven reformats. 48 files touched, all concentrated in `wacp-console/crates/` (console subtree's pre-merge formatting drifted from wacp's stricter style).
- `76237a9` (`build(m7): split highway-ui production build from test typecheck`) — see §4.5.

§6 checklist state at the time of this log:

| § Item | Result |
|--------|--------|
| `cargo fmt --check` | ✓ zero diffs |
| `cargo clippy --workspace -- -D warnings` | ✓ exit 0 |
| `cargo build --workspace` | ✓ 1m15s |
| `cargo test --workspace` | ✓ 1442 passed, 0 failed |
| `cargo tree -d` (tonic / prost / tokio / serde single version) | ✓ |
| `console` no rusqlite / `wacp-runtime` no sqlx | ✓ |
| `cargo run -p wacp-runtime -- serve --config wacp/dev/runtime.yaml` | ✓ 7 verticals, REST on `[::1]:9093` |
| `curl http://[::1]:9093/v1/verticals` | ✓ HTTP 200, 2397 bytes fixture |
| `cargo run -p console -- serve` | ✓ taxonomy index roles=37 tools=68 verticals=7; rust-embed'd frontend serves |
| `cd wacp/highway-ui && pnpm install && pnpm build` | ✓ (typecheck pre-existing red, see §4.5) |
| `cd wacp-console/frontend && pnpm install && pnpm lint && pnpm typecheck && pnpm build` | ✓ |
| wacp `openapi.yaml` round-trip | ⚠ pre-existing drift, see §4.5 |
| console `openapi.yaml` round-trip | ✓ regenerates to no diff |
| sqlx offline query verification | N/A — console-db uses function-form `sqlx::query_as`, not `query!` macros |
| Docker / OCI image builds + `docker compose up` | ⧗ deferred, see §4.6 |
| UI login / profile / session flow | ⧗ deferred, see §4.6 |

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

### 4.4 M4 workspace-deps path semantics (plan §5.5 correction)

Plan §5.5 reads: "Flip every `path = "..."` to `{ workspace = true }` at the crate level; **remove the explicit paths** from `[workspace.dependencies]` entries on `wacp-taxonomy`/`wacp-types`/`wacp-proto`."

Cargo does not accept bare `{ version = "0.1.0" }` for unpublished crates — it tries to resolve against crates.io and fails. Paths must remain at the workspace level; the behavior §5.5 is actually reaching for is already achieved because consumers go through `{ workspace = true }` and therefore don't hardcode paths. Plan wording should read "remove `path = "..."` at consumer sites; paths remain canonical in `[workspace.dependencies]`."

### 4.5 Pre-existing drift surfaced by M7 validation (not merge-introduced)

Two items showed red under §6 but originate in the source wacp repo, not in the merge. Both are tracked for W-phase follow-up.

- **`wacp/highway-ui` `pnpm typecheck`** — source repo typecheck reported 198 errors pre-merge (vitest globals invisible to tsc + `noUncheckedIndexedAccess` strictness tripping test-code indexing). Commit `76237a9` mitigates by:
  1. Adding `"types": ["vitest/globals", "@testing-library/jest-dom"]` to `tsconfig.json` — collapses the 180+ "Cannot find name 'describe'/'expect'/'vi'" errors.
  2. Introducing `tsconfig.build.json` that extends `tsconfig.json` and excludes `*.test.*` — `pnpm build` now runs through production code only, green.
  3. `pnpm build` passes; `pnpm typecheck` remains red on 14 real pre-existing test-code strictness issues (mostly `Object is possibly 'undefined'` on `.mock.calls[0][0]` indexing + one type-mismatch in `ConnectionBanner.test.tsx`). These are genuine latent issues in the source wacp test suite, not merge regressions. Queued as a wacp W-phase task.

- **`wacp/openapi.yaml`** drift — `cargo run -p wacp-transport --bin gen_openapi` emits a 17-line diff vs the checked-in yaml (the `/healthz` response gains `content`/`schema` fields and a `503` branch). The committed yaml in the source wacp repo is byte-identical to the drift source, so this predates the merge. Left unregenerated at M7 to keep M7 purely a merge milestone; a W-phase regen + commit on the wacp side will close this.

### 4.6 §6 items deferred until out-of-band environments available

- **Docker / OCI** — `docker` is not installed in this WSL2 distro (Docker Desktop WSL integration not enabled). All Dockerfile + docker-compose edits went through static review at M5; actual `docker build` + `docker compose up` + image-size assertion (< 150 MB distroless console) need to run on a host with the docker daemon available.
- **UI login / profile / session flow** — requires a browser session against `http://[::1]:8080/`. The infrastructure path is verified: `wacp-console serve` reports `taxonomy index ready`, `serving embedded frontend`, `/api/health` returns `{"checks":{"database":"ok","runtime_agent":"ok","runtime_coordinator":"ok","runtime_highway":"ok","runtime_rest":"ok"},"status":"healthy"}`, and `GET /` serves the SPA shell HTML. The functional click-path is the last uncovered §6 bullet and will be exercised at the first W1 browser smoke.

## 5. Next Step — Tag & Push

With the checklist green (modulo §4.5 / §4.6), §5.8 M7 finalization runs:

```
git tag monorepo-v0
git remote add origin git@github.com:Madahub-dev/wacp-platform.git
git push -u origin main
git push origin monorepo-v0
git checkout -b dev && git push -u origin dev
```

The `git tag monorepo-v0` is local-only and reversible; it happens inside this execution. The remote push, branch-protection setup, and source-repo archival all happen outside this log because they touch shared infrastructure — human operator runs them once the local tag is in place. Source `wacp/` and `wacp-console/` GitHub repos are archived (read-only) only after at least one W-phase lands in the monorepo; the `pre-monorepo-*` tags on the source repos remain as the 30-day rollback anchor per §7.

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
