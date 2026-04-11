# WACP Implementation: Tool Framework

```yaml
id: wacp-impl-tool-framework
type: implementation-spec
status: draft
created: 2026-04-01
lineage: LAYER-MAPPING.md (M5)
protocol_sections:
  - §4.1 (workspace — mounts field)
  - §5 (roles and permissions — authority enforcement)
  - §9 (trail — tool invocation audit)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-sdk-agent
  - wacp-spec-workspace
  - wacp-spec-security
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, middleware, tools, execution, sandboxing, resilience]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Design Principles](#2-design-principles)
3. [Descriptor Schema](#3-descriptor-schema)
4. [Execution Contract](#4-execution-contract)
5. [Handler Interface](#5-handler-interface)
6. [Package Model](#6-package-model)
7. [Registry](#7-registry)
8. [Resilience](#8-resilience)
9. [Sandboxing](#9-sandboxing)
10. [Integration Points](#10-integration-points)
11. [Multi-Language Surface](#11-multi-language-surface)
12. [Crate Structure](#12-crate-structure)
13. [Test Requirements](#13-test-requirements)
14. [References](#14-references)

---

## 1. Purpose

This spec defines the tool framework — the middleware that lets agents interact with the outside world through structured, validated, auditable tool invocations. It answers "how do tools get described, loaded, executed, and protected" — not "what specific tools exist" (that's the ecosystem's job) or "how does an agent decide which tool to call" (that's the LLM adapter's job).

The WACP protocol is deliberately tool-agnostic. The workspace's `mounts` field (§4.1) lists available tools by name and schema, but the protocol says nothing about how tools are implemented, packaged, discovered, or executed. The protocol records tool invocations in the trail but does not define the invocation mechanics. This spec fills that gap.

**Scope.** The tool framework as a Rust crate (`wacp-tools`) with Python and TypeScript bindings. Descriptor schema (what a tool declares about itself). Execution contract (how invocations are validated, timed, and bounded). Handler interface (the function signature tool authors implement). Package model (how tools are distributed). Registry (how tools are loaded and looked up). Resilience (circuit breakers, concurrency limits). Sandboxing (three isolation levels). Integration with the agent SDK, coordinator, local-sdk, and LLM tool-use.

**Not in scope.** Specific tool implementations — filesystem, shell, git, web search, code execution. These belong to the ecosystem layer (Phase 26+). LLM prompt construction — how tool descriptors become function-calling parameters for a specific model. That belongs to the LLM adapter (Phase 21). The envelope-mediated tool flow in the existing SDK (sdk-agent.md §7) is not replaced — it is extended. The framework provides the execution engine; the routing (envelope round-trip vs. direct call) depends on the deployment context (remote agent vs. local agent).

**Design constraint.** The framework must serve two deployment contexts with identical tool code. A tool written once runs both in the runtime process (serving remote agents via the coordinator) and in the user's local process (serving local agents via the local-sdk). The tool author writes one handler; the framework handles the deployment difference. This is the framework's central design problem — everything else follows from it.

---

## 2. Design Principles

Five principles govern the framework. They resolve the tension between developer ergonomics, execution safety, and deployment flexibility.

**Principle 1: Descriptors are the contract.** A tool's descriptor — its name, capabilities, input schemas, output schemas — is the single source of truth. The framework validates inputs against the descriptor before invoking the handler. The LLM adapter reads descriptors to construct tool-use prompts. The agent SDK reads descriptors to expose available tools. The trail records the descriptor version on each invocation. If the descriptor is wrong, nothing works right. If the descriptor is right, everything else follows.

**Principle 2: Execution is mediated, never direct.** No agent calls a tool handler directly. Every invocation passes through the framework's execution engine, which validates input, enforces timeouts, limits result size, tracks concurrency, and records the invocation. This mediation is non-negotiable — it is what makes tools auditable and safe. A tool author cannot bypass it. An agent SDK cannot bypass it. The execution engine is the narrow waist through which all tool invocations flow.

**Principle 3: Failure is expected.** Tools call external systems — filesystems, APIs, databases, shell commands. External systems fail. The framework treats failure as a first-class concern, not an edge case. Every tool invocation has a timeout. External-facing tools have circuit breakers. Errors are structured and typed. The framework never panics on a tool failure — it returns a `ToolError` that the caller can act on.

**Principle 4: Isolation by default for side effects.** A tool that declares `side_effects: true` runs in a child process by default, not in the host process. This prevents a misbehaving tool (infinite loop, memory leak, segfault) from taking down the agent or the runtime. Tools that are pure (no side effects, no external calls) can run in-process for performance. The framework enforces this default; the deployer can override it.

**Principle 5: One handler, any deployment.** A tool handler is a function with a typed signature. The same function runs in the runtime process (for remote agents), in the local-sdk process (for local agents), and in a test harness. The framework abstracts the deployment context behind `ToolContext` — the handler reads its context to learn about permissions, workspace, cancellation, and configuration, but does not know or care where it is executing.

---

## 3. Descriptor Schema

A tool descriptor declares everything the framework, the LLM, and the agent need to know about a tool — without running it.

### 3.1 ToolDescriptor

```rust
pub struct ToolDescriptor {
    /// Unique name. Lowercase, alphanumeric + underscores. Max 64 chars.
    /// Used in tool invocations, LLM function-calling, trail entries.
    pub name: String,

    /// Semantic version (major.minor.patch). The framework rejects two
    /// packages with the same name and different major versions.
    pub version: String,

