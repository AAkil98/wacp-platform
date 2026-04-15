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
| [**Madahub-dev/wacp-protocol**](https://github.com/Madahub-dev/wacp-protocol) | Authoritative protocol specification (20 specs + `PROTOCOL.md` + `TAXONOMY.md`). Deliberately kept separate; spec IDs are path-independent. | CC BY-SA 4.0 |

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

## Repository Layout

```
wacp-platform/
├── Cargo.toml                  # Unified workspace — 23 members
├── Cargo.lock
├── rust-toolchain.toml
├── .cargo/config.toml          # WSL2 memory-pressure mitigation (jobs=1 + mold)
├── docker-compose.yml          # Dev compose: runtime + console
├── SEED.md                     # Umbrella seed for fresh sessions
├── README.md                   # This file
│
├── impl/
│   ├── wiring-strategy.md      # W0–W6 cross-cutting wiring plan (post-merge work)
│   ├── merge-execution-log.md  # Live record of the M0→M7 merge (this assembly)
│   └── adr-009-oci-only-console.md  # Console distribution: OCI image only
│
├── wacp/                       # Runtime subtree — preserved history
│   ├── crates/                 # 17 Rust crates (incl. wacp-proto)
│   ├── proto/                  # .proto definitions (wacp.v1)
│   ├── tests/                  # Integration tests
│   ├── ecosystem/              # 7 vertical manifests
│   ├── packages/               # TypeScript: @wacp/cli, @wacp/local
│   ├── highway-ui/             # Human oversight SPA
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
