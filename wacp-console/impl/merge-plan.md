---
id: wcon-merge-plan
type: impl
status: final
created: 2026-04-14T00:00:00
revised: 2026-04-15T00:00:00
authors: [AAkil98, Claude Opus 4.6]
tags: [monorepo, merge, wacp, workspace, migration, distribution, adr-009]
depends_on: [wcon-wiring-strategy, wcon-architecture]
---

# Monorepo Merge Plan — `wacp/` ⊕ `wacp-console/` → `wacp-platform/`

## Table of Contents

- 1. Scope & Inventory
- 2. Decisions
- 3. Target Layout
- 4. Execution Order
- 5. Step-by-Step Procedure
- 6. Validation Checklist
- 7. Rollback
- 8. Risk Map
- 9. Post-Merge Follow-Ups
- 10. Pre-M0 Open Items

---

## 1. Scope & Inventory

Merge two local Rust workspaces — `wacp/` (runtime, 16 crates) and `wacp-console/` (workbench, 6 crates + SPA) — into a new umbrella repo `wacp-platform/` at `github.com/Madahub-dev/wacp-platform`. The sibling repo `../wacp-protocol/` (protocol specs) is **out of scope** — it stays independent by deliberate design; cross-references resolve via spec IDs (path-independent).

**Estimated effort:** 1–2 working days. The "~4 hours mechanical" framing in `impl/archive/wiring-strategy.md` under-scoped git history, `[workspace.dependencies]` union, proto codegen extraction, and CI rewrite.

### 1.1 Source Inventories

**`wacp/`:** 16 Rust crates under `crates/`, plus `tests/`, `proto/` (5 `.proto` files), `highway-ui/` (Vite + Connect-Web, standalone pnpm), `packages/` (pnpm workspace: `@wacp/cli`, `@wacp/local` with `file:` deps into `ecosystem/*`), `ecosystem/` (7 vertical manifests), `sdk-python/`, `deploy/`, `impl/`, `dev/`, `.github/workflows/{ci,release}.yml`, `Dockerfile`, `openapi.yaml`, `IMPLEMENTATION.md`, `SEED-CONTEXT.md`, `LAYER-MAPPING.md`, `AUDIT-2026-04-12.md`, `README.md`, `LICENSE`, `NOTICE`. Currently on branch `dev`, one untracked `.cargo/`.

**`wacp-console/`:** 6 `console-*` crates, `migrations/`, `frontend/` (Vite + React 19 + OpenAPI codegen, standalone pnpm), `specs/` (12 `wcon-*` design specs), `impl/` (phase-1..6 evals + `wiring-strategy.md` + this file), `.github/workflows/ci.yml`, `.cargo/`, `.claude/`, `rust-toolchain.toml`, `openapi.yaml`, `SEED_CONTEXT.md`, `IMPLEMENTATION.md`. Currently on branch `dev`, `M Cargo.lock`.

### 1.2 Collision Map

| Path | Resolution |
|------|------------|
| `Cargo.toml` (root) | Merge into one unified workspace (§5.3) |
| `Cargo.lock` | Regenerate from merged workspace at M2 |
| `.cargo/`, `.claude/`, `rust-toolchain.toml`, `.gitignore` | Hoist to umbrella root; union entries |
| `.github/workflows/` | Rewrite as per-project + shared lint + release (§5.7) |
| `README.md` | New umbrella README at root; keep subproject READMEs |
| `LICENSE`, `NOTICE` | Hoist to root (verify Apache-2.0 identical) |
| `IMPLEMENTATION.md` | Keep both in their subdirs |
| `SEED-CONTEXT.md` / `SEED_CONTEXT.md` | Rename both to `<project>/SEED.md` to avoid muscle-memory confusion |
| `openapi.yaml` | One per subproject; stays in subdir |
| `impl/` | Namespace per subdir; `wiring-strategy.md` promotes to umbrella root |

### 1.3 Cross-Repo Coupling to Resolve

- Console's `wacp-taxonomy = { path = "../wacp/crates/wacp-taxonomy" }` → becomes workspace member inheritance.
- Console's `wacp-types = { path = "../wacp/crates/wacp-types" }` → same.
- `console-runtime/build.rs` reads `../../../wacp/proto/*.proto` → replaced by new `wacp-proto` crate (§5.4).

---

## 2. Decisions

| # | Area | Value |
|---|------|-------|
| D1 | Merge direction | New umbrella repo `wacp-platform/`; neither history demoted |
| D2 | Git history | `git subtree add` from both — SHAs still resolve, `git log --follow` works |
| D3 | Frontends | Independent at M0–M7 (`highway-ui` + `console/frontend` untouched); revisit B/C post-M7 |
| D4 | Cargo workspace | Single unified workspace, all 22 crates as members |
| D5 | Proto codegen | New `wacp-proto` crate owns `tonic_build`; both consumers depend on it |
| D6 | CI | Per-project workflows with `paths:` filters + shared `ci-lint.yml` |
| D7 | Specs/docs | Per-subdir trees preserved; spec IDs path-independent; `wiring-strategy.md` promotes to umbrella root at M5 |
| D8 | Branch naming | `main` (protected) + `dev` (integration); initial subtree import lands umbrella `main` from both sides' `dev` |
| D9 | Remote | `github.com/Madahub-dev/wacp-platform`, owned by `Madahub-dev` org |
| D10 | Release tagging | Per-subproject tags: `wacp-runtime-v*` + `wacp-console-v*`, independent cadence |
| D11 | Console distribution | OCI image only (ADR-009 supersedes ADR-004); cargo-dist deferred |
| D12 | Docker | Per-subdir Dockerfiles (runtime exists; console written at M5); root `docker-compose.yml` for dev |

