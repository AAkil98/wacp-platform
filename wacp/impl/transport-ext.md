# WACP Implementation: Transport Extensions

```yaml
id: wacp-impl-transport-ext
type: implementation-spec
status: draft
created: 2026-04-01
lineage: LAYER-MAPPING.md (M4)
protocol_sections:
  - §8 (human highway — transport bindings)
  - §11 (security — authentication)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-protocol-interface
  - wacp-impl-security
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, middleware, transport, rest, websocket, auth]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [REST Gateway](#2-rest-gateway)
3. [WebSocket Binding](#3-websocket-binding)
4. [Auth Providers](#4-auth-providers)
5. [Crate Changes](#5-crate-changes)
6. [Test Requirements](#6-test-requirements)
7. [References](#7-references)

---

## 1. Purpose

This spec extends `wacp-transport` with non-gRPC bindings and pluggable auth providers. It answers "how do web dashboards, CLI tools, and third-party integrations connect to the runtime" — beyond the existing gRPC interface.

**What exists:** gRPC transport (tonic, AgentService on port 9090, HighwayService on port 9091), `Authenticator` trait, `PskAuthenticator`, `AuthRateLimiter`.

**What's added:** REST gateway (HTTP + JSON + SSE), WebSocket binding (bidirectional events), three new auth providers (API key, OAuth/OIDC, session tokens).

**Transport invariant (preserved):** Bindings add no logic. They translate wire format to runtime RPC calls. No state, no caching, no filtering.

---

## 2. REST Gateway

An HTTP server (axum) that maps proto operations to REST endpoints.

### 2.1 Endpoint Catalog

| Method | Path | Maps to | Response |
|--------|------|---------|----------|
| `POST` | `/v1/sessions` | `Authenticate` | `{ session_id, token }` |
| `DELETE` | `/v1/sessions/:id` | Session invalidation | `204` |
| `POST` | `/v1/goals` | `SubmitGoal` | `{ goal_id, workspace_id }` |
| `DELETE` | `/v1/goals/:id` | Goal cancellation | `204` |
| `GET` | `/v1/tasks` | `GetReadyTasks` | `[TaskView]` |
| `GET` | `/v1/tasks/graph` | `GetTaskGraph` | `TaskGraphView` |
| `POST` | `/v1/workspaces/:id/dispatch` | `Dispatch` | `{ workspace_id }` |
| `POST` | `/v1/workspaces/:id/abort` | `AbortWorkspace` | `204` |
| `POST` | `/v1/workspaces/:id/suspend` | `SuspendWorkspace` | `204` |
| `POST` | `/v1/workspaces/:id/resume` | `ResumeWorkspace` | `204` |
| `GET` | `/v1/workspaces/:id` | `GetWorkspace` | `WorkspaceView` |
| `POST` | `/v1/workspaces/:id/inject` | `InjectEnvelope` | `{ envelope_id }` |
| `POST` | `/v1/workspaces/:id/integrate` | `TriggerIntegration` | `IntegrationResult` |
| `GET` | `/v1/gates` | `StreamGates` (snapshot) | `[GateEvent]` |
| `POST` | `/v1/gates/:id/respond` | `RespondToGate` | `204` |
| `GET` | `/v1/escalations` | `StreamEscalations` (snapshot) | `[EscalationEvent]` |
| `POST` | `/v1/escalations/:id/respond` | `RespondToEscalation` | `204` |
| `GET` | `/v1/trail` | `QueryTrail` | `[TrailEntry]` |
| `GET` | `/v1/health` | Health check | `{ status, uptime }` |

### 2.2 SSE Streaming

| Endpoint | Maps to | Events |
|----------|---------|--------|
| `GET /v1/events/trail` | `StreamTrail` | `trail_entry` events |
| `GET /v1/events/gates` | `StreamGates` | `gate_event` events |
| `GET /v1/events/escalations` | `StreamEscalations` | `escalation_event` events |
| `GET /v1/events/signals` | `StreamSignals` | `signal_event` events |
| `GET /v1/events/workspaces` | `StreamWorkspaceChanges` | `workspace_change` events |

SSE format: `event: <type>\ndata: <json>\n\n`

### 2.3 Error Mapping

| gRPC Status | HTTP Status |
|:-:|:-:|
| `OK` | `200` |
| `INVALID_ARGUMENT` | `400` |
| `UNAUTHENTICATED` | `401` |
| `PERMISSION_DENIED` | `403` |
| `NOT_FOUND` | `404` |
| `ALREADY_EXISTS` | `409` |
| `RESOURCE_EXHAUSTED` | `429` |
| `INTERNAL` | `500` |

### 2.4 Implementation

```rust
pub struct RestGateway {
    router: axum::Router,
    port: u16,
}

impl RestGateway {
    pub fn new(
        coordinator: CoordinatorServiceClient<Channel>,
        highway: HighwayServiceClient<Channel>,
        authenticator: Arc<dyn Authenticator>,
        port: u16,
    ) -> Self;

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>>;
}
```

The gateway holds gRPC clients to the runtime services and translates HTTP requests into gRPC calls. It is a pure translation layer — no business logic.

---

## 3. WebSocket Binding

A WebSocket server for bidirectional real-time communication.

### 3.1 Protocol

JSON-RPC 2.0 over WebSocket frames:

```json
// Request
{"jsonrpc": "2.0", "method": "inject_envelope", "params": {...}, "id": 1}

// Response
{"jsonrpc": "2.0", "result": {...}, "id": 1}

