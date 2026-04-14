---
id: wcon-architecture
type: design
status: final
created: 2026-04-09T00:00:00
revised: 2026-04-14T00:00:00
authors: [AAkil98]
tags: [architecture, system-design, core, dual-transport]
depends_on: [wcon-vision, wcon-glossary]
---

# WACP Console — System Architecture

## Table of Contents

1. Overview
2. System Boundary
3. Layer Diagram
4. Components
5. Data Flows
6. Persistence
7. Concurrency Model
8. Authentication and Authorization
9. Extension Points

---

## 1. Overview

The WACP Console is a two-tier application: a Rust backend (the console backend) and a browser-based SPA (the console frontend). The backend is the system's center of gravity — it owns persistence, manages the taxonomy index, orchestrates sessions, and proxies all communication with the WACP runtime. The frontend is a rendering and interaction layer that consumes the backend's API.

The architecture is shaped by one governing constraint: the Console is a **client** of the WACP runtime, not an extension of it. Every protocol operation — creating a workspace, delivering an envelope, resolving a gate — travels through the runtime's existing services. The Console connects to the runtime over four network endpoints — three gRPC services (each on its own Tonic server) and one REST gateway:

| Endpoint | Default address | Service |
|----------|-----------------|---------|
| gRPC | `[::1]:9090` | `AgentService` |
| gRPC | `[::1]:9091` | `HighwayService` |
| gRPC | `[::1]:9092` | `CoordinatorService` |
| REST | `http://[::1]:9093` | REST gateway — vertical manifest loading (`GET /v1/verticals[/{id}]`), per ADR-001 |

The Console adds no protocol-level behavior. It adds a product-level experience over the protocol's existing surface.

Three communication patterns define the system:

1. **Request-response** — the frontend issues REST calls to the backend for CRUD operations (profiles, session configuration, taxonomy queries). The backend either serves from its own state, forwards to the runtime via gRPC, or (for vertical manifest refresh) calls the runtime's REST API.

2. **Server-push** — the backend maintains WebSocket connections to the frontend for real-time events: trail entries, gate notifications, workspace state changes, tool-layer refusals, session lifecycle transitions. These originate from the runtime's gRPC streaming RPCs, are processed by the backend (which may synthesize derived channels like `refusals`), and relayed to the frontend.

3. **Client-streaming** — the frontend sends user actions (gate approvals, directive injections, escalation responses) through the backend, which translates them into gRPC calls against the HighwayService.

## 2. System Boundary

The Console's boundary is defined by what it owns versus what it delegates to the WACP runtime.

### Console owns

| Concern | Implementation |
|---------|----------------|
| Profile storage and lifecycle | SQLite (soft-delete semantics per `wcon-data-model` §3) + filesystem YAML export |
| Taxonomy indexing and querying | In-memory index built from two sources: protocol-taxonomy YAML files (base/derived roles, protocol-level envelope and checkpoint types) and vertical manifests fetched from the runtime's REST API (`wcon-discovery` §2.2, ADR-001) |
| Session configuration (pre-launch) | Backend state: vertical + workflow + profile-to-role bindings + vertical context (`wcon-sessions` §2.1) |
| Tool-layer refusal synthesis | Session monitor observes refusal trail entries and constructs `RefusalEvent`s (`wcon-highway` §4A) |
| User authentication and authorization | Backend middleware (see §8) |
| Frontend rendering and interaction | SPA served by backend |
| Real-time event relay | WebSocket bridge between runtime gRPC streams and frontend, including Console-synthesized channels (`refusals`, `session`, `notification`) |

### Runtime owns

| Concern | Console's relationship |
|---------|----------------------|
| Workspace lifecycle (create, state transitions, close) | Console requests via CoordinatorService; runtime executes |
| Envelope delivery and validation | Console submits via AgentService; runtime validates and delivers |
| Signal propagation and task state | Console observes via trail stream; runtime drives transitions |
| Trail recording and hash-chain integrity | Console reads the trail; runtime writes it |
| Gate enforcement and timeout fallbacks | Console resolves gates via HighwayService; runtime enforces timing |
| LLM execution and tool invocation | Entirely runtime-side; Console configures via profiles, never executes |
| Checkpoint creation and chain validation | Entirely runtime-side; Console reads checkpoints from trail |

### The bright line

