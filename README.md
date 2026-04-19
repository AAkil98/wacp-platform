# WACP Platform

Umbrella monorepo for the Workspace Agent Coordination Protocol (WACP) reference implementation and its coordination workbench. Two binaries, one Cargo workspace, shared proto codegen.

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)

## Subprojects

| Path | Artifact | Role |
|------|----------|------|
| [`wacp/`](wacp/) | `wacp-runtime` | WACP protocol runtime: 4 gRPC services (Agent, Highway, Coordinator) + REST gateway, 16 Rust crates, 7 ecosystem verticals, TypeScript CLI agent, Python SDK. |
| [`wacp-console/`](wacp-console/) | `wacp-console` | Full-stack coordination workbench: 6 Rust crates (Axum backend, sqlite persistence, 66 REST endpoints), React 19 SPA. Connects to the runtime via gRPC + REST; never modifies protocol behavior. |
| [`wacp/crates/wacp-proto/`](wacp/crates/wacp-proto/) | library | Shared `tonic_build` codegen for the `wacp.v1` proto package. Single source of truth consumed by `wacp-transport`, `console-runtime`, and `console-test-support`. |

## Sibling Repositories

| Repo | Purpose | License |
|------|---------|---------|
| [**AAkil98/wacp-protocol**](https://github.com/AAkil98/wacp-protocol) | Authoritative protocol specification (20 specs + `PROTOCOL.md` + `TAXONOMY.md`). Deliberately kept separate; spec IDs are path-independent. | CC BY-SA 4.0 |

## Quick Start

Prerequisites: Rust stable, Node.js ≥22, `protoc`, `pnpm`.

```bash
# Build both binaries
cargo build --release -p wacp-runtime -p wacp-console

# Run the runtime (ports 9090–9093)
cargo run -p wacp-runtime -- serve --config wacp/dev/runtime.yaml

# In another terminal, run the console (port 8080)
cargo run -p wacp-console -- serve

# Or bring up both via docker compose:
docker compose up
```

## Development Setup

All optional — each item trims friction vs. leaving defaults. See `impl/git-strategy.md` §4 + §13 for the background.

Commit-message template — prefills the `<type>(<scope>): <subject>` skeleton and the Claude `Co-Authored-By` trailer:

```bash
git config commit.template .gitmessage
```

Conflict-resolution memory — records how a conflict was resolved so the same conflict isn't re-solved across rebase rounds:

```bash
git config --global rerere.enabled true
```

## Repository Layout

```
wacp-platform/
├── Cargo.toml                  # Unified workspace — 23 members
├── Cargo.lock
├── rust-toolchain.toml
├── .cargo/config.toml          # WSL2 memory-pressure mitigation (jobs=1 + mold)
├── docker-compose.yml          # Dev compose: runtime + console
├── SEED.md                     # Umbrella seed for fresh sessions
├── HEALTH-LOG.md               # Living drift/health log (tier 1 — append per session)
├── AUDIT-YYYY-MM-DD.md         # Dated audit snapshots (tier 2)
├── tech-debt-YYYY-MM-DD.md     # Dated tech-debt snapshots (tier 2)
├── README.md                   # This file
│
├── adr/
│   └── adr-009-oci-only-console.md  # Console distribution: OCI image only
│
├── impl/
│   ├── git-strategy.md         # Branching / commits / merges (active reference)
│   ├── merge-execution-log.md  # Historical record of the M0→M7 merge
│   ├── ci-health-2026-04-17.md # Historical record of §2.1–§2.7 CI cleanup
│   └── archive/                # Executed plans (wiring-*, ci-cleanup-2.7-plan, audit-13-7-8-plan, notes/)
│
├── wacp/                       # Runtime subtree — preserved history
│   ├── crates/                 # 17 Rust crates (incl. wacp-proto)
│   ├── proto/                  # .proto definitions (wacp.v1)
│   ├── tests/                  # Integration tests
│   ├── ecosystem/              # 7 vertical manifests
│   ├── packages/               # TypeScript: @wacp/cli, @wacp/local
│   ├── sdk-python/             # Python agent SDK
│   ├── Dockerfile              # Runtime image
│   └── SEED.md, IMPLEMENTATION.md, README.md, …
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

## Status

Post-M4 (path-dep flip) — see `impl/merge-execution-log.md` for the live execution record. `cargo check --workspace` clean across all 23 members.

## Authors

- **Akil Abderrahim** — Lead
- **Claude Opus 4.6** — Co-author

## License

Apache-2.0 across this repository. See [`wacp/LICENSE`](wacp/LICENSE) for the full text. The protocol specification in `wacp-protocol` is separately licensed under CC BY-SA 4.0.
