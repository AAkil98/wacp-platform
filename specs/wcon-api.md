---
id: wcon-api
type: design
status: final
created: 2026-04-10T00:00:00
revised: 2026-04-14T00:00:00
authors: [AAkil98]
tags: [api, rest, websocket, contract, auth]
depends_on: [wcon-auth, wcon-discovery, wcon-profiles, wcon-sessions, wcon-highway]
---

# WACP Console — API Surface

## Table of Contents

1. Overview
2. Conventions
3. Authentication
4. Error Model
5. Pagination
6. Discovery Endpoints
7. Profile Endpoints
8. Session Endpoints
9. Highway Endpoints
10. Settings Endpoints
11. System Endpoints
12. WebSocket Protocol
13. Invariants

---

## 1. Overview

The Console API is the contract between the console backend and the console frontend. It is a REST API for commands and queries, and a WebSocket API for real-time event streaming. The frontend is the only intended consumer — the API is not designed for third-party integration.

This spec consolidates every endpoint defined across the feature specs (`wcon-discovery`, `wcon-profiles`, `wcon-sessions`, `wcon-highway`) into a single reference. It adds the common patterns that cut across all endpoints: error model, pagination, authentication, and the WebSocket framing protocol.

### 1.1 Design Principles

1. **JSON everywhere.** Request and response bodies are JSON. No XML, no form encoding (except file upload for profile import).
2. **Predictable URLs.** Resources are nouns (`/api/profiles`, `/api/sessions`). Actions on resources are verbs at sub-paths (`/api/sessions/:id/launch`, `/api/profiles/:id/clone`).
3. **HTTP semantics.** GET for reads, POST for creates and actions, PUT for full replacement, PATCH for partial update, DELETE for removal. Status codes follow RFC 9110.
4. **Stateless requests.** Each request carries all context needed to process it. No server-side request session state. Authentication tokens are sent per-request.

### 1.2 Base Path

All endpoints are prefixed with `/api`. The frontend SPA is served from the root (`/`). Static assets are served from `/assets`. This separation allows a single HTTP server to serve both the API and the frontend.

## 2. Conventions

### 2.1 Request Headers

| Header | Required | Value |
|--------|----------|-------|
| `Content-Type` | For request bodies | `application/json` (or `multipart/form-data` for file uploads) |
| `Authorization` | When auth is enabled | `Bearer <api-key>` |
| `Accept` | Optional | `application/json` (default) |

### 2.2 Response Headers

| Header | Present | Value |
|--------|---------|-------|
| `Content-Type` | Always | `application/json` (or `application/x-yaml` for exports) |
| `X-Request-Id` | Always | UUID for request tracing |

### 2.3 Timestamps

All timestamps are ISO 8601 format with timezone: `2026-04-10T14:30:00.123Z`. The backend stores and returns UTC. The frontend handles timezone display.

### 2.4 Identifiers

All entity IDs are UUID v4 strings. Taxonomy identifiers (role IDs, tool names, vertical IDs) use their upstream format (e.g., `"swe:implementer"`, `"code_edit"`).

### 2.5 Empty Collections

Empty collections return `200 OK` with an empty `items` array, not `404 Not Found`.

### 2.6 Unknown Fields

The API ignores unknown fields in request bodies. This allows forward compatibility — the frontend can send fields that the backend doesn't yet recognize without causing errors.

## 3. Authentication & Auth Endpoints

All endpoints except `GET /api/health` and `POST /api/auth/login` require authentication. The full auth model is defined in `wcon-auth`; this section specifies how it maps to the API surface.

### 3.1 Authentication Mechanisms

Two mechanisms, handled by the authenticator middleware (`wcon-architecture` §8.1–8.2):

| Mechanism | Credential | Use case |
|-----------|-----------|----------|
| Browser session | `wcon_sid` cookie (HttpOnly, Secure, SameSite=Strict) | Interactive browser use |
| API token | `Authorization: Bearer wcon_t_...` header | Programmatic / automation use |

Both resolve to the same internal identity: `{ user_id, username, console_role }`. Unauthenticated requests receive `401`:

```json
{
  "error": "unauthenticated",
  "message": "Missing or invalid credentials"
}
```

### 3.2 CSRF Protection

State-changing requests (POST, PUT, PATCH, DELETE) from cookie-authenticated sessions must include the CSRF token as a header:

```
X-CSRF-Token: <value from wcon_csrf cookie>
```

Missing or mismatched CSRF tokens return `403 CSRF_VALIDATION_FAILED`. API token requests are exempt (`wcon-auth` §8).

### 3.3 Forced Password Change

When a user's `must_change_password` flag is true (bootstrap flow or admin-initiated reset), all endpoints except `POST /api/auth/change-password` and `POST /api/auth/logout` return `403 PASSWORD_CHANGE_REQUIRED`. The frontend must route to the password change screen.

### 3.4 WebSocket Authentication

WebSocket connections authenticate during the HTTP upgrade handshake via the `Cookie` header (browser sessions) or `Authorization` header (API tokens). Failed authentication rejects the upgrade with `401`. The connection inherits the authenticated identity for its lifetime.

### 3.5 Auth Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `POST` | `/api/auth/login` | none | Authenticate with username/password |
| `POST` | `/api/auth/logout` | required | Terminate browser session |
| `GET` | `/api/auth/whoami` | required | Return current user identity |
| `POST` | `/api/auth/change-password` | required | Change own password |

**Login request:**

```json
{
  "username": "admin",
  "password": "..."
}
```