// Server-push event (notification, no id)
{"jsonrpc": "2.0", "method": "trail_entry", "params": {...}}
```

### 3.2 Methods

All REST endpoints are available as JSON-RPC methods. Server-push events are sent for all streaming subscriptions.

### 3.3 Connection Lifecycle

1. Client connects via WebSocket upgrade on `GET /v1/ws`.
2. Client sends `authenticate` method with token.
3. Server validates, stores session.
4. Client subscribes to events via `subscribe` method.
5. Server pushes events as JSON-RPC notifications.
6. Ping/pong for keepalive (30s interval).
7. Client or server closes connection.

### 3.4 Implementation

```rust
pub struct WebSocketBinding {
    port: u16,  // shared with REST gateway via axum
}
```

The WebSocket handler is an axum route (`/v1/ws`) on the same server as the REST gateway. It upgrades the HTTP connection and manages the bidirectional channel.

---

## 4. Auth Providers

Three new implementations of the existing `Authenticator` trait.

### 4.1 API Key Authenticator

```rust
pub struct ApiKeyAuthenticator {
    keys: RwLock<HashMap<TokenDigest, ApiKeyEntry>>,  // TokenDigest = [u8; 32]
    rate_limiter: AuthRateLimiter,
}

struct ApiKeyEntry {
    identity: AgentIdentity,
    created_at: Instant,
    last_used: Option<Instant>,
}
```

API keys are long-lived, stored in config. Each key maps to an identity (workspace + role). The map is keyed by SHA-256 digest of the token (not the token itself), so the HashMap's internal SipHash operates over a digest — bucket-probe leakage carries no information about the original token. The post-lookup workspace-id check uses `subtle::ConstantTimeEq` over byte slices. The authenticator validates the key, updates `last_used`, and enforces rate limiting on failures.

### 4.2 Session Token Authenticator

```rust
pub struct SessionTokenAuthenticator {
    sessions: RwLock<HashMap<TokenDigest, SessionEntry>>,  // TokenDigest = [u8; 32]
    token_ttl: Duration,
    rate_limiter: AuthRateLimiter,
}

struct SessionEntry {
    user_id: UserId,
    created_at: Instant,
    expires_at: Instant,
    last_activity: Instant,
}
```

Session tokens are created at login, expire after a configurable TTL (default: 24 hours), and are renewed on activity. Used by the REST gateway and WebSocket binding.

**Methods:**
- `create_session(user_id) -> String` — create token, returns it.
- `validate_session(token) -> Result<UserId, AuthError>` — check token, update activity.
- `invalidate_session(token)` — explicit logout.
- `cleanup_expired()` — remove expired sessions (called periodically).

### 4.3 OAuth/OIDC Authenticator

```rust
pub struct OAuthAuthenticator {
    issuer_url: String,
    audience: String,
    jwks: RwLock<Option<JwkSet>>,
    rate_limiter: AuthRateLimiter,
}
```

Validates JWT bearer tokens against an OIDC provider's JWKS. The JWKS is fetched from `{issuer_url}/.well-known/openid-configuration` and cached with periodic refresh.

**Simplified initial implementation:** Validates JWT structure (header, payload, signature) and checks `iss`, `aud`, `exp` claims. Full JWKS signature verification uses the `jsonwebtoken` crate.

---

## 5. Crate Changes

**New crate:** `crates/wacp-security/` (§6 of security.md)

**Extended crate:** `crates/wacp-transport/`

```
crates/wacp-transport/src/
├── auth.rs                  # EXISTING: Authenticator trait, PskAuthenticator, AuthRateLimiter
├── auth_api_key.rs          # NEW: ApiKeyAuthenticator
├── auth_session.rs          # NEW: SessionTokenAuthenticator
├── auth_oauth.rs            # NEW: OAuthAuthenticator
├── rest_gateway.rs          # NEW: RestGateway (axum router, endpoints, SSE)
├── websocket.rs             # NEW: WebSocket binding (JSON-RPC, upgrade, events)
├── grpc_agent.rs            # UNCHANGED
├── grpc_highway.rs          # UNCHANGED
├── grpc_server.rs           # UNCHANGED
└── ...
```

**New dependencies for wacp-transport:** `axum` (already in workspace), `axum-extra` (SSE), `tokio-tungstenite` (WebSocket).

---

## 6. Test Requirements

| Module | Tests |
|--------|-------|
| `auth_api_key.rs` | Valid key → success. Invalid key → error. Rate limit after failures. last_used updated on success. |
| `auth_session.rs` | Create session → validate → success. Expired session → error. Invalidated session → error. Activity extends session. cleanup_expired removes old. |
| `auth_oauth.rs` | Valid JWT structure accepted. Expired JWT rejected. Wrong audience rejected. Wrong issuer rejected. |
| `rest_gateway.rs` | POST /v1/goals → 200 + body. GET /v1/workspaces/:id → 200. Unauthenticated → 401. Not found → 404. SSE /v1/events/trail streams events. |
| `websocket.rs` | Connect + authenticate. Send method → receive result. Subscribe → receive events. Unauthenticated disconnect. Ping/pong keepalive. |

**Total target: ~25 tests for transport extensions.** Combined with security crate (~30), Phase 23 total: ~55 tests.

---

## 7. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Protocol interface spec | §4–5 | §2 | gRPC service contracts mapped to REST |
| Protocol interface spec | §8 | §4 | Existing Authenticator trait |
| Runtime spec | §2 (boundaries) | §2 | Transport invariant |
| Security spec | §4 (secret store) | §4 | Token storage |
| LAYER-MAPPING.md | M4 | §1 | Transport architecture |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
