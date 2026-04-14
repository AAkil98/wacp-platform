---
id: wcon-auth
type: design
status: final
created: 2026-04-14T00:00:00
authors: [AAkil98]
tags: [auth, security, identity, authorization, audit]
depends_on: [wcon-vision, wcon-glossary, wcon-architecture]
---

# WACP Console — Authentication & Authorization

## Table of Contents

1. Purpose & Scope
2. Identity Model
3. Authentication
4. Authorization
5. Ownership & Visibility
6. Bootstrap Flow
7. Password Policy
8. CSRF Protection
9. Rate Limiting & Account Lockout
10. Audit Log
11. Threat Model
12. OIDC Extension Path
13. Invariants

---

## 1. Purpose & Scope

This spec defines how the Console identifies users, authenticates requests, authorizes actions, and records mutations. It is the canonical reference for all auth-related behavior — every other spec that touches identity, access control, or audit defers to this document.

### 1.1 Motivation

The original spec set deferred multi-user auth to a future phase (`wcon-architecture` §8). `TECH_STACK_PROPOSAL.md` Q2 reversed that decision: multi-user auth ships in Phase 1. The rationale is structural — retrofitting auth after single-user launch means migrating every table to add `owner_user_id`, changing every API contract under live users, and grafting a login screen onto single-user assumptions. Shipping auth from day one avoids all three.

### 1.2 Scope

**In scope:**
- Local identity store (users, passwords)
- Two authentication mechanisms: browser sessions (cookie-based) and API tokens (bearer)
- Three-level authorization hierarchy: admin, operator, viewer
- Ownership and visibility model for profiles and sessions
- Bootstrap flow for first-launch credential provisioning
- Password policy
- CSRF protection
- Rate limiting and account lockout
- Audit log