---

## 3. Target Layout

```
wacp-platform/                    (github.com/Madahub-dev/wacp-platform)
├── Cargo.toml                    # single workspace, members = ["wacp/crates/*", "wacp/tests", "wacp-console/crates/*"]
├── Cargo.lock
├── rust-toolchain.toml
├── .cargo/
├── .claude/                      # umbrella CLAUDE.md + subdir CLAUDE.md preserved
├── .github/workflows/
│   ├── ci-wacp.yml               # paths: wacp/**
│   ├── ci-console.yml            # paths: wacp-console/**
│   ├── ci-lint.yml               # workspace-wide fmt/clippy
│   ├── release-runtime.yml       # on: push: tags: ['wacp-runtime-v*']
│   └── release-console.yml       # on: push: tags: ['wacp-console-v*']
├── .gitignore
├── README.md                     # umbrella + link to wacp-protocol sibling
├── LICENSE
├── NOTICE
├── docker-compose.yml            # dev: runtime + console + sqlite volume
├── impl/
│   ├── wiring-strategy.md        # relocated from wacp-console/impl/ at M5
│   └── adr-009-oci-only-console.md  # supersedes ADR-004; written at M5
├── wacp/
│   ├── crates/
│   │   ├── wacp-proto/           # NEW at M3 — shared tonic_build
│   │   └── …16 existing
│   ├── tests/
│   ├── proto/
│   ├── highway-ui/
│   ├── packages/                 # pnpm workspace (file: deps to ecosystem/* stay intra-subtree)
│   ├── ecosystem/
│   ├── sdk-python/
│   ├── deploy/
│   ├── impl/
│   ├── dev/
│   ├── Dockerfile
│   ├── openapi.yaml
│   ├── IMPLEMENTATION.md
│   ├── SEED.md                   # renamed from SEED-CONTEXT.md
│   ├── LAYER-MAPPING.md
│   └── AUDIT-2026-04-12.md
└── wacp-console/
    ├── crates/                   # console, console-api, console-core, console-db, console-runtime, console-test-support
    ├── migrations/
    ├── frontend/                 # `gen:api` → ../openapi.yaml still resolves (relative to frontend/)
    ├── specs/                    # 12 wcon-* specs
    ├── impl/                     # phase-1..6 evals + this merge plan
    ├── openapi.yaml
    ├── Dockerfile                # NEW at M5 — multi-stage: pnpm → cargo → distroless
    ├── IMPLEMENTATION.md
    └── SEED.md                   # renamed from SEED_CONTEXT.md
```

---

## 4. Execution Order

| Milestone | What | Proc |
|-----------|------|------|
| **M0** | Pre-flight (clean working trees, local tags) | §5.1 |
| **M1** | Create umbrella + subtree import both sides' `dev` | §5.2 |
| **M2** | Merge Cargo workspace (single root, union deps) | §5.3 |
| **M3** | Extract `wacp-proto` crate | §5.4 |
| **M4** | Flip path deps → workspace members | §5.5 |
| **M5** | Merge tooling + Dockerfiles + docs relocation + ADR-009 | §5.6 |
| **M6** | Rewrite CI (per-project + shared lint + release workflows) | §5.7 |
| **M7** | Validate (§6), tag `monorepo-v0`, push, archive source repos | §5.8 |

Each milestone is a checkpoint; failure rolls back to the pre-milestone tag.

---

## 5. Step-by-Step Procedure

### 5.1 M0 — Pre-flight

```
# Working-tree hygiene
cd wacp          && git status            # expect: ?? .cargo/ — commit (useful config) or discard
cd wacp-console  && git status            # expect: M Cargo.lock — commit

# Local rollback anchors
cd wacp          && git tag pre-monorepo-wacp
cd wacp-console  && git tag pre-monorepo-console
```

Local-only. No push. Source repos stay untouched until M7.

### 5.2 M1 — Create umbrella + subtree import

```
mkdir wacp-platform && cd wacp-platform
git init
git commit --allow-empty -m "chore: initialize wacp-platform monorepo"

# Local-path remotes — source repos are private/local, no GitHub remote yet
git remote add wacp-origin     ../wacp
git remote add console-origin  ../wacp-console
git fetch wacp-origin
git fetch console-origin

# Import both from dev — latest work lives there (wacp phase-5 audit closure,
# console phases 1-6). Lands on umbrella main as the "post-merge stable" state.
git subtree add --prefix=wacp          wacp-origin     dev
git subtree add --prefix=wacp-console  console-origin  dev
```

Sanity check at end of M1: `cd wacp && cargo build --workspace` still works; same for `wacp-console` (their `../wacp/...` path deps resolve from the subdir). Umbrella root has no `Cargo.toml` yet.

### 5.3 M2 — Merge into single workspace

