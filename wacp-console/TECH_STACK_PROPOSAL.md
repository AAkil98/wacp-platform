# WACP Console — Tech Stack Proposition

**Status:** draft — awaiting answers to §10 open questions before promotion to an ADR in `SPEC_BUILD.md` and a tech-stack summary in `IMPLEMENTATION.md`.
**Author:** AAkil98 (drafted with Claude, 2026-04-11)
**Scope:** Every technology choice needed for a production-ready WACP Console implementation, with rationale tied back to the spec set.

---

## 1. Philosophy

Boring, production-proven choices aligned to the spec set's constraints:

1. **Single binary, zero external services.** `wcon-vision` §6 forbids cloud APIs, external databases, or third-party runtime dependencies. The Console ships as one Rust binary that embeds the SPA and speaks gRPC + REST to the runtime. No Docker required for local use; Docker optional for deployment.
2. **Same ecosystem as upstream WACP.** WACP is already Rust/Tonic/Axum/Tokio. The Console should consume the same protobuf types without reimplementing them, and share the same tooling discipline (`clippy`, `rustfmt`, structured `tracing`).
3. **Typed at boundaries.** Every API boundary (gRPC ↔ Rust, Rust ↔ REST, Rust ↔ frontend) is typed end-to-end. No hand-maintained DTO drift between layers.
4. **No novelty for its own sake.** React and sqlx over Svelte and SeaORM, not because the alternatives are worse, but because the hiring pool and long-term maintenance story are clearer.

## 2. Backend — Rust workspace

### 2.1 Core runtime

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Async runtime | **Tokio** | Explicit in `wcon-architecture` §7 ("async Rust application built on Tokio"). No real alternative; it's the ecosystem default. |
| HTTP server | **Axum 0.8** | Tower-based middleware, first-class WebSocket support, same team as Tonic/Tokio, matches upstream WACP's `wacp-transport::rest_gateway`. Serves both the REST API and embedded static frontend assets. |
| gRPC client | **Tonic 0.12** | De facto Rust gRPC. Connects to `runtime.grpc_address` multiplexing `AgentService` / `CoordinatorService` / `HighwayService` on one HTTP/2 channel per `wcon-architecture` §7. |
| REST client | **reqwest 0.12** | Standard Rust HTTP client. Used exclusively for `GET /v1/verticals[/{id}]` per `wcon-discovery` §2.2. Connection pooling by default. |
| WebSocket | **`axum::extract::ws`** (backed by tokio-tungstenite) | First-party Axum integration; handles the 7 channels in `wcon-api` §12.2. |

### 2.2 Persistence

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Relational store | **SQLite + sqlx 0.8** | `wcon-data-model` §2 locks in SQLite. sqlx chosen over rusqlite/seaorm for **compile-time query verification** — every SQL query is checked against the schema at `cargo check`. Soft-delete predicates (`deleted_at IS NULL`), the unique index on `session_assignments(session_id, slot_position)`, and the schema-version key get type safety. Async-native, no blocking wrapper. |
| Migrations | **sqlx-cli `sqlx migrate`** | Built-in migration runner, SQL-file-based, versioned. Handles `wcon-data-model` §9.2's forward-only migration requirement. |
| Connection mode | **WAL enabled via pragma at open** | Explicit in `wcon-data-model` §2.1. Single writer, concurrent readers. sqlx `SqliteConnectOptions::journal_mode(SqliteJournalMode::Wal)`. |
| Atomic taxonomy swap | **`arc-swap` crate** | Explicit in `wcon-data-model` §6.3 and §9.3 / `wcon-discovery` §9.3. The invariant "index is either fully built or the previous index is visible" is guaranteed by `ArcSwap::store`. |
| In-memory state | `dashmap` for shared concurrent maps (e.g. `ActiveSession` registry keyed by `session_id`), `tokio::sync::broadcast` for WebSocket fan-out per-session, `tokio::sync::RwLock` only where necessary | Matches the broadcast fan-out described in `wcon-architecture` §7. |

### 2.3 Serialization & schema

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Serde core | **serde 1 + serde_json 1** | Universal. |
| YAML (profile export/import, protocol taxonomy parse) | **serde_yml** | `serde_yaml` is unmaintained; `serde_yml` is the active fork with drop-in API. Used in `wcon-profiles` §7.1/§7.3 export/import and `wcon-discovery` §3.2 protocol taxonomy parsing. |
| ZIP (bulk profile export) | **zip 2.x** | Required by `wcon-profiles` §8.3 bulk export (`application/zip`). |
| Protobuf codegen | **tonic-build + prost** | Standard gRPC stack. Consumes the WACP `.proto` files directly. |
| Upstream types | **Git-depend on `wacp-taxonomy`** for `VerticalManifest` | Per `wcon-data-model` §6.1 the Console's `VerticalEntry` "is a one-to-one projection of `wacp-taxonomy::VerticalManifest`". Depending directly on the upstream crate means serde-compatible deserialization with **no schema duplication** and compile-time catching of upstream schema drift (`wcon-test` §10.4 Mock Fidelity invariant). |
| Forward-compat deserialization | **serde `#[serde(default)]` + `#[serde(other)]` for opaque unknowns** | Satisfies `wcon-discovery` §2.2.3 forward-compat clause without special tools. Unknown fields land in `raw_manifest: serde_json::Value` (defined in `wcon-data-model` §6.1). |

