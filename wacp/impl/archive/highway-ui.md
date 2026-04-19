# WACP Implementation: Highway UI

> **Superseded 2026-04-19** by the `wacp-console` oversight dashboard (W4, per `wcon-vision`). The standalone highway-ui TypeScript SPA was retired and its subtree deleted; the console's oversight surface now covers trail, gates, escalations, refusals, workspace tree, and directive injection across 7 WebSocket channels. Rationale + decision captured in `tech-debt-2026-04-18.md` §3.1 A.2 and `impl/closeout-plan.md` §3.1 P1.2.
>
> This spec is preserved for historical reference — its design of connect-web + gRPC-Web + protobuf-over-http transport informed the console's REST+WS translation layer.

```yaml
id: wacp-impl-highway-ui
type: implementation-spec
status: superseded
created: 2026-03-20
superseded: 2026-04-19
superseded_by: wcon-vision (W4 — console oversight dashboard)
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §8 (human highway)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-protocol-interface
  - wacp-spec-human-highway
  - wacp-spec-user
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, highway, typescript, ui, gates, escalation, visibility, superseded]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Architecture Overview](#2-architecture-overview)
3. [Technology Stack](#3-technology-stack)
4. [gRPC-Web Transport Layer](#4-grpc-web-transport-layer)
5. [Connection Lifecycle](#5-connection-lifecycle)
6. [State Management](#6-state-management)
7. [Visibility — Trail Stream](#7-visibility--trail-stream)
8. [Gate Management](#8-gate-management)
9. [Escalation Handling](#9-escalation-handling)
10. [Envelope Injection](#10-envelope-injection)
11. [Workspace and Task Views](#11-workspace-and-task-views)
12. [Autonomy Preset Configuration](#12-autonomy-preset-configuration)
13. [Authentication](#13-authentication)
14. [Error Handling](#14-error-handling)
15. [Component Hierarchy](#15-component-hierarchy)
16. [Testing Strategy](#16-testing-strategy)
17. [References](#17-references)

## 1. Purpose

This spec defines how the WACP highway becomes a TypeScript application. It answers "how does the human interact with the protocol" — not "what the highway can do" (that's the human-highway spec's job) or "what crosses the wire" (that's the protocol-interface spec's job).

The highway UI is the human's window into the runtime. Through it, the human exercises the four highway capabilities: **visibility** (live trail stream), **gates** (approve/reject/modify transitions), **injection** (send envelopes to any workspace), and **escalation handling** (respond to agents that need human input). The UI does not enforce protocol rules — the runtime does that. The UI renders state, captures intent, and relays decisions.

**Scope.** The TypeScript client application that connects to the runtime's `HighwayService` (port 9091). gRPC-Web transport and connection management. Client-side state management for real-time streams. UI components for trail viewing, gate resolution, escalation response, envelope composition, workspace inspection, and task graph visualization. Authentication flow. Autonomy preset configuration.

**Not in scope.** Runtime internals (runtime spec). Wire format and protobuf definitions (protocol-interface spec). Protocol semantics — gate mechanics, timeout behavior, fallback logic, trail recording (human-highway spec). Agent SDK surface (sdk-agent spec). The UI renders what the runtime provides and sends what the protocol accepts; it does not implement protocol logic.

**Design constraint.** The UI is a lens, not a governor. It shapes how the human sees the system but does not change what the system does (human-highway spec, §7). Every UI action maps to exactly one `HighwayService` RPC. No client-side protocol logic — no local gate timeout tracking, no client-side permission checks, no optimistic state transitions. The runtime is the single source of truth; the UI reflects it.

## 2. Architecture Overview

The highway UI is a single-page web application that connects to the runtime over gRPC-Web. It is a pure client — no application server, no backend-for-frontend, no server-side rendering. The runtime's `HighwayService` is the only backend.

**Component architecture:**

```
┌─────────────────────────────────────────────────────┐
│                   Highway UI (Browser)               │
│                                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐           │
│  │  Trail    │  │  Gate    │  │ Escalation│           │
│  │  Viewer   │  │  Panel   │  │  Panel    │           │
│  └────┬─────┘  └────┬─────┘  └────┬──────┘           │
│       │              │              │                  │
│  ┌────┴──────────────┴──────────────┴──────┐          │
│  │            State Store                   │          │
│  └────────────────┬─────────────────────────┘          │
│                   │                                    │
│  ┌────────────────┴─────────────────────────┐          │
│  │         gRPC-Web Transport Layer          │          │
│  └────────────────┬─────────────────────────┘          │
└───────────────────┼─────────────────────────────────┘
                    │ gRPC-Web (HTTP/2 or HTTP/1.1+framing)
                    │
┌───────────────────┼─────────────────────────────────┐
│  Envoy / grpc-web proxy (if needed)                  │
└───────────────────┼─────────────────────────────────┘
                    │ gRPC (HTTP/2)
                    │