Create root `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "wacp/crates/*",
    "wacp/tests",
    "wacp-console/crates/*",
]

[workspace.package]
edition = "2024"
version = "0.1.0"
license = "Apache-2.0"
repository = "https://github.com/Madahub-dev/wacp-platform"

[workspace.dependencies]
# Union of both source tables. Known reconciliation:
#   tonic 0.13 (both ✓)              prost 0.13 (both ✓)
#   tokio "1"                         → union features from both
#   reqwest 0.12                      → keep console's + wacp's "stream" feature
#   serde / serde_json / serde_yaml_ng → identical ✓
#   axum 0.8 ws (both ✓)
#   sqlx                              → console-only, keep
#   rusqlite (bundled)                → wacp-only, keep
#   arc-swap, utoipa, rust-embed, argon2 → console-only
#   jsonschema, prometheus, ring, zstd  → wacp-only

[profile.dev]
codegen-units = 512
debug = "line-tables-only"

[profile.test]
codegen-units = 512
debug = "line-tables-only"
```

Delete `wacp/Cargo.toml` and `wacp-console/Cargo.toml`. Each inner-crate `Cargo.toml` keeps its `workspace.package` / `workspace.dependencies` inheritance markers — they now inherit from root.

Regenerate lockfile: `rm Cargo.lock && cargo build --workspace`. Resolve fallout (missing deps, feature mismatches).

### 5.4 M3 — Extract `wacp-proto`

```
mkdir -p wacp/crates/wacp-proto/src
# wacp-proto/Cargo.toml: tonic, prost workspace deps; tonic-build as build-dep
# wacp-proto/build.rs: compile_protos(&["../../proto/*.proto"], &["../../proto"])
# wacp-proto/src/lib.rs: pub mod agent, coordinator, highway, primitives, taxonomy
```

Update `wacp-runtime` (and any other wacp crate doing its own `tonic_build`) → depend on `wacp-proto` via workspace. Update `console-runtime/Cargo.toml` likewise; **delete `console-runtime/build.rs`**.

Single atomic commit. If partial, CI breaks on both sides simultaneously.

### 5.5 M4 — Flip path deps

In root `Cargo.toml` `[workspace.dependencies]`, remove explicit paths on `wacp-taxonomy`/`wacp-types`/`wacp-proto` — they're workspace members now. Consumers reference via `wacp-taxonomy = { workspace = true }` and resolve through the members list.

### 5.6 M5 — Merge tooling, Dockerfiles, docs

- Union `.gitignore` (dedupe).
- Move `rust-toolchain.toml` from `wacp-console/` to root.
- Merge `.cargo/config.toml` `[build]` / `[env]` sections (union).
- Merge `.claude/`: keep each subproject's `CLAUDE.md` in its subdir; add umbrella `CLAUDE.md` at root pointing to both.
- Rename `wacp/SEED-CONTEXT.md` → `wacp/SEED.md`; `wacp-console/SEED_CONTEXT.md` → `wacp-console/SEED.md`; grep both subtrees for old names and update references.
- Write root `README.md`: 1-page umbrella, links to both subprojects and to the sibling `wacp-protocol/` repo.
- Write root `docker-compose.yml`: services for `wacp-runtime` (exposes 9090–9093) and `wacp-console` (exposes HTTP port), sqlite volume for `console.db`, env overrides so console's `runtime.*_address` points at the compose-internal `wacp-runtime` hostname instead of `[::1]`.
- Write `wacp-console/Dockerfile` (per D11): multi-stage — Stage 1 `node:22-alpine` runs `pnpm install && pnpm build` in `frontend/`. Stage 2 `rust:1-slim` runs `cargo build --release -p console` (rust-embed picks up `frontend/dist/`). Stage 3 distroless or `debian:stable-slim` with binary + non-root user. CMD `["console", "serve"]`.
- Write `adr/adr-009-oci-only-console.md` at umbrella root: supersedes ADR-004; defers cargo-dist; ships console as OCI image.
- Move `wacp-console/impl/archive/wiring-strategy.md` → umbrella `impl/archive/wiring-strategy.md` (cross-cutting per D7).

### 5.7 M6 — Rewrite CI

Delete `wacp/.github/workflows/*` and `wacp-console/.github/workflows/*` (they moved up with subtree — remove from subdirs). Create `.github/workflows/` at umbrella root:

- `ci-wacp.yml` — `paths: ['wacp/**', 'Cargo.toml', 'Cargo.lock']`. Jobs: rust build/clippy/test (scoped `-p wacp-*`), highway-ui typecheck/build, `packages/*` typecheck/build, wacp OpenAPI drift.
- `ci-console.yml` — `paths: ['wacp-console/**', 'Cargo.toml', 'Cargo.lock']`. Jobs: rust build/clippy/test (scoped `-p console*`), frontend lint/typecheck/test/build, console OpenAPI drift.
- `ci-lint.yml` — workspace-wide `cargo fmt --check`.
- `release-runtime.yml` — port from `wacp/.github/workflows/release.yml`. Trigger: tags `wacp-runtime-v*`. Builds runtime Docker image + GitHub release with binary artifact.
- `release-console.yml` — NEW. Trigger: tags `wacp-console-v*`. Builds OCI image from `wacp-console/Dockerfile` (multi-platform: `linux/amd64`, `linux/arm64`), pushes to `ghcr.io/madahub-dev/wacp-console:<tag>` and `:latest`. Gated by `cargo check --workspace`.

Validate release workflows at M7 by pushing scratch tags (`wacp-console-v0.0.0-test`, `wacp-runtime-v0.0.0-test`) to observe workflow firing, then delete the tags.

### 5.8 M7 — Validate & tag

Run §6 checklist. When green:

```
git tag monorepo-v0
git remote add origin git@github.com:Madahub-dev/wacp-platform.git
git push -u origin main
git push origin monorepo-v0
git checkout -b dev && git push -u origin dev
```

