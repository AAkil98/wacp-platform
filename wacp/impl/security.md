# WACP Implementation: Security Framework

```yaml
id: wacp-impl-security
type: implementation-spec
status: draft
created: 2026-04-01
lineage: LAYER-MAPPING.md (M7)
protocol_sections:
  - §11 (security model — trust root, boundaries)
  - §9 (trail — audit events)
  - §5 (roles and permissions)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-tool-framework
  - wacp-impl-llm-adapters
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, middleware, security, content-filter, secrets, audit]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Trust Boundaries](#2-trust-boundaries)
3. [Content Filter](#3-content-filter)
4. [Secret Store](#4-secret-store)
5. [Audit Events](#5-audit-events)
6. [Crate Structure](#6-crate-structure)
7. [Test Requirements](#7-test-requirements)
8. [References](#8-references)

---

## 1. Purpose

This spec defines the security middleware — cross-cutting concerns that span all other middleware frameworks. It answers "how do we prevent secrets from leaking, PII from reaching LLMs, and security events from going unrecorded" — not "how does the runtime enforce permissions" (that's wacp-permissions) or "how are connections authenticated" (that's wacp-transport auth).

**Scope.** New `wacp-security` crate. Content filtering (PII redaction at the LLM boundary, secret scanning in checkpoints). Secret management (config-injected secrets, never logged, never in trail). Audit event types (auth events, tool invocations as structured trail entries).

**Not in scope.** Authentication (existing `Authenticator` trait in wacp-transport). Permission enforcement (wacp-permissions crate). TLS configuration (wacp-runtime). These exist and work — this crate complements them.

---

## 2. Trust Boundaries

Four boundaries, each with its security concern:

| Boundary | Location | Concern |
|----------|----------|---------|
| **Runtime boundary** | wacp-transport auth | All inbound requests authenticated |
| **Workspace boundary** | wacp-permissions | Agents see only granted visibility |
| **Tool boundary** | wacp-tools sandboxing | Tools execute with scoped permissions |
| **LLM boundary** | **this crate** — content filter | No secrets/PII in LLM prompts |

The content filter sits at the LLM boundary — the last point before data leaves the system to an external LLM provider. Everything that passes through `LlmAdapter.complete()` or `complete_stream()` should be filtered.

---

## 3. Content Filter

```rust
/// Content filter applied at the LLM boundary.
pub struct ContentFilter {
    rules: Vec<FilterRule>,
    policy: FilterPolicy,
}

/// A single redaction rule.
pub struct FilterRule {
    /// Rule name (for logging).
    pub name: String,
    /// Regex pattern to match.
    pub pattern: regex::Regex,
    /// Replacement string. Default: "[REDACTED]".
    pub replacement: String,
    /// Whether this rule is enabled.
    pub enabled: bool,
}

/// Filter policy — what to do when content matches.
#[derive(Debug, Clone, Copy)]
pub enum FilterAction {
    /// Replace matched content with the replacement string.
    Redact,
    /// Block the entire request and return an error.
    Block,
    /// Log a warning but allow the content through.
    Warn,
}

/// Per-workspace filter configuration.
pub struct FilterPolicy {
    /// Default action for matched content.
    pub default_action: FilterAction,
    /// Whether filtering is enabled.
    pub enabled: bool,
}
```

**Built-in rules:**

| Rule | Pattern | Catches |
|------|---------|---------|
| `api_key` | `(sk-[a-zA-Z0-9]{20,})|(key-[a-zA-Z0-9]{20,})` | API keys (Anthropic, OpenAI style) |
| `bearer_token` | `Bearer\s+[A-Za-z0-9\-._~+/]+=*` | OAuth bearer tokens |
| `aws_key` | `AKIA[0-9A-Z]{16}` | AWS access key IDs |
| `private_key` | `-----BEGIN\s+(RSA\s+)?PRIVATE\s+KEY-----` | PEM private keys |
| `email` | `[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}` | Email addresses |
| `ssn` | `\b\d{3}-\d{2}-\d{4}\b` | US Social Security Numbers |
| `credit_card` | `\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b` | Credit card numbers |

**API:**

```rust
impl ContentFilter {
    /// Create with default rules (all enabled, Redact action).
    pub fn with_defaults() -> Self;

    /// Create with no rules (pass-through).
    pub fn disabled() -> Self;

    /// Add a custom rule.
    pub fn add_rule(&mut self, rule: FilterRule);

    /// Filter a string. Returns (filtered_string, Vec<redactions>).
    pub fn filter(&self, input: &str) -> FilterResult;

    /// Filter all messages in a conversation.
    pub fn filter_messages(&self, messages: &[Message]) -> (Vec<Message>, Vec<Redaction>);
}

pub struct FilterResult {
    pub output: String,
    pub redactions: Vec<Redaction>,
}