### 2.4 API surface tooling

| Concern | Choice | Rationale |
|---------|--------|-----------|
| OpenAPI generation | **utoipa 5** | Annotate Axum handlers with `#[utoipa::path(...)]` derive macros; output `openapi.yaml` at build time. Gives the frontend a machine-readable contract for the 58 endpoints in `wcon-api` §Endpoint Summary, and serves as living API docs. |
| Error codes | **thiserror 2** (crate errors) + **anyhow 1** (top-level) | Standard Rust split. The `TOOL_NOT_IN_ROLE_VERTICAL` / `MISSING_CONTEXT` / `INVALID_CONTEXT` enum-like error taxonomy in `wcon-api` §4.3 becomes a `#[derive(thiserror::Error)]` enum that serializes into the spec's error body shape. |
| Pagination cursors | **bincode or base64+JSON** | Cursor opaque string per `wcon-discovery` §4.2 / `wcon-api` §5.3. Either works; bincode is smaller. |
| Request IDs | **tower-http `RequestIdLayer`** | Populates the `X-Request-Id` header from `wcon-api` §2.2. |
| Request validation | **axum + garde** or handwritten | `garde` for declarative body validation against struct constraints. Alternative: handwritten in service layer. Pick per team preference. |

### 2.5 Concurrency primitives

| Need | Tool |
|------|------|
| Broadcast session events to N WebSocket writers | `tokio::sync::broadcast` (bounded channel, slow-consumer drop per `wcon-architecture` §7) |
| Session monitor mailbox (MPSC from 4 stream subscribers) | `tokio::sync::mpsc` |
| Session registry (keyed lookup of active sessions) | `dashmap` — lock-free concurrent map |
| Taxonomy index swap | `arc_swap::ArcSwap` |
| Graceful shutdown | `tokio::signal::ctrl_c()` + `tokio_util::sync::CancellationToken` propagation |
| Reconnection backoff | `tokio::time::sleep` with exponential schedule (`wcon-sessions` §8.1 specifies 100ms→5s cap, 30 attempts) |

### 2.6 Observability

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Logging / tracing | **`tracing` + `tracing-subscriber`** | Structured JSON logging to stdout per `wcon-architecture` §4.1 Infrastructure. `tracing-subscriber::fmt().json()` in production, pretty console in dev. `tracing::instrument` on handlers for per-request spans. |
| OpenTelemetry (optional) | **`tracing-opentelemetry` + OTLP exporter** | Opt-in; disabled by default to preserve "no external dependencies". When enabled, exports traces to an OTLP endpoint the operator configures. |
| Metrics (optional) | **`metrics` + `metrics-exporter-prometheus`** | Expose `/metrics` scrape endpoint. Track session counts, gRPC round-trip times, taxonomy index build duration. Off by default. |

### 2.7 Security

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Credential handling | **`secrecy` crate** | `runtime.auth_credential` (`wcon-data-model` §5.2) wrapped in `SecretString` so it's never accidentally logged or formatted. |
| TLS (outbound to runtime) | **rustls** (via reqwest feature + tonic feature) | Pure-Rust TLS, no OpenSSL dependency. For when the runtime addresses are `https://` / gRPC-over-TLS. |
| HTTP middleware | **tower-http** | Provides compression, CORS, trace, request-id, timeout layers. |
| Auth middleware | Hand-rolled `Authenticator` trait per `wcon-architecture` §8 | Pluggable for the future OAuth/OIDC extension described in §8's non-goal path. Initial implementation: a single `ApiKeyAuthenticator` that reads from settings. |

### 2.8 CLI

| Concern | Choice |
|---------|--------|
| CLI parsing | **clap 4** with derive API. Subcommands: `serve` (run the backend), `migrate` (run sqlx migrations), `doctor` (health-check runtime connectivity + database). |
| Platform dirs | **`directories` crate** for XDG / AppData / Library-aware default data directory. |

### 2.9 Rust workspace layout

```
wacp-console/
├── Cargo.toml                  # workspace root with [workspace.dependencies]
├── rust-toolchain.toml         # pin Rust 1.83 or latest stable
├── crates/
│   ├── console/                # main binary (thin — wires services together)
│   ├── console-api/            # Axum routes, handlers, utoipa OpenAPI annotations
│   ├── console-core/           # domain services: profile, session, taxonomy, highway
│   ├── console-db/             # sqlx types, queries, migrations module
│   ├── console-runtime/        # gRPC client (Tonic) + REST client (reqwest) to WACP
│   └── console-test-support/   # shared fixtures for tests (fixture-simple/complex manifests)
├── migrations/                 # sqlx-compatible SQL migration files
└── frontend/                   # see §3 below
```

Rationale: the spec's "four services + infrastructure layer" (`wcon-architecture` §4.1) maps cleanly to crates. `console` is a trivial binary that instantiates each service and binds the router. `console-core` has **no** I/O — it's pure logic testable by `cargo test` without services. `console-runtime` encapsulates everything that talks to WACP (gRPC + REST), so mocking it for integration tests means replacing one crate.