Protect `main`; set `dev` as default PR target for W1–W6 work. Archive source `wacp/` and `wacp-console/` GitHub repos as read-only (keep for ~30 days as rollback fallback — do not delete until at least one W-phase has landed in the monorepo).

---

## 6. Validation Checklist

**Rust:**
- [ ] `cargo fmt --check` — zero diffs
- [ ] `cargo clippy --workspace -- -D warnings` — zero warnings
- [ ] `cargo build --workspace` — succeeds
- [ ] `cargo test --workspace` — all wacp + all 99 console tests pass
- [ ] sqlx compile-time query verification works for `console-db`
- [ ] `cargo tree -d` — no duplicate versions of tonic, prost, tokio, serde
- [ ] `cargo tree -p console | grep -i sqlite` — only `sqlx`, no `rusqlite`
- [ ] `cargo tree -p wacp-runtime | grep -i sqlite` — only `rusqlite`, no `sqlx`

**Proto:**
- [ ] `cargo run -p wacp-runtime -- --help` runs
- [ ] `console-runtime` gRPC clients compile against `wacp-proto` re-exports

**Frontends:**
- [ ] `cd wacp/highway-ui && pnpm install && pnpm typecheck && pnpm build` succeeds
- [ ] `cd wacp-console/frontend && pnpm install && pnpm lint && pnpm typecheck && pnpm build` succeeds
- [ ] `cd wacp-console/frontend && pnpm gen:api` — OpenAPI-derived types resolve (confirms `../openapi.yaml` path still works)

**OpenAPI drift:**
- [ ] `cargo run -p wacp-transport --bin gen_openapi > wacp/openapi.yaml` — no diff
- [ ] Console `openapi.yaml` regenerates to no diff

**Git:**
- [ ] `git log --follow wacp/crates/wacp-runtime/src/lib.rs` shows pre-merge history
- [ ] `git log --follow wacp-console/crates/console-api/src/lib.rs` shows pre-merge history
- [ ] `git blame` resolves correctly in both subtrees

**End-to-end smoke:**
- [ ] `cargo run -p wacp-runtime -- serve --config wacp/dev/runtime.yaml` starts, loads verticals, serves REST on `[::1]:9093`
- [ ] `curl http://[::1]:9093/v1/verticals` returns fixture data
- [ ] `cargo run -p console -- serve` from root starts; loads taxonomy via REST
- [ ] Login, create profile, launch session — UI flow works (sessions still hollow pre-W2)

**Docker / OCI:**
- [ ] `docker build -f wacp/Dockerfile -t wacp-runtime .` — container serves gRPC/REST
- [ ] `docker build -f wacp-console/Dockerfile -t wacp-console .` — container serves SPA + API
- [ ] `docker compose up` brings up both; console reaches runtime via compose DNS
- [ ] Console image < 150 MB with distroless base
- [ ] Frontend embedded in binary (no separate `dist/` in final image)

**CI:**
- [ ] `ci-wacp.yml` green on a wacp-only change; does not trigger on a console-only change
- [ ] `ci-console.yml` green on a console-only change; does not trigger on a wacp-only change
- [ ] Both trigger and are green on a `Cargo.toml` change
- [ ] `release-runtime.yml` and `release-console.yml` fire correctly on scratch tag push

---

## 7. Rollback

At any checkpoint:

```
git reset --hard <pre-milestone-tag>
```

If already pushed to the new remote:

```
git revert <merge-commit>
git push
```

Source repos untouched until M7. `pre-monorepo-wacp` and `pre-monorepo-console` tags on the source repos remain as rollback anchors. Do not delete source repos until at least one W-phase has landed in the monorepo (~30-day fallback window).

---

## 8. Risk Map

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `cargo build --workspace` fails with dep conflict at M2 | High | Medium | Pre-diff `[workspace.dependencies]` tables; union features explicitly |
| Subtree import inflates repo size | Medium | Low | Acceptable — history is worth it |
| Proto path breakage at M3 | High | Medium | Introduce `wacp-proto` in one atomic commit, not piecemeal |
| CI `paths:` filters misconfigured | Medium | Low | Test with scratch branch before merging to `main` |
| sqlx macros break on layout change | Low | Medium | `DATABASE_URL` still points at `wacp-console/migrations/`; verify at M2 |
| Double SQLite linkage | Low | Low | `cargo tree -p <bin> \| grep -i sqlite` per binary (validation §6) |
| `wacp-runtime --config dev/runtime.yaml` breaks if CWD changes | Low | Medium | Post-merge invocation: `--config wacp/dev/runtime.yaml` from workspace root |
| docker-compose service discovery (console → runtime at hostname, not `[::1]`) | Medium | Medium | compose file sets `runtime.*_address` env vars on the console service |
| `rust-embed` frontend embed path changes in Dockerfile | Medium | Medium | Stage 1 emits `frontend/dist/` in the same relative path stage 2's cargo build expects |
| Release workflows untested at M7 | Medium | Low | Push scratch tags (`*-v0.0.0-test`) to validate, then delete |

---

## 9. Post-Merge Follow-Ups