pub struct Redaction {
    pub rule_name: String,
    pub original_length: usize,
    pub position: usize,
}
```

**Integration point.** The content filter is called by the LLM adapter layer (or by the application) before passing messages to `LlmAdapter.complete()`. The framework does NOT automatically intercept LLM calls — the caller is responsible for filtering. This keeps the filter composable and testable.

---

## 4. Secret Store

```rust
/// Manages secrets injected via configuration.
pub struct SecretStore {
    secrets: HashMap<String, SecretValue>,
}

/// A secret value that redacts itself in Debug/Display.
pub struct SecretValue {
    value: String,
}

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Self;

    /// Access the actual value. Use sparingly — only when constructing auth headers.
    pub fn expose(&self) -> &str;
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretValue(***)")
    }
}

impl std::fmt::Display for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "***")
    }
}
```

**API:**

```rust
impl SecretStore {
    pub fn new() -> Self;

    /// Store a secret. Overwrites if key exists.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>);

    /// Retrieve a secret. Returns None if not found.
    pub fn get(&self, key: &str) -> Option<&SecretValue>;

    /// Check if a key exists.
    pub fn contains(&self, key: &str) -> bool;

    /// Remove a secret.
    pub fn remove(&mut self, key: &str) -> bool;

    /// Number of stored secrets.
    pub fn len(&self) -> usize;

    /// Scan a string for any stored secret values. Returns matched key names.
    pub fn scan_for_leaks(&self, content: &str) -> Vec<String>;
}
```

**`scan_for_leaks`** checks if any secret value appears as a substring in the content. Used by the content filter and audit layer to detect accidental secret inclusion in checkpoints, trail entries, or error messages.

---

## 5. Audit Events

Structured audit event types that extend the trail.

```rust
/// An audit event — recorded in the trail for security observability.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "audit_type", rename_all = "snake_case")]
pub enum AuditEvent {
    /// Authentication attempt (success or failure).
    AuthAttempt {
        identity: String,
        method: String,      // "psk", "api_key", "oauth", "session_token"
        success: bool,
        reason: Option<String>,
    },

    /// Rate limit triggered.
    RateLimited {
        identity: String,
        limit_type: String,  // "auth_failure", "request", "token"
    },

    /// Secret access (a secret was exposed for use).
    SecretAccess {
        key_name: String,
        purpose: String,     // "llm_auth", "tool_credential"
    },

    /// Content filter triggered.
    ContentFiltered {
        rule_name: String,
        action: String,      // "redacted", "blocked", "warned"
        context: String,     // "llm_request", "checkpoint", "envelope"
    },

    /// Tool invocation (input/output hash, no actual content).
    ToolInvocation {
        tool_name: String,
        capability: String,
        input_hash: String,  // SHA-256 of input
        output_hash: Option<String>,  // SHA-256 of output (None if error)
        duration_ms: u64,
        success: bool,
        error_code: Option<String>,
    },
}

impl AuditEvent {
    /// Serialize to JSON bytes for trail storage.
    pub fn to_trail_payload(&self) -> Vec<u8>;
}
```

**Design principle.** Audit events record *that something happened* and *a hash of what*, never *the actual content*. Tool invocations record input/output SHA-256 hashes, not the input/output itself. This enables verification ("was this tool called with this input?") without storing sensitive data in the trail.

---

## 6. Crate Structure

```
crates/wacp-security/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Public exports
│   ├── filter.rs       # ContentFilter, FilterRule, FilterResult, built-in rules
│   ├── secrets.rs      # SecretStore, SecretValue
│   └── audit.rs        # AuditEvent enum, trail payload serialization
└── tests/
```

**Dependencies:** `regex`, `serde`, `serde_json`, `sha2`.

---

## 7. Test Requirements

| Module | Tests |
|--------|-------|
| `filter.rs` | Default rules detect: API key, bearer token, AWS key, PEM key, email, SSN, credit card. Redact replaces matched content. Block returns error. Warn allows but records. Custom rule added and matches. Disabled filter passes everything. Multiple matches in one string. filter_messages processes all messages. No false positives on normal text. |
| `secrets.rs` | Set/get/contains/remove lifecycle. SecretValue Debug shows "***". SecretValue Display shows "***". expose() returns actual value. scan_for_leaks finds stored secret in content. scan_for_leaks returns empty for clean content. Overwrite existing key. |
| `audit.rs` | Every AuditEvent variant serializes to JSON. AuthAttempt includes success/failure. ToolInvocation includes hashes. to_trail_payload produces valid JSON bytes. |

**Total target: ~30 tests.**

---

## 8. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| PROTOCOL.md | §11 | §2 | Trust root, security boundaries |
| Runtime spec | §5 (permission engine) | §2 | Permission enforcement |
| LLM adapters spec | §2 (credentials never leak) | §3, §4 | LLM boundary filtering |
| Tool framework spec | §4 (error model) | §5 | Tool invocation audit |
| LAYER-MAPPING.md | M7 | §1 | Security architecture |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
