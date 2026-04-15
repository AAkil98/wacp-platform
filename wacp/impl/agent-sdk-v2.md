# WACP Implementation: Agent SDK v2

```yaml
id: wacp-impl-agent-sdk-v2
type: implementation-spec
status: draft
created: 2026-04-01
lineage: LAYER-MAPPING.md (M1)
protocol_sections:
  - §4.1 (workspace — agent operations)
  - §4.2 (envelope — query/feedback)
  - §4.3 (signal — lifecycle signals)
  - §4.4 (checkpoint — progress recording)
  - §5 (roles and permissions)
depends_on:
  - wacp-impl-sdk-agent
  - wacp-impl-tool-framework
  - wacp-impl-runtime
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, middleware, sdk, agent, context]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Design Principles](#2-design-principles)
3. [AgentContext API](#3-agentcontext-api)
4. [Tool Integration](#4-tool-integration)
5. [Convenience Methods](#5-convenience-methods)
6. [Query-Response Pattern](#6-query-response-pattern)
7. [Cancellation](#7-cancellation)
8. [Relationship to Existing Agent](#8-relationship-to-existing-agent)
9. [Crate Changes](#9-crate-changes)
10. [Test Requirements](#10-test-requirements)
11. [References](#11-references)

---

## 1. Purpose

This spec defines `AgentContext` — the middleware-level agent API that wraps the existing protocol-level `Agent`. It answers "what do agent developers use day-to-day" — not "what protocol operations exist" (that's the existing sdk-agent spec) or "how does the runtime enforce rules" (that's the runtime spec).

The existing `Agent` (wacp-sdk) exposes raw protocol operations: `signal()`, `checkpoint()`, `send_envelope()`, `inbox()`, `commands()`, `query_trail()`. These are correct and complete but verbose for common patterns. `AgentContext` adds:

1. **Tool integration** — `tool(name, args)` and `tools()` delegate to the tool framework (Phase 20).
2. **Convenience methods** — `complete()`, `blocked()`, `escalate()`, `query()` wrap multi-step patterns into single calls.
3. **Cancellation** — a `CancellationToken` for cooperative shutdown.

**Scope.** `AgentContext` struct, its methods, tool registry integration, cancellation semantics. Changes to `crates/wacp-sdk/`. This does not change the existing `Agent` type — `AgentContext` wraps it.

**Not in scope.** LLM integration — `AgentContext` does not call LLMs. The LLM adapter (Phase 21) is used by the application layer (Phase 25 CLI), not the SDK. New proto RPCs — the AgentService proto is unchanged; all AgentContext methods map to existing RPCs.

---

## 2. Design Principles

**Principle 1: Wrap, don't replace.** `AgentContext` wraps `Agent`. The `Agent` type remains public and usable directly. Developers who want raw protocol access use `Agent`. Developers who want ergonomics use `AgentContext`. Both are valid.

**Principle 2: Tool-use is the bridge.** The primary value of `AgentContext` over `Agent` is `tool()` and `tools()`. These connect the agent to the tool framework, enabling structured, validated, auditable tool invocations. Without `AgentContext`, the agent would construct raw query envelopes to invoke tools — possible but error-prone.

**Principle 3: No hidden state.** Every `AgentContext` method maps to one or more `Agent` calls. There is no hidden buffering, caching, or deferred execution. `complete()` calls `signal(Complete)` immediately. `query()` calls `send_envelope()` + waits on `inbox()` immediately. The developer can reason about network calls by reading the method's documentation.

---

## 3. AgentContext API

```rust
/// Middleware-level agent API. Wraps Agent + ToolRegistry.
pub struct AgentContext {
    agent: Agent,
    tools: Option<Arc<wacp_tools::ToolRegistry>>,
    cancellation: CancellationToken,
}

impl AgentContext {
    /// Create from an existing Agent, optionally with tools.
    pub fn new(
        agent: Agent,
        tools: Option<Arc<wacp_tools::ToolRegistry>>,
    ) -> Self;