- [ ] Cut fresh `dev` off umbrella `main`; protect `main`; set `dev` as default PR target for W1–W6 work.
- [ ] Update `wacp-console/IMPLEMENTATION.md` for new layout.
- [ ] Update `wacp-console/SEED.md` — W0 ✅.
- [ ] Update umbrella `impl/archive/wiring-strategy.md` — mark W0 complete; strike the §3 monorepo section.
- [ ] Archive source `wacp/` and `wacp-console/` GitHub repos as read-only (after ~30-day fallback window).
- [ ] Begin W1 (gRPC pool → AppState) in the monorepo.
- [ ] Write `scripts/release.sh <runtime|console|both> <semver>` — coordinated proto-bump releases.
- [ ] Clarify `wacp/highway-ui/` status → pick frontend D3 follow-up: Option B (unify pnpm workspace) if canonical, Option C (port + delete) if legacy.
- [ ] Revive cargo-dist if native-binary distribution is demanded externally (writes new ADR superseding ADR-009).

---

## 10. Pre-M0 Open Items

Surfaced during the collapse-to-essentials review. None block the plan; each benefits from an explicit answer before M0 begins so the procedure runs deterministically.

### 10.1 The untracked `?? .cargo/` in `wacp/`

`git status` on `wacp/` shows one untracked entry: `.cargo/`. M0 currently says "commit (useful config) or discard." Needs a call.

**Question:** Inspect its contents — is it legitimate project config that should be committed to the merged repo, or stray local artifact to discard?

**Decision:** **Commit and hoist to umbrella root at M5.** Single file (`wacp/.cargo/config.toml`, 26 lines) — load-bearing WSL2 OOM mitigation: `[build] jobs = 1` plus `mold` linker via clang. Header comment dates the constraint (2026-04-13 → -04-14) and explicitly invites contributors with more RAM to raise `jobs`. CI runners and bigger machines override locally via `CARGO_BUILD_JOBS` env. The `mold` linker config benefits all linux-gnu builds, not just constrained ones.

**Action at M0:** `cd wacp && git add .cargo/config.toml && git commit -m "chore: commit WSL2 build config (jobs=1, mold linker)"` so the subtree import at M1 carries it. At M5 the file moves from `wacp/.cargo/config.toml` → umbrella root `.cargo/config.toml` (no merge needed — `wacp-console/` has no `.cargo/`; the §1.1 inventory entry overstates this).

**Plan correction:** §1.1 lists `.cargo/` under `wacp-console/`; it doesn't exist there. §1.2 collision row should read "*hoist `wacp/.cargo/`*" not "union entries."

### 10.2 docker-compose service discovery — env-var overrides for console

Console's `runtime.agent_address`, `runtime.highway_address`, `runtime.coordinator_address`, `runtime.rest_address` default to `[::1]:909{0,1,2,3}`. Inside Docker Compose, the runtime container is at hostname `wacp-runtime` (or whatever the service name is), not loopback. The compose file needs env-var overrides on the console service, and the console must honor env-var overrides for those config keys.

**Question:** Does `console` already read runtime addresses from env vars, or does it only read from its config file? If config-file-only, a small code change is needed to support env-var override (e.g., `WACP_RUNTIME_AGENT_ADDRESS`) before the compose scenario works.

**Finding:** Neither. Console reads runtime addresses from **CLI flags only** (`crates/console/src/main.rs:40-54`) — no config file, no env vars. `RuntimeConfig` is constructed directly from parsed clap args (`main.rs:92-102`). There is no `--config <file>` flag at all (the `SEED_CONTEXT.md` "Before writing any wiring code" line that says `cargo run -p console -- serve --config …` is a doc bug; log a SEED.md correction in §9).

Additional gap surfaced: `--listen` defaults to `[::1]:8080`. Inside a container, loopback is unreachable from the host port-mapping — the listen address also needs env-var override (`WACP_CONSOLE_LISTEN`, typically `[::]:8080` at container runtime).

**Decision:** **Add env-var fallback via clap's built-in `env = "..."` attribute at M5.** Bundles with `docker-compose.yml` + `wacp-console/Dockerfile` since all three logically co-ship; M7 validation checklist (§6) tests `docker compose up` end-to-end and requires this.

**Scope (exactly one file — `crates/console/src/main.rs`):**
```rust
#[arg(long, env = "WACP_RUNTIME_AGENT_ADDRESS",       default_value = "[::1]:9090")]       agent_address: String,
#[arg(long, env = "WACP_RUNTIME_HIGHWAY_ADDRESS",     default_value = "[::1]:9091")]       highway_address: String,
#[arg(long, env = "WACP_RUNTIME_COORDINATOR_ADDRESS", default_value = "[::1]:9092")]       coordinator_address: String,
#[arg(long, env = "WACP_RUNTIME_REST_ADDRESS",        default_value = "http://[::1]:9093")] rest_address: String,
#[arg(long, env = "WACP_CONSOLE_LISTEN",              default_value = "[::1]:8080")]       listen: SocketAddr,
```

