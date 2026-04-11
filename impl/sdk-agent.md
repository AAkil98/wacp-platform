# WACP Implementation: Agent SDK Design

```yaml
id: wacp-impl-sdk-agent
type: implementation-spec
status: complete
created: 2026-03-19
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4.1 (workspace)
  - §4.2 (envelope)
  - §4.3 (signal)
  - §4.4 (checkpoint)
  - §5 (roles and permissions)
  - §6 (workspace lifecycle)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-protocol-interface
  - wacp-spec-workspace
  - wacp-spec-envelope
  - wacp-spec-signal
  - wacp-spec-checkpoint
  - wacp-spec-roles
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, sdk, python, rust, agent, llm]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [SDK Principles](#2-sdk-principles)
3. [Python SDK Surface](#3-python-sdk-surface)
4. [Rust SDK Surface](#4-rust-sdk-surface)
5. [Connection Lifecycle](#5-connection-lifecycle)
6. [LLM Agent Mapping](#6-llm-agent-mapping)
7. [Tool Mounting](#7-tool-mounting)
8. [Error Handling](#8-error-handling)
9. [Testing Support](#9-testing-support)
10. [Packaging and Distribution](#10-packaging-and-distribution)
11. [References](#11-references)

## 1. Purpose

This spec defines the agent-facing SDKs — the libraries that agent developers use to connect to the WACP runtime, receive directives, do work, and report results. Two SDKs: Python (for LLM agents) and Rust (for system agents and tooling).

The SDKs are thin clients. They do not enforce protocol rules — the runtime does that. They handle connection management, message serialization, and developer ergonomics. An SDK makes it easy to write a correct agent; the runtime makes it impossible to write an agent that violates the protocol.

**Scope.** Public API surface for both SDKs. Connection lifecycle (connect, bind, reconnect, disconnect). How an LLM agent's natural interface (prompt in, text out) maps to the protocol's interface (directive in, checkpoints out). Tool mounting — how the workspace's `mounts` field translates into callable tools. Error handling strategy. Testing utilities.

**Not in scope.** Runtime internals (runtime spec). Wire format and gRPC details (protocol-interface spec). Specific LLM provider integrations (application concern — the SDK provides the hooks, the application provides the model).

**Design constraint.** The Python SDK must feel native to Python developers working with LLMs. `async/await`, type hints, dataclasses. No Java-style factories or builders. The Rust SDK must feel native to Rust developers. Traits, strong types, `Result` everywhere. Neither SDK should require understanding the protocol spec to use — the protocol's concepts (workspace, envelope, checkpoint) are exposed, but the machinery (trail writes, permission checks, hash chains) is invisible.

---

## 2. SDK Principles

Four principles govern both SDKs. They resolve design tensions between ergonomics and protocol fidelity.

**Principle 1: Expose protocol concepts, hide protocol machinery.** The developer sees workspaces, envelopes, checkpoints, and signals — the nouns of the protocol. They do not see trail entries, hash chains, permission matrices, or state machine transitions — the machinery. A developer calls `agent.checkpoint(payload, intent="implemented auth")` and the SDK handles serialization, gRPC call, and response parsing. The developer never constructs a `CreateCheckpointRequest` protobuf message.

**Principle 2: The SDK is a convenience layer, not a trust boundary.** The SDK does not validate permissions, check state transitions, or enforce role constraints. It trusts the runtime to do all of that. If a developer calls `agent.send_envelope()` to a workspace they have no send right to, the SDK submits the request and the runtime rejects it. The SDK surfaces the rejection as a typed error. This keeps the SDK thin and avoids duplicating runtime logic that would inevitably drift.

**Principle 3: Async by default.** Both SDKs are async. The Python SDK uses `asyncio`. The Rust SDK uses `tokio`. The protocol's natural interaction pattern is asynchronous — envelopes arrive at unpredictable times, signals propagate concurrently, checkpoints are created between directive processing steps. A synchronous SDK would fight the protocol's design. Blocking wrappers are provided for simple use cases but are not the primary API.

**Principle 4: The SDK maps the protocol, not a framework.** The SDK provides the protocol surface — connect, send, receive, emit, checkpoint. It does not provide an agent framework — no agent loop, no decision engine, no memory management, no prompt construction. Frameworks are built on top of the SDK. The SDK is the floor, not the ceiling. This keeps it stable — framework opinions change, protocol surfaces don't.

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
## 3. Python SDK Surface

The Python SDK is a single package — `wacp` — providing a high-level async client for agents. It is generated from the `.proto` files (via `betterproto`) and wrapped in a Pythonic API layer.

**Core class: `Agent`.**

```python
from wacp import Agent, Checkpoint, Envelope, Signal