┌───────────────────┼─────────────────────────────────┐
│          WACP Runtime — HighwayService :9091          │
└─────────────────────────────────────────────────────┘
```

**Three layers:**

1. **Transport layer.** Manages the gRPC-Web connection, protobuf serialization/deserialization, stream lifecycle, and reconnection. Wraps every `HighwayService` RPC in a typed TypeScript function. Components never touch raw protobuf — they call typed functions and receive typed objects.

2. **State store.** Holds all client-side state derived from server streams: trail entries, pending gates, active escalations, workspace snapshots, task graph. The store is append-only for trail data (new entries arrive, old entries are never modified) and replace-on-update for entity state (workspace views, task graph). Components subscribe to store slices; the store drives re-renders.

3. **UI components.** Render state and capture user actions. Each component maps to one highway capability: trail viewer (visibility), gate panel (gates), escalation panel (escalation handling), injection form (injection), workspace view (read queries), task graph view (read queries). Components emit actions that the transport layer translates to RPCs.

**Data flow is unidirectional.** Server streams flow into the state store. Components read from the store. User actions flow through the transport layer to the runtime. The runtime's response arrives as a stream event, updating the store, which triggers a re-render. The UI never mutates state directly — every state change originates from the server.

**No server-side component.** The UI is static files (HTML, JS, CSS) served from any web server or CDN. The only dynamic backend is the WACP runtime itself. This keeps the deployment simple — the highway UI is a build artifact, not a running service. An Envoy proxy or the runtime's built-in gRPC-Web support bridges the browser to the gRPC backend.

## 3. Technology Stack

| Layer | Choice | Reasoning |
|-------|--------|-----------|
| Language | TypeScript (strict mode) | Type safety for protobuf-generated types. Consistent with D-001 (IMPLEMENTATION.md). |
| UI framework | React 19 | Component model maps cleanly to highway capabilities. Concurrent rendering for high-frequency trail updates. |
| Build tool | Vite | Fast dev server, ESM-native, minimal configuration. |
| gRPC-Web | `@connectrpc/connect-web` | First-class TypeScript codegen from `.proto` files. Supports both unary and server-streaming RPCs. Uses the Connect protocol (superset of gRPC-Web) — works without Envoy in development via Connect's HTTP/1.1 fallback. |
| Protobuf codegen | `@bufbuild/protobuf` + `@connectrpc/protoc-gen-connect-es` | Generates idiomatic TypeScript types and service clients from the same `.proto` files used by the Rust runtime. Single source of truth for message shapes (protocol-interface spec, §1). |
| State management | Zustand | Lightweight, no boilerplate, works with React concurrent features. Store slices map to highway capabilities. |
| Styling | Tailwind CSS | Utility-first, no runtime CSS-in-JS overhead. Consistent with a dense, data-heavy dashboard UI. |
| Testing | Vitest + React Testing Library | Vitest for unit and integration tests. RTL for component behavior tests. |
| Package manager | pnpm | Strict dependency resolution, workspace support if needed. |

**Protobuf codegen pipeline.** The `.proto` files in `proto/` are the shared contract (protocol-interface spec, §1). The TypeScript codegen runs `buf generate` with the Connect-ES plugin, producing:

- **Message types** — TypeScript classes for every protobuf message (`GateEvent`, `TrailEntry`, `WorkspaceView`, etc.). Fields are typed; enums are TypeScript enums.
- **Service clients** — A typed `HighwayService` client with methods for every RPC. Unary RPCs return `Promise<Response>`. Server-streaming RPCs return `AsyncIterable<Message>`.

The generated code lives in `highway-ui/src/gen/` and is checked into the repository. Regeneration is a build step, not a runtime operation. CI verifies that the checked-in generated code matches the current `.proto` files (same check as the Rust proto codegen — protocol-interface spec, §9).

**Why Connect over raw gRPC-Web.** The Connect protocol is wire-compatible with gRPC-Web but adds three things the highway UI needs: (1) server-streaming over HTTP/1.1 (no Envoy required in development), (2) JSON encoding for debugging (switch from binary to JSON with a flag), (3) native `fetch` integration (works with browser devtools, CORS, service workers). In production, the same client talks to the runtime's gRPC endpoint through Envoy or tonic's gRPC-Web support — no code change.

## 4. gRPC-Web Transport Layer

The transport layer wraps every `HighwayService` RPC in a typed TypeScript function. It is the only module that touches protobuf-generated code directly. Everything above it works with plain TypeScript types.

**Transport client initialization:**

```typescript
import { createClient } from "@connectrpc/connect";
import { createGrpcWebTransport } from "@connectrpc/connect-web";
import { HighwayService } from "./gen/highway_connect";

const transport = createGrpcWebTransport({
  baseUrl: "http://localhost:9091",
  // Connect protocol: works over HTTP/1.1 in dev, HTTP/2 in prod
});

const client = createClient(HighwayService, transport);
```

**RPC wrappers.** The transport layer exports one function per RPC, translating between protobuf messages and application-level TypeScript types. Two categories:

**Unary RPCs** — request/response. The wrapper sends the request, awaits the response, and returns a typed result or throws a typed error.

| RPC | Wrapper signature |
|-----|-------------------|
| `Authenticate` | `authenticate(token: string): Promise<Session>` |
| `InjectEnvelope` | `injectEnvelope(req: InjectRequest): Promise<InjectResult>` |
| `RespondToGate` | `respondToGate(gateId: string, decision: GateDecision, modifications?: Uint8Array): Promise<GateAck>` |
| `RespondToEscalation` | `respondToEscalation(escalationId: string, action: EscalationAction): Promise<EscalationAck>` |
| `QueryTrail` | `queryTrail(filter: TrailFilter): Promise<TrailEntry[]>` |
| `GetWorkspace` | `getWorkspace(workspaceId: string): Promise<WorkspaceView>` |
| `GetTaskGraph` | `getTaskGraph(): Promise<TaskGraphView>` |
| `GetCheckpoint` | `getCheckpoint(checkpointId: string): Promise<CheckpointView>` |

**Server-streaming RPCs** — the server sends a continuous stream of messages. The wrapper returns an `AsyncIterable` that the state store consumes. Stream lifecycle (start, cancel, error, reconnect) is managed by the transport layer, not by components.

| RPC | Wrapper signature |
|-----|-------------------|
| `StreamTrail` | `streamTrail(filter: TrailStreamFilter): AsyncIterable<TrailEntry>` |
| `StreamGates` | `streamGates(): AsyncIterable<GateEvent>` |
| `StreamEscalations` | `streamEscalations(userId?: string): AsyncIterable<EscalationEvent>` |
| `StreamWorkspaceChanges` | `streamWorkspaceChanges(workspaceId?: string): AsyncIterable<WorkspaceStateChange>` |

**Request ID tracking.** Every mutating RPC carries a `client_request_id` — a client-generated UUID that correlates the request with its acknowledgment (protocol-interface spec, §5). The transport layer generates this ID, attaches it to the request, and matches it against the acknowledgment's `client_request_id`. This enables the UI to track in-flight operations: "I sent a gate response — has the server confirmed it?"

**Metadata injection.** After authentication, the transport layer attaches the session token to every RPC as gRPC metadata (`authorization: Bearer <token>`). This is configured once at session start and applied transparently to all subsequent calls. The runtime validates the token on every request (protocol-interface spec, §7).

**Error mapping.** gRPC status codes are mapped to application-level error types (§14). The transport layer catches gRPC errors, maps them to typed errors, and either throws (for unary RPCs) or emits error events on the stream (for streaming RPCs). Components never see raw gRPC status codes.

## 5. Connection Lifecycle

The highway UI manages a single logical session that spans multiple underlying gRPC streams. The session has four states:

```
disconnected → authenticating → connected → disconnected
                                    ↓
                              reconnecting → connected
                                    ↓
                              disconnected (max retries exceeded)