## 3. Frontend — Vite + React + TypeScript

### 3.1 Core framework

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Framework | **React 19 + TypeScript 5 (strict)** | `wcon-ui` specs a complex four-surface SPA with real-time dashboards, dynamic forms generated from `context_schema`, and rich tables. React's ecosystem supports all of it with mature libraries. TypeScript is non-negotiable for a project this size. |
| Build tool | **Vite 6** | Best-in-class dev server, instant HMR, tree-shaken production builds. Simple config vs Next.js (which we don't need — no SSR, Axum serves the bundle). |
| Routing | **React Router 7** (data router mode) | Standard. Declarative routes for the screens in `wcon-ui` §3, with loaders for per-screen data fetching. |
| Package manager | **pnpm** | Fast, strict, disk-efficient; fine with monorepo layouts. |

### 3.2 State, data, real-time

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Server state | **TanStack Query v5** | The industry standard for REST state with cache, mutation tracking, optimistic updates, and invalidation. Implements the "fetch on mount" and "optimistic update" patterns in `wcon-ui` §11.2 directly. Cache invalidation rules in §11.3 map to `queryClient.invalidateQueries` calls. |
| Client UI state | **Zustand 5** | Lightweight store for UI state (sidebar open/closed, filter selections, the currently-selected session in the dashboard). Simpler than Redux; less ceremony than Context for shared state. |
| WebSocket | **Hand-rolled hook + `reconnecting-websocket` or similar** | TanStack Query doesn't handle push-streaming well. A thin `useSessionStream(sessionId)` hook that owns the WebSocket, parses JSON frames per the seven channels in `wcon-api` §12.2, and fans events into Zustand slices. Reconnection with exponential backoff per `wcon-sessions` §8.3. |
| Forms | **React Hook Form + Zod resolver** | Step 4 of the wizard (`wcon-ui` §6.2) generates forms dynamically from `context_schema`. RHF supports dynamic field arrays and validation; Zod is the bridge to runtime validation. |
| Schemas | **Zod 3** | Define schemas once, derive TypeScript types, validate at runtime. Parse API responses at the boundary (defensive) and validate form submissions. |

### 3.3 UI library

**Recommended: shadcn/ui + Tailwind + Radix Primitives + TanStack Table.**

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Component base | **shadcn/ui** (Radix primitives + Tailwind) | Not a library — copy-pastable React components owned in your repo. Maximum control over styling and accessibility. Matches `wcon-ui` §12.3 keyboard accessibility invariant through Radix's built-in focus management, modal focus-trap, and roving-tabindex support. |
| Styling | **Tailwind CSS 4** | Utility-first; plays well with shadcn/ui. Dark/light theme via CSS variables (`wcon-data-model` §5.2 `ui.theme` setting). |
| Table | **TanStack Table v8** (headless) | Sortable, filterable, paginated tables for role list, tool list, profile library, session list. Headless = no visual lock-in. Cursor-based pagination via `manualPagination` matches the Console's server-side cursor model. |
| Data grid virtualization | **TanStack Virtual** | Essential for the trail stream (§7.2 in `wcon-ui`), which can hit the `ui.trail_buffer_size` default of 1000 entries. Windowing prevents DOM bloat. |
| Notifications / toasts | **Sonner** or **react-hot-toast** | Matches the toast + auto-dismiss rules in `wcon-highway` §9.2. |
| Icons | **Lucide React** | Ships with shadcn/ui. Comprehensive. |
| Keyboard shortcuts | **react-hotkeys-hook** | Implements the `G`/`Enter`/`A`/`R`/`E`/`F` shortcuts in `wcon-ui` §7.2. |
| Date display | **date-fns** | Tree-shakable, standard. Formats ISO 8601 timestamps from `wcon-api` §2.3. |
| Code editor (injection, YAML import preview) | **CodeMirror 6** | Lighter than Monaco, modular (import only the languages we need — JSON, YAML, plain text). Used for the injection payload editor in `wcon-highway` §6 and the profile YAML preview in `wcon-profiles` §7.3 import flow. |
| Workflow DAG rendering (future) | **@xyflow/react (React Flow)** | Per-stage detail isn't in the manifest today (`wcon-data-model` §6.1 note), but when upstream adds it the workflow cards in `wcon-ui` §4.5 and §6.2 expand to full DAGs. React Flow is the industry-standard library for node-edge graphs. Hold for phase N+1. |

**Alternative considered: Mantine 7.** All-in-one UI library with great tables, forms, dates, modals. Rejected in favor of shadcn/ui because Mantine's opinionated look/feel is harder to escape later, and the dashboard's information density probably wants custom styling. Revisit if the team wants batteries-included speed at the cost of flexibility.

### 3.4 Frontend directory layout

```
frontend/
├── package.json
├── vite.config.ts
├── tailwind.config.ts
├── tsconfig.json               # strict: true, noUncheckedIndexedAccess: true
├── src/
│   ├── main.tsx                # Root: QueryClient, Router, Toaster
│   ├── api/
│   │   ├── client.ts           # Generated fetch client (from OpenAPI)
│   │   ├── types.ts            # Generated TS types (from OpenAPI)
│   │   └── hooks/              # TanStack Query hooks wrapping each endpoint
│   ├── realtime/
│   │   └── useSessionStream.ts # WebSocket hook per-session
│   ├── surfaces/
│   │   ├── discovery/          # Discovery Browser (wcon-ui §4)
│   │   ├── profiles/           # Profile Studio (wcon-ui §5)
│   │   ├── sessions/           # Session Launcher + list (wcon-ui §6)
│   │   └── oversight/          # Oversight Dashboard (wcon-ui §7)
│   ├── components/             # Shared: Sidebar, ContextBadge, RefusalCard, …
│   ├── lib/                    # Utilities (date, format, validation)
│   └── store/                  # Zustand slices (ui, notifications, …)
└── tests/                      # Vitest + RTL + Playwright
```

## 4. Shared contract — Rust ↔ TypeScript

**Single source of truth: Rust types → OpenAPI YAML → TypeScript types.**

1. Backend handlers are annotated with `utoipa::path` + derive macros on their request/response types.
2. `cargo run --bin gen-openapi` (a small binary in the `console-api` crate) writes `openapi.yaml`.
3. Frontend's `pnpm gen:api` runs **`openapi-typescript`** to generate `src/api/types.ts` + a thin `fetch` wrapper.
4. CI check: `pnpm gen:api && git diff --exit-code` — fails if generated types are stale.

This means:
- Adding a new endpoint is a one-file change in Rust; TypeScript regenerates automatically.
- Renaming a field breaks both ends at compile time.
- The OpenAPI schema is also a user-facing artifact — `wcon-api` §13 invariants stay in sync with reality.

**Alternative rejected:** `ts-rs` (derive macro that generates TS from Rust structs directly). Simpler but sidesteps the OpenAPI documentation benefit and creates a Rust-only tooling dependency the frontend would otherwise not need.

## 5. Build & distribution

### 5.1 The single-binary story

1. `pnpm build` in `frontend/` produces `frontend/dist/` (static assets).
2. `cargo build --release` in the workspace root picks up those assets via **`rust-embed`** and embeds them in the binary.
3. Axum serves them at `/` (and `/assets/*`) per `wcon-api` §1.2 base path split.
4. The final artifact is a ~20–40 MB binary that runs anywhere.

### 5.2 Platforms

| Tier | Target | Build via |
|------|--------|-----------|
| Tier 1 | `x86_64-unknown-linux-gnu` | GitHub Actions Linux runner |
| Tier 1 | `aarch64-apple-darwin` | GitHub Actions macOS runner |
| Tier 2 | `x86_64-pc-windows-msvc` | GitHub Actions Windows runner |
| Tier 2 | `aarch64-unknown-linux-gnu` | Cross-compile via `cross` |

### 5.3 Docker (optional)

Multi-stage Dockerfile (Rust builder → pnpm frontend builder → distroless runtime) for teams that prefer containers. The binary runs fine without it.

### 5.4 Desktop packaging (deferred)

**Tauri** is the obvious future option for packaging the Console as a native desktop app. It was **considered and rejected** for the initial stack:

- The spec's architecture (`wcon-architecture` §1, §8) assumes a server-client split over HTTP/WebSocket with pluggable authentication middleware. Tauri would collapse that to direct IPC, ruling out the "team lead watches gates from their laptop" multi-client scenario (Secondary target user in `wcon-vision` §4).
- Migrating to Tauri later is a strictly cheaper operation than migrating away from it — we can always wrap the existing binary in a Tauri webview if demand emerges.

## 6. Observability & operations

| Concern | Approach |
|---------|----------|
| Logs | `tracing` subscriber: pretty in dev, JSON in prod. Log level via `RUST_LOG`. |
| Health | `GET /api/health` per `wcon-api` §11.1 with separate `runtime_grpc` and `runtime_rest` check fields. |
| Readiness vs liveness | Health endpoint handles both with distinct semantics (db up + taxonomy built = ready; process responsive = live). |
| Metrics (opt-in) | `/metrics` Prometheus endpoint via `metrics-exporter-prometheus`. |
| Tracing (opt-in) | `OTEL_EXPORTER_OTLP_ENDPOINT` env-gated export via `tracing-opentelemetry`. |
| Panics | `std::panic::set_hook` to log a structured panic event and shut down cleanly. |

## 7. Testing — per layer

Aligned with `wcon-test` §2 (four test layers).

### 7.1 Backend unit tests

| Tool | Purpose |
|------|---------|
| `cargo test` | Built-in runner. |
| `rstest` | Parameterized tests (e.g., `ROLE_MISMATCH` across Mode A and Mode B). |
| `pretty_assertions` | Readable diffs on struct comparisons. |
| `insta` | Snapshot testing for complex response bodies (profile validation warnings, refusal events). Optional but useful. |
| In-memory sqlx | Per-test `SqlitePool::connect(":memory:")` with migrations applied (per `wcon-test` §3.3). |

### 7.2 Frontend unit tests

| Tool | Purpose |
|------|---------|
| **Vitest 2** | Vite-native test runner. Fast, ESM-first. |
| **React Testing Library** | Component tests focused on user-visible behavior. |
| **MSW (Mock Service Worker)** | HTTP mocking for component tests (matches the `mockApi()` pattern in `wcon-test` §4.3). |

### 7.3 Integration tests (backend ↔ mock runtime)

| Tool | Purpose |
|------|---------|
| **Tonic in-process server** | Mock gRPC per `wcon-test` §5.2. Tonic supports binding a `tonic::transport::Server` to a random port in-process. |
| **Axum in-process server** | Mock REST per the same section — serves fixture `VerticalManifest` responses. |
| **`wiremock` (optional)** | For simpler REST mock scenarios; plain Axum is sufficient. |
| Fixture crate | `console-test-support` exports `fixture_simple_manifest()` / `fixture_complex_manifest()` (`wcon-test` §7.1). |

### 7.4 End-to-end tests

| Tool | Purpose |
|------|---------|
| **Playwright** | Cross-browser (Chromium default), parallelized, great network interception, built-in tracing for flake debugging. Matches the E2E scenarios in `wcon-test` §6.3. |
| Harness | A small `console-e2e` crate starts the full binary with mock runtime sidecar, seeds fixtures, and spawns Playwright against it. |

### 7.5 Mock fidelity invariant

Per `wcon-test` §10.4: the mock runtime uses the same `wacp-taxonomy::VerticalManifest` Rust struct as the real runtime (via git dep), so schema drift is caught at `cargo check`. Protobuf contracts for gRPC are similarly shared — if upstream regenerates a proto, the mock fails to compile.

## 8. Dev experience & CI

### 8.1 Local dev loop

```
# terminal 1: backend with live reload
cargo watch -x 'run -- serve'

# terminal 2: frontend dev server
pnpm dev   # Vite on :5173, proxies /api → :8080

# terminal 3: mock runtime (Tonic + Axum) seeded with fixtures
cargo run -p mock-runtime
```

`cargo-watch` re-runs the backend on source changes. Vite handles frontend HMR. The mock runtime runs as a sidecar during development so the Console has something to talk to.

### 8.2 Linting, formatting, pre-commit

| Tool | Purpose |
|------|---------|
| `cargo fmt` | Enforced in CI (`cargo fmt --check`). |
| `cargo clippy` | Pedantic lints, `-D warnings` in CI. |
| `cargo deny` | License allow-list, advisory DB check, duplicate version detection. |
| `cargo audit` | RustSec advisory scan. Can be folded into `cargo deny`. |
| ESLint + typescript-eslint | Frontend lint, zero warnings in CI. |
| Prettier | Frontend format. |
| pre-commit | Optional: runs `fmt`, `clippy --fix`, `eslint --fix` before commits. |

### 8.3 CI matrix (GitHub Actions)

Stages and timings per `wcon-test` §8.1:

| Stage | Jobs | Target |
|-------|------|--------|
| Lint | `cargo fmt --check`, `cargo clippy -- -D warnings`, `pnpm lint`, `pnpm typecheck` | <2 min |
| Unit | `cargo test` (Linux + macOS + Windows), `pnpm test` | <5 min |
| Integration | `cargo test --test '*'` (in-process mock runtime) | <5 min |
| E2E | `pnpm test:e2e` against full binary + mock runtime | <15 min |
| OpenAPI drift | `cargo run --bin gen-openapi && git diff --exit-code openapi.yaml` | <30s |
| TS codegen drift | `pnpm gen:api && git diff --exit-code` | <30s |

Cache layers: `~/.cargo/registry`, `target/`, `node_modules`, pnpm store.

### 8.4 Pinned toolchains

- **`rust-toolchain.toml`** pins the Rust version (recommend stable + 1 behind for safety, e.g. whichever stable is 1–2 releases back from absolute latest, to match what CI distros ship).
- **`.nvmrc`** / **`package.json#engines`** pins Node ≥22 LTS.
- **`packageManager` in package.json** pins pnpm version (Corepack-aware).

## 9. Explicit non-choices

Things considered and rejected, with brief reasons.

| Rejected | Reason |
|----------|--------|
| **SeaORM / Diesel** | Sqlx's compile-time SQL checking is the feature; an ORM layer obscures it. The domain is simple enough to write hand-crafted SQL. |
| **Actix-web** | Separate ecosystem from Tonic/Tokio mainline; Axum integrates better with Tonic (which is a hard requirement). |
| **Warp / Rocket / Poem** | Smaller communities; Axum has become the pragmatic default. |
| **SvelteKit / Vue / Solid** | Smaller ecosystem for the specific libraries we need (TanStack Table, React Flow, CodeMirror bindings). React is the safer bet for a project with many components and real-time state. |
| **Next.js / Remix** | We don't need SSR, RSC, or file-based routing. Vite is simpler. |
| **Redux Toolkit** | TanStack Query handles server state; Zustand handles UI state. Redux is overkill. |
| **Material UI / Ant Design / Chakra / Mantine** | Opinionated design systems that would be hard to match to the dense dashboard layout in `wcon-ui` §7. shadcn/ui offers control without rewriting from scratch. |
| **Electron wrapper** | Binary size, memory overhead, and the spec's "no external dependencies" all argue against it. Tauri is the better future option if desktop packaging becomes a requirement. |
| **ts-rs (Rust → TS direct)** | No OpenAPI benefit. utoipa + openapi-typescript gives us API docs for free. |
| **Pulsar / NATS / Redis for session monitor state** | `wcon-architecture` §6 puts in-memory state in memory. Each active session is at most a few dozen KB; no need for external state stores. Backend restart recovery is handled by re-subscribing to the runtime streams (`wcon-sessions` §8.2). |
| **GraphQL** | The frontend consumes well-defined REST endpoints. GraphQL would add a resolver layer for no real payoff. |
| **WebAssembly frontend (Yew/Leptos/Dioxus)** | Mature React ecosystem wins for a complex dashboard. Revisit if a compelling reason appears. |
| **Monaco over CodeMirror 6** | Monaco is heavier (~2MB min) and brings the VS Code rendering model. CodeMirror 6 is modular and covers our edit-text needs. |

## 10. Open questions (need product input)

Answer each inline below the prompt. Format: leave "**Answer:**" as-is and write the decision after it.

---

### Q1. Target distribution model

Single binary + `curl | sh` install, Homebrew tap, distro packages (deb/rpm), Docker image, all of the above? This affects CI complexity and release tooling (`cargo-dist`, `goreleaser`, or custom).

**Answer:**

Ship five channels in Phase 1 via `cargo-dist`; defer deb/rpm packages.

**Shipped in Phase 1:**
1. GitHub Releases with prebuilt binaries for all Tier 1/Tier 2 targets in §5.2.
2. Shell installer (`curl https://…/install.sh | sh`) — auto-generated by `cargo-dist`.
3. Homebrew tap — auto-generated by `cargo-dist`.
4. Windows: MSI installer or winget manifest (pick one; `cargo-dist` supports both).
5. Docker image — multi-stage, distroless base, per §5.3.

**Deferred:** deb/rpm packages. Rationale: each distro carries real per-version maintenance cost (systemd units, FHS layout conventions, postinst/preinst scripts, signing, upstream maintainer adoption) that does not serve the Console's operator-facing dev-tool audience. Peer tools (`kubectl`, `gh`, `k9s`, `lazygit`, `bat`, `ripgrep`) skip distro packaging and ship the same five channels listed above. Revisit when operator demand for native distro packages surfaces; `nfpm` / `cargo-deb` / `cargo-rpm` can generate packages later without rewriting the release pipeline.

**Rejected tooling:** Goreleaser (Go-first ecosystem); hand-rolled custom release scripts (maintenance burden).

---

### Q2. Authentication scope

The spec defers multi-user auth but mentions it as a future path. Do we stub `ApiKeyAuthenticator` in the initial implementation and schedule OIDC for a clearly marked phase later, or is single-user with no auth the endpoint for phase 1?

**Answer:**

**Multi-user authentication is in scope for Phase 1.** This requires spec revisions before implementation can proceed — tracked in `SPEC_BUILD.md` Open Work.

**Design summary:**

| Dimension | Choice |
|-----------|--------|
| **Identity store** | Local users; Argon2id password hashing; stored in SQLite. No external IdP dependency in Phase 1, preserving `wcon-vision` §6 zero-external-services. |
| **Browser session** | Cookie-based: HttpOnly, Secure, SameSite=Strict. Rotated on login. CSRF-protected on all state-changing endpoints. |
| **Programmatic access** | Named API tokens (bearer), scoped per user, revocable from the admin UI, hashed at rest, displayed once at creation. |
| **Authorization** | Three hierarchical roles — `admin` ⊃ `operator` ⊃ `viewer`. Maps to the operator / team-lead / auditor personas in `wcon-vision` §4. |
| **Ownership & visibility** | Profiles and sessions carry `owner_user_id`. Profiles have `visibility: private \| shared`. Shared profiles are readable by all operators and editable by owner + admins. |
| **Bootstrap** | First launch generates a one-time admin credential printed to stdout (or written to `$XDG_STATE_HOME/wacp-console/bootstrap-token`). Mandatory password change on first login. No default credentials, ever. |
| **Audit** | Append-only `audit_log` table captures `user_id`, `timestamp`, `action`, `target_kind`, `target_id`, `ip`, `user_agent` on every mutation. |
| **Rate limiting** | Per-IP and per-account login-attempt throttling with exponential-backoff lockout. |
| **Future OIDC path** | Second `Authenticator` implementation via the `openidconnect` crate. Trait shape is designed to accommodate it; not shipped in Phase 1. |

**Stack additions:**
- `argon2` — password hashing (Argon2id)
- `tower-sessions` + `tower-sessions-sqlx-store` — cookie session storage backed by SQLite
- `axum-csrf` or hand-rolled double-submit middleware — CSRF protection
- `subtle` — constant-time comparison (likely already transitive)
- `openidconnect` — deferred to the future OIDC authenticator

**Required spec revisions** (must land before `/impl-plan`):

1. **New spec: `wcon-auth`** — identity model, session model, authorization model, bootstrap flow, password policy, audit log schema, threat model.
2. **`wcon-architecture` §8** — promote multi-user auth from future path to in-scope; define the `Authenticator` trait with `LocalAuthenticator` as the shipped implementation.
3. **`wcon-data-model` §5** — new tables (`users`, `user_sessions`, `api_tokens`, `audit_log`, `login_attempts`); add `owner_user_id` and `visibility` columns to profile and session-launch records.
4. **`wcon-api`** — add `/v1/auth/*`, `/v1/users/*`, `/v1/tokens/*`, `/v1/audit-log` endpoint families; every existing endpoint gains authenticated-user context; extend the error taxonomy with `UNAUTHENTICATED`, `FORBIDDEN`, `PASSWORD_TOO_WEAK`, `ACCOUNT_LOCKED`.
5. **`wcon-profiles`** — ownership, visibility, ACL rules on operations.
6. **`wcon-sessions`** — launch identity, stream authorization (operators see own + shared, admins see all), highway gate participation authorization.
7. **`wcon-ui`** — login screen, user menu, permission-gated affordances, admin user-management screen, audit log viewer.

**Rejected alternative:** shipping single-user in Phase 1 and retrofitting multi-user later. Strictly worse: every table migration later grows an `owner_user_id`, every API contract changes under live users, and the frontend ends up with a login screen grafted onto single-user assumptions.

---

### Q3. Cryptographic trust boundary for gRPC TLS

Does the runtime present a TLS cert backed by a CA, a self-signed cert the Console must pin, or does gRPC run plaintext on localhost? Affects `runtime.grpc_address` format and reqwest's TLS config.

**Answer:**

**Three modes, selected by address scheme at connect time, fail-closed by default.**

| Mode | When | How |
|------|------|-----|
| **Plaintext** | Resolved address is loopback (`127.0.0.1`, `::1`, `localhost`) | Allowed implicitly for dev. Non-loopback plaintext is rejected unless `WCON_ALLOW_INSECURE_TLS=1` env var is set — a "yes I know what I'm doing" ritual. |
| **System trust store** | `https://` / `grpcs://` scheme, no explicit CA configured | `rustls-native-certs` pulls the OS CA bundle. Default for public-CA deployments. |
| **Explicit CA / pinned cert** | `runtime.tls_ca_pem` or `runtime.tls_pinned_cert_sha256` set | Supports internal CAs, self-signed pinning, and air-gapped deployments. PEM stored inline or as `@/path/to/file`. |

**Rationale:** forcing TLS on loopback adds dev friction with zero security gain (traffic never leaves the box); forcing a CA everywhere breaks self-signed and internal-CA deployments; forcing system trust excludes air-gapped deployments. Pattern mirrors `kubectl`, `etcdctl`, `psql`.

**`runtime` table additions** (revise `wcon-data-model` §5.2):

| Column | Type | Semantics |
|--------|------|-----------|
| `tls_mode` | enum: `auto \| system \| custom_ca \| pinned \| insecure_plaintext` | `auto` is the default; picks by scheme + loopback check. |
| `tls_ca_pem` | TEXT nullable | PEM bundle (inline or `@/path/to/file`). |
| `tls_pinned_cert_sha256` | TEXT nullable | Hex SHA-256 of server cert for exact-match pinning. |
| `tls_server_name` | TEXT nullable | SNI override for address-pinned connections. |
| `tls_client_cert_pem` | TEXT nullable | mTLS client cert (future). |
| `tls_client_key_pem` | `SecretString` | mTLS private key (future). |

**Startup ritual:** `insecure_plaintext` on a non-loopback address logs a bright warning and refuses to start without the `WCON_ALLOW_INSECURE_TLS=1` env var. `rustls` everywhere; no OpenSSL. Both `reqwest` and `tonic` feature-gate rustls.

---

### Q4. Embedded vs external frontend bundle

Should `rust-embed` be mandatory (single binary) or is an external `dist/` dir acceptable for development flexibility? Recommended: embed by default, `--frontend-path <dir>` flag override for dev convenience.

**Answer:**

**Confirmed:** embed by default via `rust-embed`, preserving the single-binary invariant from `wcon-vision` §6. A `--frontend-path <dir>` flag overrides to serve from disk for dev convenience. Axum serves embedded assets in release builds and from disk when the flag is present; release and dev paths share the same router setup so there is no dual-code-path drift.

---

### Q5. Telemetry opt-in policy

Does the Console ever phone home to Anthropic/WACP for usage telemetry? Recommendation per the zero-external-dependency principle: **never** by default, with a settable `telemetry.enabled` flag that is off unless the operator turns it on.

**Answer:**

**Confirmed:** the Console never phones home. `settings.telemetry.enabled` is off by default. When the operator explicitly enables it, the Console exports OTLP traces to an *operator-configured* endpoint only — no Anthropic/WACP-owned URL is ever hard-coded anywhere in the binary. Aligned with `wcon-vision` §6 zero-external-services constraint.

---

### Q6. Profile YAML schema versioning

The export format in `wcon-data-model` §8.1 has no `version:` field. When the format evolves (new fields, renamed fields), importers from old Consoles may fail. Should we add a `format_version: 1` key up front? Recommendation: yes, add now at version `1` so future migrations have a hook.

**Answer:**

**Confirmed:** add `format_version: 1` as a required top-level integer key in all profile exports now.

**Revise `wcon-data-model` §8.1** to document:
- `format_version` is required; absence is a validation error.
- Importers reject unknown *major* versions with a new error code `INVALID_FORMAT_VERSION`.
- Within a known version, unknown fields are preserved and flagged as import warnings (forward-compat).

**Revise `wcon-profiles` §7.3** so that the import flow documents the version check as the first validation step (before structural validation).

---

### Q7. License

Assumes Apache-2.0 or MIT to match the upstream WACP license. Needs confirmation.

**Answer:**

**Apache-2.0.** Matches `wacp/Cargo.toml`'s declared `license = "Apache-2.0"`.

Rationale:
1. **Upstream compatibility** — the Console depends on `wacp-taxonomy` and shared proto definitions via git dep; both crates should carry the same software license to avoid interop friction.
2. **Patent grant** — Apache-2.0's explicit patent grant is meaningful for a coordination-protocol component that touches agent invocation surfaces.
3. **Ecosystem alignment** — Tonic, Tokio, Axum, and most direct dependencies are Apache-2.0 (or dual-licensed with it).
4. **Not dual MIT+Apache** — the Rust-idiomatic dual license serves library crates consumed via Cargo. The Console ships as a binary application, so single Apache-2.0 is cleaner and matches upstream exactly.

**Upstream license inconsistency — resolved 2026-04-11:** The upstream `wacp` repo's root `LICENSE` (CC BY-SA 4.0) previously conflicted with its `Cargo.toml` (Apache-2.0), and 10 TypeScript `package.json` files declared no license at all (silently inheriting the root CC BY-SA 4.0 under npm conventions — a latent share-alike infection risk for any consumer of the `wacp-taxonomy` git-dep). Resolved via physical repo split in commit `ef20421`:

- **`github.com/Madahub-dev/wacp`** — reference implementation, uniformly Apache-2.0. Covers the Rust runtime (16 crates), TypeScript CLI + local SDK, Python SDK, highway UI, 7 ecosystem verticals, and 17 `impl/*.md` specs. All TypeScript `package.json` files now declare Apache-2.0 explicitly.
- **`github.com/Madahub-dev/wacp-protocol`** — new sibling repo, CC BY-SA 4.0. Contains only the protocol specification: `PROTOCOL.md`, `TAXONOMY.md`, and the `primitives/` / `foundations/` / `mechanisms/` / `topology/` spec folders (20 specs total). Extracted from upstream via `git filter-repo` with history preserved.

The Console's Apache-2.0 choice and rationale above are unchanged. The `wacp-taxonomy` git-dep is now unambiguously Apache-2.0 — the share-alike infection risk that motivated this advisory is gone. `SPEC_BUILD.md` "Upstream WACP License Clarification" is closed.

---

## 11. Summary table (the short version)

| Layer | Choice |
|-------|--------|
| Language (backend) | Rust (workspace, pinned via rust-toolchain.toml) |
| Async runtime | Tokio |
| HTTP framework | Axum 0.8 |
| gRPC client | Tonic 0.12 |
| REST client | reqwest 0.12 + rustls |
| Database | SQLite via sqlx 0.8 (compile-checked) |
| Migrations | sqlx migrate |
| Atomic index | arc-swap |
| Serialization | serde + serde_json + serde_yml |
| Upstream types | Git dep on `wacp-taxonomy` |
| API schema | utoipa → openapi.yaml → openapi-typescript |
| Logging | tracing + tracing-subscriber (JSON) |
| Errors | thiserror + anyhow |
| CLI | clap 4 derive |
| Language (frontend) | TypeScript 5 strict |
| Framework | React 19 |
| Bundler | Vite 6 |
| Routing | React Router 7 |
| Server state | TanStack Query v5 |
| UI state | Zustand 5 |
| Forms | React Hook Form + Zod |
| UI components | shadcn/ui + Radix + Tailwind 4 |
| Tables | TanStack Table v8 + Virtual |
| Code editor | CodeMirror 6 |
| Icons | Lucide React |
| Date | date-fns |
| Backend tests | cargo test + rstest + insta |
| Frontend tests | Vitest + RTL + MSW |
| E2E | Playwright |
| CI | GitHub Actions, 4-stage fail-fast pipeline |
| Package | Single binary with embedded frontend via rust-embed |
| Package manager | pnpm (frontend), Cargo workspace (backend) |

---

## 12. Next steps

Once §10 is answered:

1. Promote the decisions to `SPEC_BUILD.md` as a set of ADRs (ADR-002 backend stack, ADR-003 frontend stack, ADR-004 API contract generation, ADR-005 build & distribution, …) — one per cross-cutting decision, following the ADR-001 format.
2. Write the tech-stack summary into `IMPLEMENTATION.md` (new file — not yet in the repo).
3. Proceed to `/impl-plan` for the phased build plan.
4. Then `/setup-dev` to materialize the directory structure, lockfiles, and CI pipeline.

*WACP Console — Tech Stack Proposition, drafted 2026-04-11*