    /// Human-readable description. Used in LLM tool-use prompts.
    /// One sentence preferred — LLMs perform better with concise descriptions.
    pub description: String,

    /// The tool's callable capabilities. Most tools have exactly one.
    /// Multi-capability tools (e.g., a "git" tool with "status", "diff",
    /// "commit" capabilities) group related operations under one name.
    pub capabilities: Vec<Capability>,

    /// Per-deployment configuration schema. Optional. When present,
    /// the registry validates the deployer's config against this schema
    /// at load time — not at invocation time.
    pub config_schema: Option<JsonSchema>,

    /// Metadata tags for filtering and discovery. Not used by the
    /// execution engine. Used by ecosystem verticals to categorize tools
    /// (e.g., ["swe", "filesystem", "read-only"]).
    pub tags: Vec<String>,
}
```

**Name uniqueness.** Names are globally unique within a registry instance. Two tools cannot share a name. If two packages attempt to register the same name, the second registration fails. Names are stable identifiers — changing a tool's name is a breaking change.

**Version semantics.** The framework enforces semver. A registry accepts exactly one version of each tool name. Upgrades replace the previous version. The framework does not support running multiple versions of the same tool simultaneously — this avoids the complexity of version routing. The version is recorded in trail entries for auditability.

### 3.2 Capability

```rust
pub struct Capability {
    /// Capability name. Scoped to the tool — "read" is unambiguous
    /// within a tool, even if another tool also has a "read" capability.
    pub name: String,

    /// Human-readable description. Used in LLM tool-use prompts.
    pub description: String,

    /// JSON Schema for the input. The execution engine validates
    /// invocation arguments against this schema before calling the handler.
    /// Also used by LLM adapters to construct function-calling parameters.
    pub input_schema: JsonSchema,

    /// JSON Schema for the output. Used for documentation and by
    /// downstream consumers that parse tool results programmatically.
    /// The execution engine does NOT validate outputs against this schema —
    /// output validation would mask bugs instead of surfacing them.
    pub output_schema: JsonSchema,

    /// Default timeout in milliseconds. Overridable per-invocation.
    /// Bounded above by the framework maximum (§4.2).
    /// None means "use the framework default" (30_000ms).
    pub timeout_ms: Option<u64>,

    /// Whether this capability is idempotent — calling it twice with
    /// the same input produces the same result. The framework uses this
    /// to decide retry behavior (§8). Idempotent capabilities can be
    /// retried on transient failure; non-idempotent cannot.
    pub idempotent: bool,

    /// Whether this capability has side effects — modifies external state
    /// (filesystem, database, API). Determines the default sandbox level
    /// (§9). Side-effecting capabilities default to process isolation.
    pub side_effects: bool,
}
```

**JSON Schema choice.** JSON Schema (draft 2020-12) is the descriptor's schema language for both inputs and outputs. Three reasons. First, every major LLM API accepts JSON Schema for function-calling parameters — Anthropic, OpenAI, and all OpenAI-compatible providers. Using JSON Schema means the descriptor's `input_schema` can be passed directly to the LLM with no translation. Second, JSON Schema has mature validation libraries in all three target languages (Rust: `jsonschema`, Python: `jsonschema`, TypeScript: `ajv`). Third, JSON Schema is well-understood by developers — it has lower adoption friction than Protocol Buffers, Avro, or custom schema languages.

**No output validation.** The execution engine validates inputs but not outputs. This is deliberate. Input validation catches caller mistakes before the handler runs — fast failure, clear error, no wasted work. Output validation would catch handler bugs after the handler runs — the work is already done, the side effects have occurred, and the "fix" would be to swallow the result and return an error, which is worse than returning the malformed result. Handler bugs are caught by tests, not by runtime validation.

### 3.3 Configuration Schema

Tools that need per-deployment configuration declare a `config_schema`. Examples: API keys, base URLs, working directories, feature flags.

The configuration lifecycle:

1. **Descriptor declares schema.** The `config_schema` field defines what the tool accepts.
2. **Deployer provides config.** At load time, the registry receives the deployer's configuration (from YAML, environment variables, or programmatic injection).
3. **Registry validates.** The config is validated against the schema. Invalid config → load failure for that tool (not a fatal error — other tools load normally).
4. **Config injected into handler.** The validated config is available on `ToolContext.config` at invocation time. The handler reads it but cannot modify it.

Configuration is immutable after load. A tool cannot modify its own configuration at runtime. If config changes are needed (e.g., API key rotation), the tool is unloaded and reloaded with new config.

**Secrets in config.** API keys and credentials are config values. The framework treats config opaquely — it does not distinguish secrets from non-secrets. Secret management (encryption at rest, redaction in logs) is the security layer's responsibility (Phase 23, M7). The framework's contract is: config values are never included in trail entries, tool errors, or descriptors. They exist only in the handler's `ToolContext`.

---

## 4. Execution Contract

Every tool invocation passes through the execution engine. The engine enforces six guarantees regardless of the tool, the caller, or the deployment context.

### 4.1 Input Validation

Before the handler is called, the engine validates the invocation arguments against the capability's `input_schema`.

1. Deserialize the arguments as a JSON value.
2. Validate against the JSON Schema. Use the same validator in all three languages to avoid cross-language divergence.
3. On validation failure: return `ToolError::ValidationFailed` with the schema violation details. The handler is never invoked.
4. On success: pass the validated arguments to the handler.

**Type coercion.** The engine does not coerce types. If the schema says `"type": "integer"` and the caller sends `"42"` (a string), validation fails. The caller must send `42` (a number). This strictness catches bugs early — an LLM that sends a string instead of a number gets a clear error, not a silent coercion that breaks downstream logic.

### 4.2 Timeout Enforcement

Every invocation has a timeout. If the handler does not return within the timeout, the invocation is cancelled and the engine returns `ToolError::Timeout`.

**Timeout hierarchy (three levels, from specific to general):**

| Level | Source | Example |
|-------|--------|---------|
| **Invocation** | Caller passes `timeout_ms` per-call | `registry.execute("shell", args, timeout_ms=5000)` |
| **Capability** | Descriptor's `Capability.timeout_ms` | `timeout_ms: Some(30_000)` in the descriptor |
| **Framework** | Global maximum, configurable at registry creation | Default: 300_000ms (5 minutes) |

Resolution: invocation overrides capability, capability overrides framework default, framework maximum caps everything. An invocation timeout of 600_000ms with a framework maximum of 300_000ms resolves to 300_000ms.

**Cancellation mechanism.** The engine sets a deadline and provides a cancellation token to the handler via `ToolContext.cancellation_token`. In Rust, this is a `tokio::CancellationToken`. In Python, an `asyncio.Event` set on timeout. In TypeScript, an `AbortSignal`. Cooperative cancellation — the handler must check the token periodically to stop promptly. If the handler ignores the token, the engine waits until the timeout and then abandons the handler's task (dropping the future in Rust, cancelling the task in Python/TypeScript).

**Process-isolated timeouts.** For tools running in a child process (§9), the engine enforces the timeout from the parent. If the child does not respond within the timeout, the engine kills the child process (`SIGKILL` on Unix). This is the hard boundary — a tool running in process isolation cannot exceed its timeout under any circumstances.

### 4.3 Error Model

Tool errors are structured, typed, and actionable.

```rust
pub struct ToolError {
    /// Error category. Determines caller behavior.
    pub code: ToolErrorCode,