```

**State definitions:**

| State | Description |
|-------|-------------|
| `disconnected` | No active connection. The UI shows a login screen or a "disconnected" banner. No streams are active. |
| `authenticating` | The `Authenticate` RPC is in flight. The UI shows a loading indicator. |
| `connected` | Authentication succeeded. All four streams are active (`StreamTrail`, `StreamGates`, `StreamEscalations`, `StreamWorkspaceChanges`). The UI is fully operational. |
| `reconnecting` | A stream dropped or an RPC failed with a transient error. The transport layer is attempting to re-establish streams. The UI shows a "reconnecting" indicator but remains interactive for cached data. |

**Session establishment.** On login, the UI calls `Authenticate` with the user's auth token. On success, the transport layer stores the returned `user_id` and `capabilities`, then opens all four server-streaming RPCs in parallel. `StreamTrail` is opened with `from_beginning: true` to replay historical entries and catch up to the current state (protocol-interface spec, §5). The other three streams deliver live events only.

**Stream supervision.** The transport layer monitors all four streams. If any stream terminates unexpectedly (server disconnect, network error), the session transitions to `reconnecting`. The reconnection logic:

1. Wait with exponential backoff (1s, 2s, 4s, 8s, capped at 30s).
2. Re-authenticate — the session token may have expired.
3. Re-open all streams. `StreamTrail` uses `from_beginning: false` on reconnect — the state store already has historical entries. The trail stream resumes from live events only.
4. After successful reconnection, refresh entity state by calling `GetTaskGraph` and `GetWorkspace` for all known workspace IDs — streams may have missed events during the gap.

**Maximum retries.** After 5 consecutive failed reconnection attempts, the session transitions to `disconnected` and the UI prompts the user to re-authenticate. This prevents infinite retry loops when the runtime is down or the token is permanently invalid.

**Graceful disconnect.** When the user logs out or closes the tab, the transport layer cancels all active streams using `AbortController`. This sends a cancellation signal to the server, freeing server-side resources. No cleanup RPC exists — the runtime cleans up highway sessions on connection close.

**Multi-tab behavior.** Each browser tab maintains its own session. There is no cross-tab coordination. This is consistent with the protocol's multi-user semantics (protocol-interface spec, §5) — multiple highway clients connect simultaneously, and the runtime handles concurrency. Two tabs from the same user are indistinguishable from two different users to the runtime.

## 6. State Management

The state store is a Zustand store partitioned into slices by highway capability. Each slice owns one domain of client-side state, is fed by one or more server streams, and exposes selectors that components subscribe to.

**Store structure:**

```typescript
interface HighwayStore {
  // --- Session ---
  session: {
    state: "disconnected" | "authenticating" | "connected" | "reconnecting";
    userId: string | null;
    capabilities: string[];
  };

  // --- Trail (Visibility) ---
  trail: {
    entries: TrailEntry[];       // append-only, ordered by timestamp
    filters: TrailFilter;        // active filter state (event type, workspace, actor)
  };

  // --- Gates ---
  gates: {
    pending: Map<string, GateEvent>;     // gate_id → event, for unresolved gates
    resolved: Map<string, GateAck>;      // gate_id → ack, for recently resolved gates
    inFlight: Set<string>;               // gate_ids with a response RPC in flight
  };

  // --- Escalations ---
  escalations: {
    active: Map<string, EscalationEvent>;   // escalation_id → event
    resolved: Map<string, EscalationAck>;   // escalation_id → ack
    inFlight: Set<string>;
  };

  // --- Workspaces ---
  workspaces: {
    views: Map<string, WorkspaceView>;       // workspace_id → latest view
    changes: WorkspaceStateChange[];         // recent state changes, bounded buffer
  };