The Console never:
- Accesses runtime internals outside of the two documented transports (gRPC for sessions/agents/highway, REST for vertical manifests — no shared memory, no direct database access, no filesystem coupling beyond the protocol taxonomy YAML files)
- Creates protocol events directly (every event originates from the runtime in response to a gRPC call)
- Mutates runtime state via REST — the REST transport is strictly `GET /v1/verticals[/{id}]`, never POST/PUT/DELETE
- Modifies protocol-taxonomy YAML files (read-only relationship)
- Reads vertical YAML files from the filesystem (ADR-001 — the runtime is the registry)
- Executes LLM calls or tool invocations (that is the runtime's responsibility, configured through profiles)
- Enforces tool-layer policies (the Console surfaces them as warnings and refusal events; enforcement is exclusively the runtime's responsibility)

## 3. Layer Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                      Console Frontend (SPA)                     │
│                                                                 │
│  ┌──────────────┐ ┌──────────────┐ ┌────────────┐ ┌──────────┐ │
│  │  Discovery    │ │   Profile    │ │  Session   │ │ Oversight│ │
│  │  Browser     │ │   Studio     │ │  Launcher  │ │ Dashboard│ │
│  └──────┬───────┘ └──────┬───────┘ └─────┬──────┘ └────┬─────┘ │
│         │                │               │              │       │
│         └────────────────┴───────┬───────┴──────────────┘       │
│                                  │                              │
│                        REST + WebSocket                         │
├──────────────────────────────────┼──────────────────────────────┤
│                      Console Backend (Rust)                     │
│                                  │                              │
│  ┌──────────────┐ ┌──────────────┤ ┌────────────┐ ┌──────────┐ │
│  │  Taxonomy    │ │   Profile    │ │  Session   │ │ Highway  │ │
│  │  Index       │ │   Store      │ │  Manager   │ │ Bridge   │ │
│  └──────┬───────┘ └──────┬───────┘ └─────┬──────┘ └────┬─────┘ │
│         │                │               │              │       │
│         │           SQLite + FS          │              │       │
│         │                                │              │       │
│         └────────────────────────────────┴──────────────┘       │
│                                  │                              │
│          gRPC clients (×3)       │      REST client             │
│     :9090  :9091  :9092          │         :9093                │
├──────────────────────────────────┼──────────────────────────────┤
│                       WACP Runtime (external)                   │
│                                  │                              │
│  ┌────────────┐ ┌────────────┐ ┌┴───────────┐ ┌─────────────┐  │
│  │ AgentSvc   │ │ HighwaySvc │ │CoordSvc    │ │ REST Gateway│  │
│  │ :9090      │ │ :9091      │ │:9092       │ │ :9093       │  │
│  └────────────┘ └────────────┘ └────────────┘ └─────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

Communication between layers:

| From → To | Protocol | Pattern |
|-----------|----------|---------|
| Frontend → Backend | REST (HTTP/JSON) | Request-response for CRUD, queries |
| Backend → Frontend | WebSocket (JSON frames) | Server-push for real-time events |
| Backend → Runtime (agents) | gRPC (protobuf) on `runtime.agent_address` (`[::1]:9090`) | Envelope delivery, checkpoint submission |
| Backend → Runtime (highway) | gRPC (protobuf) on `runtime.highway_address` (`[::1]:9091`) | Trail/gate/escalation streaming, gate resolution, injection |
| Backend → Runtime (coordinator) | gRPC (protobuf) on `runtime.coordinator_address` (`[::1]:9092`) | Session orchestration, task graph, workspace lifecycle |
| Backend → Runtime (vertical manifests) | REST (HTTP/JSON) on `runtime.rest_address` (`[::1]:9093`) | Request-response at startup and on manual taxonomy reload (ADR-001) |

## 4. Components

### 4.1 Console Backend

The backend is a single Rust binary composed of four services and a shared infrastructure layer.

#### Taxonomy Index

Builds and serves a queryable, in-memory representation of the WACP taxonomy from two upstream sources.

| Responsibility | Detail |
|----------------|--------|
| Load (protocol taxonomy) | Reads protocol-taxonomy YAML files from `taxonomy.path` at startup — base/derived roles, protocol-level envelope and checkpoint types |
| Load (verticals) | Calls `GET /v1/verticals` on the runtime's REST API and then `GET /v1/verticals/{id}` per listed vertical; deserializes each response into a `VerticalEntry` (`wcon-data-model` §6.1). Per ADR-001, the runtime is the authoritative vertical registry |
| Index | Structures data for fast lookup: roles by name/base, tools by vertical with policy metadata, protocol-level types, verticals with full extended-schema projection (context_schema, tool_policies, checkpoint_types, quality_criteria, task_types) |
| Query | Serves filtered, paginated queries from the discovery browser; per-vertical sub-endpoints (context-schema, task-types, etc.) for the session launcher |
| Reload | Rebuilds the index on demand (manual trigger via `POST /api/taxonomy/reload`). Both sources are refetched together — the reload is atomic per `wcon-discovery` §7.3 |
| Atomic swap | New index replaces old via `ArcSwap` (§7); readers never observe a partially-built state |

The taxonomy index holds no mutable user state. It is a read-only projection of upstream WACP data.

#### Profile Store

Manages the profile library — CRUD, versioning, validation, soft delete, and import/export.

| Responsibility | Detail |
|----------------|--------|
| Create/Update | Validates profile against taxonomy index — role exists, tools belong to the role's vertical (vertical-coarse per `wcon-discovery` §3.4), effective set is non-empty; emits non-blocking warnings for policy-gated tools (`TOOL_HAS_RUNTIME_POLICY`) and autonomous-worker profiles with write-capable tools. Persists to SQLite |
| Version | Each save creates a new version row; previous versions are retained; history is append-only (`wcon-data-model` §7.2) |
| Soft delete | `DELETE` sets `deleted_at` on all version rows; rows survive for FK integrity with historical session assignments. Undelete is not supported |
| Export | Serializes a profile to YAML on the filesystem |
| Import | Parses YAML, validates against taxonomy, inserts into SQLite. Succeeds with warnings; fails only on violations |
| Query | Lists live profiles (`deleted_at IS NULL`) with filtering (by role, by vertical, by tag), pagination |

#### Session Manager

Orchestrates the session lifecycle from configuration through teardown.

| Responsibility | Detail |
|----------------|--------|
| Configure | Accepts a session configuration: vertical, workflow, profile-to-role assignments (via Mode A stage-aware or Mode B role-aware slot derivation per `wcon-sessions` §2.4), vertical context tags (from the vertical's `context_schema`), and budget overrides |
| Validate | Checks that all role slots are filled, profiles are valid (including the vertical-coarse tool check), context required fields are present and typed correctly (`MISSING_CONTEXT` / `INVALID_CONTEXT`), budgets are within limits, runtime is reachable |
| Launch | Translates session configuration into gRPC calls: `CreateSession` on the coordinator, `Dispatch` per worker workspace with role bindings, `ResourceBudget`, and directive payloads derived from profiles. Directive payloads carry the session's vertical context as a pass-through field alongside `llm`/`tools`/`system_prompt` (`wcon-profiles` §4.2) |
| Monitor | Subscribes to four highway streams (trail, gates, escalations, workspace changes); aggregates into in-memory session state; updates session record on state transitions |
| Refusal synthesis | Detects tool-layer refusal trail entries (matching known error codes), constructs `RefusalEvent`s with policy metadata resolved from the taxonomy index, maintains `pending_refusals` list, clears refusals when prerequisite checkpoints appear or workspaces transition out of BLOCKED (`wcon-sessions` §6.3) |
| Teardown | On user request or session completion, records final state, releases gRPC streams, emits final `session` channel event |

The session manager is the most complex backend component. It bridges the user's mental model (a session with named profiles assigned to role slots and vertical context) and the runtime's model (a workspace tree with protocol-level bindings).

#### Highway Bridge

Proxies highway interactions between the frontend and the WACP HighwayService, plus synthesizes Console-layer channels for refusals, session lifecycle, and cross-cutting notifications.

| Responsibility | Detail |
|----------------|--------|
| Trail relay | Subscribes to `StreamTrail` via gRPC; filters, annotates (workspace labels, vertical-specific checkpoint field schemas), and pushes to connected frontends via WebSocket `trail` channel |
| Gate relay | Receives gate events from `StreamGates`; enriches with session context and vertical-specific rationale (`wcon-highway` §4.7); pushes to `gates` channel |
| Escalation relay | Receives events from `StreamEscalations`; enriches with session context; pushes to `escalations` channel |
| Workspace change relay | Receives events from `StreamWorkspaceChanges`; updates in-memory workspace state; pushes to `workspaces` channel; triggers refusal classification on BLOCKED transitions |
| Refusal synthesis | Detects tool-layer refusal trail entries, builds `RefusalEvent`s (§Session Manager), emits on the synthesized `refusals` channel (`wcon-highway` §4A) |
| Session lifecycle synthesis | Aggregates workspace state to derive session-level transitions (`session_active`, `session_completed`, `session_failed`, `session_cancelled`); emits on the synthesized `session` channel |
| Notification synthesis | Combines cross-cutting events (new gate, gate timeout warning, new escalation, new refusal, runtime disconnect) into a single `notification` channel feeding the frontend nav badge and toast system |
| Action proxy | Receives user actions from frontend (gate approvals, escalation responses, directive injections); translates to `RespondToGate`, `RespondToEscalation`, `InjectEnvelope` gRPC calls |

The highway bridge is the real-time backbone of the oversight dashboard. It maintains **four** gRPC streams per active session (trail, gates, escalations, workspace changes) and fans out to all connected frontends observing that session via seven WebSocket channels (`trail`, `gates`, `escalations`, `refusals`, `workspaces`, `session`, `notification`).

#### Infrastructure Layer

Shared concerns available to all services:

| Concern | Implementation |
|---------|----------------|
| HTTP server | Serves REST endpoints and static frontend assets |
| WebSocket server | Manages persistent connections for real-time push |
| gRPC client pool | Three persistent Tonic channels — one per runtime gRPC service (`runtime.agent_address` default `[::1]:9090`, `runtime.highway_address` default `[::1]:9091`, `runtime.coordinator_address` default `[::1]:9092`). Each channel handles automatic reconnection with exponential backoff independently. Per-service health is tracked and surfaced through `/api/health` |
| REST client | HTTP client for the runtime's REST gateway (`runtime.rest_address`, default `http://[::1]:9093`); used by the Taxonomy Index at startup and on reload for `GET /v1/verticals[/{id}]`. Does **not** issue non-GET requests — the REST transport is read-only (see §2) |
| Configuration | Loads from SQLite `settings` table with defaults per `wcon-data-model` §5.2 |
| Logging | Structured JSON logging (stdout) |
| Health | Readiness probe (backend up + database ready + taxonomy index built). Liveness probe (backend responsive). `GET /api/health` exposes per-service runtime reachability (AgentService, HighwayService, CoordinatorService, REST gateway) — any unreachable service is a `degraded` state, not `unhealthy` (`wcon-api` §11.1) |

### 4.2 Console Frontend

The frontend is a single-page application served as static assets by the backend.

#### Discovery Browser

Presents the taxonomy index as a browsable, searchable catalog.

| Capability | Detail |
|------------|--------|
| Role browser | Lists base and derived roles grouped by vertical; shows capabilities, vertical membership, and the role's vertical tool list (per `wcon-discovery` §3.4 vertical-coarse mapping) |
| Tool browser | Lists tools grouped by vertical; shows name, description, owning vertical, and policy indicator (lock icon) for tools with a `ToolEntry.policy`. Tool input schemas live in the runtime and are not displayed by the Console (they are not in the manifest) |
| Type browser | Three sections: envelope types (protocol-level), checkpoint types (protocol-level), vertical-specific checkpoint types grouped by vertical with field schemas |
| Vertical browser | Lists verticals with defining constraint; per-vertical detail shows name, defining_constraint, context_schema (with typed fields), tool_policies, vertical-specific checkpoint types, quality_criteria, task_types, workflows (with stage/gate counts), default_profiles, and tools |
| Search | Full-text search across all indexed entities including vertical-specific checkpoint types, context fields, and tool policies |
| Detail views | Drill-down panels showing full definition for any entity |

#### Profile Studio

Two-part interface: the profile editor (create/edit form) and the profile library (list/manage saved profiles).

| Capability | Detail |
|------------|--------|
| Editor | Form-based profile configuration: role selector (populated from taxonomy), LLM settings, autonomy preset, tool allowlist/denylist (filtered to the role's owning vertical's full tool set, with lock indicators for policy-gated tools), budget fields |
| Validation | Real-time feedback as the user edits — invalid role, tool outside the role's vertical, budget out of range. Policy-gated tools are saved with a non-blocking warning banner (`TOOL_HAS_RUNTIME_POLICY`); autonomous-worker profiles with write-capable tools surface a non-blocking caution |
| Library | Paginated list of live profiles (soft-deleted filtered out) with filtering, version history, clone, delete (soft) |
| Import/Export | File upload for YAML import; download button for YAML export. Import surfaces non-blocking warnings (new runtime policies, etc.) in the detail view after save |

#### Session Launcher

Six-step wizard for creating and launching a session (see `wcon-ui` §6.2 for UI detail and `wcon-sessions` §2.1 for behavior).

| Step | Detail |
|------|--------|
| 1. Select vertical | Card list populated from vertical registry; each card shows name, defining_constraint, and summary counts |
| 2. Select workflow | Workflow cards from the selected vertical's `WorkflowSummary` list; each card shows description, stage count, and gated stage count |
| 3. Assign profiles | For each role slot (Mode A stage-aware or Mode B role-aware per `wcon-sessions` §2.4), assign a profile from the library or create one inline |
| 4. Vertical context | Dynamically generated form from the vertical's `context_schema`; skipped when empty (e.g., SWE); required fields block progression |
| 5. Set overrides | Optional budget overrides at session or per-assignment level; MLOps compute budget flows through context (step 4), not overrides |
| 6. Review and launch | Summary view showing vertical, workflow, context, assignments, budgets; launch button fires the session |

#### Oversight Dashboard

Real-time monitoring and control surface for active sessions.

| Capability | Detail |
|------------|--------|
| Session header | Session name, state, elapsed time, and vertical context badges derived from `session.context` (e.g., `[finance]` `[scope=equities]` `[jurisdiction=SEC]`) |
| Trail stream | Scrolling feed of trail entries, filtered by event type, workspace, severity, vertical checkpoint type, or refusal code. Vertical-specific checkpoint creations are rendered with a structured field table from the indexed `CheckpointSchema.fields`; tool-layer refusal entries are rendered with a red left border |
| Workspace view | Visual representation of the workspace tree with current state per workspace; refusal badge on workspaces blocked by tool-layer refusal |
| Task view | Task DAG visualization showing status, dependencies, and progress |
| Gate queue | Ordered list of pending gates with context, vertical rationale subtitle (from `wcon-highway` §4.7), approve/reject controls, timeout countdown |
| Escalation inbox | List of active escalations with reason, agent context, and response controls |
| Refusal panel | List of `pending_refusals` (`wcon-highway` §4A) with workspace, tool, error code, and unblock hint; navigation-only actions (no "resolve refusal" button — the Console never resolves refusals directly) |
| Quality report panel | End-of-session rendering of per-criterion verdicts (pass/warn/fail) from the vertical's `quality_criteria`, sourced from a trail entry emitted by the vertical's autonomous observer |
| Injection panel | Form for sending directives or feedback envelopes to any workspace in the session |

## 5. Data Flows

### 5.0 Vertical Registry Load (startup / reload)

```
Backend                    Filesystem              WACP Runtime (REST)
   │                           │                         │
   │── read protocol tax. ───▶│                         │
   │◀── YAML content ────────│                         │
   │── parse → index         │                         │
   │                           │                         │
   │── GET /v1/verticals ─────────────────────────────▶│
   │◀── VerticalSummary[] ─────────────────────────────│
   │                           │                         │
   │  ┌─ per summary ────────────────────────────────┐  │
   │  │ GET /v1/verticals/{id} ─────────────────────▶│  │
   │  │◀── VerticalManifest ─────────────────────────│  │
   │  │  project into VerticalEntry + populate       │  │
   │  │  ToolEntry.policy, CheckpointSchema.req_by   │  │
   │  └────────────────────────────────────────────────┘
   │                           │                         │
   │  ArcSwap: new index replaces old atomically        │
```

Happens at startup and on `POST /api/taxonomy/reload`. Both sources (filesystem protocol taxonomy + REST vertical manifests) are fetched together; the new index either replaces the old atomically or is discarded on fatal failure. Per ADR-001 (`SPEC_BUILD.md`), the Console does not read vertical manifests from its own filesystem — the runtime is the authoritative vertical registry.

### 5.1 Discovery Query

```
Frontend                    Backend                     Taxonomy Index (in-memory)
   │                           │                            │
   │── GET /api/roles ────────▶│                            │
   │                           │── lookup ──────────────────▶│
   │                           │◀── RoleEntry list ─────────│
   │◀── JSON role list ───────│                            │
```

Entirely local to the Console. The taxonomy index is pre-built (§5.0); queries never hit the WACP runtime or the filesystem.

### 5.2 Profile Save

```
Frontend                    Backend                     SQLite
   │                           │                          │
   │── POST /api/profiles ────▶│                          │
   │                           │── validate vs taxonomy   │
   │                           │── INSERT profile ───────▶│
   │                           │◀── row ID ──────────────│
   │◀── 201 Created ─────────│                          │
```

Validation happens in the backend against the taxonomy index before persistence. Invalid profiles are rejected with specific error messages.

### 5.3 Session Launch

```
Frontend      Backend             CoordinatorService      AgentService      HighwayService
   │             │                        │                    │                 │
   │── POST ────▶│                        │                    │                 │
   │  launch     │                        │                    │                 │
   │             │── CreateSession ──────▶│ (coordinator ws)   │                 │
   │             │◀── session_id, ws_id ─│                    │                 │
   │             │                        │                    │                 │
   │             │── SubmitGoal ─────────▶│ (task graph root)  │                 │
   │             │◀── ack ───────────────│                    │                 │
   │             │                        │                    │                 │
   │             │  ┌─ per assignment (role slot) ─────────────────────────┐    │
   │             │  │ Dispatch ───────────▶│ (worker ws created)            │    │
   │             │  │  role, budget, task  │                                 │    │
   │             │  │◀── workspace_id ───│                                 │    │
   │             │  │                      │                                 │    │
   │             │  │ SendEnvelope ────────┼───────────────────▶│ directive │    │
   │             │  │  (directive payload: llm, tools,           │ with       │    │
   │             │  │   system_prompt, context passthrough)      │ context    │    │
   │             │  │◀── ack ─────────────┼─────────────────────│            │    │
   │             │  └─────────────────────────────────────────────────────────┘    │
   │             │                        │                    │                  │
   │             │── StreamTrail ────────────────────────────────────────────────▶│
   │             │── StreamGates ────────────────────────────────────────────────▶│
   │             │── StreamEscalations ──────────────────────────────────────────▶│
   │             │── StreamWorkspaceChanges ─────────────────────────────────────▶│
   │             │                        │                    │                  │
   │◀── 202 ────│                        │                    │                  │
   │◀── WebSocket events (ongoing) ──────│                    │                  │
```

This is the heaviest flow. The backend translates a single user action (launch session) into a sequence of gRPC calls that create the workspace tree and deliver directives. The directive envelope carries the session's vertical context as a pass-through field alongside `llm`/`tools`/`system_prompt`. Once launched, the backend subscribes to four gRPC streams and relays events to the frontend via WebSocket channels.

### 5.4 Gate Resolution

```
Frontend              Backend               HighwayService
   │                     │                       │
   │◀── gate event ─────│◀── stream event ─────│
   │                     │                       │
   │── POST ────────────▶│                       │
   │  /api/gates/:id     │                       │
   │  {action: approve}  │                       │
   │                     │── ResolveGate ───────▶│
   │                     │◀── ack ─────────────│
   │◀── 200 OK ────────│                       │
   │                     │                       │
   │◀── trail entry ────│◀── stream (ws resumed)│
```

Gate events arrive via the streaming subscription established at session launch. The user's approval travels back through the same path in reverse.

### 5.5 Directive Injection

```
Frontend              Backend               HighwayService
   │                     │                       │
   │── POST ────────────▶│                       │
   │  /api/inject        │                       │
   │  {ws_id, payload}   │                       │
   │                     │── InjectDirective ───▶│
   │                     │◀── ack ─────────────│
   │◀── 200 OK ────────│                       │
   │                     │                       │
   │◀── trail entry ────│◀── stream (new entry)─│
```

## 6. Persistence

### Storage engine: SQLite

The Console uses SQLite as its single relational store. Rationale:

- **No external database process** — aligns with the zero-external-dependency goal (`wcon-vision` §6)
- **Sufficient scale** — the Console manages hundreds to low thousands of profiles and session records, not millions
- **Transactional** — profile versioning and session state updates benefit from ACID guarantees
- **Portable** — a single database file that can be backed up, copied, or reset trivially

### What lives in SQLite

| Entity | Key fields | Notes |
|--------|------------|-------|
| Profile | id, name, role_ref, llm_config, autonomy, tools, budget, tags, version, owner_user_id, visibility | Versioned: each save creates a new row with incremented version. Owned by creating user |
| Session record | id, vertical, workflow, profile_assignments, state, created_at, closed_at, owner_user_id | Historical record of all sessions; active sessions also held in memory. Owned by launching user |
| User | id, username, password_hash, console_role, created_at, must_change_password | Local identity store; Argon2id hashing (`wcon-auth` §3) |
| Browser session | session_id, user_id, created_at, expires_at, ip, user_agent | Cookie-based auth sessions (`wcon-auth` §5) |
| API token | token_id, user_id, name, token_hash, created_at, last_used_at | Bearer tokens for programmatic access (`wcon-auth` §6) |
| Audit log | id, user_id, timestamp, action, target_kind, target_id, ip, user_agent | Append-only mutation record (`wcon-auth` §10) |
| Login attempt | id, identifier, ip, attempted_at, success | Rate-limiting and lockout tracking (`wcon-auth` §9) |
| Settings | key, value | Console-level configuration (runtime addresses, taxonomy path, UI preferences) |

### What lives on the filesystem

| Artifact | Location | Notes |
|----------|----------|-------|
| Profile YAML exports | Configured export directory | User-triggered, not automatic |
| Protocol-taxonomy YAML files | Configured path (read-only) | Owned by WACP, not the Console |
| SQLite database file | Configured data directory | Single file: `console.db` |

Vertical manifests are **not** on the Console's filesystem — they are fetched from the runtime's REST API at startup and on reload (ADR-001). The Console never persists a local copy.

### What lives in memory only

| Data | Lifetime | Rebuilt from |
|------|----------|-------------|
| Taxonomy index | Process lifetime | Protocol taxonomy YAML files + runtime REST responses (`GET /v1/verticals[/{id}]`) on startup or manual reload |
| Active session state | Session lifetime | Runtime trail stream (recovered on reconnect) |
| Pending refusals | Session lifetime | Synthesized from trail entries matching refusal codes |
| WebSocket connection state | Connection lifetime | N/A — ephemeral |
| gRPC stream subscriptions | Session lifetime | Re-established on reconnect |

## 7. Concurrency Model

The backend is an async Rust application built on Tokio.

### Task structure

| Tokio task | Count | Purpose |
|------------|-------|---------|
| HTTP server | 1 | Accepts REST requests and serves static assets |
| WebSocket acceptor | 1 | Accepts WebSocket upgrade requests |
| WebSocket writer | 1 per frontend connection | Sends real-time events to a connected frontend |
| gRPC StreamTrail subscriber | 1 per active session | Reads trail entries for a session |
| gRPC StreamGates subscriber | 1 per active session | Reads gate events for a session |
| gRPC StreamEscalations subscriber | 1 per active session | Reads escalation events for a session |
| gRPC StreamWorkspaceChanges subscriber | 1 per active session | Reads workspace state changes for a session |
| Session monitor | 1 per active session | Aggregates events from all four stream subscribers, derives session-level state, detects refusals from trail entries, fans out to WebSocket writers, updates SQLite on state transitions |

### Channel topology

```
StreamTrail ─────────▶ ┌─────────────────┐ ──▶ WebSocket writer (frontend A)
StreamGates ─────────▶ │ Session Monitor │ ──▶ WebSocket writer (frontend B)
StreamEscalations ───▶ │                 │
StreamWorkspaceChanges▶│  - refusal      │ ──▶ Session record (SQLite)
                       │    synthesis    │
                       │  - session      │
                       │    lifecycle    │
                       │  - notification │
                       │    generation   │
                       └─────────────────┘
                               │
                               ├──▶ trail channel
                               ├──▶ gates channel
                               ├──▶ escalations channel
                               ├──▶ refusals channel (synthesized)
                               ├──▶ workspaces channel
                               ├──▶ session channel (synthesized)
                               └──▶ notification channel (synthesized)
```

Each active session has one session monitor task that receives from four gRPC streams and fans out to N WebSocket writers (one per connected frontend observing that session). The monitor synthesizes three additional channels (`refusals`, `session`, `notification`) from the four gRPC-sourced channels. The fan-out uses a broadcast channel. Slow consumers are dropped with a warning — the trail is the authoritative record; the frontend can reconnect and catch up.

### Concurrency boundaries

- **SQLite access** is serialized through a single connection with WAL mode enabled. Write operations (profile save, session record update) are infrequent; read operations (profile list, taxonomy query) tolerate WAL-mode read concurrency.
- **Taxonomy index** is read-only after construction. Reload creates a new index atomically and swaps it in via `ArcSwap`. No locking on read path.
- **gRPC client pool** is three independent Tonic channels — one to each runtime gRPC service (`agent_address`, `highway_address`, `coordinator_address`). Each channel is shared across concurrent requests via Tonic's built-in HTTP/2 stream multiplexing. Per-service reconnection means a HighwayService outage does not disrupt AgentService calls.
- **REST client** is a single `reqwest` (or equivalent) HTTP client to `runtime.rest_address`, used only during taxonomy index build. No persistent state; connections pooled by the HTTP client.

## 8. Authentication and Authorization

Multi-user authentication and authorization ship in Phase 1. The full specification lives in `wcon-auth`; this section defines how auth integrates into the architecture's request pipeline and component model.

### 8.1 Request Pipeline

Every HTTP request passes through two middleware layers before reaching a handler:

```
Request → Authenticator → Authorizer → Handler → Response
            │                 │
            │ 401             │ 403
            ▼                 ▼
         Reject            Reject
```

1. **Authenticator** extracts a user identity from the request. Accepts two credential types: a `wcon_sid` cookie (browser session) or an `Authorization: Bearer wcon_t_...` header (API token). Both resolve to the same internal representation: `AuthenticatedUser { user_id, username, console_role }`. Unauthenticated requests receive `401`. See `wcon-auth` §3 for the full authentication specification.

2. **Authorizer** checks whether the authenticated user may perform the requested action given their console role (`admin`, `operator`, `viewer`). The permission matrix is defined in `wcon-auth` §4.2. Unauthorized requests receive `403`.

Both are implemented as pluggable traits to support future authentication mechanisms (OIDC — `wcon-auth` §12) without restructuring the pipeline.

### 8.2 Authenticator Trait

```
trait Authenticator {
    async fn authenticate(&self, request: &Request) -> Result<AuthenticatedUser, AuthError>;
}
```

Phase 1 ships `LocalAuthenticator`:
- Validates cookie-based browser sessions against the `user_sessions` table (`wcon-data-model` §5).
- Validates bearer API tokens against the `api_tokens` table (`wcon-data-model` §5).
- Produces `AuthenticatedUser` on success; `AuthError::Unauthenticated` on failure.

The trait is async to accommodate a future `OidcAuthenticator` that performs network calls to an external IdP. The `LocalAuthenticator` resolves from local SQLite — no network, no blocking.

### 8.3 Authorizer Trait

```
trait Authorizer {
    fn authorize(&self, user: &AuthenticatedUser, action: &Action) -> Result<(), AuthzError>;
}
```

Phase 1 ships `RoleAuthorizer`:
- Implements the three-level console role hierarchy: `admin` ⊃ `operator` ⊃ `viewer`.
- Each API endpoint declares its required minimum console role. The authorizer compares the user's role against the requirement.
- Ownership checks (e.g., "operator can only modify own profiles") are part of the action context passed to the authorizer, not hardcoded per-endpoint.

### 8.4 CSRF Protection

State-changing requests (POST, PUT, PATCH, DELETE) from cookie-authenticated sessions require a CSRF token via the double-submit cookie pattern (`wcon-auth` §8). API token requests are exempt — bearer tokens are not automatically attached by browsers.

The CSRF check is a third middleware layer, inserted between the authenticator and the authorizer, active only when the authenticator identifies a cookie-based session.

### 8.5 Rate Limiting

The login endpoint (`POST /api/auth/login`) is rate-limited per-IP and per-account before authentication is attempted (`wcon-auth` §9). Rate limiting is implemented as a pre-authentication middleware layer on the login route only — it does not affect the general request pipeline.

### 8.6 Runtime Authentication

The Console authenticates to the WACP runtime on both transports. This is orthogonal to user authentication — all Console users share the same runtime credentials.

- **gRPC transport:** each of the three gRPC client channels attaches the credential (from `runtime.auth_credential` in `wcon-data-model` §5.2) to outgoing metadata on every RPC via a shared interceptor.
- **REST transport:** the same credential is sent as `Authorization: Bearer <credential>` on `GET /v1/verticals[/{id}]` requests when `runtime.auth_method` is set.

The runtime supports multiple authenticator types (PSK, API key, OAuth, mTLS). The Console's runtime credentials are configured in the `runtime.auth_method` and `runtime.auth_credential` settings. Per-user runtime credential mapping is not supported — the Console connects as a single service identity.

### 8.7 Audit Integration

Every state-changing handler, after successful authorization, writes an audit log entry recording the actor (`user_id`), action, target, and request metadata (`ip`, `user_agent`). The audit log is append-only and admin-readable. See `wcon-auth` §10 for the full audit specification.

### 8.8 Component Impact

Auth adds the following to the component model (`wcon-architecture` §4):

| Component | Role |
|-----------|------|
| **Auth Service** | Manages users, browser sessions, API tokens, login attempts. Owns the `users`, `user_sessions`, `api_tokens`, `login_attempts` tables. Provides the `LocalAuthenticator` and `RoleAuthorizer` implementations. |
| **Audit Service** | Writes and queries the `audit_log` table. Called by all other services after mutations. |

Both are backend services following the same pattern as existing services (§4): a service struct with access to shared infrastructure, exposed through REST endpoints registered in the HTTP router.

The Profile Store and Session Manager gain ownership-awareness: queries filter by `owner_user_id` and `visibility` based on the authenticated user's identity and console role.

## 9. Extension Points

### Vertical pluggability

The Console discovers verticals by calling `GET /v1/verticals` on the runtime's REST API at startup and on manual taxonomy reload. The runtime is the authoritative registry (ADR-001): adding a new vertical means adding its manifest to the runtime's ecosystem directory and restarting the runtime; the Console picks up the change on its next reload — no Console redeploy, no Console-side filesystem coordination, no code changes.

The taxonomy index rebuilds on manual reload, picking up new verticals, their extended-schema fields (context_schema, tool_policies, checkpoint_types), and their derived roles, tools, and types. The discovery browser, session launcher, profile studio, and oversight dashboard all read from the taxonomy index and adapt to whatever verticals are present — this is the manifest-driven rendering invariant (`wcon-ui` §12.6).

**Forward compatibility.** When the runtime adds new manifest fields (a new `ContextField` type, a new `ToolPolicyKind` variant, new fields on existing structs), the Console's Rust deserializer ignores unknown fields and preserves unknown enum values as opaque strings. The raw manifest is stored in `VerticalEntry.raw_manifest` for debug/search purposes. The Console's typed projection can catch up in a later release without breaking startup — new verticals appear correctly even if the Console doesn't yet understand every field.

### Authenticator and authorizer traits

Both user authentication and authorization are implemented as pluggable traits (§8.2, §8.3). Phase 1 ships `LocalAuthenticator` (local user store + Argon2id) and `RoleAuthorizer` (three-level console role hierarchy). New authentication mechanisms (OIDC — `wcon-auth` §12) can be added by implementing the `Authenticator` trait. New authorization models can be added by implementing the `Authorizer` trait.

### Frontend surface extensibility

The frontend is composed of four surfaces (discovery browser, profile studio, session launcher, oversight dashboard). Each surface is a self-contained module consuming the backend REST/WebSocket API. Adding a new surface (e.g., a workflow designer, a cost analytics view) means adding a new frontend module that consumes existing or new API endpoints — existing surfaces are not modified.

### Backend service extensibility

New backend services follow the same pattern as existing ones: a service struct with access to the shared infrastructure (database, taxonomy index, gRPC client pool), exposed through REST endpoints registered in the HTTP router. The infrastructure layer is not coupled to any specific service.

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-vision | Product Vision | constrains scope and boundary (§2, §6, §7) |
| wcon-glossary | Glossary | informs all terminology including `vertical-coarse tool mapping`, `role slot derivation`, `refusal event` |
| wcon-discovery | Agent & Role Discovery | defines taxonomy ingestion pipeline (§2.2) that the Taxonomy Index component implements |
| wcon-auth | Authentication & Authorization | defines identity model, auth pipeline, permission matrix, audit log — §8 of this spec defers to it |
| wcon-data-model | Data Model | defines SQLite schemas and taxonomy index schema (§6.1) that the Profile Store and Session Manager components implement |
| wcon-profiles | Profile System | defines validation and directive payload semantics the Profile Store and Session Manager implement |
| wcon-sessions | Session Lifecycle | defines session state machine, Mode A/B slot derivation (§2.4), refusal detection (§6.3) that the Session Manager implements |
| wcon-highway | Highway Integration | defines the four gRPC streams and synthesized channels the Highway Bridge implements |
| wcon-api | API Surface | consolidates the contract the frontend consumes |
| wcon-ui | UI Design | defines the four frontend surfaces and the manifest-driven rendering invariant (§12.6) |
| wacp-protocol | WACP Protocol Specification | defines runtime gRPC services and protocol behavior |
| wacp-taxonomy | WACP Taxonomy crate | defines `VerticalManifest` struct served over REST |
| wacp-transport | WACP Transport crate | defines REST handlers for `GET /v1/verticals[/{id}]` |
| SPEC_BUILD.md | Project build log | records ADR-001 (runtime as vertical registry) cited throughout this spec |

*WACP Console -- authored by AAkil98*