async def main():
    # Connect and bind to a workspace
    agent = await Agent.connect(
        runtime_url="localhost:9400",
        workspace_id="ws-abc123",
        auth_token="tok-xyz",
    )

    # Read the directive
    directive = agent.directive        # Envelope
    context = agent.context            # bytes
    role = agent.role                  # str
    visibility = agent.visibility      # list[str]
    authority = agent.authority        # list[str]

    # Emit a signal
    await agent.signal(Signal.STARTED)

    # Do work...

    # Create a checkpoint
    cp = await agent.checkpoint(
        payload=b"...",
        type="artifact",
        intent="implemented the feature",
        status="provisional",
        confidence="high",
    )
    # cp.id, cp.content_hash, cp.timestamp available

    # Send a query envelope to the coordinator
    resp = await agent.send_envelope(
        to=agent.coordinator,
        type="query",
        payload=b'{"question": "should I proceed?"}',
    )

    # Receive envelopes (async iterator)
    async for envelope in agent.inbox:
        # process envelope
        pass

    # Receive commands (async iterator)
    async for command in agent.commands:
        # handle feedback, visibility grants, termination
        pass

    # Declare completion
    await agent.checkpoint(
        payload=b"final output",
        type="artifact",
        intent="task complete",
        status="final",
        confidence="high",
    )
    await agent.signal(Signal.COMPLETE)
```

**Public API surface:**

| Method / Property | Type | Maps to |
|-------------------|------|---------|
| `Agent.connect(url, workspace_id, auth_token)` | classmethod → `Agent` | `Bind` RPC |
| `agent.directive` | `Envelope` | Bind response |
| `agent.context` | `bytes` | Bind response |
| `agent.role` | `str` | Bind response |
| `agent.visibility` | `list[str]` | Bind response |
| `agent.authority` | `list[str]` | Bind response |
| `agent.coordinator` | `str` | Parent workspace id |
| `agent.signal(type, reason?, context?)` | async → `None` | `EmitSignal` RPC |
| `agent.checkpoint(payload, type, intent, status, confidence, resource_usage?)` | async → `CheckpointResult` | `CreateCheckpoint` RPC |
| `agent.send_envelope(to, type, payload, in_reply_to?, priority?)` | async → `EnvelopeResult` | `SendEnvelope` RPC |
| `agent.query_trail(workspace_id?, event_type?, from?, to?, limit?)` | async → `list[TrailEntry]` | `QueryTrail` RPC |
| `agent.read_resource(resource_id)` | async → `bytes` | `ReadResource` RPC |
| `agent.inbox` | `AsyncIterator[Envelope]` | `ReceiveEnvelopes` stream |
| `agent.commands` | `AsyncIterator[Command]` | `ReceiveCommands` stream |
| `agent.disconnect()` | async → `None` | Close gRPC channel |

**Data classes.** Protocol messages are exposed as Python dataclasses with type hints — not raw protobuf objects. The `betterproto`-generated classes are used internally; the public API wraps them in cleaner types where the protobuf ergonomics are poor (e.g., enums as string constants instead of integer values, `Optional[str]` instead of empty strings for absent fields).

**Signal constants.** `Signal.STARTED`, `Signal.BLOCKED`, `Signal.COMPLETE`, `Signal.FAILED`, `Signal.ESCALATION`. The closed set is exposed as class constants — not an enum, because Python's `enum.Enum` is verbose for this use case. The SDK accepts both constants and strings (`"started"`, `"blocked"`) for convenience.

## 4. Rust SDK Surface

The Rust SDK is a crate — `wacp-sdk` — providing a typed async client. It shares the `wacp-types` crate with the runtime, so protocol types are identical on both sides of the boundary. No translation layer needed.

**Core struct: `Agent`.**

```rust
use wacp_sdk::{Agent, AgentConfig, Signal, CheckpointStatus, Confidence};