  // --- Task Graph ---
  taskGraph: {
    tasks: Task[];
    lastFetched: number | null;   // timestamp of last GetTaskGraph call
  };
}
```

**Slice isolation.** Each slice has its own actions (functions that mutate the slice) and selectors (functions that derive read-only views). Slices do not reference each other directly. Cross-slice reads happen in components — a component may select from both `gates` and `workspaces` to render a gate event with its workspace context.

**Stream → store binding.** Each server stream feeds exactly one slice:

| Stream | Target slice | Mutation |
|--------|-------------|----------|
| `StreamTrail` | `trail` | Append entry to `entries` array |
| `StreamGates` | `gates` | Insert into `pending` map |
| `StreamEscalations` | `escalations` | Insert into `active` map |
| `StreamWorkspaceChanges` | `workspaces` | Update `views` map, append to `changes` |

The stream consumers run outside the React render cycle — they are plain `async` loops started when the session enters `connected` and cancelled on disconnect. Each loop calls the appropriate store action on every received message.

**Bounded buffers.** The trail `entries` array and workspace `changes` array are bounded. Trail entries are capped at 10,000 — older entries are evicted from the front when the cap is reached. The full trail is always available via `QueryTrail` on demand. Workspace changes are capped at 1,000. These caps prevent unbounded memory growth in long-running sessions.

**Optimistic state for user actions.** When the user responds to a gate, the store immediately moves the gate ID into `inFlight`. When the `GateResponseAck` arrives, the store moves it from `inFlight` to `resolved` (if `applied: true`) or back to `pending` (if `applied: false` — another user or the fallback resolved it first). This is not optimistic state mutation — the gate remains in its server-assigned state. The `inFlight` set is a UI concern only, used to disable the "respond" button and show a loading indicator.

**Selectors.** Components subscribe to computed views, not raw store data:

| Selector | Returns | Used by |
|----------|---------|---------|
| `selectFilteredTrail` | Trail entries matching active filters | Trail Viewer |
| `selectPendingGates` | Pending gates sorted by urgency (timeout remaining) | Gate Panel |
| `selectActiveEscalations` | Unresolved escalations for the current user | Escalation Panel |
| `selectWorkspaceTree` | Workspace views organized as a tree by parent | Workspace View |
| `selectTaskDag` | Tasks with dependency edges computed | Task Graph View |

## 7. Visibility — Trail Stream

Visibility is the highway's passive capability (human-highway spec, §2.1). The trail viewer renders the global trail as a live, filterable event log. It is the UI's primary surface — the human's real-time view of everything happening in the system.

**Data source.** The `StreamTrail` RPC delivers trail entries as they are appended to the global trail. On initial connection, `from_beginning: true` replays historical entries, then switches to live streaming. The trail store accumulates entries up to the 10,000-entry cap (§6). For historical queries beyond the buffer, the `QueryTrail` RPC fetches on demand.

**Rendering model.** The trail viewer is a virtualized list — only visible entries are rendered to the DOM. This is essential: a long-running workflow may produce thousands of entries per minute. Without virtualization, the browser would grind to a halt. The list auto-scrolls to the bottom (latest entries) unless the user has scrolled up to inspect history. A "jump to latest" affordance appears when the user is scrolled away from the tail.

**Entry rendering.** Each trail entry is rendered as a row with consistent structure:

| Column | Content | Source field |
|--------|---------|-------------|
| Timestamp | HLC timestamp, formatted as `HH:MM:SS.mmm` with hover for full ISO-8601 | `timestamp` |
| Event type | Badge with color coding per category (gate, injection, escalation, workspace, envelope, signal, checkpoint, task, integration) | `event_type` |
| Actor | User ID for human actions, agent identity for agent actions, system token for automated actions | `actor` |
| Workspace | Workspace ID, clickable to navigate to workspace view. Empty for system-level events. | `workspace_id` |
| Summary | One-line human-readable summary derived from event type and key fields | Computed |

**Filters.** The trail viewer supports client-side filtering on the buffered entries. Filter controls:

| Filter | Type | Effect |
|--------|------|--------|
| Event type | Multi-select from known event types | Show only matching types |
| Workspace | Text input with autocomplete from known workspace IDs | Show only entries for that workspace |
| Actor | Text input with autocomplete | Show only entries by that actor |
| Category | Toggle buttons: gate, injection, escalation, workspace, envelope, signal, checkpoint, task, integration | Show/hide entire categories |

Filters are applied client-side on the buffered entries via the `selectFilteredTrail` selector. They do not affect the stream itself — the stream always delivers all entries, and the store always accumulates them. This ensures that changing a filter instantly updates the view without a server round-trip.

**Server-side filtering.** The `StreamTrail` RPC accepts `workspace_id` and `event_type` as stream parameters. These are used when the user navigates to a workspace detail view and wants a workspace-scoped trail — the UI opens a second, filtered trail stream scoped to that workspace. The primary trail stream remains unfiltered.

**Event detail expansion.** Clicking a trail entry expands it to show the full event body — all fields, formatted as a key-value table. For events that reference other objects (e.g., `gate_triggered` references a subject, `human_injection` references an envelope), the expanded view includes a link to navigate to that object's detail view.

**Pause/resume.** A toggle pauses the auto-append behavior — new entries still accumulate in the store but the rendered list freezes at the current position. This lets the human inspect a moment in time without the list scrolling away. Unpausing jumps to the latest entry.

## 8. Gate Management

Gates are the highway's active control mechanism (human-highway spec, §2.2). The gate panel renders pending gates, captures the human's decision, and sends it to the runtime. This is the UI's most time-sensitive surface — a gate has a timeout, and the human must act before it expires or the fallback executes.

**Data source.** The `StreamGates` RPC delivers `GateEvent` messages as gates are triggered. Each event enters the `gates.pending` map in the store. When the human responds (or the gate is resolved by another user, timeout, or protocol), the gate moves to `gates.resolved`.

**Gate card rendering.** Each pending gate is rendered as a card with:

| Element | Content | Source |
|---------|---------|--------|
| Gate type | Badge: `task_approval`, `workspace_create`, `envelope_delivery`, `integration`, `conflict_resolution`, `workspace_abort` | `type` |
| Countdown | Live countdown timer showing time remaining before fallback. Color transitions: green (>50% remaining) → yellow (10-50%) → red (<10%) → pulsing red (< 30 seconds) | Computed from `created_at` + `timeout_ms` |
| Fallback label | What happens if the human does not act: "auto-approve", "auto-reject", or "escalate to coordinator" | `fallback_action` |
| Subject summary | Human-readable summary of the object awaiting approval. Content depends on gate type (see below). | Deserialized `subject` |
| Workspace context | Clickable workspace ID. Null for `task_approval` gates (no workspace yet). | `workspace_id` |
| Task context | Clickable task ID when applicable. | `task_id` |
| Action buttons | **Approve**, **Reject**, **Modify** | User actions |

**Subject rendering by gate type:**

| Gate type | Subject display |
|-----------|----------------|
| `task_approval` | Task name, description, priority, dependencies (as a mini-graph), resource estimate |
| `workspace_create` | Target role, parent workspace, budget, task binding |
| `envelope_delivery` | Envelope type, sender, target workspace, priority, payload preview |
| `integration` | Source workspace, merge strategy, checkpoint reference |
| `conflict_resolution` | Conflict type, conflicting workspaces, proposed resolution strategy |
| `workspace_abort` | Workspace ID, current state, abort reason |

The `subject` field on `GateEvent` is `bytes` — a serialized protobuf message whose schema depends on the gate type. The UI deserializes it using the appropriate protobuf message type, determined by the `type` field. The transport layer handles this dispatch — the gate card receives a typed `GateSubject` union, not raw bytes.

**Countdown timer.** The countdown is computed entirely client-side from `created_at` and `timeout_ms`. The UI does not track the gate's server-side timeout — it computes the remaining time from the gate event's own fields. If `timeout_ms` is 0 (no timeout — `gated` preset), the countdown is replaced with an "indefinite" label. The countdown is a display concern only — the runtime enforces the actual timeout regardless of what the UI displays.

**Responding to a gate.** The three actions:

- **Approve** — calls `RespondToGate` with `decision: APPROVE`, empty `modifications`. The gate ID moves to `inFlight`.
- **Reject** — calls `RespondToGate` with `decision: REJECT`, empty `modifications`. The gate ID moves to `inFlight`.
- **Modify** — opens an inline editor pre-populated with the subject's modifiable fields. The modifiable fields depend on the gate type (human-highway spec, §3: budget and priority for `workspace_create`; priority and description for `task_approval`; payload for `envelope_delivery`). On submit, calls `RespondToGate` with `decision: MODIFY` and the altered fields serialized as `modifications`. The gate ID moves to `inFlight`.

**Acknowledgment handling.** When `GateResponseAck` arrives:
- `applied: true` — the gate moves from `inFlight` to `resolved`. The card disappears from the pending list with a brief success indicator.
- `applied: false` — the gate was already resolved by another user, the timeout, or protocol invalidation. The gate moves from `inFlight` to `resolved` with a notice: "Gate already resolved." The card disappears.

**Ordering.** Pending gates are sorted by urgency — shortest remaining time first. Gates with no timeout sort to the bottom. This ensures the human sees the most time-critical gate first, consistent with the protocol's FIFO queue (human-highway spec, §3) while prioritizing the UI for human attention.

**Batch task approval.** When a task graph is submitted, multiple `task_approval` gates arrive in rapid succession. The gate panel renders all of them as individual cards, but provides a batch action bar: **Approve All**, **Reject All**. The batch action sends individual `RespondToGate` RPCs for each gate — the protocol processes each independently (human-highway spec, §4). The UI batches the presentation and the user intent, not the protocol resolution.

**Sound/notification.** When a new gate arrives, the UI plays a brief audio notification and, if the browser tab is not focused, requests a system notification via the Notifications API. Gates are time-sensitive — the human needs to know one has arrived even if they are not looking at the screen.

## 9. Escalation Handling

Escalation handling is the highway's demand-response capability (human-highway spec, §2.4). An agent is stuck and needs human input. The escalation panel surfaces these requests and captures the human's response. Unlike gates (which may fall through on timeout), escalations represent agents that cannot proceed — a missed escalation wastes work.

**Data source.** The `StreamEscalations` RPC delivers `EscalationEvent` messages. The stream is filtered by the authenticated user's `user_id` by default — the human sees only escalations routed to them (escalations route to the workspace's owner, human-highway spec, §2.4). The UI may open an unfiltered stream if the user has the appropriate capability.

**Escalation card rendering.** Each active escalation is rendered as a card with:

| Element | Content | Source |
|---------|---------|--------|
| Workspace | Clickable workspace ID — links to workspace detail view | `workspace_id` |
| Owner | The user this escalation is routed to (should match the current user) | `owner` |
| Context | The agent's explanation of why it escalated — rendered as text. This is the `context` bytes field, decoded as UTF-8 text. | `context` |
| Timestamp | When the escalation was raised | `created_at` |
| Action buttons | **Send Feedback**, **Abort Workspace**, **Delegate to Coordinator** | User actions |

**The three responses:**

- **Send Feedback** — opens the envelope injection form (§10) pre-targeted to the escalating workspace. The human composes an envelope (feedback, clarification, or a new directive) and sends it. The envelope unblocks the agent. On submit, calls `RespondToEscalation` with `action: feedback` carrying the composed envelope. The escalation moves to `inFlight`, then to `resolved` on acknowledgment.

- **Abort Workspace** — a destructive action. The UI shows a confirmation dialog: "This will fail the workspace. The agent's work will be lost. Continue?" On confirm, calls `RespondToEscalation` with `action: abort = true`. The runtime transitions the workspace to `failed` with `reason: aborted_by_human`.

- **Delegate to Coordinator** — the human defers to the coordinator's judgment. Calls `RespondToEscalation` with `action: delegate_to_coordinator = true`. The coordinator processes the escalation using its normal blocked-agent logic. No confirmation dialog — delegation is non-destructive.

**Workspace context.** The escalation card includes a "View Workspace" link that opens the workspace detail view (§11). The human can inspect the workspace's state, recent trail entries, and checkpoints before deciding how to respond. This context is critical — the agent's `context` field explains why it escalated, but the workspace state shows what the agent was doing when it got stuck.

**Notification urgency.** Escalation notifications are more intrusive than gate notifications. When a new escalation arrives:
1. Audio notification (distinct tone from gates).
2. Browser system notification with the escalation context preview.
3. The escalation panel's tab/header shows a badge count of active escalations.
4. If the escalation panel is not visible, a banner appears at the top of the UI: "Agent needs help — [workspace_id] escalated."

This elevated urgency reflects the protocol's semantics — a gate that times out proceeds or cancels automatically, but an escalation that times out may fail the workspace and waste all prior work (human-highway spec, §2.4).

**Escalation timeout display.** Escalation timeouts are configured in the workflow's highway block, not on the individual event. The `EscalationEvent` message does not carry a `timeout_ms` field (unlike `GateEvent`). The UI does not display a countdown for escalations — it does not know the timeout. Instead, the trail viewer shows `escalation_timeout` events when they fire, and the escalation card disappears from the active list when the runtime resolves it. The human is aware that escalations have timeouts (from the workflow configuration) but the UI does not track them client-side.

## 10. Envelope Injection

Injection is the highway's additive capability (human-highway spec, §2.3). The human composes and sends envelopes to any workspace at any time. The injection form is the UI's authoring surface — the only place where the human creates new protocol objects rather than responding to existing ones.

**Data source.** Injection is a unary RPC (`InjectEnvelope`), not a stream. The form captures user input; the transport layer sends the request; the acknowledgment confirms delivery. The injected envelope appears in the trail stream as a `human_injection` event.

**Injection form fields:**

| Field | Input type | Validation | Maps to |
|-------|-----------|------------|---------|
| Target workspace | Text input with autocomplete from known workspace IDs | Required. Must be a valid workspace ID from the `workspaces.views` map. | `to_workspace` |
| Envelope type | Dropdown populated from taxonomy-registered envelope types | Required. Must be a registered type. Server validates. | `type` |
| Priority | Radio buttons: Normal, Urgent, Blocking | Required. Defaults to Normal. | `priority` |
| Payload | Text area. Content is application-defined — the protocol treats it as opaque bytes (protocol-interface spec, §6, rule 2). | Optional. Encoded as UTF-8 bytes. | `payload` |

**Pre-population.** The injection form can be opened in three contexts:

1. **Standalone** — from the main navigation. All fields are empty.
2. **From escalation response** — "Send Feedback" on an escalation card (§9). Target workspace is pre-filled with the escalating workspace ID. Envelope type defaults to `feedback`.
3. **From workspace view** — "Inject Envelope" action on a workspace detail view (§11). Target workspace is pre-filled.

**Submission flow:**

1. User fills the form and clicks **Send**.
2. The transport layer generates a `client_request_id` and calls `InjectEnvelope`.
3. The form enters a "sending" state — the Send button is disabled, a spinner appears.
4. On success (`InjectEnvelopeResponse` with `envelope_id`): the form shows a brief success message with the assigned envelope ID, then resets. The injected envelope will appear in the trail stream momentarily.
5. On failure: the form shows the error message. Common failures:
   - Workspace does not exist → "Workspace not found."
   - Workspace is terminal (`closed` or `failed`) → "Cannot inject into a terminal workspace" (human-highway spec, §6.2).
   - Workspace is sealed (`integrating`) → "Cannot inject into a workspace being integrated" (human-highway spec, §6.3).
   - User is not `active` → "Your account is not active. Injection denied" (human-highway spec, §2.3).
   - Unregistered envelope type → "Envelope type not registered in taxonomy."

**No client-side workspace state validation.** The UI does not check whether the target workspace is terminal or sealed before sending. The runtime validates and rejects — the UI reports the rejection. This follows the design constraint (§1): no client-side protocol logic. The `workspaces.views` map may be stale; the runtime's state is authoritative.

**Payload editing.** The payload field is a plain text area. The UI does not interpret payload content — it is opaque bytes per the protocol (protocol-interface spec, §6, rule 2). Future iterations may add structured payload editors for common types (JSON with schema validation, markdown with preview), but the baseline is raw text encoded as UTF-8.

## 11. Workspace and Task Views

The workspace and task views are the highway's read-only inspection surfaces. They let the human see what the system is doing — the structure of the workspace tree, the state of each workspace, the shape of the task graph, and the content of checkpoints. These views support the other capabilities: the human uses them to gather context before approving a gate, responding to an escalation, or injecting an envelope.

### 11.1 Workspace Tree View

The workspace tree renders all active workspaces as a hierarchical tree, reflecting the parent-child structure maintained by the coordinator.

**Data source.** The `workspaces.views` map in the store, populated by `StreamWorkspaceChanges` and on-demand `GetWorkspace` calls. The tree structure is derived from the `parent` field on each `WorkspaceView`.

**Tree node rendering:**

| Element | Content | Source |
|---------|---------|--------|
| Workspace ID | Truncated ID with full ID on hover | `id` |
| State | Color-coded badge: green (active), yellow (blocked/suspended), blue (integrating/migrating), red (failed), grey (idle/closed) | `state` |
| Role | The agent's role in this workspace | `role` |
| Owner | User ID of the workspace's owner | `owner` |
| Task | Linked task ID, if bound | `task_id` |
| Resource usage | Progress bar: current usage / budget for tokens | `current_usage`, `budget` |
| Last activity | Relative time ("3s ago", "2m ago") | `last_activity` |

**Interactions:**
- **Click a node** — opens the workspace detail panel (see below).
- **Expand/collapse** — shows or hides child workspaces.
- The tree auto-expands nodes that have active escalations or pending gates — drawing the human's attention to workspaces that need action.

### 11.2 Workspace Detail Panel

Clicking a workspace in the tree opens a detail panel with full workspace state and workspace-scoped actions.

**Sections:**

1. **Header** — workspace ID, state badge, role, owner, originator, creation timestamp.
2. **Resource meter** — token usage, wall time, storage, network, cost. Bar chart with budget limits marked.
3. **Checkpoints** — list of checkpoints created in this workspace, ordered by timestamp. Each shows type, status (provisional/final), confidence, and a "View" link that calls `GetCheckpoint` and displays the payload.
4. **Trail (scoped)** — a mini trail viewer filtered to this workspace. Opens a second `StreamTrail` with `workspace_id` set, showing only events for this workspace.
5. **Actions** — "Inject Envelope" button (opens injection form pre-targeted to this workspace, §10).

### 11.3 Task Graph View

The task graph renders the coordinator's task DAG as a directed graph, showing task dependencies, statuses, and the flow of work through the system.

**Data source.** The `taskGraph.tasks` array in the store, populated by `GetTaskGraph` calls. The graph is fetched on initial connection and refreshed when a `task_status_changed` trail event is observed (detected by filtering the trail stream).

**Graph rendering.** Tasks are nodes. Dependencies (`depends_on`) are directed edges. The layout is top-to-bottom (roots at the top, leaves at the bottom), computed client-side using a layered graph layout algorithm (Sugiyama-style). The graph is rendered as SVG for zoom and pan support.

**Node rendering:**

| Element | Content | Source |
|---------|---------|--------|
| Task name | Primary label | `name` |
| Status | Color-coded: grey (draft), blue (pending), cyan (assigned), green (in_progress), dark green (completed), red (failed), purple (integrated), strikethrough (cancelled) | `status` |
| Priority | Icon: normal (none), urgent (arrow up), blocking (double arrow) | `priority` |
| Workspace binding | Dotted link to the workspace tree node, if the task is bound to a workspace | `workspace_id` (from workspace view cross-reference) |

**Interactions:**
- **Click a node** — opens a task detail popover showing full task fields: name, description, dependencies, priority, resource estimate, status history.
- **Hover an edge** — highlights the dependency chain upstream and downstream.
- **Zoom/pan** — standard scroll-to-zoom, drag-to-pan on the SVG canvas.

**Gate integration.** Tasks in `draft` status that have a pending `task_approval` gate are rendered with a pulsing border. Clicking the node navigates to the corresponding gate card in the gate panel. This connects the graph visualization to the approval workflow — the human sees the plan and approves from either the graph or the gate panel.

### 11.4 Checkpoint Viewer

The checkpoint viewer renders the payload of a single checkpoint, fetched via `GetCheckpoint`.

**Data source.** Unary `GetCheckpoint` RPC, called on demand when the human clicks a checkpoint reference (from workspace detail, trail entry expansion, or gate subject).

**Rendering.** The checkpoint viewer shows metadata (ID, type, status, confidence, workspace, content hash) in a header, and the payload in the body. The payload is `bytes` — the viewer attempts UTF-8 text decoding. If the payload is valid UTF-8, it renders as preformatted text. If not, it renders as a hex dump. The content hash is displayed alongside a "Verified" badge if the runtime's integrity check passed (the `GetCheckpoint` RPC returns only verified payloads — protocol-interface spec, §5).

## 12. Autonomy Preset Configuration

The autonomy spectrum is the highway's policy surface (human-highway spec, §5). The configuration panel lets the human view and modify the workflow's highway settings — which gates are enabled, what timeouts apply, and what fallbacks fire. This is the only UI surface that writes configuration rather than responding to protocol events.

**Scope constraint.** The highway configuration is a workflow-level setting, not a runtime-level setting. The UI displays the active configuration and allows modification within the current workflow's session. The mechanism for persisting configuration changes to the workflow definition is deployment-defined — the UI sends the configuration to the runtime, which applies it to the active session. Whether the runtime also persists it to disk, a config file, or a database is outside this spec.

**No RPC exists yet.** The `HighwayService` contract (protocol-interface spec, §5) does not currently define an RPC for reading or writing highway configuration. This section defines what the UI needs; the corresponding RPC will be added to the contract when this spec is implemented. Until then, the configuration panel reads from a local configuration file or environment variable at startup and operates in read-only mode.

**Configuration display.** The panel renders the active highway configuration as a structured form mirroring the schema (human-highway spec, §5):

| Section | Fields | Display |
|---------|--------|---------|
| Preset | Active preset name or "custom" | Dropdown: autonomous, supervised, gated, custom |
| Visibility | `visibility: boolean` | Toggle switch |
| Gates | One row per gate type with enabled/disabled toggle | 6 toggle switches with labels |
| Gate defaults | `timeout` (duration input), `fallback` (dropdown: approve, reject, escalate) | Duration picker + dropdown |
| Gate overrides | Per-gate-type timeout and fallback, collapsible | Expandable rows under each gate type |
| Injection | `injection: boolean` | Toggle switch |
| Escalation | `enabled`, `timeout`, `fallback` | Toggle + duration picker + dropdown |

**Preset selection.** Selecting a preset (autonomous, supervised, gated) pre-fills all fields with the preset's values (human-highway spec, §5). Modifying any field after selecting a preset changes the label to "custom." The three presets are read-only templates — they define starting points, not constraints.

**Validation.** The UI validates configuration locally before sending:
- Timeout values must be non-negative. Zero means no timeout (indefinite wait).
- Fallback must be one of `approve`, `reject`, `escalate_to_coordinator`.
- At least `task_approval` should be enabled — the UI warns (but does not prevent) disabling the only default gate.
- The `gated` preset with all timeouts set to zero is valid but the UI warns: "Indefinite timeouts on all gates. A missing human will deadlock the system."

**Read-only mode.** Until the configuration RPC is added to `HighwayService`, the panel is read-only. It displays the configuration the runtime was started with, but the "Apply" button is disabled with a tooltip: "Configuration changes require a runtime restart in the current version." This is an honest limitation, not a stub — the UI is ready for the RPC when it arrives.

## 13. Authentication

The protocol requires authenticated identity before any highway action (PROTOCOL.md §11.3, human-highway spec, §7). The UI authenticates the human at session start and carries the authenticated identity through every subsequent RPC.

**Authentication mechanism is deployment-defined.** The protocol specifies what must be authenticated (the human's `user_id`), not how (human-highway spec, §7). The UI abstracts authentication behind a provider interface, allowing different deployments to plug in different mechanisms.

**Provider interface:**

```typescript
interface AuthProvider {
  // Returns an auth token suitable for the Authenticate RPC.
  // May prompt the user (login form, OAuth redirect, etc.).
  getToken(): Promise<string>;