**Login success response (`200`):**

```json
{
  "user": {
    "id": "uuid",
    "username": "admin",
    "display_name": "Administrator",
    "console_role": "admin",
    "must_change_password": false
  }
}
```

The response sets two cookies: `wcon_sid` (session) and `wcon_csrf` (CSRF token). When `must_change_password` is true, the frontend must redirect to the password change flow before any other operation.

**Login failure responses:**
- `401` — invalid credentials (does not reveal whether the username exists)
- `401 ACCOUNT_LOCKED` — account locked due to excessive failed attempts (`wcon-auth` §9)
- `429 TOO_MANY_REQUESTS` — IP-level rate limit exceeded

**Logout:** clears cookies, deletes the server-side session record.

**Whoami response (`200`):**

```json
{
  "id": "uuid",
  "username": "admin",
  "display_name": "Administrator",
  "console_role": "admin"
}
```

**Change password request:**

```json
{
  "current_password": "...",
  "new_password": "..."
}
```

**Change password failures:**
- `401` — current password incorrect
- `422 PASSWORD_TOO_WEAK` — new password fails policy (`wcon-auth` §7.2)

### 3.6 User Management Endpoints

Admin-only (`wcon-auth` §4.2). All return `403` for non-admin users.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/users` | List users |
| `POST` | `/api/users` | Create user |
| `GET` | `/api/users/:id` | Get user detail |
| `PATCH` | `/api/users/:id` | Update user (display_name, console_role) |
| `POST` | `/api/users/:id/disable` | Disable user (invalidates all sessions/tokens) |
| `POST` | `/api/users/:id/enable` | Re-enable a disabled user |
| `POST` | `/api/users/:id/reset-password` | Reset user's password (sets must_change_password) |
| `POST` | `/api/users/:id/unlock` | Clear temporary account lockout |

**Create user request:**

```json
{
  "username": "jane",
  "display_name": "Jane Doe",
  "password": "...",
  "console_role": "operator"
}
```

**Create user response (`201`):** user entity (password omitted).

**List filters:** `console_role`, `disabled` (boolean), `q` (search username/display_name). Default limit: 50.

**Guard:** disabling or demoting the last active admin returns `409 LAST_ADMIN`.

### 3.7 API Token Endpoints

Operators and admins can manage own tokens. Admins can manage any user's tokens (`wcon-auth` §4.2).

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/tokens` | List own tokens (admin: list all or filter by user) |
| `POST` | `/api/tokens` | Create token for self |
| `DELETE` | `/api/tokens/:id` | Revoke token |

**Create token request:**

```json
{
  "name": "CI pipeline"
}
```

**Create token response (`201`):**

```json
{
  "id": "uuid",
  "name": "CI pipeline",
  "token": "wcon_t_abc123...",
  "created_at": "2026-04-14T10:00:00Z"
}
```

The `token` field is returned **exactly once** at creation. Subsequent reads return only the token metadata (name, created_at, last_used_at, revoked status).