#[tokio::main]
async fn main() -> Result<(), wacp_sdk::Error> {
    let agent = Agent::connect(AgentConfig {
        runtime_url: "http://localhost:9400".into(),
        workspace_id: "ws-abc123".parse()?,
        auth_token: "tok-xyz".into(),
    }).await?;

    // Read the directive
    let directive = agent.directive();    // &Envelope
    let context = agent.context();        // &[u8]
    let role = agent.role();              // &str
    let visibility = agent.visibility();  // &[ResourceId]
    let authority = agent.authority();     // &[ResourceId]

    // Emit a signal
    agent.signal(Signal::Started).await?;

    // Create a checkpoint
    let cp = agent.checkpoint()
        .payload(b"...")
        .checkpoint_type("artifact")
        .intent("implemented the feature")
        .status(CheckpointStatus::Provisional)
        .confidence(Confidence::High)
        .create()
        .await?;

    // Send a query envelope
    let resp = agent.send_envelope()
        .to(agent.coordinator())
        .envelope_type("query")
        .payload(b"...")
        .send()
        .await?;

    // Receive envelopes
    let mut inbox = agent.inbox();
    while let Some(envelope) = inbox.next().await {
        let envelope = envelope?;
        // process envelope
    }

    // Declare completion
    agent.signal(Signal::Complete).await?;

    Ok(())
}
```

**Public API surface:**

| Method | Signature | Maps to |
|--------|-----------|---------|
| `Agent::connect(config)` | `async fn(AgentConfig) -> Result<Agent>` | `Bind` RPC |
| `agent.directive()` | `fn(&self) -> &Envelope` | Bind response |
| `agent.context()` | `fn(&self) -> &[u8]` | Bind response |
| `agent.role()` | `fn(&self) -> &str` | Bind response |
| `agent.visibility()` | `fn(&self) -> &[ResourceId]` | Bind response |
| `agent.authority()` | `fn(&self) -> &[ResourceId]` | Bind response |
| `agent.coordinator()` | `fn(&self) -> &WorkspaceId` | Parent workspace id |
| `agent.signal(type)` | `async fn(Signal) -> Result<()>` | `EmitSignal` RPC |
| `agent.signal_blocked(reason)` | `async fn(&str) -> Result<()>` | `EmitSignal` with reason |
| `agent.signal_failed(reason)` | `async fn(&str) -> Result<()>` | `EmitSignal` with reason |
| `agent.signal_escalation(context)` | `async fn(&[u8]) -> Result<()>` | `EmitSignal` with context |
| `agent.checkpoint()` | `fn() -> CheckpointBuilder` | Builder pattern |
| `agent.send_envelope()` | `fn() -> EnvelopeBuilder` | Builder pattern |
| `agent.query_trail()` | `fn() -> TrailQueryBuilder` | Builder pattern |
| `agent.read_resource(id)` | `async fn(ResourceId) -> Result<Vec<u8>>` | `ReadResource` RPC |
| `agent.inbox()` | `fn() -> InboxStream` | `ReceiveEnvelopes` stream |
| `agent.commands()` | `fn() -> CommandStream` | `ReceiveCommands` stream |
| `agent.disconnect()` | `async fn() -> Result<()>` | Close gRPC channel |

**Builder pattern for complex operations.** Checkpoints, envelopes, and trail queries have multiple optional fields. The Rust SDK uses the builder pattern — `agent.checkpoint().payload(...).intent(...).create().await?` — rather than functions with many parameters. Builders validate required fields at compile time where possible (`payload` and `checkpoint_type` are required on `CheckpointBuilder`; calling `.create()` without them is a compile error).

**Shared types.** The Rust SDK depends on `wacp-types` — the same crate the runtime uses. `Signal`, `WorkspaceState`, `CheckpointStatus`, `Confidence`, `EnvelopePriority` are the exact same Rust enums in both the SDK and the runtime. No translation, no mapping, no drift. This is an advantage unique to the Rust SDK — the Python SDK must translate between protobuf-generated types and its own dataclasses.

**Streams.** `InboxStream` and `CommandStream` implement `futures::Stream`. They compose naturally with the async ecosystem — `StreamExt::next()`, `StreamExt::filter()`, `StreamExt::take()`. The underlying gRPC stream is managed by `tonic` — reconnection on stream error is handled by the connection lifecycle (§5).

## 5. Connection Lifecycle

The SDK manages the gRPC connection to the runtime. The developer calls `connect` and gets an `Agent`. The SDK handles reconnection, stream recovery, and clean shutdown behind the scenes.

**Connection states.** The SDK's internal connection has four states:

| State | Meaning | Agent methods |
|-------|---------|---------------|
| `connecting` | Establishing gRPC channel and calling `Bind` | All methods block until connected |
| `connected` | Bound to workspace, streams active | All methods operational |
| `reconnecting` | Connection lost, attempting to re-establish | Action methods queue or return error (configurable). Streams pause. |
| `disconnected` | Explicitly closed or permanently failed | All methods return error |

**Initial connection.** `Agent.connect()` (Python) / `Agent::connect()` (Rust) performs:

1. Open a gRPC channel to the runtime URL.
2. Call the `Bind` RPC with workspace id and auth token.
3. On success: store the bind response (directive, context, role, visibility, authority, budget). Open the `ReceiveEnvelopes` and `ReceiveCommands` streams. Transition to `connected`.
4. On failure: raise/return an error. No retry — the initial connection failure is reported directly. The developer decides whether to retry.

**Reconnection.** If the gRPC connection drops while in `connected` state:

1. Transition to `reconnecting`.
2. Retry connection with exponential backoff: 100ms, 200ms, 400ms, 800ms, ... capped at 10 seconds. Maximum 30 attempts (configurable).
3. On each attempt: open a new gRPC channel, call `Bind` again. The runtime resumes the workspace from its current state — no state is lost server-side.
4. On success: re-open the envelope and command streams. Transition to `connected`. Queued actions (if configured to queue) are flushed.
5. On failure after max attempts: transition to `disconnected`. All pending actions receive a connection error.

**Stream recovery.** Server-streaming RPCs (`ReceiveEnvelopes`, `ReceiveCommands`) may drop independently of the main connection. The SDK detects stream termination and re-opens the stream without a full reconnection. Envelopes delivered during the gap are not lost — they are queued server-side in the workspace actor's inbox and delivered when the stream re-opens.

**Clean shutdown.** `agent.disconnect()` performs:

1. Close the envelope and command streams.
2. Close the gRPC channel.
3. Transition to `disconnected`.
4. The SDK does not emit any signal on disconnect — it is the developer's responsibility to emit `complete` or `failed` before disconnecting. The SDK is a transport layer, not a lifecycle manager.

**Thread safety.** Both SDKs allow calling action methods from multiple concurrent tasks/coroutines. The gRPC channel handles multiplexing. The SDK holds no mutable state that requires synchronization — the bind response is immutable after connection, and the gRPC client is thread-safe. In Rust, `Agent` is `Send + Sync`. In Python, the `Agent` is safe to share across `asyncio` tasks (not across threads — `asyncio` is single-threaded by design).

---

## 6. LLM Agent Mapping

Most WACP agents will be LLMs. An LLM's natural interface is: receive a prompt, produce text, optionally call tools. The protocol's interface is: receive a directive, produce checkpoints, optionally send envelopes. This section defines how the two map to each other.

**The mapping is the application's responsibility, not the SDK's.** The SDK provides the protocol surface. The application — an agent framework, a custom harness, or a simple script — bridges between the LLM's interface and the SDK. The SDK does not import any LLM library, call any model API, or construct any prompt.

**Why not build this into the SDK.** LLM APIs differ (OpenAI, Anthropic, local models). Prompt engineering is domain-specific. Agent architectures vary (ReAct, plan-and-execute, tree of thought). Baking any of these into the SDK creates coupling that limits adoption. The SDK is the stable foundation; the LLM mapping is the variable layer above it.

**Reference mapping.** This is a recommended pattern, not a requirement. An LLM agent framework built on the Python SDK would typically:

```
WACP directive → System prompt + initial user message
    The directive's payload becomes the task description.
    The workspace's context becomes background information.
    The role and visibility inform the system prompt's boundaries.