Precedence becomes: CLI flag → env var → default. Zero new parsing code. No tests needed (clap covers the env path). ~10 minutes implementation + 5 minutes verification (`WACP_RUNTIME_REST_ADDRESS=http://example:9093 cargo run -p console -- serve` and confirm it's honored).

**Compose file then sets on the console service:**
```yaml
environment:
  WACP_CONSOLE_LISTEN: "[::]:8080"
  WACP_RUNTIME_AGENT_ADDRESS:       "wacp-runtime:9090"
  WACP_RUNTIME_HIGHWAY_ADDRESS:     "wacp-runtime:9091"
  WACP_RUNTIME_COORDINATOR_ADDRESS: "wacp-runtime:9092"
  WACP_RUNTIME_REST_ADDRESS:        "http://wacp-runtime:9093"
```

**Plan correction:** §10.2 asked about "config file" fallback; there is none. The change is clap-only. §6 validation row "Console reaches runtime via compose DNS" depends on this landing at M5.

### 10.3 `rust-embed` build-stage handoff in `wacp-console/Dockerfile`

Multi-stage build: Stage 1 (node) emits `frontend/dist/`; Stage 2 (rust) compiles `console` with `rust-embed` which expects the built assets at a specific path. Misconfigured, you silently ship a binary with no embedded frontend (blank SPA, 404s on asset paths).

**Question:** Where does `rust-embed` expect the `dist/` to live at cargo-build time? Path in `#[folder = "..."]` attribute — needs to be verified and the Dockerfile `COPY --from=frontend-builder` destination lined up with it.

**Finding — the integration itself doesn't exist yet.** `rust-embed` is declared as a dep (`Cargo.toml:78`, `crates/console/Cargo.toml:21`) but never imported or used. Grep returns zero matches for `RustEmbed`, `#[folder`, `use rust_embed`, or any Axum static-asset serving (`ServeDir`, `fallback_service`). `main.rs:38` declares `--frontend-path` but `main.rs:84` discards the value (`frontend_path: _`). `IMPLEMENTATION.md:454` lists this as task 7.1, and Phase 7 (distribution) is postponed per `SEED_CONTEXT.md`.

The question reframes: it's not "align the Dockerfile to existing `#[folder]`." It's "implement Phase-7 slice 7.1 at M5 alongside the Dockerfile," because §5.6 and §8 both implicitly assume wiring that isn't there.

**Decision:** **Implement minimal rust-embed integration at M5, before the Dockerfile stage handoff.** ~1–2 hours. Scope:

1. **New file** `crates/console/src/assets.rs` (~30 LOC):
   ```rust
   use rust_embed::RustEmbed;

   #[derive(RustEmbed)]
   #[folder = "$CARGO_MANIFEST_DIR/../../frontend/dist/"]
   pub struct FrontendAssets;
   ```
   `$CARGO_MANIFEST_DIR` is always set by cargo, so the path is cwd-independent. From `wacp-console/crates/console/Cargo.toml` it resolves to `wacp-console/frontend/dist/` — exactly where Vite writes.

2. **Axum router wiring** in `console-api`: `.fallback_service(...)` that serves `FrontendAssets::get(path)`, falls back to `index.html` for unknown paths (SPA history-mode), sets MIME types from extension, and honors the existing `--frontend-path <dir>` override to read from disk in dev.

3. **Dockerfile path alignment:**
   ```dockerfile
   FROM node:22-alpine AS frontend-builder
   WORKDIR /build/wacp-console/frontend
   COPY wacp-console/frontend/package.json wacp-console/frontend/pnpm-lock.yaml ./
   RUN corepack enable && pnpm install --frozen-lockfile
   COPY wacp-console/frontend/ ./
   RUN pnpm build   # emits /build/wacp-console/frontend/dist/

   FROM rust:1-slim AS cargo-builder
   WORKDIR /build
   COPY . .
   COPY --from=frontend-builder /build/wacp-console/frontend/dist /build/wacp-console/frontend/dist
   RUN cargo build --release -p console
   # $CARGO_MANIFEST_DIR/../../frontend/dist/ → /build/wacp-console/frontend/dist/ ✅
   ```

**Safety net:** `rust-embed` fails-loudly at compile time in release builds if the folder is missing or empty (`debug-embed` feature is off by default). A path misalignment surfaces as a cargo error, not a silent empty-binary ship. §8 risk row impact downgrades from Medium → Low.

**Plan corrections:**
- **§5.6** currently reads "rust-embed picks up `frontend/dist/`" as if wired. Rewrite to: "Write `crates/console/src/assets.rs` with `#[derive(RustEmbed)] #[folder = \"$CARGO_MANIFEST_DIR/../../frontend/dist/\"]`; wire `.fallback_service` into Axum router in `console-api`; implement `--frontend-path` dev-mode fallback (currently declared but discarded at `main.rs:84`)."
- **§8** risk row `rust-embed frontend embed path changes in Dockerfile`: downgrade Impact Medium → Low (compile-time failure is loud, not silent).
- **§6 validation** add: `cargo build --release -p console` succeeds only when `frontend/dist/` is present and populated (implicit test of the #[folder] path).

### 10.4 Release workflow validation strategy

§5.7 says "validate release workflows at M7 by pushing scratch tags (`wacp-console-v0.0.0-test`, `wacp-runtime-v0.0.0-test`) then deleting." This triggers a real workflow run that pushes to ghcr.io — which creates a real (but useless) image tag.

**Question:** Acceptable to push a test image to ghcr.io and leave it (or delete via API)? Alternative: use `act` locally to dry-run the workflow without hitting ghcr.io; slightly less confidence but zero registry side-effects.

**Options considered:**

| Option | Side-effects | Confidence | Cost | Cleanup |
|--------|--------------|-----------|------|---------|
| A. Real scratch tag → cleanup | 1 GHCR version + 1 GH Release per workflow | Full (tests OIDC + `GITHUB_TOKEN` + ghcr auth) | 40–80 min runner (runtime matrix), ~5 min (console) | 4 `gh` CLI commands |
| B. `act` local dry-run | None | Partial — OIDC unsupported, `docker/login-action` can't auth without real creds | WSL2 setup friction; matrix builds may OOM given `jobs=1` constraint | — |
| C. Hybrid (`act -l` + `--dryrun`, then real tag) | Same as A | Full + marginal YAML safety | A's cost + minor | Same as A |

**Decision:** **Option A with a runtime-specific `-test` fast-path.** `act` rejected because (a) WSL2 memory constraint (see `wacp/.cargo/config.toml` — `jobs=1`), (b) `act` fundamentally can't validate the OIDC/ghcr-login step, which is exactly where real workflow failures live. For workflows this small (console is one `docker/build-push-action` job), Option C's marginal benefit over A doesn't justify the setup cost.

**Runtime-workflow optimization (reduces scratch validation from ~40 min → ~5 min):** Port `wacp/.github/workflows/release.yml` into `release-runtime.yml` with `*-test`-tag guards so the native-binary matrix and GH-Release steps are skipped on `-test` tags; only the `docker` job runs. Scratch validation still covers the OCI push path (which is what matters for the console-distribution pivot in D11/ADR-009):

```yaml
jobs:
  build:
    if: "!endsWith(github.ref_name, '-test')"
    # … existing 4-target matrix
  publish:
    if: "!endsWith(github.ref_name, '-test')"
    needs: build
    # … existing GH Release
  docker:
    # no guard — runs on -test tags too
    # … existing docker/build-push-action
```

Console workflow is docker-only already; no guards needed there.

**Cleanup runbook (must be executed after observing green workflow runs at M7):**

```bash
# Delete the GitHub Releases (also deletes the underlying tags):
gh release delete wacp-console-v0.0.0-test --cleanup-tag -y
gh release delete wacp-runtime-v0.0.0-test --cleanup-tag -y

# Delete the ghcr.io image versions tagged with 0.0.0-test:
for PKG in wacp-console wacp-runtime; do
  gh api /orgs/Madahub-dev/packages/container/$PKG/versions \
    --jq '.[] | select(.metadata.container.tags[] | contains("0.0.0-test")) | .id' \
    | xargs -I{} gh api -X DELETE /orgs/Madahub-dev/packages/container/$PKG/versions/{}
done

# Confirm nothing remains:
gh release list | grep -v "v0.0.0-test" || true
gh api /orgs/Madahub-dev/packages/container/wacp-console/versions --jq '.[].metadata.container.tags'
gh api /orgs/Madahub-dev/packages/container/wacp-runtime/versions --jq '.[].metadata.container.tags'
```

**Plan corrections:**
- **§5.7** last paragraph mentions validation in passing. Expand to include (a) the `*-test` `if:` guards in `release-runtime.yml` and (b) a pointer to this §10.4 cleanup runbook.
- **§8** risk row "Release workflows untested at M7": Mitigation rewords from "Push scratch tags (`*-v0.0.0-test`) to validate, then delete" to "Scratch-tag validation with `*-test` fast-path (runtime: docker job only, skipping native matrix); cleanup runbook in §10.4."
- **§9** post-merge follow-ups: add "Execute §10.4 cleanup runbook after M7 validation observes green."

### 10.5 `wacp/packages/` pnpm workspace — confirm intra-subtree resolution

`wacp/packages/wacp-cli/package.json` has `file:` deps into `../../ecosystem/*`. Inside the subtree (which is fully preserved by `git subtree add --prefix=wacp`), these paths resolve correctly because `packages/` and `ecosystem/` both live under `wacp/`.

**Question:** Confirmation only — no action needed if the subtree preserves intra-subtree paths verbatim. Worth a 30-second check at end of M1 to validate by running `cd wacp/packages/wacp-cli && pnpm install`.

**Decision:** **Confirmed.** Live test against the highest-risk package (wacp-cli, 8 `file:` deps reaching one sibling + all 7 ecosystem verticals):

```
$ cd wacp/packages/wacp-cli && pnpm install --frozen-lockfile
Lockfile is up to date, resolution step is skipped
Packages: +8
Done in 825ms
```

Structural reasoning: there is no root `pnpm-workspace.yaml`; 9 independent pnpm projects, each with its own committed `pnpm-lock.yaml`. Lock files record `file:` deps as relative paths (`'@wacp/local@file:../wacp-local'`, `'@wacp/swe@file:../../ecosystem/swe'`). Since `git subtree add --prefix=wacp` transplants the entire tree verbatim, all relative-path topology is invariant, and every `pnpm install --frozen-lockfile` continues to resolve identically post-subtree.

**M1 validation scope (plan was already right):** `wacp-cli` alone suffices — it has the most `file:` deps (8); the other 8 packages each have ≤1, so if `wacp-cli` passes, the simpler ones are guaranteed. No need to loop through all 9 at M1; CI matrix (§5.7 `typescript-packages` port) covers that at M6.

**Two unrelated gitignore anomalies surfaced during inspection — fix at M5:**

1. **`Cargo.lock` disagreement.** `wacp/.gitignore:5` ignores `Cargo.lock`; `wacp-console/` tracks it. Unifying naively at M5 would carry wacp's rule forward, silently ignoring the merged workspace's freshly-regenerated lockfile from §5.3. **Drop the `Cargo.lock` line from the unioned `.gitignore`** — the unified workspace is an application project (two binaries); committing `Cargo.lock` is correct.
2. **`node_modules/` relies on user's global gitignore in both repos.** Neither `.gitignore` has an explicit `node_modules` entry; git only ignores it via the user's `~/.gitignore_global` / `core.excludesFile`. Fragile for future contributors. **Add explicit `**/node_modules/` to the umbrella `.gitignore` at M5.**

**Minor warning to triage at M6:** `pnpm install` emits `Ignored build scripts: esbuild@0.27.7, protobufjs@7.5.4.` (pnpm 9+ default). CI jobs in `ci-wacp.yml` need to decide: run `pnpm approve-builds` pre-install, or accept the warning. Not blocking.

**Plan corrections:**
- **§5.6** `.gitignore` union step: explicitly note "drop the `Cargo.lock` line carried over from `wacp/.gitignore`; add `**/node_modules/`."
- **§10.5** marker: confirmed (no action at M0 beyond the live-test already performed).

### 10.6 `wacp/highway-ui/` CI job in `ci-wacp.yml`

§5.7 specifies `ci-wacp.yml` includes a `highway-ui typecheck/build` job. D3 (frontend coexistence) stays "independent" at M0–M7. But the original `wacp/.github/workflows/ci.yml` already has a `typescript-highway` job — it should be preserved in the port.

**Question:** Port the existing `typescript-highway` job from `wacp/.github/workflows/ci.yml` verbatim (just relocate + rename), or rewrite? Similarly for `packages/*` jobs.

**Decision — the verbatim/rewrite framing is a false binary.** Truthful answer: **port the job bodies, rewrite the wiring.** Hybrid per-job:

| Job | Can copy verbatim | Must change |
|-----|-------------------|-------------|
| `typescript-highway` | All 10 steps | `working-directory: highway-ui` → `wacp/highway-ui`; `cache-dependency-path: highway-ui/…` → `wacp/highway-ui/…` |
| `typescript-packages` (matrix ×9) | All 6 steps | Each matrix entry gets `wacp/` prefix: `packages/wacp-cli` → `wacp/packages/wacp-cli`, etc. |
| `python` | All steps | `working-directory: sdk-python` → `wacp/sdk-python` |
| `proto` | All steps | `protoc --proto_path=proto …` → `--proto_path=wacp/proto` |
| `rust` | ❌ not verbatim | Was `cargo build --workspace`; becomes scoped to wacp crates (see below) |
| Workflow trigger | ❌ not verbatim | Add `paths:` filter |

**Rust job scoping (critical).** Post-merge, `cargo build --workspace` compiles both sides' 22 crates — wasteful and conflates concerns. Scope `ci-wacp.yml`'s `rust` job explicitly. Recommended: positive list over negative exclusion because it breaks loudly when a new wacp crate is added:

```yaml
- name: Build
  run: cargo build -p wacp-runtime -p wacp-transport -p wacp-taxonomy -p wacp-types -p wacp-agent -p wacp-highway -p wacp-coordinator # …enumerate
```

Alternative (less maintainable but short): `cargo build --workspace --exclude console --exclude console-api --exclude console-core --exclude console-db --exclude console-runtime --exclude console-test-support`.

**Workflow trigger.** Plan specifies `paths: ['wacp/**', 'Cargo.toml', 'Cargo.lock']`. Add the workflow file itself so edits are self-testing: `['wacp/**', 'Cargo.toml', 'Cargo.lock', '.github/workflows/ci-wacp.yml']`. Standard practice — without this, a broken workflow edit isn't caught until some wacp-code change coincidentally triggers it.

**`ci-console.yml` becomes much simpler post-M4.** The current `wacp-console/.github/workflows/ci.yml` does two checkouts per job (console + sibling `wacp` repo) because of path deps into `wacp/crates/wacp-taxonomy`. M4 flips those to workspace members → cross-repo checkout dissolves. Concrete simplifications: drop the second `actions/checkout@v4` block, drop the `path: wacp-console`/`path: wacp` dance, keep `working-directory: wacp-console` on the cargo steps, scope via `-p console*`. This is a **net reduction** of ~100 lines vs. the current file.

**Version drift — leave alone for M6.** `wacp` CI uses pnpm 9 + node 22; `wacp-console` CI uses pnpm 10 + node 24. Unifying at M6 risks surfacing unrelated bugs during the merge itself. Defer to §9 post-merge follow-up.

**`pnpm approve-builds` warning (from §10.5).** CI will emit `Ignored build scripts: esbuild@0.27.7, protobufjs@7.5.4.` on every install. Decision: **accept the warning** in CI logs. The ignored scripts are well-known low-risk vendors; adding an approve-builds step introduces config churn for cosmetic noise. Revisit if build scripts actually become required.

**Plan corrections:**
- **§5.7** reframe from binary "port vs rewrite" to the per-job hybrid above; be explicit about `rust` job scoping and workflow trigger rewrites.
- **§5.7** `paths:` filter should also include `.github/workflows/ci-{wacp,console}.yml` so workflow edits are self-testing.
- **§5.7** add a note: "`ci-console.yml` drops the cross-repo checkout entirely post-M4 — ~100-line simplification vs. current `wacp-console/.github/workflows/ci.yml`."
- **§9** post-merge follow-up: "Consider unifying pnpm version (9 vs 10) and node version (22 vs 24) between `ci-wacp.yml` and `ci-console.yml` — deferred from M6 to avoid surfacing unrelated bugs during merge."

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-wiring-strategy | Wiring Strategy | extends (W0 detail) |
| wcon-architecture | System Architecture | informs |

*WACP Platform — authored by AKIL Abderrahim and Claude Opus 4.6*