**List filters:** `user_id` (admin only, filter to a specific user's tokens).

### 3.8 Audit Log Endpoint

Admin-only (`wcon-auth` §10.3).

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/audit-log` | Query audit log entries |

**Filters:** `user_id`, `action`, `target_kind`, `target_id`, `since` (ISO 8601), `until` (ISO 8601). Paginated via the standard cursor model (§5). Default limit: 50, cap: 200.

**Response item:**

```json
{
  "id": "uuid",
  "user_id": "uuid",
  "username": "admin",
  "timestamp": "2026-04-14T10:00:00Z",
  "action": "profile.create",
  "target_kind": "profile",
  "target_id": "uuid",
  "detail": { "name": "Fast Implementer" },
  "ip": "192.168.1.10",
  "user_agent": "Mozilla/5.0 ..."
}
```

The `username` field is a denormalized convenience (joined from the users table at query time) so the frontend does not need a second round-trip to resolve user IDs.

## 4. Error Model

All errors follow a consistent structure.

### 4.1 Error Response

```json
{
  "error": "error_code",
  "message": "Human-readable description",
  "details": { ... }
}
```

| Field | Type | Present | Description |
|-------|------|---------|-------------|
| `error` | string | Always | Machine-readable error code (snake_case) |
| `message` | string | Always | Human-readable description |
| `details` | object | Conditional | Additional structured data (varies by error type) |

### 4.2 HTTP Status Codes

| Code | Meaning | When used |
|------|---------|-----------|
| `200` | OK | Successful read or update |
| `201` | Created | Successful resource creation |
| `202` | Accepted | Async operation started (session launch) |
| `204` | No Content | Successful deletion |
| `400` | Bad Request | Malformed JSON, invalid query parameters, parse failures |
| `401` | Unauthorized | Missing or invalid authentication; account locked (`ACCOUNT_LOCKED`) |
| `403` | Forbidden | Authenticated but insufficient permissions; CSRF failure; forced password change |
| `404` | Not Found | Resource does not exist |
| `409` | Conflict | State conflict (e.g., launching an already-launched session, deleting a profile assigned to an active session) |
| `422` | Unprocessable Entity | Validation failure (well-formed request, invalid content) |
| `429` | Too Many Requests | Rate limit exceeded (directive injection, login attempts) |
| `500` | Internal Server Error | Unexpected server error |
| `502` | Bad Gateway | WACP runtime unreachable or returned an error |
| `503` | Service Unavailable | Backend not ready (startup, taxonomy loading) |

### 4.3 Validation Errors (422)

Validation failures return all violations in a single response (`wcon-profiles` §3.5, `wcon-sessions` §3.1):

```json
{
  "error": "validation_failed",
  "message": "Profile validation failed",
  "violations": [
    {
      "field": "role_ref",
      "code": "UNKNOWN_ROLE",
      "message": "Role 'swe:architect' does not exist in the taxonomy",
      "value": "swe:architect"
    }
  ],
  "warnings": []
}
```

**Known error codes by category:**

| Category | Codes | Source |
|----------|-------|--------|
| Profile validation | `UNKNOWN_ROLE`, `UNKNOWN_TOOL`, `TOOL_NOT_IN_ROLE_VERTICAL`, `EMPTY_TOOL_SET`, `INVALID_NAME`, `DUPLICATE_NAME`, `INVALID_PROVIDER`, `INVALID_MODEL`, `INVALID_TEMPERATURE`, `INVALID_MAX_TOKENS`, `INVALID_AUTONOMY`, `INVALID_THRESHOLD`, `INVALID_BUDGET`, `INVALID_TAGS` | `wcon-profiles` §3.1–§3.3 |
| Profile warnings | `TOOL_HAS_RUNTIME_POLICY`, `TOOL_NOT_IN_ROLE_VERTICAL` on denylist (non-blocking), autonomous-worker warning (non-blocking) | `wcon-profiles` §3.2, §3.3 |
| Session validation | `UNKNOWN_VERTICAL`, `UNKNOWN_WORKFLOW`, `MISSING_ASSIGNMENT`, `UNKNOWN_PROFILE`, `UNKNOWN_VERSION`, `ROLE_MISMATCH`, `INVALID_PROFILE`, `MISSING_CONTEXT`, `INVALID_CONTEXT`, `INVALID_BUDGET`, `RUNTIME_UNREACHABLE` | `wcon-sessions` §3.1 |
| Auth errors | `UNAUTHENTICATED`, `ACCOUNT_LOCKED`, `FORBIDDEN`, `PASSWORD_CHANGE_REQUIRED`, `CSRF_VALIDATION_FAILED`, `PASSWORD_TOO_WEAK`, `LAST_ADMIN` | `wcon-auth` §3–§9 |
| Tool-layer refusal relay | `TOOL_POLICY_VIOLATION` (generic), plus specific codes `COMPLIANCE_NOT_APPROVED`, `PHI_ACCESS_NOT_GRANTED`, `HYPOTHESIS_NOT_DECLARED`, `COMPUTE_BUDGET_EXCEEDED`, `ENVIRONMENT_GATE_REQUIRED`, `SQL_DESTRUCTIVE_GATE_REQUIRED`, `CLASSIFICATION_BLOCKED` | `wcon-highway` §4A.1 — these are not API errors but trail-relayed refusals surfaced in the refusals channel |

**Violations vs. warnings.** Violations block the operation (the profile is not saved, the session does not launch). Warnings do not block — they are attached to the successful response body for the client to surface (`wcon-profiles` §3.5, `wcon-ui` §5.4). The same code can appear in both sections depending on context — for example, `TOOL_NOT_IN_ROLE_VERTICAL` is a violation when it appears in the allowlist but a warning when it appears in the denylist (denying an unavailable tool is harmless).

### 4.4 Runtime Errors (502)

When the WACP runtime returns an error or is unreachable:

```json
{
  "error": "runtime_error",
  "message": "WACP runtime returned an error",
  "details": {
    "grpc_status": "UNAVAILABLE",
    "grpc_message": "Connection refused",
    "service": "CoordinatorService",
    "method": "CreateSession"
  }
}
```

## 5. Pagination

List endpoints use cursor-based pagination (`wcon-discovery` §4.2).

### 5.1 Request Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `limit` | integer | Endpoint-specific (20–50) | Items per page |
| `cursor` | string | — | Opaque cursor from previous response |

### 5.2 Response Envelope

All list endpoints return:

```json
{
  "items": [ ... ],
  "cursor": "opaque-string",
  "has_more": true
}
```

| Field | Type | Description |
|-------|------|-------------|
| `items` | array | Page of results |
| `cursor` | string or null | Cursor for next page; null if no more pages |
| `has_more` | boolean | Whether more pages exist |

### 5.3 Cursor Encoding

Cursors are base64-encoded, opaque to the client. The backend encodes the sort key of the last item in the current page. Cursors are stable across requests but not across data mutations — a cursor obtained before a delete may skip or duplicate entries.

## 6. Discovery Endpoints

Source spec: `wcon-discovery` §4–§5.

### 6.1 Roles

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/roles` | List roles |
| `GET` | `/api/roles/:id` | Get role detail |

**List filters:** `base_role` (string), `vertical` (string)

**Detail response includes:** role definition, resolved tool list, vertical membership, sendable/receivable envelope types, creatable checkpoint types.

### 6.2 Tools

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/tools` | List tools |
| `GET` | `/api/tools/:name` | Get tool detail |

**List filters:** `vertical` (string, filter to tools in a vertical), `has_policy` (boolean, filter to tools with / without a tool-layer policy)

**Detail response includes:** tool definition (name, description), owning vertical, roles associated with the tool (per `wcon-discovery` §3.4), and the resolved `policy` object when the tool has a tool-layer policy declared by its vertical.

### 6.3 Types

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/envelope-types` | List envelope types |
| `GET` | `/api/envelope-types/:name` | Get envelope type detail |
| `GET` | `/api/checkpoint-types` | List checkpoint types |
| `GET` | `/api/checkpoint-types/:name` | Get checkpoint type detail |

**Envelope type list filters:** `sender_role` (string), `receiver_role` (string)

**Checkpoint type list filters:** `allowed_role` (string)

### 6.4 Verticals

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/verticals` | List verticals (summaries) |
| `GET` | `/api/verticals/:id` | Get vertical detail (full manifest projection) |
| `GET` | `/api/verticals/:id/workflows` | List workflows for a vertical |
| `GET` | `/api/verticals/:id/workflows/:wf_id` | Get workflow detail |
| `GET` | `/api/verticals/:id/context-schema` | Get the vertical's context schema (for the session launcher step 4) |
| `GET` | `/api/verticals/:id/task-types` | Get the vertical's task types with keywords |
| `GET` | `/api/verticals/:id/quality-criteria` | Get the vertical's quality criteria rubric |
| `GET` | `/api/verticals/:id/tool-policies` | Get the vertical's tool-layer policies |
| `GET` | `/api/verticals/:id/checkpoint-types` | Get the vertical's declared checkpoint type schemas |

**List response (`GET /api/verticals`):** array of summaries, each containing `id`, `name`, `defining_constraint`, `task_type_count`, `workflow_count`, `tool_count`. This mirrors the upstream `VerticalSummary` (`wcon-discovery` §2.2.1) so the frontend can render the step 1 wizard cards without a second round trip.

**Detail response (`GET /api/verticals/:id`):** the full indexed `VerticalEntry` (`wcon-data-model` §6.1) including `defining_constraint`, `context_schema`, `tool_policies`, `checkpoint_types`, `quality_criteria`, `task_types`, `workflows`, `default_profiles`, and the tools list. Equivalent to a pass-through of the upstream `GET /v1/verticals/{id}` with the Console's forward-compat projection applied.

**Workflow detail:** full stage list with role references, dependencies, and gate flags **when per-stage detail is available**. The upstream manifest does not include per-stage detail today, so this endpoint may return summary-only content for some verticals — the frontend handles this by rendering workflow cards from summary data (`wcon-ui` §6.2 step 2).

**Context schema (`GET /api/verticals/:id/context-schema`):** returns the `context_schema` map as a standalone payload for the session launcher's step 4. Response:

```json
{
  "fields": {
    "compliance_scope": {
      "type": "string",
      "required": true,
      "description": "Regulatory scope for trades in this session."
    },
    "jurisdiction": {
      "type": "enum",
      "required": true,
      "description": "Regulatory jurisdiction governing trades.",
      "enum_values": ["SEC", "FINRA", "MiFID II", "FCA", "other"]
    }
  }
}
```

Returns `{ "fields": {} }` for verticals with an empty `context_schema`. This sub-endpoint is a convenience for the launcher's dynamic form — the same data is available via `GET /api/verticals/:id`.

**Task types, quality criteria, tool policies, checkpoint types:** each returns the corresponding slice of the vertical manifest. These sub-endpoints exist so the frontend can fetch only what a specific view needs without pulling the full manifest on every navigation.

### 6.5 Search

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/search` | Search across all entity types |

**Parameters:** `q` (required, min 2 chars), `type` (optional entity type filter), `limit` (per-type, default 10, cap 50)

**Response:** results grouped by entity type, ranked by match quality (`wcon-discovery` §5.4).

### 6.6 Taxonomy Reload

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/taxonomy/reload` | Trigger taxonomy index rebuild |

**Response:** reload status, duration, entity counts, warnings, errors (`wcon-discovery` §7.4).

## 7. Profile Endpoints

Source spec: `wcon-profiles` §2, §5–§8.

### 7.1 CRUD

| Method | Path | Description | Success |
|--------|------|-------------|---------|
| `POST` | `/api/profiles` | Create profile | `201` |
| `GET` | `/api/profiles` | List live profiles (soft-deleted filtered out) | `200` |
| `GET` | `/api/profiles/:id` | Get live profile (current version); returns `404` if the profile is soft-deleted | `200` |
| `PUT` | `/api/profiles/:id` | Update live profile (creates new version); returns `404` if the profile is soft-deleted | `200` |
| `DELETE` | `/api/profiles/:id` | Soft-delete profile (sets `deleted_at`; rows retained for FK integrity with historical sessions). Returns `404` if already soft-deleted. Returns `409` if the profile is assigned to an active session | `204` |

**List filters:** `role_ref`, `vertical`, `tag`, `q` (name/description search), `sort` (`name`, `created_at`, `role_ref`), `order` (`asc`, `desc`). Default limit: 50, cap: 200. The list only returns live profiles (`is_current = 1 AND deleted_at IS NULL`, `wcon-profiles` §8.1). Historical session detail views can still resolve deleted profiles by `(profile_id, profile_version)` directly — they do not go through this endpoint.

**Get response includes:** all fields, derived fields (`role_name`, `vertical`, `available_tools`, `policy_gated_tools`, `version_count`), `is_valid` flag.

**Create/Update request body:**

```json
{
  "name": "Fast Implementer",
  "description": "High-autonomy implementer with aggressive budget",
  "tags": ["swe", "fast"],
  "role_ref": "swe:implementer",
  "llm_provider": "anthropic",
  "llm_model": "claude-sonnet-4-20250514",
  "llm_temperature": 0.3,
  "llm_max_tokens": 8192,
  "autonomy": "autonomous",
  "tool_allowlist": ["code_edit", "file_read", "file_write", "terminal"],
  "tool_denylist": null,
  "budget_max_cost_micros": 500000,
  "budget_max_tokens": 100000,
  "budget_max_wall_time_ms": 300000,
  "budget_warning_threshold": 0.8
}
```

### 7.2 Versioning

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/profiles/:id/versions` | List version history (returns `404` for soft-deleted profiles) |
| `GET` | `/api/profiles/:id/versions/:version` | Get specific version (returns `404` for soft-deleted profiles or non-existent versions) |
| `POST` | `/api/profiles/:id/rollback` | Rollback to a previous version (returns `404` for soft-deleted profiles; returns `422` if the target version fails current-taxonomy validation) |

**Rollback request:** `{ "target_version": 3 }`

**Rollback response:** the new version entity (created from the target version's fields).

### 7.3 Clone

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/profiles/:id/clone` | Clone profile |

**Request (optional):** `{ "name": "Fast Implementer (copy)" }`

**Response:** `201` with the new profile entity.

### 7.4 Import / Export

| Method | Path | Description | Content-Type |
|--------|------|-------------|-------------|
| `GET` | `/api/profiles/:id/export` | Export current version as YAML | Response: `application/x-yaml` |
| `GET` | `/api/profiles/:id/versions/:version/export` | Export specific version as YAML | Response: `application/x-yaml` |
| `POST` | `/api/profiles/import` | Import profile from YAML | Request: `multipart/form-data` |

**Import response:** `201` with the new profile entity. `422` if validation fails (response includes parsed fields with errors highlighted).

### 7.5 Bulk Operations

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/profiles/bulk-delete` | Delete multiple profiles |
| `POST` | `/api/profiles/bulk-export` | Export multiple profiles as ZIP |

**Bulk delete request:** `{ "ids": ["uuid-1", "uuid-2"] }`

**Bulk delete response:** `{ "deleted": [...], "failed": [{ "id": "...", "reason": "..." }] }`

**Bulk export response:** `application/zip` containing one YAML file per profile.

## 8. Session Endpoints

Source spec: `wcon-sessions` §2–§7, §9.

### 8.1 Lifecycle

| Method | Path | Description | Success |
|--------|------|-------------|---------|
| `POST` | `/api/sessions` | Create session (configuring state) | `201` |
| `GET` | `/api/sessions` | List sessions | `200` |
| `GET` | `/api/sessions/:id` | Get session detail | `200` |
| `PATCH` | `/api/sessions/:id` | Update session overrides | `200` |
| `POST` | `/api/sessions/:id/launch` | Validate and launch | `202` |
| `POST` | `/api/sessions/:id/cancel` | Cancel session (from any non-terminal state) | `200` |
| `POST` | `/api/sessions/:id/clone` | Clone session configuration | `201` |

**List filters:** `state`, `vertical`, `sort` (`created_at`, `launched_at`, `state`), `order` (`desc`, `asc`). Default limit: 20, cap: 100.

**Create request:**

```json
{
  "vertical": "finance",
  "workflow": "finance:trade-execution",
  "context": {
    "compliance_scope": "equities",
    "jurisdiction": "SEC"
  }
}
```

The `context` field is a JSON object carrying vertical-specific context tags keyed by the field names declared in `VerticalEntry.context_schema` for the selected vertical. It is optional at create time — the session may be created without context and updated later via `PATCH`. It is required to be complete and valid at launch time per `wcon-sessions` §3.1 (`MISSING_CONTEXT` / `INVALID_CONTEXT`).

For verticals with an empty `context_schema` (e.g., SWE), the `context` field may be omitted:

```json
{
  "vertical": "swe",
  "workflow": "swe:implement-feature"
}
```

**PATCH request (update overrides and/or context):**

```json
{
  "budget_max_cost_micros": 1000000,
  "context": {
    "compliance_scope": "fixed-income",
    "jurisdiction": "FINRA"
  }
}
```

A `PATCH` with a `context` field replaces the stored context map wholesale — partial context updates are not supported. Valid only while the session is in `configuring` state; returns `409 Conflict` otherwise.

**Launch response on success:** `202 Accepted` with session in `launching` state.

**Launch response on failure:** `422` with validation violations (`wcon-sessions` §3.2), which may now include `MISSING_CONTEXT` and `INVALID_CONTEXT`:

```json
{
  "error": "validation_failed",
  "message": "Session launch validation failed",
  "violations": [
    {
      "check": "MISSING_CONTEXT",
      "field": "jurisdiction",
      "required_by_vertical": "finance",
      "message": "Field 'jurisdiction' is required by vertical 'finance' but is not set"
    },
    {
      "check": "INVALID_CONTEXT",
      "field": "compliance_scope",
      "value": null,
      "expected_type": "string",
      "message": "Field 'compliance_scope' must be a non-null string"
    }
  ],
  "warnings": []
}
```

### 8.2 Assignments

| Method | Path | Description |
|--------|------|-------------|
| `PUT` | `/api/sessions/:id/assignments` | Set all role assignments |

The request body depends on the session's slot derivation mode (`wcon-sessions` §2.4). Mode B (current default) uses a flat `role_ref → profile_id` mapping; Mode A additionally carries `stage_id` on each entry. See `wcon-sessions` §2.2 for the full body shapes and the Mode A vs Mode B selection rule.

**Example (Mode B):**

```json
{
  "assignments": [
    {
      "role_ref": "finance:analyst",
      "profile_id": "uuid-1"
    },
    {
      "role_ref": "finance:portfolio_manager",
      "profile_id": "uuid-2",
      "budget_max_cost_micros": 200000
    }
  ]
}
```

**Example (Mode A):**

```json
{
  "assignments": [
    { "role_ref": "finance:analyst",  "stage_id": "analyze",    "profile_id": "uuid-1" },
    { "role_ref": "finance:compliance_officer", "stage_id": "compliance", "profile_id": "uuid-3" },
    { "role_ref": "finance:analyst",  "stage_id": "review",     "profile_id": "uuid-4" }
  ]
}
```

Only valid while the session is in `configuring` state. Returns `409` otherwise. Mixing Mode A and Mode B assignments in the same request body returns `422 Unprocessable Entity`.

### 8.3 Monitoring

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/sessions/:id/state` | Current in-memory session state |
| `GET` | `/api/sessions/:id/trail` | Trail entries from buffer |
| `GET` | `/api/sessions/:id/refusals` | Pending tool-layer refusals |

**State response includes:** workspace states, task states, pending gate count, pending escalation count, pending refusal count, aggregate resource usage, and a snapshot of `config.context` (the session's vertical context tag map) for the frontend to render context badges in the dashboard header.

**Trail parameters:** `workspace_id`, `event_type`, `since` (ISO 8601), `limit` (default 100, cap 500).

**Refusals response:** `{ "items": [RefusalEvent, ...] }` where each `RefusalEvent` matches `wcon-sessions` §6.5 / `wcon-highway` §4A.2 (tool name, policy kind, error code, reason, policy reference, unblock hint, trail entry id, created_at). Refusals are not paginated — the list is bounded by the number of blocked workspaces in the session.

### 8.4 Real-Time Stream

| Protocol | Path | Description |
|----------|------|-------------|
| `WebSocket` | `/api/sessions/:id/stream` | Real-time event stream |

See §12 for WebSocket protocol details.

## 9. Highway Endpoints

Source spec: `wcon-highway` §4–§6.

### 9.1 Gates

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/gates/pending` | List pending gates across sessions |
| `POST` | `/api/sessions/:sid/gates/:gid/resolve` | Resolve a gate |
| `POST` | `/api/sessions/:sid/gates/batch-resolve` | Resolve multiple gates |

**Pending gates filters:** `session_id`, `type`, `sort` (`urgency`, `timeout`, `created_at`).

**Resolve request:**

```json
{
  "decision": "approve",
  "reason": "Looks good, proceed",
  "modifications": null
}
```

`decision`: `"approve"`, `"reject"`, or `"modify"`. `modifications` required when decision is `"modify"`.

**Batch resolve request:**

```json
{
  "resolutions": [
    { "gate_id": "gate-1", "decision": "approve", "reason": "Batch approved" },
    { "gate_id": "gate-2", "decision": "reject", "reason": "Out of scope" }
  ]
}
```

**Batch resolve response:** per-gate success/failure.

### 9.2 Escalations

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/escalations/pending` | List pending escalations across sessions |
| `POST` | `/api/sessions/:sid/escalations/:eid/respond` | Respond to escalation |

**Pending escalations filters:** `session_id` (omit for all), `sort` (`age` for oldest-first, `created_at`). Default sort: `age`.

**Respond request:**

```json
{
  "response": "Use token-based auth per ADR-007",
  "attachments": []
}
```

### 9.3 Refusals (Read-Only)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/refusals/pending` | List pending tool-layer refusals across sessions |

**Pending refusals filters:** `session_id` (omit for all), `policy_kind` (filter to a single `ToolPolicyKind`), `sort` (`created_at` default, `tool_name`).

Response body is `{ "items": [RefusalEvent, ...] }` using the same `RefusalEvent` shape as `GET /api/sessions/:id/refusals` (§8.3) and `wcon-highway` §4A.2.

There is no `POST /api/sessions/:sid/refusals/:rid/resolve` endpoint — refusals cannot be resolved directly by the Console (`wcon-highway` §10.8). This section is read-only; it exists so the Oversight nav badge can aggregate refusal counts across sessions and so the session selector (`wcon-ui` §7.1) can display the "Refs." column.

### 9.4 Injection

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/sessions/:sid/inject` | Inject directive into workspace |

**Request:**

```json
{
  "workspace_id": "ws-uuid",
  "payload": "Focus on authentication module only",
  "envelope_type": "feedback"
}
```

**Constraints:** workspace must be `ACTIVE` or `BLOCKED`, payload non-empty, max 64 KB, rate limit 10/min per session (`wcon-highway` §6.3).

Rate limit exceeded returns `429`:

```json
{
  "error": "rate_limited",
  "message": "Injection rate limit exceeded (10/min)",
  "details": {
    "retry_after_ms": 4200
  }
}
```

## 10. Settings Endpoints

### 10.1 CRUD

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/settings` | Get all settings |
| `GET` | `/api/settings/:key` | Get a specific setting |
| `PUT` | `/api/settings/:key` | Set a specific setting |
| `DELETE` | `/api/settings/:key` | Reset setting to default |

**Get all response:**

```json
{
  "settings": {
    "runtime.agent_address": "[::1]:9090",
    "runtime.highway_address": "[::1]:9091",
    "runtime.coordinator_address": "[::1]:9092",
    "runtime.rest_address": "http://[::1]:9093",
    "runtime.auth_method": "none",
    "runtime.auth_credential": "",
    "taxonomy.path": "../wacp/protocol/taxonomy",
    "export.directory": "./exports",
    "ui.theme": "system",
    "ui.trail_buffer_size": 1000,
    "auth.session_ttl_hours": 24
  }
}
```

Absent keys return their default values per `wcon-data-model` §5.2. The response always includes every known key, even ones that were never explicitly set — the defaults are materialized server-side. Unknown keys (user-added settings not in the known-keys table) are returned alongside known keys.

**Set request:**

```json
{
  "value": "dark"
}
```

Values are JSON-encoded (`wcon-data-model` §5). The backend validates known keys against their expected types. Unknown keys are accepted and stored without type validation (extensibility).

## 11. System Endpoints

### 11.1 Health

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/health` | Health check |

**Response:**

```json
{
  "status": "healthy",
  "checks": {
    "database": "ok",
    "taxonomy_index": "ok",
    "runtime_agent": "ok",
    "runtime_highway": "ok",
    "runtime_coordinator": "ok",
    "runtime_rest": "ok"
  },
  "version": "0.1.0"
}
```

| `status` | Meaning |
|----------|---------|
| `healthy` | All checks pass: database reachable, taxonomy index built, both runtime transports reachable |
| `degraded` | Database and taxonomy OK, but at least one runtime endpoint is unreachable. Per-service checks (`runtime_agent`, `runtime_highway`, `runtime_coordinator`, `runtime_rest`) report independently. Any gRPC service down degrades session launch and oversight; `runtime_rest` down prevents vertical registry refresh, though the last-loaded registry remains usable. |
| `unhealthy` | Database or taxonomy index unavailable — the Console cannot serve any meaningful request |

Each check returns one of: `ok`, `unreachable` (connection failed), `error` (connection OK but returned an error), `degraded` (partial — e.g., taxonomy index loaded protocol taxonomy but zero verticals).

The health endpoint does not require authentication. It is used by load balancers and monitoring tools.

### 11.2 Info

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/info` | System information |

**Response:**

```json
{
  "version": "0.1.0",
  "auth_mode": "local",
  "taxonomy_loaded": true,
  "taxonomy_entity_counts": {
    "roles": 34,
    "tools": 67,
    "envelope_types": 3,
    "checkpoint_types": 2,
    "vertical_checkpoint_types": 6,
    "verticals": 7
  },
  "active_sessions": 2
}
```

Counts are illustrative; they reflect whatever the runtime served at the last taxonomy reload. `vertical_checkpoint_types` counts checkpoint types declared by verticals (Finance `compliance_check`, Healthcare `phi_access_grant`, etc.), distinct from the `checkpoint_types` field which counts protocol-level custom types.

## 12. WebSocket Protocol

### 12.1 Connection

The frontend establishes a WebSocket connection to observe a session's real-time events:

```
GET /api/sessions/:id/stream
Upgrade: websocket
Connection: Upgrade
Cookie: wcon_sid=<session-token>
```

Or, for API token authentication:

```
GET /api/sessions/:id/stream
Upgrade: websocket
Connection: Upgrade
Authorization: Bearer wcon_t_<token>
```

The backend verifies authentication during the upgrade handshake (§3.4). Authorization is also checked — operators can only stream their own sessions; admins can stream any session (`wcon-auth` §4.2). If the session does not exist, the upgrade is rejected with `404`. If the session is not in `active` state, `409`. If the user lacks access, `403`.

### 12.2 Server-to-Client Frames

All server-to-client messages are JSON frames with a common envelope:

```json
{
  "channel": "trail" | "gates" | "escalations" | "refusals" | "workspaces" | "session" | "notification",
  "session_id": "uuid",
  "timestamp": "2026-04-10T14:30:00.123Z",
  "event": { ... }
}
```

#### Channel: `trail`

Trail entry events. Event structure: enriched trail entry (`wcon-highway` §3.1).

#### Channel: `gates`

Gate lifecycle events:

| Event type | Payload |
|-----------|---------|
| `gate_opened` | Full enriched gate event (`wcon-highway` §4.1) |
| `gate_resolved` | `{ gate_id, decision, resolved_by, reason }` |
| `gate_timeout` | `{ gate_id, fallback_action }` |

#### Channel: `escalations`

Escalation lifecycle events:

| Event type | Payload |
|-----------|---------|
| `escalation_opened` | Full enriched escalation event (`wcon-highway` §5.1) |
| `escalation_resolved` | `{ escalation_id, resolved_by }` |

#### Channel: `refusals`

Tool-layer refusal lifecycle events (`wcon-highway` §4A):

| Event type | Payload |
|-----------|---------|
| `refusal_opened` | Full enriched refusal event (`wcon-highway` §4A.2) |
| `refusal_resolved` | `{ refusal_id, resolved_by: "checkpoint_created" \| "tool_retry_succeeded" \| "workspace_transitioned" \| "session_cancelled" }` |

Refusals do not carry a `resolved_via_console` flag because the Console does not provide a direct resolution mechanism — refusals resolve when upstream conditions change. The `resolved_by` field describes what the session monitor observed that caused it to drop the refusal from `pending_refusals`.

#### Channel: `workspaces`

Workspace state change events:

```json
{
  "workspace_id": "ws-uuid",
  "workspace_label": "swe:implementer (Fast Implementer)",
  "previous_state": "ACTIVE",
  "new_state": "BLOCKED",
  "reason": "Gate: task_approval",
  "resource_usage": { "tokens": 12000, "cost_micros": 45000, "wall_time_ms": 8200 }
}
```

#### Channel: `session`

Session-level lifecycle events:

| Event type | Payload |
|-----------|---------|
| `session_active` | `{ launched_at }` |
| `session_completed` | `{ closed_at }` |
| `session_failed` | `{ closed_at, reason }` |
| `session_cancelled` | `{ closed_at }` |

#### Channel: `notification`

Cross-cutting notification events (`wcon-highway` §9.1):

```json
{
  "type": "gate_timeout_warning",
  "priority": "high",
  "title": "Gate expiring",
  "message": "task_approval gate in session 'Auth Feature' — 2m remaining",
  "reference": { "gate_id": "gate-uuid", "session_id": "uuid" }
}
```

### 12.3 Client-to-Server Frames

The WebSocket connection is primarily server-to-client. The client sends only control frames:

| Frame type | Payload | Effect |
|-----------|---------|--------|
| `ping` | `{}` | Server responds with `pong` |
| `subscribe` | `{ "channels": ["trail", "gates"] }` | Filter to specific channels (default: all) |
| `unsubscribe` | `{ "channels": ["trail"] }` | Stop receiving events on specified channels |

Channel subscription reduces bandwidth when the frontend only needs specific event types (e.g., a gate-only view).

### 12.4 Connection Lifecycle

| Event | Behavior |
|-------|----------|
| Session enters terminal state | Server sends final `session` event, then closes WebSocket with code `1000` (normal) |
| Backend shutdown | Server closes with code `1001` (going away) |
| Client disconnect | Server cleans up subscriber entry; no effect on session |
| Authentication revoked | Server closes with code `4001` (auth revoked) |
| Invalid session ID | Upgrade rejected with HTTP `404` |

### 12.5 Heartbeat

The server sends a WebSocket ping frame every 30 seconds. If the client does not respond with a pong within 10 seconds, the server considers the connection dead and closes it. The frontend should implement automatic reconnection (`wcon-sessions` §8.3).

## 13. Invariants

### 13.1 Consistent Error Shape

Every non-2xx response returns a JSON body with at minimum `error` and `message` fields. No endpoint returns a bare status code without a body. No endpoint returns HTML error pages.

### 13.2 Idempotent Reads

All GET requests are safe and idempotent. They produce no side effects and return the same result for the same input (modulo concurrent writes).

### 13.3 Content-Type Fidelity

The response `Content-Type` always matches the actual body format. JSON responses are `application/json`. YAML exports are `application/x-yaml`. ZIP exports are `application/zip`. No mismatches.

### 13.4 Authentication Consistency

Every endpoint except `GET /api/health` and `POST /api/auth/login` requires a valid credential. No endpoint is accidentally public. WebSocket connections are authenticated at upgrade time. Authorization is enforced at the backend for every request — the frontend hides affordances as a convenience, not a security measure (`wcon-auth` §4.3).

### 13.5 State Transition Safety

Endpoints that trigger state transitions (`launch`, `cancel`) verify the current state before proceeding. Concurrent calls to the same state-changing endpoint return `409` for the loser — state transitions are serialized per entity.

### 13.6 Pagination Completeness

Iterating through all pages of a paginated endpoint (following `cursor` until `has_more` is false) yields every entity that existed at the start of iteration, subject to concurrent mutations. No entity is silently omitted by the pagination mechanism.

## Endpoint Summary

Every row counts distinct (method, path-template) pairs. `WebSocket /api/sessions/:id/stream` is counted as one session endpoint.

| Count | Category | Endpoints |
|-------|----------|-----------|
| 16 | Auth | 4 auth (login, logout, whoami, change-password), 8 user management (list, create, get, patch, disable, enable, reset-password, unlock), 3 tokens (list, create, revoke), 1 audit log |
| 19 | Discovery | 2 for roles, 2 for tools, 2 for envelope types, 2 for protocol checkpoint types, 2 for verticals (list + detail), 7 for per-vertical sub-endpoints (workflows, workflow detail, context-schema, task-types, quality-criteria, tool-policies, checkpoint-types), search, taxonomy reload |
| 14 | Profiles | 5 CRUD, 3 versioning (list, get, rollback), 1 clone, 3 import/export (export current, export version, import), 2 bulk (delete, export) |
| 12 | Sessions | 8 lifecycle (create, list, get, patch, assignments, launch, cancel, clone), 3 monitoring (state, trail, refusals), 1 real-time stream |
| 7 | Highway | 1 cross-session gate list, 2 gate resolution (single + batch), 1 cross-session escalation list, 1 escalation respond, 1 cross-session refusal list, 1 injection |
| 4 | Settings | get all, get one, set one, delete one |
| 2 | System | health, info |
| **74** | **Total** | |

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-auth | Authentication & Authorization | defines identity model, auth mechanisms, permission matrix, audit log — §3 of this spec implements its API surface |
| wcon-discovery | Agent & Role Discovery | defines discovery query model (§4), search model (§5), reload (§7) |
| wcon-profiles | Profile System | defines profile lifecycle (§2), validation (§3), versioning (§5), import/export (§7), library operations (§8) |
| wcon-sessions | Session Lifecycle | defines session configuration (§2), validation (§3), launch (§4), monitoring (§6), teardown (§7) |
| wcon-highway | Highway Integration | defines gate resolution (§4), escalation handling (§5), directive injection (§6), event structure (§3, §7) |
| wcon-architecture | System Architecture | defines HTTP server, WebSocket server, communication patterns (§3, §4.1) |
| wcon-data-model | Data Model | defines settings schema (§5), profile schema (§3), session schema (§4) |

*WACP Console -- authored by AAkil98*
