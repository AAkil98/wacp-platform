# wacp-console — operator workbench for multi-agent coordination

A web UI for people running autonomous-agent systems. See what your agents are doing, approve their next move, audit what they decided and why.

Part of **wacp-platform** — a monorepo that ships both the operator workbench (`wacp-console`) and the protocol runtime (`wacp-runtime`) it talks to.

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Protocol: CC BY-SA 4.0](https://img.shields.io/badge/Protocol-CC_BY--SA_4.0-green.svg)](https://github.com/AAkil98/wacp-protocol)

> **v0.1.** Maintained by [@AAkil98](https://github.com/AAkil98); implementation co-authored with Claude Opus (4.6 / 4.7). Accepting contributors, maintainers, and co-maintainers — see [§Status](#status) for the repo-state snapshot and [§Looking for help](#looking-for-help) for how to get involved.

---

## What it does

Multi-agent systems are opaque. Agents decompose goals, invoke tools, produce artifacts, and occasionally fail in ways that nobody who cares about the outcome finds out about in time. The industry's answer has been either dashboards bolted onto a framework (narrow, vendor-locked) or nothing (hope).

**wacp-console** is a different layer: an operator surface that talks to *any* runtime implementing the Workspace Agent Coordination Protocol (WACP). Four surfaces:

| Surface | What it does |
|---------|-------------|
| **Discovery Browser** | Read-only taxonomy — roles, tools, verticals, capabilities. Search + filter + drill-down. |
| **Profile Studio** | Create/edit/clone/delete agent profiles. YAML import/export. Validated against the taxonomy. |
| **Session Launcher** | Six-step wizard: vertical → workflow → profile assignments → context → budgets → review + launch. |
| **Oversight Dashboard** | Real-time: trail stream, gate queue, escalation inbox, refusal panel, workspace tree, directive injection. Seven WebSocket channels. |

The architectural stance is that human oversight is protocol-level — gates, escalations, and injection are first-class operations, not hooks bolted onto a framework. The console is the UI for those operations. The runtime is the protocol implementation. The [protocol spec itself](https://github.com/AAkil98/wacp-protocol) lives in a separate repo under CC BY-SA 4.0.

---

## Quick start

### Docker compose — recommended

```bash
git clone https://github.com/AAkil98/wacp-platform
cd wacp-platform
docker compose up --build
```

Then open **<http://localhost:8080>**.

First boot compiles two Rust binaries + the React SPA from scratch — expect **~12–20 minutes** on a cold machine. Subsequent `up` calls reuse the layer cache and take seconds. Tail the build / startup logs with:

```bash
docker compose logs -f wacp-console
```

First-run credential: the console emits a one-time bootstrap token at startup; look for `BOOTSTRAP TOKEN: <43-char-base64>` in the `wacp-console` container logs. Sign in with that, then set a real password at the forced change-password prompt.

Tear down with `docker compose down` (add `-v` to wipe the sqlite + runtime data volumes).

### From source (contributors)

Prerequisites: Rust stable, Node.js ≥22, `protoc`, `pnpm`.

```bash
# Build both binaries
cargo build --release -p wacp-runtime -p wacp-console

# Run the runtime (ports 9090–9094)
cargo run -p wacp-runtime -- serve --config wacp/dev/runtime.yaml

# In another terminal, build the frontend + run the console (port 8080)
cd wacp-console/frontend && pnpm install && pnpm build && cd ../..
cargo run -p wacp-console -- serve
```

---

## Architecture

Two binaries shipping from one workspace, wired together by shared proto codegen.

```
           ┌─────────────────────┐
 operator  │    wacp-console     │  web UI + REST + 7 WebSocket channels
 (browser) │ Rust/Axum + React   │
           │  + SQLite (auth)    │
           └──────────┬──────────┘
                      │ gRPC (3 services) + REST
           ┌──────────▼──────────┐
  agents   │    wacp-runtime     │  4 gRPC services: Agent, Highway,
   (API)   │  16 Rust crates +   │    Coordinator, + /v1/* REST gateway
           │  7 ecosystem YAMLs  │
           └─────────────────────┘
```

| Subproject | Artifact | Role |
|------|----------|------|
| [`wacp-console/`](wacp-console/) | `wacp-console` | Full-stack coordination workbench — 6 Rust crates (Axum backend, sqlite persistence, 66 REST endpoints), React 19 SPA. |
| [`wacp/`](wacp/) | `wacp-runtime` | Protocol runtime — 4 gRPC services + REST gateway, 16 Rust crates, 7 ecosystem verticals, TypeScript CLI agent, Python SDK. |
| [`wacp/crates/wacp-proto/`](wacp/crates/wacp-proto/) | library | Shared `tonic_build` codegen for the `wacp.v1` proto package. |
| [`AAkil98/wacp-protocol`](https://github.com/AAkil98/wacp-protocol) *(sibling repo)* | spec | 20-spec protocol definition + `PROTOCOL.md` + `TAXONOMY.md`. CC BY-SA 4.0. |

Deeper reading: `wacp-console/specs/wcon-architecture.md` for the console's internals; `wacp-protocol/PROTOCOL.md` for the coordination protocol itself.

---

## Status

v0.1. Protocol spec complete; implementation reference-grade.

- **Scope.** Two binaries (`wacp-runtime`, `wacp-console`) in a 23-crate Cargo workspace. 7 ecosystem verticals (SWE, DevOps, MLOps, finance, healthcare, analytics, datasci). TypeScript CLI agent + Python SDK alongside the Rust runtime.
- **Surface.** Full console UI wired end-to-end against the runtime — Discovery, Profile Studio, 6-step Session Launcher, live Oversight dashboard (trail / gates / escalations / refusals / workspace tree / injection over 7 WebSocket channels). Multi-user auth (Argon2id, CSRF double-submit, per-account rate limiting). 66 console REST endpoints + 16 runtime REST endpoints + 4 gRPC services (Agent, Highway, Coordinator, Transport).
- **Tests.** ~1,280 runtime, ~190 console-core, 143 console-api, 52 cross-binary integration, 124 React component, 7 Playwright E2E. `cargo-llvm-cov` + Vitest v8 + `coverage.py` + Codecov. `cargo-mutants` weekly on four critical modules; three at 100 %, one at 98 %.
- **CI.** Four push-triggered workflows (`ci-lint`, `ci-wacp`, `ci-console`, `coverage`) + `ci-mutation` cron. Branch protection on `main` + `dev`. `cargo-deny` supply-chain gate; SBOM (CycloneDX) + Trivy on every release.

Pre-v0.1 release cuts remaining: UI polish, expanded error-state handling, tagged OCI publication to GHCR.

---

## Screenshots

*(placeholder — demo GIF + 3 screenshots landing shortly as launch-prep completes)*

---

## Looking for help

This is v0.1 and it needs more hands. Three tiers of involvement:

| Role | What you do | What you get |
|------|-------------|---------------|
| **Contributor** | Open PRs — bug fixes, UI polish, docs, small features | Experience on a novel multi-agent project; credit in the contributor list |
| **Maintainer** | Review PRs, triage issues, help shape roadmap | Commit access, input on direction, co-author credit on releases |
| **Co-maintainer** | Shared ownership; treat the project as yours too | Equal say in governance, public recognition as co-maintainer |

**First PR path:** see `CONTRIBUTING.md` *(landing shortly)* — setup + conventions + how to pick a first issue.

**Good first issues:** filter on the [`good first issue`](https://github.com/AAkil98/wacp-platform/labels/good%20first%20issue) label *(populating during launch week)*.

**Maintainer conversations:** open an issue titled "maintainer interest" or email `aakilabderr22@gmail.com` with a link to your recent work + what draws you to this.

No "we're a team". No "enterprise-ready". If something is broken or unclear, that's signal — open an issue or a PR. Real feedback is the only useful kind.

---

## Repository layout

```
wacp-platform/
├── Cargo.toml                  # Unified workspace — 23 members
├── Cargo.lock
├── rust-toolchain.toml
├── .cargo/config.toml          # WSL2 memory-pressure mitigation (jobs=1 + mold)
├── docker-compose.yml          # Dev compose: runtime + console
├── LICENSE                     # Apache-2.0 (this repo)
├── README.md                   # This file
├── SEED.md                     # Umbrella seed for fresh sessions
├── HEALTH-LOG.md               # Living drift/health log (tier 1 — append per session)
├── AUDIT-YYYY-MM-DD.md         # Dated audit snapshots (tier 2)
├── tech-debt-YYYY-MM-DD.md     # Dated tech-debt snapshots (tier 2)
│
├── adr/
│   └── adr-009-oci-only-console.md  # Console distribution: OCI image only
│
├── impl/
│   ├── git-strategy.md         # Branching / commits / merges (active reference)
│   ├── merge-execution-log.md  # Historical record of the M0→M7 merge
│   ├── ci-health-2026-04-17.md # Historical record of §2.1–§2.7 CI cleanup
│   └── archive/                # Executed plans (wiring-*, ci-cleanup, audit-13-7-8, …)
│
├── wacp/                       # Runtime subtree — preserved history
│   ├── crates/                 # 17 Rust crates (incl. wacp-proto)
│   ├── proto/                  # .proto definitions (wacp.v1)
│   ├── tests/                  # Integration tests
│   ├── ecosystem/              # 7 vertical manifests
│   ├── packages/               # TypeScript: @wacp/cli, @wacp/local
│   ├── sdk-python/             # Python agent SDK
│   ├── Dockerfile              # Runtime image
│   └── SEED.md, IMPLEMENTATION.md, …
│
└── wacp-console/               # Console subtree — preserved history
    ├── crates/                 # 6 Rust crates (console, console-api, …)
    ├── frontend/               # React 19 + Vite SPA
    ├── migrations/             # sqlx SQL migrations
    ├── specs/                  # 12 wcon-* design specs
    ├── impl/                   # merge-plan, phase evals (1–6)
    ├── Dockerfile              # Console image (multi-stage)
    └── SEED.md, IMPLEMENTATION.md, …
```

---

## Development setup

All optional — each item trims friction vs. leaving defaults. See `impl/git-strategy.md` §4 + §13 for the background.

Commit-message template — prefills the `<type>(<scope>): <subject>` skeleton and the Claude `Co-Authored-By` trailer:

```bash
git config commit.template .gitmessage
```

Conflict-resolution memory — records how a conflict was resolved so the same conflict isn't re-solved across rebase rounds:

```bash
git config --global rerere.enabled true
```

Opt-in pre-push hook — runs `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` before each push so fmt/clippy drift fails locally rather than in CI (adds ~2–5 min per push depending on cache state):

```bash
./scripts/install-hooks.sh     # uninstall: git config --unset core.hooksPath
```

---

## Authors

- **Akil Abderrahim** — design, architecture, maintainer
- **Claude Opus 4.6 / 4.7** — implementation co-author (credited per-commit via `Co-Authored-By` trailers)

---

## License

- **This repository** — Apache-2.0 across all code. See [`LICENSE`](LICENSE) for the full text.
- **Protocol specification** (sibling repo [`AAkil98/wacp-protocol`](https://github.com/AAkil98/wacp-protocol)) — CC BY-SA 4.0.

The split is deliberate: the protocol is designed to be implemented by others, the code is designed to be forked and extended.