  // Called when the runtime rejects the token (expired, revoked).
  // The provider should clear cached credentials and re-prompt.
  onTokenRejected(): Promise<string>;
}
```

**Built-in providers:**

| Provider | Mechanism | Use case |
|----------|-----------|----------|
| `TokenAuthProvider` | User pastes a pre-issued token into a login form. The token is sent as-is to `Authenticate`. | Development, single-user deployments. |
| `OAuthAuthProvider` | Standard OAuth 2.0 / OIDC redirect flow. The provider exchanges the authorization code for a token, then sends the token to `Authenticate`. | Production, multi-user deployments with an identity provider (Auth0, Keycloak, etc.). |

**Login flow:**

1. The UI starts in `disconnected` state. The login screen renders the auth provider's input (token field or "Sign in" button).
2. The auth provider returns a token.
3. The transport layer calls `Authenticate(token)`.
4. On success: the session stores `user_id` and `capabilities` from the response. The session transitions to `connected`. Streams open.
5. On failure: the login screen shows the error ("Invalid token", "Authentication failed"). The session remains `disconnected`.

**Token lifecycle.** The token is stored in memory only — not in `localStorage`, not in cookies. Closing the tab loses the session. This is deliberate: the highway grants powerful capabilities (injection, gate resolution, escalation response), and persistent sessions without re-authentication are a security risk. The `OAuthAuthProvider` may use its own refresh token mechanism (via the identity provider), but the WACP session token is ephemeral.

**Re-authentication.** When any RPC fails with `UNAUTHENTICATED` (gRPC status code 16), the transport layer calls `authProvider.onTokenRejected()` to obtain a fresh token, then retries the `Authenticate` RPC. If re-authentication fails, the session transitions to `disconnected`. This handles token expiration transparently — the human is not interrupted unless re-authentication itself fails.

**Capabilities.** The `AuthenticateResponse` includes a `capabilities` list — what this user is allowed to do. The UI uses capabilities to show or hide action buttons:

| Capability | UI effect |
|------------|-----------|
| `inject` | Show/hide the injection form and "Inject Envelope" buttons |
| `gate_respond` | Show/hide approve/reject/modify buttons on gate cards |
| `escalation_respond` | Show/hide response buttons on escalation cards |
| `view_all_escalations` | Allow opening an unfiltered escalation stream (not just own escalations) |

Capabilities are a UI hint, not enforcement. The runtime validates every action server-side regardless of what the UI displays. Hiding buttons prevents confusion, not bypass.

## 14. Error Handling

The UI encounters errors at two boundaries: the transport layer (gRPC errors from the runtime) and the browser (network failures, stream drops). The error handling strategy is: classify, display, and recover when possible — never silently swallow.

**Error classification.** All errors are mapped to one of four categories:

| Category | gRPC codes | User-visible behavior | Recovery |
|----------|-----------|----------------------|----------|
| **Authentication** | `UNAUTHENTICATED` (16) | Re-authentication flow (§13). If re-auth fails, redirect to login. | Automatic re-auth attempt |
| **Permission** | `PERMISSION_DENIED` (7) | Inline error on the action that was denied. "You do not have permission to [action]." | None — the action is not retried. The user lacks the capability. |
| **Validation** | `INVALID_ARGUMENT` (3), `NOT_FOUND` (5), `FAILED_PRECONDITION` (9) | Inline error on the form or action that triggered it. Error message from the server's status detail. | None — the user must correct the input. |
| **Transient** | `UNAVAILABLE` (14), `DEADLINE_EXCEEDED` (4), `INTERNAL` (13), `UNKNOWN` (2), network errors | Banner: "Connection issue — retrying..." Session transitions to `reconnecting` (§5). | Automatic reconnection with backoff |

**Unary RPC errors.** When a unary RPC fails, the transport layer throws a typed error. The calling component catches it and displays an inline error message next to the action that triggered it. The error message includes the server's detail string when available (e.g., "Cannot inject into a terminal workspace") and falls back to a generic message per category when not.

**Stream errors.** When a server-streaming RPC terminates with an error, the transport layer emits an error event on the stream. The stream consumer (the async loop feeding the store, §6) handles the error by:

1. Logging the error to the browser console with full context (stream name, error code, message).
2. If transient: triggering reconnection (§5). The session transitions to `reconnecting`.
3. If authentication: triggering re-authentication (§13).
4. If other: logging and surfacing a banner. The stream is not retried — a non-transient stream error indicates a server-side problem that retrying will not fix.

**Concurrent error suppression.** During reconnection, multiple streams may fail simultaneously. The session state machine (§5) handles this — once the session is in `reconnecting`, additional stream failures are logged but do not trigger additional reconnection attempts. One reconnection cycle handles all streams.

**Error display hierarchy:**

1. **Inline errors** — displayed next to the action that caused them (form validation, gate response rejection, injection failure). Cleared when the user takes a new action.
2. **Banners** — displayed at the top of the UI for session-level issues (connection lost, reconnecting, re-authentication required). Persistent until the condition resolves.
3. **Toast notifications** — brief, auto-dismissing messages for non-blocking warnings (gate already resolved by another user, escalation timed out while composing response).

**No error modals.** Errors never block the UI with a modal dialog. The human may be responding to a time-sensitive gate — a modal would prevent interaction. All errors are non-blocking: inline, banner, or toast.

## 15. Component Hierarchy

The UI is organized as a component tree rooted at `App`. The hierarchy reflects the three-layer architecture (§2): layout components at the top, capability-specific panels in the middle, and reusable display components at the leaves.

```
App
├── AuthGate                          # Shows login or main UI based on session state
│   ├── LoginScreen                   # Auth provider input (token form or OAuth button)
│   └── MainLayout                    # Authenticated UI shell
│       ├── ConnectionBanner          # Reconnecting / disconnected indicator
│       ├── EscalationBanner          # "Agent needs help" alert when escalation panel not visible
│       ├── Sidebar                   # Navigation between panels
│       │   ├── NavItem: Trail
│       │   ├── NavItem: Gates        # Badge with pending gate count
│       │   ├── NavItem: Escalations  # Badge with active escalation count
│       │   ├── NavItem: Workspaces
│       │   ├── NavItem: Task Graph
│       │   ├── NavItem: Inject
│       │   └── NavItem: Settings
│       └── ContentArea               # Renders the active panel
│           ├── TrailViewer
│           │   ├── TrailFilterBar
│           │   ├── TrailEntryList    # Virtualized list
│           │   │   └── TrailEntryRow # One per visible entry
│           │   │       └── TrailEntryDetail  # Expanded view on click
│           │   └── TrailPauseToggle
│           ├── GatePanel
│           │   ├── BatchActionBar    # Approve All / Reject All for task_approval gates
│           │   └── GateCard          # One per pending gate
│           │       ├── GateCountdown
│           │       ├── GateSubjectView  # Type-dispatched subject renderer
│           │       ├── GateActionButtons
│           │       └── GateModifyEditor # Inline editor, shown on Modify click
│           ├── EscalationPanel
│           │   └── EscalationCard    # One per active escalation
│           │       ├── EscalationContext
│           │       └── EscalationActionButtons
│           ├── WorkspaceTreeView
│           │   ├── WorkspaceTreeNode # Recursive, one per workspace
│           │   └── WorkspaceDetailPanel  # Shown when a node is selected
│           │       ├── WorkspaceHeader
│           │       ├── ResourceMeter
│           │       ├── CheckpointList
│           │       │   └── CheckpointViewer  # On-demand payload view
│           │       └── ScopedTrailViewer
│           ├── TaskGraphView
│           │   ├── TaskGraphCanvas   # SVG with zoom/pan
│           │   │   ├── TaskNode      # One per task
│           │   │   └── DependencyEdge # One per dependency
│           │   └── TaskDetailPopover # Shown on node click
│           ├── InjectionForm
│           │   ├── WorkspaceSelector
│           │   ├── EnvelopeTypeSelector
│           │   ├── PrioritySelector
│           │   └── PayloadEditor
│           └── SettingsPanel
│               └── AutonomyConfig   # Highway configuration form (§12)
```

**Routing.** The `ContentArea` renders one panel at a time, selected by the sidebar navigation. The UI uses client-side routing (`react-router`) with paths: `/trail`, `/gates`, `/escalations`, `/workspaces`, `/tasks`, `/inject`, `/settings`. The default route is `/trail` — visibility is the primary capability.

**Panel independence.** Each panel manages its own data subscriptions and lifecycle. Switching away from a panel does not cancel its data streams — the store continues accumulating data from all four server streams regardless of which panel is active. Switching back to a panel shows the latest state immediately, with no loading delay.

**Responsive layout.** The sidebar collapses to icons on narrow viewports. The content area is the full width. Gate and escalation badge counts remain visible in the collapsed sidebar — the human must always see at a glance whether actions are pending.

## 16. Testing Strategy

The highway UI is tested at three levels: unit tests for pure logic, component tests for UI behavior, and integration tests for the transport-to-store pipeline. No end-to-end tests against a running runtime — the runtime has its own test suite, and the UI's contract with the runtime is the protobuf definition. The UI tests verify that the UI correctly consumes and produces protocol messages, not that the runtime handles them correctly.

### 16.1 Unit Tests

Pure functions tested in isolation with Vitest. No DOM, no React, no mocked RPCs.

| Module | What is tested |
|--------|---------------|
| Trail entry formatting | Timestamp display, event type badge mapping, summary text generation |
| Gate countdown computation | Remaining time from `created_at` + `timeout_ms`, color thresholds, indefinite timeout handling |
| Store selectors | `selectFilteredTrail` with various filter combinations, `selectPendingGates` ordering, `selectWorkspaceTree` tree construction, `selectTaskDag` edge computation |
| Error classification | gRPC status code → error category mapping for every code in the table (§14) |
| Auth provider | `TokenAuthProvider` token storage and retrieval, `onTokenRejected` credential clearing |

### 16.2 Component Tests

React components tested with React Testing Library. Components are rendered with a pre-populated Zustand store. RPC calls are mocked at the transport layer — the mock returns typed responses, not raw protobuf.

| Component | What is tested |
|-----------|---------------|
| `TrailEntryRow` | Renders all columns correctly for each event type category. Click expands detail view. |
| `TrailFilterBar` | Filter state updates propagate to the store. Active filters visually indicated. |
| `GateCard` | Renders subject summary per gate type. Countdown updates. Action buttons call correct RPCs. Disabled state during `inFlight`. |
| `GateModifyEditor` | Pre-populates modifiable fields. Submits only changed fields. |
| `BatchActionBar` | Appears when multiple `task_approval` gates are pending. "Approve All" sends individual RPCs for each gate. |
| `EscalationCard` | Renders context. "Send Feedback" opens injection form pre-targeted. "Abort" shows confirmation dialog. |
| `WorkspaceTreeNode` | Recursive rendering. State badge colors. Auto-expand for nodes with active escalations. |
| `ResourceMeter` | Bar widths proportional to usage/budget. Overflow indication when usage exceeds budget. |
| `TaskNode` | Status color coding. Pulsing border for draft tasks with pending gates. Click opens detail popover. |
| `InjectionForm` | Validation: required fields, submission flow. Pre-population from escalation and workspace contexts. Error display on server rejection. |
| `LoginScreen` | Token input submission. Error display on authentication failure. |
| `ConnectionBanner` | Visible during `reconnecting` and `disconnected`. Hidden during `connected`. |

### 16.3 Integration Tests

The transport-to-store pipeline tested end-to-end within the browser environment. A mock gRPC server (built with Connect's `createRouterTransport`) replays scripted message sequences. The test verifies that messages flow from the mock server through the transport layer into the store, and that user actions flow from the store through the transport layer to the mock server.

| Scenario | Verification |
|----------|-------------|
| Trail stream replay | `from_beginning: true` populates the trail store with historical entries in order. Live entries append after replay. |
| Gate lifecycle | Gate event arrives → appears in `pending`. User approves → moves to `inFlight`. Ack arrives with `applied: true` → moves to `resolved`. |
| Gate race | Gate event arrives → user approves → ack arrives with `applied: false` (another user resolved it). Gate moves to `resolved` with "already resolved" notice. |
| Escalation feedback | Escalation arrives → user clicks "Send Feedback" → injection form opens pre-targeted → user submits → `RespondToEscalation` sent with feedback envelope. |
| Reconnection | Mock server drops the trail stream. Session transitions to `reconnecting`. Mock server accepts reconnection. Streams re-open with `from_beginning: false`. Entity state refreshed via `GetTaskGraph` and `GetWorkspace`. |
| Auth expiry | Unary RPC returns `UNAUTHENTICATED`. Transport calls `onTokenRejected`. Re-auth succeeds. Original RPC is not retried (user re-initiates). |
| Bounded buffer | Push 10,001 trail entries. Store contains 10,000. Oldest entry evicted. |

### 16.4 Visual Smoke Tests

Not automated. A Storybook catalog of key components with representative data:

- `GateCard` for each of the six gate types, with and without countdown.
- `EscalationCard` with short and long context text.
- `TrailEntryRow` for each event type category.
- `TaskGraphCanvas` with a 10-node DAG showing all status colors.
- `WorkspaceTreeView` with 3 levels of nesting and mixed states.
- `ResourceMeter` at 0%, 50%, 90%, and over-budget.

These are developer tools for visual regression checking, not CI-gated tests.

## 17. References

This spec depends on four WACP specs and two implementation specs. Cross-references are inline throughout; this section collects them for navigability.

| Spec | Sections referenced | Relationship |
|------|-------------------|--------------|
| [PROTOCOL.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/PROTOCOL.md) | §8 (human highway), §11.3 (authentication) | Defines the highway's capabilities and security requirements |
| [Human Highway](https://github.com/Madahub-dev/wacp-protocol/blob/main/mechanisms/human-highway.md) | §2 (four capabilities), §3 (gate schema), §4 (task approval), §5 (autonomy spectrum), §6 (edge cases), §7 (interface boundary), §8 (trail events) | The normative spec for everything the UI renders and captures |
| [User](https://github.com/Madahub-dev/wacp-protocol/blob/main/primitives/user.md) | §2 (user_id), §3.1 (user states), §4.1 (ownership, escalation routing), §5.1 (originator) | User identity, state-gated actions, escalation routing |
| [Runtime Architecture](runtime.md) | §14 (concurrency model) | Runtime is the trust root; UI defers all enforcement |
| [Protocol Interface](protocol-interface.md) | §5 (highway service contract), §6 (serialization rules), §7 (authentication at boundary), §9 (gRPC implementation) | Defines the wire contract — RPCs, messages, and codegen pipeline |
| [Agent SDK Design](sdk-agent.md) | §1 (scope boundary) | Agent SDK is a peer client; the highway UI is the human-facing peer |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/PROTOCOL.md) | Taxonomy: [TAXONOMY.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/TAXONOMY.md)*