**Out of scope:**
- OIDC / OAuth / SSO (future — §12 describes the extension path)
- Per-user runtime credentials (all users share the Console's runtime connection)
- Multi-tenancy and organizational units
- Password recovery (no email system — admin resets via CLI or admin panel)

### 1.3 Terminology

All auth-specific terms follow `wcon-glossary` §5 and §8. Key disambiguation:
- **"session"** = Console coordination run. **"browser session"** = authenticated connection.
- **"role"** = WACP protocol role (coordinator/worker/observer). **"console role"** = admin/operator/viewer.
- **"audit log"** = Console mutation log. **"trail"** = WACP protocol event record.

## 2. Identity Model

A **user** is a record in the Console's local identity store. The Console does not federate identity with the WACP runtime or any external system in Phase 1.

### 2.1 User Entity

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `id` | UUID v4 | PK | Stable identifier; referenced by ownership, audit, and session records |
| `username` | string | unique, 3–64 chars, `[a-zA-Z0-9_.-]` | Login identifier; immutable after creation |
| `display_name` | string | 1–128 chars | Human-readable name; mutable |
| `password_hash` | string | Argon2id | Never exposed via API; see §7 |
| `console_role` | enum | `admin` \| `operator` \| `viewer` | Authorization level; see §4 |
| `must_change_password` | bool | default `true` | Set on creation and password reset; cleared on successful change |
| `disabled_at` | timestamp | nullable | When set, the user cannot authenticate; existing browser sessions are invalidated |
| `created_at` | timestamp | not null | Creation time |
| `updated_at` | timestamp | not null | Last modification time |

Physical schema: `wcon-data-model` §5 (`users` table).

### 2.2 Constraints

- Usernames are case-insensitive for lookup, stored in the case the admin provided at creation. Uniqueness is enforced on the lowercased form.
- A user cannot be deleted — only disabled. Disabling preserves audit log attribution and session ownership history. Disabled users cannot log in and their API tokens are rejected, but their name and ID remain in the system.
- The `admin` console role cannot be self-demoted by the last remaining admin. The system must always have at least one active admin.

## 3. Authentication

The Console supports two authentication mechanisms, handled by the **authenticator** trait (`wcon-glossary` §7). Both mechanisms produce the same internal representation: an authenticated user identity carrying `user_id`, `username`, and `console_role`.

### 3.1 Browser Session Authentication

For interactive (browser) use. The flow:

1. **Login.** `POST /api/auth/login` with `{ "username": "...", "password": "..." }`.
2. **Backend validates** the password against the stored Argon2id hash.
3. **On success:** a browser session record is created, and a session cookie is set:
   - Cookie name: `wcon_sid`
   - Flags: `HttpOnly`, `Secure` (when TLS is enabled), `SameSite=Strict`
   - Value: opaque token (256-bit random, base64url-encoded)
   - Expiry: server-controlled via the session record's `expires_at`
4. **On failure:** see §9 (rate limiting).

If `must_change_password` is true, the login response includes `"must_change_password": true`. The frontend must call `POST /api/auth/change-password` before any other operation. All other endpoints return `403 PASSWORD_CHANGE_REQUIRED` until the password is changed.

### 3.2 Browser Session Entity

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID v4 | PK |
| `user_id` | UUID v4 | FK → users |
| `token_hash` | string | SHA-256 of the session cookie value |
| `ip` | string | Client IP at login |
| `user_agent` | string | Client User-Agent at login |
| `created_at` | timestamp | Login time |
| `expires_at` | timestamp | Absolute expiry (24 hours from creation by default) |

Physical schema: `wcon-data-model` §5 (`user_sessions` table).

**Session lifecycle:**
- Session tokens are hashed (SHA-256) before storage — a database leak does not yield usable tokens.
- Sessions expire after 24 hours (configurable via a `auth.session_ttl_hours` setting key).
- **One active browser session per user.** Sessions are rotated on login: a new token is issued, the old session record (if any) is deleted. Logging in from a second device invalidates the first. This is a deliberate simplification for a small-team tool — concurrent browser sessions are a future extension if needed.
- Logout (`POST /api/auth/logout`) deletes the session record and clears the cookie.
- Disabling a user deletes all their session records.

### 3.3 API Token Authentication

For programmatic (non-browser) use. API tokens are long-lived bearer credentials.

**Creation flow:**
1. An authenticated user (or admin acting on behalf of a user) calls `POST /api/tokens` with `{ "name": "..." }`.
2. The backend generates a 256-bit random token, prefixed with `wcon_t_` for recognizability.
3. The response includes the full token **exactly once**. The backend stores only the SHA-256 hash.
4. Subsequent requests include the token: `Authorization: Bearer wcon_t_...`.

### 3.4 API Token Entity

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID v4 | PK |
| `user_id` | UUID v4 | FK → users |
| `name` | string | User-assigned label (e.g., "CI pipeline") |
| `token_hash` | string | SHA-256 of the full token |
| `created_at` | timestamp | Creation time |
| `expires_at` | timestamp | Optional absolute expiry; NULL means no expiry |
| `last_used_at` | timestamp | Updated on each successful authentication |
| `revoked_at` | timestamp | When set, the token is rejected |

Physical schema: `wcon-data-model` §5 (`api_tokens` table).

**Token lifecycle:**
- Tokens carry the console role of the owning user at request time (not at creation time). If a user is demoted from admin to operator, their tokens immediately reflect the lower role.
- Revoked tokens are rejected. Revocation is immediate and permanent.
- Disabling a user implicitly revokes all their tokens (by the disabled-user check in the authenticator, not by modifying token records).
- Tokens have no scope restrictions beyond the owning user's console role. Per-token scoping is a future extension.

### 3.5 WebSocket Authentication

WebSocket connections authenticate during the HTTP upgrade handshake. The `Cookie` header (for browser sessions) or `Authorization` header (for API tokens) is present on the upgrade request. If authentication fails, the upgrade is rejected with `401`. Once established, the WebSocket connection inherits the authenticated identity for its lifetime — no per-message re-authentication.

### 3.6 Authenticator Trait

The authenticator is a request-level middleware that:
1. Extracts credentials from the request (cookie or `Authorization` header).
2. Validates the credential against the identity store.
3. Returns the authenticated user identity or rejects with `401`.

Phase 1 ships `LocalAuthenticator`. The trait shape accommodates a future `OidcAuthenticator` (§12) without restructuring the request pipeline. The trait receives an HTTP request and returns `Result<AuthenticatedUser, AuthError>`.

## 4. Authorization

Authorization determines what an authenticated user may do. The **authorizer** trait (`wcon-glossary` §7) receives the authenticated identity and the requested action and returns allow or deny.

### 4.1 Console Roles

Three hierarchical levels, each a strict superset of the level below:

| Console Role | Persona (`wcon-vision` §4) | Capabilities |
|-------------|---------------------------|--------------|
| `viewer` | Explorer | Read-only access to all non-admin resources. Browse discovery, view profiles (own + shared), view sessions (own), view oversight dashboard (own sessions). Cannot create, modify, or launch anything. |
| `operator` | Practitioner / Overseer | Everything viewer can do, plus: create/edit/delete own profiles, create/launch/cancel own sessions, approve gates and handle escalations on own sessions, inject directives on own sessions, create/revoke own API tokens. Covers both the Practitioner (configuration/launch) and Overseer (gate approval/escalation handling) personas. |
| `admin` | Administrator | Everything operator can do, plus: manage all users (create, disable, change roles), view and act on all sessions regardless of ownership, view and manage all profiles regardless of ownership/visibility, view the audit log, reset user passwords, manage any user's API tokens. Maps to the Administrator persona. |

### 4.2 Permission Matrix

The action set and role requirements:

| Resource | Action | viewer | operator | admin |
|----------|--------|--------|----------|-------|
| **Discovery** | Browse taxonomy | yes | yes | yes |
| **Profiles** | List own + shared | yes | yes | yes |
| **Profiles** | List all (including others' private) | no | no | yes |
| **Profiles** | View own + shared | yes | yes | yes |
| **Profiles** | View others' private | no | no | yes |
| **Profiles** | Create | no | yes | yes |
| **Profiles** | Edit own | no | yes | yes |
| **Profiles** | Edit others' (shared) | no | no | yes |
| **Profiles** | Delete own | no | yes | yes |
| **Profiles** | Delete others' | no | no | yes |
| **Profiles** | Export own + shared | yes | yes | yes |
| **Profiles** | Import | no | yes | yes |
| **Sessions** | List own | yes | yes | yes |
| **Sessions** | List all | no | no | yes |
| **Sessions** | View own oversight dashboard | yes | yes | yes |
| **Sessions** | View others' oversight dashboard | no | no | yes |
| **Sessions** | Create/launch | no | yes | yes |
| **Sessions** | Cancel own | no | yes | yes |
| **Sessions** | Cancel others' | no | no | yes |
| **Sessions** | Approve gates (own) | no | yes | yes |
| **Sessions** | Approve gates (others') | no | no | yes |
| **Sessions** | Handle escalations (own) | no | yes | yes |
| **Sessions** | Handle escalations (others') | no | no | yes |
| **Sessions** | Inject directives (own) | no | yes | yes |
| **Sessions** | Inject directives (others') | no | no | yes |
| **Users** | List users | no | no | yes |
| **Users** | Create user | no | no | yes |
| **Users** | Disable user | no | no | yes |
| **Users** | Change user's console role | no | no | yes |
| **Users** | Reset user's password | no | no | yes |
| **API Tokens** | List own | yes | yes | yes |
| **API Tokens** | Create own | no | yes | yes |
| **API Tokens** | Revoke own | no | yes | yes |
| **API Tokens** | List/revoke others' | no | no | yes |
| **Audit Log** | View | no | no | yes |
| **Settings** | View | no | yes | yes |
| **Settings** | Modify | no | no | yes |

### 4.3 Authorization Enforcement

Authorization is enforced at the API layer, not the UI layer. The frontend hides affordances the user lacks permission for, but the backend independently validates every request. A request that fails authorization returns `403 Forbidden`:

```json
{
  "error": "forbidden",
  "message": "Insufficient permissions",
  "details": {
    "required_role": "admin",
    "actual_role": "operator"
  }
}
```

### 4.4 Authorizer Trait

The authorizer receives `(AuthenticatedUser, Action)` and returns `Result<(), AuthzError>`. Phase 1 ships `RoleAuthorizer`, which implements the permission matrix in §4.2. The trait shape accommodates future ABAC or per-resource policies.

## 5. Ownership & Visibility

### 5.1 Ownership

Every profile and session carries an `owner_user_id` referencing the user who created it. Ownership is set at creation and is immutable.

**Effect of ownership:**
- Operators can only see and act on resources they own (subject to visibility for profiles).
- Admins bypass ownership checks entirely.
- Viewers can see own resources and shared profiles, but cannot modify anything.

### 5.2 Profile Visibility

Each profile has a `visibility` field: `private` (default) or `shared`.

| Visibility | Who can see | Who can use in sessions | Who can edit |
|-----------|------------|------------------------|-------------|
| `private` | Owner + admins | Owner + admins | Owner + admins |
| `shared` | All authenticated users | Operators + admins | Owner + admins |

Changing visibility is a profile edit — it creates a new profile version and is recorded in the audit log.

### 5.3 Session Access

Sessions do not have a visibility field. Access is determined by:
- The session owner has full access to their session.
- Admins have full access to all sessions.
- *(Non-normative, not in Phase 1.)* A future extension may allow operators to view sessions launched with shared profiles they contributed. Phase 1 restricts operators to own sessions only.
- Viewers can view only their own sessions (which they cannot create, so this is relevant only if a viewer was previously an operator).

## 6. Bootstrap Flow

The Console must never ship with default credentials. The bootstrap flow handles first-launch provisioning.

### 6.1 First Launch Detection

On startup, the Console checks whether the `users` table contains any rows. If empty, bootstrap mode activates.

### 6.2 Credential Generation

1. Generate a random 24-character alphanumeric password.
2. Create a user with:
   - `username`: `"admin"`
   - `console_role`: `admin`
   - `must_change_password`: `true`
   - `password_hash`: Argon2id hash of the generated password
3. Output the credential through two channels:
   - **stdout**: `Console bootstrap: admin / <password>` (logged at `WARN` level so it is visible even with default log filters).
   - **File**: write the credential to `$XDG_STATE_HOME/wacp-console/bootstrap-token` (or `~/.local/state/wacp-console/bootstrap-token` if `$XDG_STATE_HOME` is unset). The file is created with `0600` permissions.

### 6.3 First Login

The bootstrap credential works for exactly one purpose: logging in and changing the password. After login, the `must_change_password` flag forces a password change before any other operation. Once changed:
- The bootstrap token file is deleted (best-effort).
- The `must_change_password` flag is cleared.
- A normal browser session is issued.

### 6.4 Recovery

If the bootstrap credential is lost before first login, the operator must delete the database file (`console.db`) and restart to re-trigger bootstrap. There is no backdoor.

If an admin loses their password after initial setup, another admin can reset it. If the sole admin loses their password, the operator must use a CLI escape hatch: `wacp-console reset-admin-password` — a subcommand that reads the database directly, bypassing the HTTP API. This command:
1. Prompts for a new password on stdin.
2. Hashes with Argon2id.
3. Updates the admin's `password_hash` and sets `must_change_password = true`.
4. Prints confirmation to stdout.

This is a server-side operation requiring filesystem access to the database — it is not exposed over the network.

## 7. Password Policy

### 7.1 Hashing

All passwords are hashed with **Argon2id** using the `argon2` crate. Parameters:

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Algorithm | Argon2id | Hybrid memory-hard + data-dependent; recommended by OWASP |
| Memory cost | 19 MiB (19456 KiB) | OWASP minimum recommendation |
| Time cost | 2 iterations | Balance between security and login latency |
| Parallelism | 1 | Single-threaded; the Console is not a high-throughput auth service |
| Salt | 16 bytes, random | Per-password, generated by the `argon2` crate |
| Output | 32 bytes | Standard hash length |

The hash string is stored in PHC format (`$argon2id$v=19$m=...,t=...,p=...$<salt>$<hash>`), so the parameters are self-describing and can be upgraded per-password on next login.

### 7.2 Strength Requirements

| Rule | Value |
|------|-------|
| Minimum length | 12 characters |
| Maximum length | 128 characters |
| Character classes | No explicit class requirements (length is the primary defense) |
| Disallowed | Username as substring (case-insensitive) |

Passwords that fail these rules are rejected with `422 PASSWORD_TOO_WEAK`:

```json
{
  "error": "password_too_weak",
  "message": "Password must be at least 12 characters",
  "details": {
    "violations": ["min_length"]
  }
}
```

### 7.3 Password Change

`POST /api/auth/change-password` with `{ "current_password": "...", "new_password": "..." }`.

- Requires the current password (even for the bootstrap forced-change flow — the bootstrap password is the "current" password).
- The new password is validated against §7.2.
- On success: the password hash is updated, `must_change_password` is cleared, all existing browser sessions for the user (except the current one) are invalidated, and an audit log entry is recorded.

Admin-initiated password reset (`POST /api/users/:id/reset-password` with `{ "new_password": "..." }`) sets the target user's password and sets `must_change_password = true`. Does not require the target user's current password. Recorded in the audit log with the admin's identity.

## 8. CSRF Protection

All state-changing requests (POST, PUT, PATCH, DELETE) from browser sessions are protected against cross-site request forgery via the **double-submit cookie** pattern:

1. On login, the backend sets an additional cookie:
   - Name: `wcon_csrf`
   - Flags: `SameSite=Strict`, **not** `HttpOnly` (the frontend must read it)
   - Value: 256-bit random token, base64url-encoded
2. The frontend reads `wcon_csrf` and includes it as a request header: `X-CSRF-Token: <value>`.
3. The backend compares the cookie value with the header value using constant-time comparison (`subtle::ConstantTimeEq`). Mismatch returns `403 CSRF_VALIDATION_FAILED`.

**API token requests are exempt** from CSRF checks — bearer tokens are not automatically attached by browsers, so CSRF is not a viable attack vector for API token authentication.

## 9. Rate Limiting & Account Lockout

### 9.1 Login Rate Limiting

Two independent rate limits protect the login endpoint:

| Dimension | Window | Limit | Lockout |
|-----------|--------|-------|---------|
| Per-IP | 15 minutes | 20 attempts | IP blocked for 15 minutes; returns `429 TOO_MANY_REQUESTS` |
| Per-account | 15 minutes | 5 failed attempts | Account locked for 15 minutes; returns `401 ACCOUNT_LOCKED` |

Account lockout is temporary and automatic. No admin intervention required to unlock — the lockout expires. Admin can clear a lockout early via `POST /api/users/:id/unlock`.

### 9.2 Login Attempts Entity

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID v4 | PK |
| `ip` | string | Client IP |
| `username` | string | Attempted username (may not match a real user) |
| `attempted_at` | timestamp | Attempt time |
| `success` | bool | Whether the attempt succeeded |

Physical schema: `wcon-data-model` §5 (`login_attempts` table).

Login attempts older than 24 hours are garbage-collected on a background schedule. The table is not part of the audit log — it exists solely for rate limiting and lockout decisions.

### 9.3 Non-Login Rate Limits

Non-login API endpoints are not individually rate-limited in Phase 1. The directive injection endpoint (`POST /api/sessions/:id/inject`) retains its existing per-session throttle (`wcon-highway` §5.3), but that is a domain limit, not an auth limit.

## 10. Audit Log

The audit log is an append-only record of every state-changing operation in the Console. It exists for accountability and forensic review, not for real-time alerting.

### 10.1 Audit Log Entry

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID v4 | PK |
| `user_id` | UUID v4 | FK → users (the actor) |
| `timestamp` | timestamp | When the action occurred (server clock, UTC) |
| `action` | string | Machine-readable action name (see §10.2) |
| `target_kind` | string | Entity type acted upon: `user`, `profile`, `session`, `token`, `settings` |
| `target_id` | string | ID of the entity acted upon |
| `detail` | JSON | Action-specific structured data (e.g., changed fields, old/new values for settings) |
| `ip` | string | Client IP |
| `user_agent` | string | Client User-Agent |

Physical schema: `wcon-data-model` §5 (`audit_log` table).

### 10.2 Audited Actions

| Action | Target Kind | When |
|--------|------------|------|
| `user.create` | user | Admin creates a new user |
| `user.disable` | user | Admin disables a user |
| `user.enable` | user | Admin re-enables a user |
| `user.change_role` | user | Admin changes a user's console role |
| `user.reset_password` | user | Admin resets a user's password |
| `auth.login` | user | Successful login (failed attempts are in `login_attempts`, not the audit log) |
| `auth.logout` | user | Logout |
| `auth.change_password` | user | User changes own password |
| `token.create` | token | User creates an API token |
| `token.revoke` | token | User or admin revokes a token |
| `profile.create` | profile | User creates a new profile |
| `profile.update` | profile | User updates a profile (new version) |
| `profile.delete` | profile | User soft-deletes a profile |
| `profile.clone` | profile | User clones a profile |
| `profile.import` | profile | User imports a profile from YAML |
| `profile.visibility_change` | profile | User changes profile visibility |
| `session.create` | session | User creates a session |
| `session.launch` | session | User launches a session |
| `session.cancel` | session | User cancels a session |
| `session.gate_approve` | session | User approves a gate |
| `session.gate_reject` | session | User rejects a gate |
| `session.escalation_respond` | session | User responds to an escalation |
| `session.inject_directive` | session | User injects a directive |
| `settings.update` | settings | Admin updates a setting |

### 10.3 Audit Log Access

The audit log is read-only via `GET /api/audit-log` (admin only). Supports filtering by `user_id`, `action`, `target_kind`, `target_id`, and time range. Paginated via the standard cursor model (`wcon-api` §5).

The audit log is never truncated through the API. Operators who need to manage disk usage can back up and truncate the SQLite table directly — this is an operational concern, not an application feature.

## 11. Threat Model

### 11.1 Assumptions

- The Console is deployed on a private network or behind a reverse proxy. It is not designed for direct internet exposure without TLS termination.
- The SQLite database file is protected by filesystem permissions. An attacker with read access to the database has access to password hashes and session tokens (hashed, but still).
- The WACP runtime is trusted — the Console does not authenticate individual runtime responses.

### 11.2 Threats and Mitigations

| Threat | Severity | Mitigation |
|--------|----------|------------|
| **Credential stuffing** | High | Per-IP and per-account rate limiting (§9); Argon2id slows offline attacks |
| **Session hijacking** | High | HttpOnly + Secure + SameSite=Strict cookies; session tokens hashed in storage; 24h expiry |
| **CSRF** | Medium | Double-submit cookie pattern (§8); SameSite=Strict as defense-in-depth |
| **Brute-force password guessing** | Medium | Argon2id (memory-hard); 12-char minimum; account lockout after 5 failures |
| **Privilege escalation via API** | Medium | Backend-enforced authorization on every endpoint (§4.3); frontend is convenience, not a security boundary |
| **Token leakage in logs** | Medium | API tokens prefixed `wcon_t_` for grep/revoke; tokens shown once at creation; `secrecy` crate wraps secrets in the backend to prevent accidental logging |
| **Database theft** | Medium | Passwords hashed with Argon2id; session/API tokens hashed with SHA-256; no plaintext secrets in the database |
| **Timing attacks on authentication** | Low | Constant-time comparison for CSRF tokens (`subtle`); Argon2id hash verification is inherently constant-time on valid-length inputs; failed-username lookups execute a dummy hash to prevent user-enumeration timing |
| **Bootstrap credential exposure** | Low | File written with 0600 permissions; deleted after first password change; printed to stdout only at WARN level |
| **XSS leading to token theft** | Low | Session cookie is HttpOnly (inaccessible to JS); CSRF cookie is readable by JS but not useful without the session cookie; CSP headers should be configured at the reverse proxy level |

### 11.3 What This Spec Does Not Protect Against

- **An attacker with shell access to the Console host.** They can read the database, extract hashed tokens, and impersonate users. Filesystem-level security is the operator's responsibility.
- **A compromised WACP runtime.** The Console trusts runtime responses. A malicious runtime could feed false trail data, fake gate events, etc. This is a deployment trust boundary, not a Console auth concern.
- **Denial of service.** Rate limiting protects the login endpoint, but the Console does not defend against volumetric DoS. That is a network-level concern (firewall, reverse proxy).

## 12. OIDC Extension Path

Phase 1 ships `LocalAuthenticator` only. The architecture supports a future `OidcAuthenticator` via the authenticator trait (§3.6).

### 12.1 Design Constraints for the Trait

The authenticator trait must accommodate OIDC without breaking the local flow:

- **Input:** HTTP request (headers, cookies).
- **Output:** `AuthenticatedUser { user_id, username, console_role }`.
- **Async:** the trait method is async (OIDC requires network calls to the IdP).
- **No session creation inside the trait:** session management (cookie issuance) is separate from identity extraction. The OIDC authenticator validates an ID token and maps claims to a Console user; session creation happens in the login handler, not the authenticator.

### 12.2 OIDC-Specific Considerations (Not Implemented)

When OIDC is added:
- The `openidconnect` crate handles discovery, token validation, and claim extraction.
- Console role mapping: a claim (e.g., `groups`) maps to console roles. Configurable via settings.
- User provisioning: first OIDC login auto-creates a Console user record (JIT provisioning). The user's `password_hash` is empty (OIDC users cannot log in with a password).
- The local password flow remains available as a fallback (admin-configurable: `auth.local_enabled` setting, per §4.2 settings modify permission).

## 13. Invariants

1. **No default credentials.** The system never ships with a known username/password pair. The bootstrap flow generates a random credential at first launch.
2. **Backend is the security boundary.** Every authorization check is enforced at the API layer. The frontend is a convenience layer — hiding a button is not a security measure.
3. **Secrets are never stored in plaintext.** Passwords are Argon2id-hashed. Session tokens and API tokens are SHA-256-hashed. The `secrecy` crate prevents accidental logging of secret values.
4. **Audit log is append-only.** No API endpoint modifies or deletes audit log entries. Every state-changing operation produces exactly one audit log entry.
5. **At least one admin always exists.** The last active admin cannot be disabled or demoted.
6. **Disabled users are immediately locked out.** Disabling a user invalidates all their browser sessions and implicitly rejects all their API tokens.
7. **Ownership is immutable.** `owner_user_id` is set at creation and never changes. Transfer of ownership is not supported.
8. **CSRF protection covers all state-changing browser requests.** POST, PUT, PATCH, DELETE from cookie-authenticated sessions require a valid CSRF token. API token requests are exempt.
9. **Rate limits are enforced independently of authentication.** The login endpoint is rate-limited by IP and by account before the password is checked.
10. **Failed-username lookups do not leak timing.** A login attempt for a nonexistent username executes a dummy Argon2id verification to prevent timing-based user enumeration.

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-vision | Product Vision | §4 defines the four personas (Practitioner, Overseer, Explorer, Administrator) that inform the three console roles |
| wcon-glossary | Glossary | §5, §7, §8 define auth terminology |
| wcon-architecture | System Architecture | §8 defines the authenticator/authorizer trait slots |
| wcon-data-model | Data Model | §5 will carry the physical schemas for users, user_sessions, api_tokens, audit_log, login_attempts |
| wcon-api | API Surface | §3 will be revised to reference this spec for auth endpoints |
| wcon-profiles | Profile System | ownership and visibility model (§5 of this spec) |
| wcon-sessions | Session Lifecycle | launch identity and session authorization (§4, §5 of this spec) |
| wcon-ui | UI Design | login screen, admin panel, permission-gated affordances |
| TECH_STACK_PROPOSAL.md | Tech Stack Proposal | §10 Q2 — the decision that multi-user auth ships in Phase 1 |

*WACP Console -- authored by AAkil98*