LLM tool calls → SDK actions
    A "create_file" tool call → agent.checkpoint(payload=file_content, ...)
    A "ask_coordinator" tool call → agent.send_envelope(to=coordinator, type="query", ...)
    A "report_blocked" tool call → agent.signal(Signal.BLOCKED, reason="...")

LLM text output → Checkpoint payload
    The model's final response → agent.checkpoint(status="final", ...)

WACP feedback envelope → Follow-up user message
    Coordinator feedback arrives via agent.inbox → appended to conversation

WACP command (graceful termination) → Instruction to wrap up
    "You have N seconds to produce a final checkpoint."
```

**Resource tracking.** The LLM agent framework reports token usage through the checkpoint's `resource_usage` field. The SDK accepts this as an optional parameter on `agent.checkpoint()`. The runtime's resource meter (runtime spec, §12) uses this for budget tracking. The framework is responsible for reading token counts from the LLM API response and passing them to the SDK.

**Streaming vs. batch.** LLM responses may stream token-by-token. The SDK does not support streaming checkpoint creation — a checkpoint is a single atomic operation. The framework buffers the LLM's streaming output and creates a checkpoint when the response is complete. Provisional checkpoints can be created at intermediate points (e.g., after each tool-use cycle) to record incremental progress.

**Multi-turn conversations.** An LLM agent typically has a multi-turn conversation within a single workspace. The workspace's inbox delivers feedback envelopes that become follow-up turns. The conversation history is the agent framework's responsibility — it lives in the LLM's context window, not in the protocol. The protocol records checkpoints (the outputs) and trail entries (the events), not the full conversation history. If conversation persistence is needed, the framework stores it in working memory (§6.1, component 4).

## 7. Tool Mounting

The workspace's `mounts` field (§4.1) lists the tools available to an agent. Tools are the mechanism by which agents interact with the outside world — file systems, APIs, databases, code execution environments. The SDK exposes mounted tools as callable functions.

**Tool definition.** A tool is defined by the coordinator at workspace creation and included in the bind response. Each tool has:

- **name** — identifier used to invoke it (e.g., `"read_file"`, `"execute_code"`, `"http_get"`).
- **description** — what the tool does, in natural language. Used by LLM agent frameworks to construct tool-use prompts.
- **parameters** — JSON Schema describing the tool's input. Used by LLM agent frameworks for function-calling and by the SDK for validation.
- **endpoint** — how to reach the tool. The runtime mediates tool access — agents do not call tools directly.

**Tool invocation flow.** The agent does not call tools by reaching out to external services. All tool invocations are mediated by the runtime through envelopes:

1. The agent sends a `query` envelope to the coordinator with a tool invocation payload: tool name, parameters.
2. The coordinator validates: is this tool mounted in this workspace? Do the parameters match the schema?
3. The coordinator executes the tool (or delegates execution to a tool service) and returns the result as a `feedback` envelope.
4. The agent receives the result through its inbox.

This mediation is deliberate — it ensures tool access is recorded in the trail, bounded by the authority set, and subject to the permission engine. An agent cannot call a tool that isn't mounted. The runtime can rate-limit, audit, or disable tools without agent cooperation.

**SDK convenience layer.** The SDK provides a helper that wraps the envelope round-trip into a function call:

Python:
```python
# Raw envelope approach
await agent.send_envelope(
    to=agent.coordinator,
    type="query",
    payload=json.dumps({"tool": "read_file", "params": {"path": "/src/main.rs"}}).encode(),
)
result = await agent.inbox.__anext__()

