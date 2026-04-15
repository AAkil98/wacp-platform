# WACP Console

## Project Identity

- **Prefix:** wcon
- **Type:** application
- **Authors:** AKIL Abderrahim, Claude Opus 4.6

The prefix namespaces all spec IDs, branch names, and artifact identifiers across the project. The type (protocol, application, or library) informs which spec categories are most relevant but does not restrict which commands are available — all commands are opt-in regardless of project type.

## Description

Full-stack coordination workbench for the WACP ecosystem. Users discover available agent roles and capabilities, create and manage agent profiles, launch coordination sessions against a live WACP runtime, and oversee agent work in real-time through the human highway.

## Upstream Dependency

This application builds on top of WACP (Workspace Agent Coordination Protocol), located at `../wacp/`. The WACP runtime, gRPC services, SDKs, and ecosystem definitions are the foundation — this project consumes them, it does not duplicate them.

## Project State

**Design phase: complete.** 12 design specs finalized (`specs/`), 8 ADRs accepted (`SPEC_BUILD.md`), tech stack selected (`TECH_STACK_PROPOSAL.md`).

**Implementation phase: starting.** Next step is `/impl-plan` to create `IMPLEMENTATION.md` with a phased build plan. No code exists yet — no `Cargo.toml`, no `package.json`, no source.

Read `SEED_CONTEXT.md` for a compressed summary of the full design before writing any code.

## Tech Stack (ADR-003)

**Backend:** Rust workspace — Tokio + Axum 0.8 + Tonic 0.12 + sqlx 0.8 (SQLite, compile-time checked) + arc-swap + serde. Git dep on `wacp-taxonomy` for upstream types.

**Frontend:** React 19 + TypeScript 5 strict + Vite 6 + shadcn/ui + Radix + Tailwind 4 + TanStack Query/Table/Virtual + React Hook Form + Zod + CodeMirror 6. Playwright for E2E.

**Distribution:** Single binary via `rust-embed` + `cargo-dist`. Apache-2.0.

## Runtime Connection Model

The Console connects to the WACP runtime over four network endpoints:

| Service | Default address | Console config key |
|---------|----------------|--------------------|
| AgentService (gRPC) | `[::1]:9090` | `runtime.agent_address` |
| HighwayService (gRPC) | `[::1]:9091` | `runtime.highway_address` |
| CoordinatorService (gRPC) | `[::1]:9092` | `runtime.coordinator_address` |
| REST gateway | `http://[::1]:9093` | `runtime.rest_address` |

Three separate Tonic channels (NOT multiplexed). Per-service health tracking and reconnection.

## Workspace Layout (ADR-003)

```
wacp-console/
├── Cargo.toml                  # workspace root
├── rust-toolchain.toml         # pin Rust stable
├── crates/
│   ├── console/                # binary — wires services together
│   ├── console-api/            # Axum routes, handlers, utoipa annotations
│   ├── console-core/           # domain logic (no I/O): profile, session, taxonomy, highway
│   ├── console-db/             # sqlx types, queries, migrations
│   ├── console-runtime/        # gRPC + REST clients to WACP runtime
│   └── console-test-support/   # shared test fixtures
├── migrations/                 # sqlx SQL migration files
├── frontend/                   # Vite + React + TypeScript SPA
│   ├── src/surfaces/           # discovery, profiles, sessions, oversight
│   ├── src/api/                # generated from OpenAPI
│   └── src/realtime/           # WebSocket hook
└── specs/                      # 12 finalized design specs
```

## Spec Conventions

### Frontmatter

Every spec file begins with YAML frontmatter:

```yaml
---
id: wcon-<scope>
type: design | impl | coding
status: draft | review | final
created: YYYY-MM-DDTHH:MM:SS
revised: YYYY-MM-DDTHH:MM:SS
authors: [AAkil98]
tags: [relevant, topic, tags]
depends_on: [wcon-<other-scope>]
---
```

Required fields: `id`, `type`, `status`, `created`, `authors`.
Optional fields: `revised`, `tags`, `depends_on`.

### Identifiers

Spec IDs follow `wcon-<scope>` using descriptive slugs (e.g., `wcon-architecture`, `wcon-profiles`).

### Structure

```
# Title
## Table of Contents
---
## 1. First Section
### 1.1 Subsection
...
## N. Last Section
## References
*WACP Console -- authored by AAkil98*
```

Sections are numbered (`## N. Title`), subsections follow (`### N.M`). The Table of Contents lists all sections by number. Every spec ends with a References table and footer line.

### Cross-References

Within this project, reference by spec ID and section number: `wcon-<scope>` §N.M.

Across projects, use the full spec ID: `wacp-protocol` §5.1, `wcon-architecture` §3.2. Spec IDs are stable across repos and refactors — never use file paths for cross-references.

### References Table

```markdown
## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-<scope> | Spec title | informs / constrains / extends / implements |
```

## Spec Types

### Design Specs

What the system *is*. Architecture, data model, feature designs, glossary, vision. Source of truth — implementation answers to design, not the reverse.

Location: `specs/`.

### Implementation Specs

How design becomes code. Phased build plans, module mappings, tech stack decisions (ADR format), per-phase bridge documents.

Location: `impl/`.

### Coding Specs

Granular per-task specifications: scope, dependencies, types, function signatures, internal design, test cases, acceptance criteria.

Location: `specs/coding/`.

## Quality Gates

### Spec Quality

Structural checks:
- Frontmatter complete (all required fields present, status accurate)
- All TOC sections have content (no empty placeholders)
- Cross-references resolve (referenced specs exist, section numbers valid)
- Terminology consistent with glossary
- Formatting conventions followed (section numbering, table format, footer)

Substantive checks:
- Invariants and rules explicitly stated
- Edge cases addressed
- Security considerations present where relevant
- State machines have defined transitions for all states

### Code Quality

**Rust:**
- `cargo fmt --check` — zero formatting violations
- `cargo clippy -- -D warnings` — zero warnings (pedantic)
- `cargo test --workspace` — all tests pass
- sqlx compile-time query verification on all SQL
- No `unwrap()` in production code (use `?` or explicit error handling)

**TypeScript:**
- `pnpm lint` (ESLint + typescript-eslint) — zero warnings
- `pnpm typecheck` (tsc --noEmit, strict mode) — zero errors
- `pnpm test` (Vitest) — all tests pass

**Shared contract:**
- `cargo run --bin gen-openapi && git diff --exit-code` — OpenAPI not stale
- `pnpm gen:api && git diff --exit-code` — TS types not stale

### Commit Conventions

```
<type>(<scope>): <subject>
Types: feat, fix, refactor, docs, test, chore, ci
Scope: module name, spec ID, or phase identifier
Subject: imperative mood, lowercase, no period
```

## Project Files

### SPEC_BUILD.md

Tracks spec construction: the spec map (from `/spec-eval`), architectural decision records, open questions, and session log.

### IMPLEMENTATION.md

Tracks the build: tech stack decisions, phased implementation plan, phase status, and retrospective notes.

## Workflow

### Design Phase

```
/spec-eval → /glossary [GATE] → [/spec-scaffold → /spec-write ×N → /grill [GATE] → /spec-review] ×M
```

### Bridge Phase

```
/tech-stack → /impl-plan → /setup-dev
```

### Code Phase (per phase)

```
/impl-spec → /coding-spec ×T → /code-scaffold (phase 0 only) → /test-plan → [coding] → /phase-eval
```

### Output Locality

All commands produce local markdown output. No automatic GitHub issue creation, no external service dependencies. The author decides where output goes.
