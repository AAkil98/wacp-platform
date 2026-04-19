# WACP — Reference Implementation

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)

**Reference implementation of the Workspace Agent Coordination Protocol (WACP).**

WACP is a formal protocol for coordinating autonomous agents — particularly AI agents — across isolated workspaces with explicit messaging, immutable progress records, capability-based security, and first-class human oversight. This repository contains the Rust reference implementation, TypeScript CLI agent, Python SDK, and seven ecosystem verticals that demonstrate domain-specific extension.

The **protocol specification itself** is maintained in a separate repository:

**[github.com/Madahub-dev/wacp-protocol](https://github.com/Madahub-dev/wacp-protocol)** — licensed under CC BY-SA 4.0

The specification is the authoritative definition of WACP. This repository implements it; it does not define it. Other implementations are welcome and should conform to the specs in `wacp-protocol`.

---

## Table of Contents

- [What's in This Repository](#whats-in-this-repository)
- [Quick Start](#quick-start)
- [Repository Structure](#repository-structure)
- [Status](#status)
- [Related Repositories](#related-repositories)
- [Contributing](#contributing)
- [Authors](#authors)
- [License](#license)

---

## What's in This Repository

This is the reference implementation across three language ecosystems:

**Rust runtime and middleware** (`crates/`, 16 crates):
- Core protocol types, clock, FSMs, taxonomy loader, permissions
- Trail storage with hash chaining, tiered retention, snapshots
- Workspace and coordinator actors
- Three gRPC services (Agent, Highway, Coordinator) + REST gateway + WebSocket binding
- Transport authentication: API key, session token, OAuth (OIDC/JWT)
- Tool framework, LLM adapters (Anthropic + OpenAI), security framework (content filter, secret store, audit events)
- Agent SDK v2, Coordinator SDK

**TypeScript applications and middleware** (`packages/`, `ecosystem/`):
- `@wacp/local` — local session SDK with autonomy spectrum and workflow executor
- `@wacp/cli` — protocol-aware CLI agent that spawns the runtime, loads the full ecosystem, and dispatches goals across verticals

**Python agent SDK** (`sdk-python/`):
- Agent, tools, LLM, coordinator, local-session modules mirroring the Rust surface

**Ecosystem verticals** (`ecosystem/`, 7 verticals):
- **SWE** — planner, implementer, tester, reviewer roles with code-focused tools
- **DevOps** — environment-scaled blast-radius gating
- **MLOps** — compute-budget + reproducibility checkpoints
- **Finance** — regulatory pre-check + forbidden-pattern screen on trade execution
- **Healthcare** — HIPAA PHI access grant gating clinical tools
- **Data Analytics** — SQL safety classification + query reproducibility
- **Data Science** — pre-declared hypothesis contract for statistical testing

Each vertical ships its own roles, task types, tools, profiles, workflows, and quality dimensions, plus a committed `vertical.yaml` manifest consumable by the runtime's REST gateway.

---

## Quick Start

Prerequisites: Rust (stable), Node.js ≥22, Python ≥3.11, `protoc`, `pnpm`.

```bash
# Build the runtime binary
cargo build --release --bin wacp-runtime

# Run the runtime on default ports (see IMPLEMENTATION.md §4.1 for the canonical port map)
./target/release/wacp-runtime serve

# Run the full test suite across all three language ecosystems
cargo test --workspace
cd packages/wacp-cli && pnpm install && pnpm test
cd ../../sdk-python && pip install -e ".[dev]" && pytest tests/
```

For the TypeScript CLI agent:

```bash
cd packages/wacp-cli
pnpm install
pnpm build
node dist/main.js   # loads all 7 verticals at boot, starts the REPL
```

---

## Repository Structure

```
wacp/
├── IMPLEMENTATION.md        # Forward strategy — runtime productionization + Phase 28/29
├── SEED.md          # Current state primer for new sessions
├── LAYER-MAPPING.md         # Historical architectural map (referenced by impl specs)
├── LICENSE                  # Apache-2.0
├── NOTICE                   # Attribution + pointer to wacp-protocol
│
├── Cargo.toml               # Workspace manifest — 16 crates
├── Cargo.lock
├── rust-toolchain.toml      # Pinned Rust version (where present)
│
├── proto/                   # Protobuf definitions (5 files)
│   ├── primitives.proto
│   ├── agent.proto
│   ├── highway.proto
│   ├── coordinator.proto
│   └── taxonomy.proto
│
├── crates/                  # Rust implementation (16 crates)
│   ├── wacp-types/          # Protocol enums, newtypes, structs
│   ├── wacp-clock/          # HLC timestamps
│   ├── wacp-fsm/            # Workspace / envelope / task state machines
│   ├── wacp-taxonomy/       # YAML/JSON loader + VerticalManifest
│   ├── wacp-permissions/    # Permission matrix, port rights
│   ├── wacp-trail/          # Storage, hash chain, snapshots, tiered retention
│   ├── wacp-workspace/      # Workspace actor (9 components)
│   ├── wacp-coordinator/    # Decision engine, migration, task graph
│   ├── wacp-transport/      # gRPC services, REST gateway, WebSocket, 4 auth providers
│   ├── wacp-recovery/       # Trail replay, snapshot recovery
│   ├── wacp-runtime/        # Binary: config, CLI, TLS, metrics, health
│   ├── wacp-sdk/            # Rust agent SDK (Agent, AgentContext)
│   ├── wacp-coordinator-sdk/# Coordinator client SDK
│   ├── wacp-tools/          # Tool framework (registry, execution, resilience)
│   ├── wacp-llm/            # LLM adapters (Anthropic, OpenAI, streaming)
│   └── wacp-security/       # Content filter, secrets, audit events
│
├── tests/                   # Cross-crate integration + E2E tests
│
├── impl/                    # Implementation specs (17 files — Rust/Tonic/SQLite-specific)
│   # Note: these describe how the reference implementation is built, not the
│   # portable protocol. Protocol specs live in github.com/Madahub-dev/wacp-protocol.
│
├── ecosystem/               # Domain verticals (7 packages)
│   ├── swe/
│   ├── devops/
│   ├── mlops/
│   ├── finance/
│   ├── healthcare/
│   ├── analytics/
│   └── datasci/
│
├── packages/                # TypeScript packages
│   ├── wacp-local/          # Local SDK
│   └── wacp-cli/            # CLI agent
│
├── sdk-python/              # Python agent SDK
│
├── deploy/                  # systemd unit
├── Dockerfile               # Multi-stage build
└── .github/workflows/       # CI
```

---

## Status

See [`SEED.md`](SEED.md) for the current-state snapshot and [`IMPLEMENTATION.md`](IMPLEMENTATION.md) for the forward strategy.

| Layer | Status |
|---|---|
| Protocol specification (separate repo: `wacp-protocol`) | Complete — v0.1 |
| Runtime core (Phases 0–19 + T1–T5) | Complete — 12 foundational crates |
| Middleware (Phases 20–24) | Complete — tool framework, LLM adapters, agent SDK v2, coordinator SDK, local SDK, security, transport extensions |
| CLI agent + SWE vertical (Phases 25, 26) | Complete |
| Remediation (Phase 26R) | Complete — no architectural gaps |
| Remaining verticals (Phase 27A–G) | Complete — DevOps, MLOps, Finance, Healthcare, Data Analytics, Data Science |
| Vertical wiring (Phase 27R) | Complete — multi-vertical ecosystem loader in the CLI |
| Vertical surfacing (Phase 27S) | Complete — `GET /v1/verticals[/{id}]` REST endpoint |
| **Runtime productionization + public API (Phase 29.1)** | **Pending** — see `IMPLEMENTATION.md` Stream A |
| **IDE + chat bridge (Phase 28)** | **Pending** — see `IMPLEMENTATION.md` Stream B |
| **Dashboard (Phase 29.2)** | **Pending** — built as `wacp-console`, separate repo |

Total test coverage across the three ecosystems runs in the low thousands; see `SEED.md` for the breakdown.

---

## Related Repositories

| Repo | Purpose | License |
|---|---|---|
| [**Madahub-dev/wacp-protocol**](https://github.com/Madahub-dev/wacp-protocol) | Authoritative protocol specification — 20 specs + `PROTOCOL.md` + `TAXONOMY.md` | CC BY-SA 4.0 |
| **Madahub-dev/wacp** (this repo) | Reference implementation — Rust runtime, TypeScript CLI, Python SDK, 7 verticals | Apache-2.0 |
| **Madahub-dev/wacp-console** (in progress) | Browser-based workbench — profile studio, session launcher, live oversight dashboard. Consumes this repo via gRPC + REST. | Apache-2.0 |

---

## Contributing

Contributions are welcome. For protocol-level changes (new primitives, new signal types, new integration rules), open a PR on `wacp-protocol` first — the implementation here follows the spec, not the reverse. For implementation-level changes (runtime internals, new REST endpoints, new vertical tools, bug fixes), open a PR here.

Before contributing:

- Run `cargo fmt --check && cargo clippy -- -D warnings && cargo test --workspace` for Rust changes.
- Run `pnpm typecheck && pnpm test` in the affected TypeScript package.
- Run `pytest tests/` in `sdk-python/` for Python SDK changes.

By submitting a pull request, you agree that your contributions will be licensed under Apache-2.0 (see `LICENSE`) consistent with the rest of this repository.

---

## Authors

- **Akil Abderrahim** — Lead
- **Claude Opus 4.6** — Co-author

---

## License

This work is licensed under the [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0). See [`LICENSE`](LICENSE) for the full text and [`NOTICE`](NOTICE) for attribution and pointer to the protocol specification.

The **protocol specification** in [Madahub-dev/wacp-protocol](https://github.com/Madahub-dev/wacp-protocol) is separately licensed under CC BY-SA 4.0. The two licenses are compatible: implementations of the WACP protocol are independent works, not derivative works of the specification in the share-alike sense, and may be licensed independently.