    /// Human-readable explanation. Included in trail entry.
    pub message: String,

    /// Whether retrying with the same input might succeed.
    /// The framework uses this to decide automatic retry (§8).
    pub retryable: bool,
}

pub enum ToolErrorCode {
    /// Input did not pass schema validation.
    ValidationFailed,
    /// Handler timed out.
    Timeout,
    /// Handler returned an error (tool-specific).
    ExecutionFailed,
    /// Handler panicked or child process crashed.
    InternalError,
    /// Tool is currently circuit-broken (§8.1).
    Unavailable,
    /// Concurrency limit reached, queue full (§8.3).
    Overloaded,
    /// Cancellation was requested and the handler honored it.
    Cancelled,
}
```

**No stack traces.** `ToolError.message` never includes stack traces, file paths, or internal handler details. Stack traces are logged at the framework level (debug log) but not exposed to callers or recorded in the trail. This prevents information leakage — a tool calling an external API should not expose the API's internal error structure to the agent.

**Handler errors.** If a handler returns an error, the engine wraps it in `ToolError::ExecutionFailed`. If a handler panics (Rust: `catch_unwind`, Python: uncaught exception, TypeScript: uncaught rejection), the engine wraps it in `ToolError::InternalError`. The handler's error message is preserved in `ToolError.message` but truncated to 4096 bytes.

### 4.4 Cancellation

Cancellation is cooperative. The engine provides the signal; the handler decides how to stop.

**When cancellation fires:**

1. **Timeout reached.** The engine's deadline expires.
2. **Caller cancelled.** The agent SDK or coordinator cancelled the invocation (e.g., workspace terminating).
3. **Framework shutdown.** The registry is shutting down.

**Handler obligation.** Handlers SHOULD check the cancellation token at natural checkpoints — before starting expensive operations, between iterations of a loop, after an external call returns. Handlers that ignore the token are not broken — they just run until the hard timeout, at which point the engine abandons them.

**Result on cancellation.** If the handler observes cancellation and returns early, the engine returns `ToolError::Cancelled`. If the engine abandons the handler after the hard timeout, it returns `ToolError::Timeout`. The distinction matters: `Cancelled` is clean (the handler stopped itself), `Timeout` is dirty (the handler was killed or abandoned).

### 4.5 Result Limits

The engine enforces a maximum result size. Default: 1 MB. Configurable per-registry.

**Why.** Tool results feed into LLM context windows. An unbounded result (e.g., `cat` on a 100 MB file) would exhaust the context budget and produce an LLM error. The limit is a safety net — tools SHOULD return reasonably-sized results, but the framework enforces a hard cap.

**Enforcement.** After the handler returns, the engine serializes the result and checks byte length. If the result exceeds the limit, the engine returns `ToolError::ExecutionFailed` with a message indicating the result was too large and its actual size. The handler's result is discarded.

**Streaming results.** The initial implementation does not support streaming tool results. A tool that produces large output (e.g., log tailing, file search across many files) must truncate or summarize within the handler. Streaming support is a future concern — the interface is designed to accommodate it (the handler signature can be extended with a stream variant) but the first implementation is request-response.

### 4.6 Concurrency

The engine limits concurrent invocations per tool. Default: 10 concurrent, 50 queued.

**Why.** A tool calling an external API should not overwhelm it with concurrent requests. A tool executing shell commands should not fork-bomb the system. Per-tool concurrency limits prevent a single tool from consuming all available resources.

**Mechanism.** Each tool in the registry has a semaphore (Rust: `tokio::sync::Semaphore`). The engine acquires a permit before invoking the handler and releases it when the handler completes.

- If a permit is available: the handler runs immediately.
- If no permit is available and the queue is not full: the invocation waits (bounded by the invocation timeout).
- If no permit is available and the queue is full: the engine returns `ToolError::Overloaded` immediately.

**Configuration.** Per-tool overrides via the registry's tool configuration:

```rust
pub struct ToolConcurrencyConfig {
    /// Maximum concurrent handler invocations. 0 = unlimited.
    pub max_concurrent: usize,  // default: 10
    /// Maximum queued invocations waiting for a permit. 0 = no queue.
    pub max_queued: usize,      // default: 50
}
```

---

## 5. Handler Interface

The handler is the function a tool author writes. The framework calls it; the author implements it.

### 5.1 Rust

```rust
/// The handler signature. Async, takes context + validated args, returns result or error.
#[async_trait]
pub trait ToolHandler: Send + Sync + 'static {
    async fn execute(
        &self,
        ctx: &ToolContext,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, ToolError>;
}
```

**`ToolContext`** carries the invocation environment:

```rust
pub struct ToolContext {
    /// Which tool and capability is being invoked.
    pub tool_name: String,
    pub capability_name: String,