    // --- Directive & identity ---
    pub fn directive(&self) -> Option<&wacp_v1::Envelope>;
    pub fn context(&self) -> &[u8];
    pub fn role(&self) -> &str;
    pub fn workspace_id(&self) -> &WorkspaceId;
    pub fn visibility(&self) -> &[String];
    pub fn authority(&self) -> &[String];

    // --- Lifecycle ---
    pub async fn complete(&self, final_payload: Option<&[u8]>) -> Result<(), Error>;
    pub async fn blocked(&self, reason: &str) -> Result<(), Error>;
    pub async fn escalate(&self, context: &[u8]) -> Result<(), Error>;

    // --- Checkpoints ---
    pub fn checkpoint(&self) -> CheckpointBuilder;
    pub async fn quick_checkpoint(
        &self, payload: &[u8], intent: &str
    ) -> Result<CheckpointResult, Error>;

    // --- Communication ---
    pub async fn query(
        &self, content: &[u8], timeout_ms: Option<u64>
    ) -> Result<wacp_v1::Envelope, Error>;
    pub async fn send(
        &self, target: &WorkspaceId, envelope_type: &str, payload: &[u8]
    ) -> Result<EnvelopeResult, Error>;
    pub async fn inbox(&self) -> Result<InboxStream, Error>;

    // --- Tools ---
    pub async fn tool(
        &self, name: &str, args: serde_json::Value
    ) -> Result<serde_json::Value, Error>;
    pub fn tools(&self) -> Vec<&wacp_tools::ToolDescriptor>;

    // --- Observation ---
    pub async fn trail(
        &self, event_type: Option<&str>, limit: u32
    ) -> Result<Vec<wacp_v1::TrailEntry>, Error>;