# Convenience approach
result = await agent.tool("read_file", path="/src/main.rs")
```

Rust:
```rust
// Convenience approach
let result = agent.tool("read_file")
    .param("path", "/src/main.rs")
    .call()
    .await?;
```

The convenience method handles: envelope construction, submission, waiting for the response envelope, extracting the result payload. It is syntactic sugar over the envelope mechanism — no new protocol concepts.

**Tool discovery.** The bind response includes the list of mounted tools with their schemas. The SDK exposes this:

- Python: `agent.tools` — `list[ToolDefinition]`, each with `.name`, `.description`, `.parameters`.
- Rust: `agent.tools()` — `&[ToolDefinition]`.

LLM agent frameworks use this to construct the tool-use section of the system prompt — listing available tools with their descriptions and parameter schemas in the format the LLM expects.

**Authority enforcement.** Tools are subject to the workspace's authority set. A `read_file` tool mounted in a workspace with `authority: ["/src"]` can only read files under `/src`. The coordinator enforces this at invocation time — the SDK does not check authority. A tool invocation that exceeds authority returns an error envelope.

---

## 8. Error Handling

The SDK surfaces runtime rejections as typed, actionable errors. The developer should be able to catch an error, understand what went wrong, and decide what to do — without consulting the protocol spec.

**Error hierarchy.** Both SDKs use a single error type with variants/subclasses matching the runtime's error categories (protocol-interface spec, §3):

Python:
```python
from wacp.errors import (
    WacpError,              # base
    PermissionDenied,       # agent tried something its role forbids
    IllegalTransition,      # signal emission in wrong state
    ValidationFailed,       # bad envelope type, bad checkpoint type, unregistered name
    BudgetExceeded,         # workspace hit resource limit
    Timeout,                # workspace timed out
    NotFound,               # target workspace or resource doesn't exist
    DeliveryFailed,         # envelope couldn't be delivered after retries
    ConnectionError,        # gRPC connection issue (SDK-level, not protocol-level)
    InternalError,          # runtime bug — should not happen
)
```

Rust:
```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("permission denied: {rule} — {message}")]
    PermissionDenied { rule: String, message: String },

    #[error("illegal transition: {rule} — {message}")]
    IllegalTransition { rule: String, message: String },

    #[error("validation failed: {rule} — {message}")]
    ValidationFailed { rule: String, message: String },

    #[error("budget exceeded: {message}")]
    BudgetExceeded { message: String },

    #[error("timeout: {message}")]
    Timeout { message: String },

    #[error("not found: {message}")]
    NotFound { message: String },

    #[error("delivery failed: {message}")]
    DeliveryFailed { message: String },

    #[error("connection error: {0}")]
    Connection(#[from] tonic::transport::Error),

    #[error("internal error: {message}")]
    Internal { message: String },
}
```

**Error fields.** Every protocol error carries three fields from the runtime's `ProtocolError` message:

- `category` — the error variant (mapped to the enum/subclass above).
- `rule` — the protocol section that was violated (e.g., `"§5.5"`). Enables the developer to look up the exact rule.
- `message` — a human-readable explanation of what happened and why.

**Error translation.** The SDK translates gRPC status codes into typed errors (protocol-interface spec, §9). The `ProtocolError` message in gRPC trailing metadata is deserialized and wrapped in the appropriate error variant. If the metadata is missing (non-protocol gRPC error), the SDK wraps it as `ConnectionError` or `InternalError`.

**No silent failures.** The SDK never swallows errors. Every failed RPC produces an exception (Python) or `Err` (Rust). The developer must handle or propagate. This aligns with the protocol's principle: "security events are never silent" (§11.8, invariant 6).

**Recoverability guidance.** Each error variant carries an implicit recoverability:

| Error | Recoverable? | Suggested action |
|-------|-------------|------------------|
| `PermissionDenied` | No | The role doesn't allow this. Don't retry. |
| `IllegalTransition` | No | Wrong state. Check agent logic. |
| `ValidationFailed` | No | Fix the input (type name, payload). |
| `BudgetExceeded` | No | Workspace is terminated. |
| `Timeout` | No | Workspace is terminated. |
| `NotFound` | Maybe | Target may not exist yet. Retry if expecting creation. |
| `DeliveryFailed` | Maybe | Recipient may recover. The coordinator is notified. |
| `ConnectionError` | Yes | SDK reconnects automatically (§5). Retry after reconnection. |
| `InternalError` | No | Runtime bug. Report. |

## 9. Testing Support

Agent developers need to test their agents without running a full WACP runtime. The SDK provides testing utilities that simulate the runtime's behavior enough to exercise agent logic.

**Python: `MockRuntime`.**

```python
from wacp.testing import MockRuntime