    /// The workspace on whose behalf this invocation occurs.
    /// None if invoked outside a workspace (e.g., during testing).
    pub workspace_id: Option<WorkspaceId>,

    /// Cancellation token. Check this in long-running handlers.
    pub cancellation_token: CancellationToken,

    /// The tool's deployment configuration (validated against config_schema).
    /// Empty object if the tool has no config_schema.
    pub config: serde_json::Value,

    /// Effective timeout for this invocation (after hierarchy resolution).
    pub timeout_ms: u64,
}
```

**Why `serde_json::Value` for args and result.** Tool inputs and outputs are JSON-typed by design — the descriptors use JSON Schema, LLMs produce JSON, and the trail stores JSON. Using `serde_json::Value` as the handler's type means: no custom deserialization per-tool (the framework already validated against the schema), no generic type parameter (handlers are object-safe, storable in the registry), and no serialization cost at the boundary (the value is already deserialized). Handlers that want typed access call `serde_json::from_value::<MyArgs>(args)` internally — the framework does not prevent this, but it does not require it.

**Stateless handlers.** The `ToolHandler` trait takes `&self`, allowing handlers to hold state (connection pools, caches). But the framework encourages stateless handlers — state goes into `ToolContext.config` (immutable) or external systems. Stateful handlers complicate testing and break the "one handler, any deployment" principle. The trait allows it; the convention discourages it.

### 5.2 Python

```python
from wacp.tools import ToolContext, ToolError

async def handler(ctx: ToolContext, args: dict) -> dict:
    """A tool handler is an async function. No class needed."""
    path = args["path"]
    content = await read_file(path)
    return {"content": content, "size": len(content)}
```

Python handlers are plain async functions. No class hierarchy, no decorator magic at the handler level. The `ToolContext` and `ToolError` types mirror the Rust equivalents.

**Decorator for convenience.** For tool authors who prefer a more declarative style, the Python binding provides a `@tool` decorator that constructs the `ToolPackage` from function metadata:

```python
from wacp.tools import tool, ToolContext