    // --- Cancellation ---
    pub fn cancellation_token(&self) -> &CancellationToken;
    pub fn is_cancelled(&self) -> bool;
}
```

---

## 4. Tool Integration

**`tool(name, args)`:** Invokes a tool from the registry.

If `AgentContext` was created with a `ToolRegistry` (local-sdk path), the tool executes locally via `registry.execute()`. If no registry is provided (remote agent path), the invocation is sent as a query envelope to the coordinator, which executes the tool and returns the result as a feedback envelope.

```rust
pub async fn tool(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value, Error> {
    if let Some(registry) = &self.tools {
        // Local path: execute directly through tool framework
        let result = registry.execute(
            name, name, args,
            wacp_tools::ExecutionOptions {
                workspace_id: Some(self.agent.workspace_id().clone()),
                ..Default::default()
            },
        ).await.map_err(Error::ToolError)?;
        Ok(result)
    } else {
        // Remote path: send query envelope, await response
        let payload = serde_json::json!({"tool": name, "args": args});
        let response = self.query(
            serde_json::to_vec(&payload).unwrap().as_slice(),
            None,
        ).await?;
        serde_json::from_slice(&response.payload)
            .map_err(|e| Error::ToolError(
                wacp_tools::ToolError::internal(format!("invalid tool response: {e}"))
            ))
    }
}
```

**`tools()`:** Returns descriptors from the registry, or an empty vec if no registry.

```rust
pub fn tools(&self) -> Vec<&wacp_tools::ToolDescriptor> {
    self.tools.as_ref().map(|r| r.list_tools()).unwrap_or_default()
}
```

---

## 5. Convenience Methods

**`complete(final_payload)`:** Optionally creates a final checkpoint, then emits `Complete` signal.

```rust
pub async fn complete(&self, final_payload: Option<&[u8]>) -> Result<(), Error> {
    if let Some(payload) = final_payload {
        self.agent.checkpoint()
            .checkpoint_type("artifact")
            .payload(payload)
            .intent("task complete")
            .status(CheckpointStatus::Final)
            .confidence(Confidence::High)
            .create().await?;
    }
    self.agent.signal(SignalType::Complete).await
}
```

**`blocked(reason)`:** Delegates to `agent.signal_blocked(reason)`.

**`escalate(context)`:** Delegates to `agent.signal_escalation(context)`.

**`quick_checkpoint(payload, intent)`:** Creates a provisional artifact checkpoint with high confidence — the most common checkpoint pattern.

```rust
pub async fn quick_checkpoint(&self, payload: &[u8], intent: &str) -> Result<CheckpointResult, Error> {
    self.agent.checkpoint()
        .checkpoint_type("artifact")
        .payload(payload)
        .intent(intent)
        .status(CheckpointStatus::Provisional)
        .confidence(Confidence::High)
        .create().await
}
```

---

## 6. Query-Response Pattern

**`query(content, timeout_ms)`:** Sends a query envelope to the coordinator and waits for the response. This is the most common communication pattern — the agent asks a question and blocks until the answer arrives.

```rust
pub async fn query(&self, content: &[u8], timeout_ms: Option<u64>) -> Result<wacp_v1::Envelope, Error> {
    // Send query to coordinator (parent workspace)
    let envelope_result = self.agent.send_envelope()
        .to(&self.coordinator_id())
        .envelope_type("query")
        .payload(content)
        .send().await?;

    // Wait for the response on inbox, with optional timeout
    let mut inbox = self.agent.inbox().await?;
    let timeout = timeout_ms.unwrap_or(30_000);
    tokio::time::timeout(
        Duration::from_millis(timeout),
        inbox.next_matching(|env| env.in_reply_to == envelope_result.id),
    ).await
    .map_err(|_| Error::QueryTimeout)?
    .ok_or(Error::StreamEnded)?
}
```

The `InboxStream` needs a `next_matching` method — a filtered next that skips non-matching envelopes (buffering them for later consumption).

---

## 7. Cancellation

`AgentContext` holds a `CancellationToken` (from `tokio-util`). It is cancelled when:
- The workspace receives a `GracefulTermination` command.
- The workspace's timeout fires.
- The caller cancels it explicitly.

Agents check `ctx.is_cancelled()` in long-running loops, or pass `ctx.cancellation_token()` to tool invocations for cooperative shutdown.

---

## 8. Relationship to Existing Agent

`AgentContext` exposes `Agent` for direct access when needed:

```rust
impl AgentContext {
    /// Access the underlying protocol-level Agent.
    pub fn agent(&self) -> &Agent { &self.agent }
}
```

The `Agent` type is unchanged. `AgentContext` is additive — new file, new type, new re-export.

---

## 9. Crate Changes

```
crates/wacp-sdk/src/
├── lib.rs              # Add: pub mod context; pub use context::AgentContext;
├── context.rs          # NEW: AgentContext struct + all methods
├── connection.rs       # UNCHANGED: Agent
├── builder.rs          # UNCHANGED: CheckpointBuilder, EnvelopeBuilder
├── error.rs            # ADD: ToolError(wacp_tools::ToolError), QueryTimeout variants
├── streams.rs          # ADD: next_matching() on InboxStream
└── tests.rs            # ADD: AgentContext unit tests
```

**New dependency:** `wacp-tools` (optional, behind a `tools` feature flag so the SDK can be used without the tool framework).

---

## 10. Test Requirements

| Area | Tests |
|------|-------|
| `AgentContext::new` | Create with tools, create without tools. |
| `complete()` | With final payload (checkpoint + signal). Without payload (signal only). |
| `blocked()` | Delegates to signal_blocked. |
| `escalate()` | Delegates to signal_escalation. |
| `quick_checkpoint()` | Creates provisional artifact with correct fields. |
| `tool()` with registry | Calls registry.execute(), returns result. Tool not found → error. |
| `tool()` without registry | Sends query envelope, awaits response. |
| `tools()` with registry | Returns descriptor list. |
| `tools()` without registry | Returns empty vec. |
| `cancellation_token()` | Not cancelled initially. Cancelled after cancel(). |
| `is_cancelled()` | Reflects token state. |

**Total target: ~15 tests.** These test the context layer — protocol-level operations are already tested in the existing 50 wacp-sdk tests.

---

## 11. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| SDK agent spec | §3–4 | §3, §8 | Existing Agent API surface |
| SDK agent spec | §7 | §4 | Tool invocation via envelopes |
| Tool framework spec | §7, §10 | §4 | ToolRegistry.execute(), list_tools() |
| Runtime spec | §8 | §7 | Workspace termination, graceful shutdown |
| LAYER-MAPPING.md | M1 | §1 | AgentContext design |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