async def test_my_agent():
    runtime = MockRuntime()

    # Create a workspace with a directive
    ws = runtime.create_workspace(
        workspace_id="ws-test",
        role="worker",
        directive_payload=b'{"task": "implement auth"}',
        tools=[{"name": "read_file", "description": "Read a file", "parameters": {...}}],
    )

    # Connect the agent under test
    agent = await Agent.connect(
        runtime_url=runtime.url,
        workspace_id="ws-test",
        auth_token=ws.token,
    )

    # Run agent logic...
    await my_agent_logic(agent)

    # Assert on what the agent did
    assert len(ws.checkpoints) == 2
    assert ws.checkpoints[-1].status == "final"
    assert ws.signals[-1].type == "complete"
    assert ws.envelopes_sent[0].type == "query"
```

**Rust: `TestRuntime`.**

```rust
use wacp_sdk::testing::TestRuntime;

#[tokio::test]
async fn test_my_agent() {
    let runtime = TestRuntime::new();

    let ws = runtime.create_workspace()
        .workspace_id("ws-test")
        .role("worker")
        .directive_payload(b"...")
        .build();

    let agent = Agent::connect(AgentConfig {
        runtime_url: runtime.url().into(),
        workspace_id: "ws-test".parse().unwrap(),
        auth_token: ws.token().into(),
    }).await.unwrap();

    // Run agent logic...

    // Assert
    assert_eq!(ws.checkpoints().len(), 2);
    assert_eq!(ws.last_signal(), Signal::Complete);
}
```

**What the mock runtime does.** It implements the transport trait (protocol-interface spec, §8) using the `InProcessTransport`. It provides:

- **Workspace creation** with configurable directive, context, role, tools, visibility, and authority.
- **Automatic signal acceptance.** All signals are accepted (no permission checking — the test focuses on agent logic, not runtime enforcement).
- **Checkpoint recording.** Checkpoints are stored in memory and accessible for assertion.
- **Envelope capture.** Envelopes sent by the agent are recorded for assertion.
- **Programmable responses.** The test can configure responses to queries — when the agent sends a query envelope, the mock runtime returns a pre-configured feedback envelope.
- **Command injection.** The test can send feedback envelopes, visibility grants, and graceful termination commands to the agent, simulating coordinator behavior.
- **Tool simulation.** The test can register tool handlers — functions that receive tool parameters and return results.

**What the mock runtime does NOT do.** It does not enforce permissions, validate state transitions, write trail entries, compute hash chains, or enforce budgets. These are runtime responsibilities. Testing them belongs in the runtime's own test suite (runtime spec, §16), not in agent tests. The mock runtime is a test double — it verifies that the agent uses the SDK correctly, not that the runtime enforces rules correctly.

**Integration testing.** For tests that need full protocol enforcement, the SDK provides a `TestHarness` that spins up an actual `wacp-runtime` binary with the `InProcessTransport`. This is slower but exercises the complete stack. Integration tests live in the SDK's test suite, not in application test suites.

## 10. Packaging and Distribution

The SDKs are distributed as standard packages in their respective ecosystems. No custom installation steps, no binary dependencies beyond the protobuf-generated code.

**Python SDK.**

| Attribute | Value |
|-----------|-------|
| Package name | `wacp` |
| Distribution | PyPI |
| Python version | 3.10+ (for modern type hints, `match` statement support) |
| Dependencies | `betterproto` (protobuf/gRPC), `grpclib` (async gRPC transport) |
| Optional dependencies | `wacp[testing]` — includes `MockRuntime` and test utilities |
| Install | `pip install wacp` |

Package structure:
```
wacp/
├── __init__.py             # Agent, Signal, Envelope, Checkpoint exports
├── _client.py              # Agent class implementation
├── _types.py               # Public dataclasses wrapping protobuf types
├── _connection.py          # Connection lifecycle, reconnection
├── _tools.py               # Tool invocation convenience layer
├── errors.py               # Error hierarchy
├── _generated/             # betterproto-generated code (not public API)
│   ├── primitives.py
│   ├── agent.py
│   └── highway.py
├── testing/
│   ├── __init__.py         # MockRuntime export
│   ├── _mock_runtime.py    # Mock runtime implementation
│   └── _fixtures.py        # Common test fixtures
└── py.typed                # PEP 561 marker for type checking
```

The `_generated/` directory is committed to the repository — not generated at install time. This ensures `pip install wacp` works without a protobuf compiler. The CI pipeline regenerates and verifies the generated code matches the `.proto` files.

**Rust SDK.**

| Attribute | Value |
|-----------|-------|
| Crate name | `wacp-sdk` |
| Distribution | crates.io |
| Rust version | MSRV 1.75+ (for async trait stabilization) |
| Dependencies | `wacp-types` (shared protocol types), `tonic` (gRPC client), `prost` (protobuf), `tokio`, `futures` |
| Feature flags | `testing` — includes `TestRuntime` |
| Install | `cargo add wacp-sdk` |

Crate structure:
```
wacp-sdk/
├── src/
│   ├── lib.rs              # Public exports
│   ├── agent.rs            # Agent struct, connect, action methods
│   ├── builders.rs         # CheckpointBuilder, EnvelopeBuilder, TrailQueryBuilder
│   ├── connection.rs       # Connection lifecycle, reconnection
│   ├── tools.rs            # Tool invocation convenience
│   ├── error.rs            # Error enum
│   ├── streams.rs          # InboxStream, CommandStream
│   └── testing.rs          # #[cfg(feature = "testing")] TestRuntime
├── build.rs                # tonic-build for client stubs
└── Cargo.toml
```

The `wacp-sdk` crate depends on `wacp-types` (the shared type crate from the runtime workspace) for protocol enums and structs. It does NOT depend on any runtime crate — it is a pure client library. The `tonic` dependency is the client side only — no server code.

**Versioning.** Both SDKs follow semantic versioning. The major version tracks the protocol interface version (`wacp.v1` → SDK 1.x). Minor versions add SDK features without protocol changes. Patch versions are bug fixes. Both SDKs declare the interface version they target — the runtime rejects connections from incompatible SDK versions (protocol-interface spec, §10).

## 11. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §4.1 (workspace) | §7 | Mounts field, workspace components |
| §4.2 (envelope) | §3, §4 | Envelope structure, types |
| §4.3 (signal) | §3, §4 | Signal types (closed set), emission rules |
| §4.4 (checkpoint) | §3, §4 | Checkpoint fields, status, confidence |
| §5 (roles and permissions) | §7, §8 | Role constraints, authority enforcement |
| §6.1 (workspace internal model) | §6 | Nine components, working memory |
| §11.8 (security invariants) | §8 | Security events are never silent |

### Implementation Specs

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Runtime spec | §3 (process model) | §9 | Transport actors, InProcessTransport |
| Runtime spec | §8 (workspace isolation) | §6 | Nine components, working memory |
| Runtime spec | §12 (resource enforcement) | §6 | Budget tracking via resource_usage |
| Runtime spec | §16 (crate structure) | §4, §10 | wacp-types shared crate |
| Protocol interface spec | §3 (protobuf types) | §3, §4 | Message definitions, error categories |
| Protocol interface spec | §4 (agent service) | §3, §4 | Agent gRPC service contract |
| Protocol interface spec | §6 (serialization rules) | §3, §4 | Wire format conventions |
| Protocol interface spec | §8 (transport trait) | §9 | InProcessTransport for testing |
| Protocol interface spec | §9 (gRPC implementation) | §8 | Error mapping, code generation |
| Protocol interface spec | §10 (versioning) | §10 | SDK version compatibility |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/PROTOCOL.md) | Implementation Journal: [IMPLEMENTATION.md](../IMPLEMENTATION.md)*