@tool(
    name="read_file",
    description="Read a file's contents",
    input_schema={"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]},
    output_schema={"type": "object", "properties": {"content": {"type": "string"}}},
    side_effects=False,
)
async def read_file(ctx: ToolContext, args: dict) -> dict:
    content = open(args["path"]).read()
    return {"content": content}
```

The decorator is sugar — it constructs a `ToolPackage` with one capability and one handler. It does not change the handler's signature or behavior.

### 5.3 TypeScript

```typescript
import { ToolContext, ToolError } from "@wacp/tools";

export async function handler(
  ctx: ToolContext,
  args: Record<string, unknown>
): Promise<Record<string, unknown>> {
  const path = args.path as string;
  const content = await Bun.file(path).text();
  return { content, size: content.length };
}
```

TypeScript handlers follow the same pattern. The `ToolContext` type mirrors Rust. `AbortSignal` is used for cancellation (available via `ctx.signal`).

---

## 6. Package Model

A tool package is the distributable unit — descriptor + handlers + lifecycle hooks. It is the atom of tool distribution.

### 6.1 Package Structure

```rust
pub struct ToolPackage {
    /// The tool's full descriptor (name, version, capabilities, config schema, tags).
    pub descriptor: ToolDescriptor,

    /// One handler per capability. The key is the capability name.
    /// The registry validates at load time that every capability in the
    /// descriptor has exactly one handler and every handler key matches
    /// a capability name.
    pub handlers: HashMap<String, Box<dyn ToolHandler>>,

    /// Called once at load time, after config validation.
    /// Use for connection pool initialization, cache warming, resource acquisition.
    /// If initialize fails, the tool is not registered (skip, log, continue).
    pub initialize: Option<Box<dyn FnOnce(serde_json::Value) -> BoxFuture<'static, Result<(), ToolError>> + Send>>,

    /// Called once at shutdown. Use for cleanup.
    /// If shutdown fails, the error is logged but does not prevent registry shutdown.
    pub shutdown: Option<Box<dyn FnOnce() -> BoxFuture<'static, Result<(), ToolError>> + Send>>,
}
```

**Handler-descriptor alignment.** The registry enforces a strict 1:1 mapping: every capability name in `descriptor.capabilities` must have a handler in `handlers`, and every key in `handlers` must match a capability name. A misalignment (capability without handler, handler without capability) is a load error.

### 6.2 Lifecycle Hooks

Two hooks: `initialize` and `shutdown`. Both are optional.

**initialize.** Called after the registry validates the descriptor and config. Receives the validated config. Use cases: open database connections, verify API key validity, create working directories, warm caches. If `initialize` returns an error, the tool is skipped — not registered, not available. The error is logged. Other tools load normally.

**shutdown.** Called when the registry is being shut down (runtime exit, hot reload). Use cases: close connections, flush buffers, release resources. If `shutdown` returns an error, the error is logged and shutdown continues. Shutdown is best-effort — the process may exit before all tools finish cleanup.

**No hot config reload.** Tools do not receive config changes after initialization. If config changes (e.g., API key rotation), the deployer unloads and reloads the tool. This is a deliberate simplification — hot config reload introduces state consistency problems (what happens to in-flight invocations?) that are not worth solving in the first implementation.

### 6.3 Configuration Injection

Configuration flows from the deployer to the tool through the registry:

1. Deployer provides a `HashMap<String, serde_json::Value>` at registry creation — tool name → config value.
2. When a package is registered, the registry looks up `config[package.descriptor.name]`.
3. If the tool has a `config_schema`: validate the config against it. Missing config with a schema → load error. Invalid config → load error.
4. If the tool has no `config_schema`: config is ignored (even if provided).
5. Validated config is passed to `initialize` and stored for inclusion in `ToolContext` on each invocation.

---

## 7. Registry

The registry is the framework's central data structure. It holds loaded tools, resolves invocations, and manages tool lifecycle.

### 7.1 Registration

```rust
impl ToolRegistry {
    /// Create a new registry with framework-level configuration.
    pub fn new(config: RegistryConfig) -> Self;

    /// Register a tool package. Validates descriptor, config, and handler alignment.
    /// Calls initialize if present. Returns error if validation or init fails.
    pub async fn register(&mut self, package: ToolPackage) -> Result<(), RegistryError>;

    /// Unregister a tool by name. Calls shutdown if present.
    /// In-flight invocations complete normally; new invocations fail with NotFound.
    pub async fn unregister(&mut self, name: &str) -> Result<(), RegistryError>;
}
```

**RegistryConfig:**

```rust
pub struct RegistryConfig {
    /// Per-tool deployment configuration. Tool name → config value.
    pub tool_configs: HashMap<String, serde_json::Value>,

    /// Framework-level defaults.
    pub default_timeout_ms: u64,            // default: 30_000
    pub max_timeout_ms: u64,                // default: 300_000
    pub max_result_bytes: usize,            // default: 1_048_576 (1 MB)
    pub default_concurrency: ToolConcurrencyConfig,
}
```

### 7.2 Discovery

Discovery is the process of finding tool packages to register. The framework provides two discovery mechanisms:

**Programmatic registration.** The caller constructs `ToolPackage` values and calls `registry.register()`. This is the primary mechanism — it is explicit, testable, and composable. The caller decides what tools to load and in what order.

**Directory scan.** The framework provides a utility function that scans a directory for tool packages. Each subdirectory must contain a `descriptor.json` (or `descriptor.yaml`) and a handler module. The scan produces a `Vec<ToolPackage>` that the caller registers. The scan is a convenience — it does not bypass `registry.register()`.

```rust
/// Scan a directory for tool packages. Returns discovered packages.
/// Each subdirectory must contain descriptor.json + handler module.
/// Invalid packages are logged and skipped.
pub async fn discover_tools(dir: &Path) -> Vec<ToolPackage>;
```

**No auto-discovery at runtime.** The registry does not watch directories, poll package managers, or auto-load tools. Discovery is explicit — the caller invokes it at boot time. This prevents surprises (a file appearing in a directory should not change runtime behavior) and makes the tool set deterministic.

### 7.3 Execution

```rust
impl ToolRegistry {
    /// Execute a tool capability. Validates input, enforces timeout,
    /// records metrics, returns result or error.
    pub async fn execute(
        &self,
        tool_name: &str,
        capability_name: &str,
        args: serde_json::Value,
        opts: ExecutionOptions,
    ) -> Result<serde_json::Value, ToolError>;

    /// List all registered tools with their descriptors.
    pub fn list_tools(&self) -> Vec<&ToolDescriptor>;

    /// Get a single tool's descriptor by name.
    pub fn get_tool(&self, name: &str) -> Option<&ToolDescriptor>;
}

pub struct ExecutionOptions {
    /// Override timeout for this invocation. Capped by framework max.
    pub timeout_ms: Option<u64>,
    /// Workspace context for this invocation.
    pub workspace_id: Option<WorkspaceId>,
    /// External cancellation token (e.g., from the agent SDK).
    pub cancellation_token: Option<CancellationToken>,
}
```

**Execution sequence.** When `execute` is called:

1. **Lookup.** Find the tool and capability by name. If not found → `ToolError::ValidationFailed`.
2. **Circuit breaker check.** If the tool's circuit breaker is open → `ToolError::Unavailable`.
3. **Concurrency permit.** Acquire a semaphore permit (or queue, or reject with `Overloaded`).
4. **Input validation.** Validate `args` against `capability.input_schema`.
5. **Context construction.** Build `ToolContext` with resolved timeout, config, workspace, cancellation.
6. **Sandbox dispatch.** Route to in-process execution, child process, or container based on sandbox policy.
7. **Handler invocation.** Call the handler with context + validated args.
8. **Result check.** Verify result size is within limit.
9. **Metrics update.** Record latency, success/failure. Update circuit breaker state.
10. **Return.** Return result or error to caller.

---

## 8. Resilience

Tools call external systems that fail, slow down, and overload. The framework provides three resilience mechanisms that protect the agent and the runtime from tool instability.

### 8.1 Circuit Breaker

A per-tool circuit breaker prevents repeated calls to a failing tool. Three states:

| State | Behavior | Transition |
|-------|----------|------------|
| **Closed** | Invocations proceed normally. Failures are counted. | → Open when failure count reaches threshold within window |
| **Open** | All invocations immediately return `ToolError::Unavailable`. | → Half-Open after cooldown period |
| **Half-Open** | One probe invocation is allowed. | → Closed if probe succeeds. → Open if probe fails. |

**Configuration:**

```rust
pub struct CircuitBreakerConfig {
    /// Number of failures within the window to trip the breaker.
    pub failure_threshold: u32,    // default: 5
    /// Time window for counting failures.
    pub failure_window: Duration,  // default: 60s
    /// How long the breaker stays open before allowing a probe.
    pub cooldown: Duration,        // default: 30s
    /// Whether the circuit breaker is enabled for this tool.
    pub enabled: bool,             // default: false
}
```

**Default: disabled.** Circuit breakers are opt-in per tool. Pure tools (filesystem reads, string manipulation) don't need them. External-calling tools (HTTP APIs, database queries) should enable them. The ecosystem vertical enables circuit breakers for its tools via the tool's deployment config.

**What counts as failure.** `ToolError::Timeout`, `ToolError::ExecutionFailed`, `ToolError::InternalError`. Not: `ToolError::ValidationFailed` (caller's fault), `ToolError::Cancelled` (caller's choice), `ToolError::Overloaded` (concurrency limit).

### 8.2 Timeout Hierarchy

Covered in §4.2. The hierarchy is the resilience mechanism — it ensures no invocation runs unbounded. The framework maximum is the hard ceiling.

### 8.3 Concurrency Limiter

Covered in §4.6. The semaphore-based limiter prevents resource exhaustion. The queue provides buffering; the queue size limit provides backpressure.

---

## 9. Sandboxing

Sandboxing controls where the handler's code runs. Three levels, from least to most isolated.

### 9.1 None (In-Process)

The handler runs in the host process — same address space, same event loop. No isolation boundary.

**When to use.** Pure tools with no side effects: JSON manipulation, text formatting, math computation, schema validation. These tools cannot harm the host, and in-process execution avoids the IPC overhead.

**Risk.** A handler that panics, leaks memory, or blocks the event loop affects the host. Use only for trusted, well-tested tools.

### 9.2 Process (Child Process)

The handler runs in a child process. The framework communicates with it via stdin/stdout (JSON messages over stdio).

**Protocol.** The framework spawns the child with: tool name, capability name, args (JSON on stdin), config (environment variable or temp file). The child writes the result (JSON on stdout) and exits. The framework reads stdout, parses the result, and returns it.

**Enforcement.** The framework enforces:
- Timeout: `SIGKILL` after timeout (no cooperative cancellation needed — hard kill).
- Result size: read at most `max_result_bytes` from stdout.
- Exit code: 0 = success (parse stdout as result), non-zero = failure (parse stdout as error message).

**Implementation.** Rust: `tokio::process::Command`. Python: `asyncio.create_subprocess_exec`. TypeScript: `Bun.spawn` / `child_process`.

**Performance.** ~5-10ms overhead per invocation (process spawn + IPC). Acceptable for tools that do meaningful work (filesystem operations, shell commands, API calls). Not acceptable for high-frequency pure tools — use in-process for those.

### 9.3 Container (Docker)

The handler runs in a Docker container. Maximum isolation — separate filesystem, network, user namespace.

**When to use.** Untrusted tools. Code execution tools (running user-provided code). Tools that need a specific environment (Python version, system libraries).

**Protocol.** Same as process sandboxing (JSON over stdio), but the child process is a Docker container instead of a bare process.

**Implementation.** The framework builds (or pulls) a container image specified in the tool's deployment config. The container receives args on stdin, writes result on stdout, and exits. The framework enforces timeout by killing the container.

**Configuration.** Per-tool container config:

```rust
pub struct ContainerConfig {
    /// Docker image to use. Must be pre-pulled or buildable.
    pub image: String,
    /// Memory limit (bytes). Default: 256 MB.
    pub memory_limit: u64,
    /// CPU limit (millicores). Default: 1000 (1 CPU).
    pub cpu_limit: u32,
    /// Network access. Default: false (no network).
    pub network: bool,
    /// Volume mounts. Default: empty (no host filesystem access).
    pub volumes: Vec<VolumeMount>,
}
```

### 9.4 Policy Selection

The framework selects the sandbox level based on the tool's declaration and the deployer's override:

| `side_effects` | `deployer_override` | Result |
|:-:|:-:|:-:|
| `false` | none | In-process |
| `true` | none | Process |
| any | `"none"` | In-process |
| any | `"process"` | Process |
| any | `"container"` | Container |

The deployer can override in either direction — they can force a pure tool into a container (for defense-in-depth) or allow a side-effecting tool to run in-process (for performance, at their own risk). The default policy is the safe default; overrides are explicit.

---

## 10. Integration Points

The tool framework connects to three consumers: the agent SDK, the coordinator, and the LLM adapter. Each uses a different facet of the framework.

### 10.1 Agent SDK Binding

The enriched agent SDK (Phase 22, M1) exposes `tool()` and `tools()` methods on `AgentContext`. These delegate to the tool framework.

**For remote agents (coordinated by the runtime):**

```
Agent calls agent.tool("read_file", {path: "/src/main.rs"})
  → Agent SDK sends query envelope to coordinator
  → Coordinator's RequestHandler receives the query
  → Coordinator calls registry.execute("read_file", "read", args)
  → Tool framework validates, executes, returns result
  → Coordinator sends feedback envelope with result
  → Agent SDK receives feedback, returns result to agent
```

The existing envelope-mediated flow (sdk-agent.md §7) is preserved. The tool framework provides the execution engine that the coordinator uses to actually run the tool. The framework replaces the opaque "coordinator executes the tool" step with a structured, validated, auditable execution.

**For local agents (local-sdk, Phase 24, M3):**

```
Agent calls agent.tool("read_file", {path: "/src/main.rs"})
  → Local SDK's self-orchestration layer handles directly
  → Local SDK calls registry.execute("read_file", "read", args)
  → Tool framework validates, executes, returns result
  → Local SDK returns result to agent
```

No envelope round-trip. The local-sdk holds a `ToolRegistry` instance and invokes tools directly. Same framework, same validation, same auditing — just no coordinator mediation.

**`tools()` method.** Returns the list of available tools from `registry.list_tools()`. The agent SDK transforms `ToolDescriptor` into the format the LLM expects (via the LLM adapter).

### 10.2 LLM Tool-Use Mapping

The LLM adapter (Phase 21, M6) reads tool descriptors and converts them to the LLM's function-calling format.

**Anthropic (Claude):**

```json
{
  "name": "read_file",
  "description": "Read a file's contents",
  "input_schema": {
    "type": "object",
    "properties": {"path": {"type": "string"}},
    "required": ["path"]
  }
}
```

The `Capability.input_schema` maps directly — no translation needed. The `name` is `tool_name` (for single-capability tools) or `tool_name.capability_name` (for multi-capability tools). The `description` is the capability's description.

**OpenAI (GPT):**

```json
{
  "type": "function",
  "function": {
    "name": "read_file",
    "description": "Read a file's contents",
    "parameters": {
      "type": "object",
      "properties": {"path": {"type": "string"}},
      "required": ["path"]
    }
  }
}
```

Same content, different envelope. The LLM adapter handles the wrapping.

**The framework does not do this mapping.** It provides descriptors; the LLM adapter transforms them. This separation keeps the framework LLM-agnostic and the LLM adapter tool-agnostic. They communicate through the descriptor schema — the contract.

### 10.3 Runtime Coordination

The coordinator uses the tool framework to:

1. **Validate tool availability at workspace creation.** When creating a workspace with `mounts: ["read_file", "shell_exec"]`, the coordinator checks that both tools exist in the registry. If a mounted tool is missing, workspace creation fails.
2. **Execute tools on agent request.** When a remote agent sends a tool invocation query (§10.1), the coordinator delegates to `registry.execute()`.
3. **Populate the bind response.** The bind response includes tool descriptors for all mounted tools, so the agent SDK and LLM adapter know what's available.

The tool framework is instantiated once in the runtime process. The coordinator holds a reference to the shared `ToolRegistry`. The registry is thread-safe (`Send + Sync`) — concurrent invocations from multiple workspace actors are safe.

---

## 11. Multi-Language Surface

Three languages, one framework. The Rust crate is the core; Python and TypeScript are bindings.

### 11.1 Rust (`wacp-tools`)

The authoritative implementation. All types, the execution engine, the registry, resilience, and sandboxing live here. Published as a Cargo crate.

**Used by:** Runtime (coordinator tool execution), Rust agent SDK.

### 11.2 Python (`wacp.tools`)

A Python package that provides tool authoring types and a registry client.

**Tool authoring types:** `ToolDescriptor`, `Capability`, `ToolPackage`, `ToolContext`, `ToolError`, `@tool` decorator. Tool authors use these to define tools in Python.

**Registry interaction:** The Python registry delegates to the Rust core. Two modes:
- **In-process (local-sdk):** Python calls Rust via PyO3 bindings. The Rust registry runs in the same process. This is the performance path — no IPC overhead for tool validation and in-process execution.
- **Remote (coordinated agent):** Tool invocations go through gRPC (the agent SDK's query envelope). The Python package does not need a local registry.

**Published as:** Part of the `wacp` PyPI package (extended from the existing `wacp` agent SDK).

### 11.3 TypeScript (`@wacp/tools`)

An npm package providing tool authoring types and a registry for the local-sdk.

**Tool authoring types:** `ToolDescriptor`, `Capability`, `ToolPackage`, `ToolContext`, `ToolError`. TypeScript interfaces matching the Rust structs.

**Registry:** A TypeScript implementation of the registry for the local-sdk (CLI agent, IDE). This is a native TypeScript implementation — not a Rust binding — because the local-sdk runs in a TypeScript process (Bun/Node) and tools authored in TypeScript should execute in-process without FFI overhead.

**Shared types via JSON Schema.** The descriptor format is JSON-native. Rust, Python, and TypeScript each parse the same `descriptor.json` file. No code generation needed — the schema IS the contract.

**Published as:** `@wacp/tools` on npm.

---

## 12. Crate Structure

```
crates/wacp-tools/
├── src/
│   ├── lib.rs              # Public exports: ToolDescriptor, ToolPackage, ToolRegistry, ToolError, etc.
│   ├── descriptor.rs       # ToolDescriptor, Capability, validation, JSON Schema support
│   ├── execution.rs        # Execution engine: validate → timeout → invoke → check result
│   ├── handler.rs          # ToolHandler trait, ToolContext, ToolError types
│   ├── package.rs          # ToolPackage, lifecycle hooks, config injection
│   ├── registry.rs         # ToolRegistry: register, unregister, execute, list, get
│   ├── resilience.rs       # CircuitBreaker, ConcurrencyLimiter
│   ├── sandbox.rs          # SandboxPolicy, ProcessSandbox, ContainerSandbox
│   └── discovery.rs        # Directory scanner, descriptor loader
├── Cargo.toml
└── tests/
    ├── descriptor_tests.rs # Schema validation, version checks, name rules
    ├── execution_tests.rs  # Timeout, cancellation, result limits, error wrapping
    ├── registry_tests.rs   # Register, unregister, lookup, handler-descriptor alignment
    ├── resilience_tests.rs # Circuit breaker states, concurrency limits, queue behavior
    └── sandbox_tests.rs    # Process spawn, timeout kill, stdio protocol
```

**Dependencies:**

| Crate | Purpose |
|-------|---------|
| `wacp-types` | `WorkspaceId`, protocol types |
| `serde`, `serde_json` | JSON serialization |
| `jsonschema` | JSON Schema validation |
| `tokio` | Async runtime, process spawn, semaphore, cancellation |
| `async-trait` | Async trait support |

**Not a dependency:** `wacp-coordinator`, `wacp-transport`, `wacp-sdk`. The tool framework is a standalone middleware crate. It is consumed by the coordinator and SDKs — it does not depend on them.

---

## 13. Test Requirements

| Module | Tests | Coverage target |
|--------|-------|----------------|
| `descriptor.rs` | Name validation (length, chars). Version parsing (valid, invalid). Capability schema validation. Config schema validation. Descriptor with 0 capabilities → error. Duplicate capability names → error. | Every validation rule has a positive and negative test. |
| `execution.rs` | Input validation pass/fail. Timeout: handler returns in time → success. Handler exceeds timeout → `Timeout` error. Cancellation: token fires → `Cancelled`. Result size: within limit → pass, exceeds → error. | Every step in the execution sequence (§7.3) is individually tested. |
| `handler.rs` | Handler returns success. Handler returns error → `ExecutionFailed`. Handler panics → `InternalError`. ToolContext fields populated correctly. | Every `ToolErrorCode` is triggered by at least one test. |
| `registry.rs` | Register valid package → success. Register duplicate name → error. Register with missing handler → error. Register with extra handler → error. Unregister → subsequent execute returns NotFound. Execute valid → success. Execute unknown tool → error. list_tools returns all. Config validation: missing required → error, invalid → error. Initialize failure → tool not registered. | Every `RegistryError` variant is triggered. |
| `resilience.rs` | Circuit breaker: closed → stays closed on success. Closed → open on N failures. Open → rejects immediately. Open → half-open after cooldown. Half-open → closed on probe success. Half-open → open on probe failure. Concurrency: within limit → all proceed. At limit → queue. Queue full → `Overloaded`. | All state transitions tested. |
| `sandbox.rs` | In-process: handler called directly. Process: child spawned, args on stdin, result on stdout, exit 0. Process timeout: child killed, `Timeout` returned. Process crash: non-zero exit → `InternalError`. | Each sandbox level has a success and failure test. |

**Total target: ~60 tests for the Rust crate.**

---

## 14. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §4.1 (workspace) | §1, §10 | Mounts field — tools declared at workspace creation |
| §5 (roles and permissions) | §10 | Authority enforcement on tool invocations |
| §9 (trail) | §4, §10 | Tool invocations recorded as trail entries |
| §11 (security) | §3, §4 | No secrets in trail, no stack traces in errors |

### Implementation Specs

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Runtime spec | §3 (process model) | §10 | Coordinator actor, workspace actor |
| Runtime spec | §5 (permission engine) | §10 | Authority enforcement |
| SDK agent spec | §7 (tool mounting) | §10 | Existing tool invocation flow |
| SDK agent spec | §3–4 (SDK surface) | §10 | `tool()` and `tools()` methods |
| LAYER-MAPPING.md | M5 | §1 | Architectural position, design specs |

### Future Specs

| Spec | Referenced in | Topic |
|------|--------------|-------|
| LLM adapters (Phase 21) | §10 | Descriptor → LLM function-calling conversion |
| Agent SDK v2 (Phase 22) | §10 | `AgentContext.tool()` and `AgentContext.tools()` |
| Local SDK (Phase 24) | §10 | Direct registry invocation, no envelope round-trip |
| Security (Phase 23) | §3 | Secret management for tool config |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/PROTOCOL.md) | Implementation Plan: [IMPLEMENTATION.md](../IMPLEMENTATION.md)*
