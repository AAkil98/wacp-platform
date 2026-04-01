# WACP Implementation: LLM Adapter Framework

```yaml
id: wacp-impl-llm-adapters
type: implementation-spec
status: draft
created: 2026-04-01
lineage: LAYER-MAPPING.md (M6)
protocol_sections:
  - §4.4 (checkpoint — resource_usage for token tracking)
  - §9 (trail — inference events)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-tool-framework
  - wacp-spec-workspace
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, middleware, llm, inference, streaming, providers, resilience]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Design Principles](#2-design-principles)
3. [Message Types](#3-message-types)
4. [Adapter Trait](#4-adapter-trait)
5. [Completion Result](#5-completion-result)
6. [Streaming](#6-streaming)
7. [Anthropic Provider](#7-anthropic-provider)
8. [OpenAI Provider](#8-openai-provider)
9. [Generic Provider](#9-generic-provider)
10. [Error Classification](#10-error-classification)
11. [Retry Logic](#11-retry-logic)
12. [Rate Limiting](#12-rate-limiting)
13. [Cost Model](#13-cost-model)
14. [Tool-Use Mapping](#14-tool-use-mapping)
15. [Crate Structure](#15-crate-structure)
16. [Test Requirements](#16-test-requirements)
17. [References](#17-references)

---

## 1. Purpose

This spec defines the LLM adapter framework — the middleware that lets agents interact with language models through a unified, provider-agnostic interface. It answers "how do agents call LLMs" — not "what agents do with LLM output" (that's the agent framework's job) or "which model to choose" (that's the coordinator's job).

WACP is deliberately LLM-agnostic at the protocol level. The protocol defines workspaces, envelopes, signals, and checkpoints — no concept of a language model, a prompt, or a token. But agents need LLMs to think. This framework provides structured, resilient, auditable access to them.

**Scope.** The LLM adapter framework as a Rust crate (`wacp-llm`) with Python and TypeScript packages. Adapter trait (the contract providers implement). Message types (the common conversation format). Three provider implementations (Anthropic, OpenAI, generic OpenAI-compatible). Streaming (SSE parsing, async token streams). Error classification (transient vs. permanent). Retry with backoff. Rate limiting (token bucket). Cost tracking (per-request, per-workspace). Tool-use mapping (descriptor → function-calling format).

**Not in scope.** Prompt engineering — how to construct system prompts, manage conversation history, or decide when to use tools. Agent decision loops — ReAct, plan-and-execute, tree-of-thought. Model selection — which provider or model to use for a given task. These are application-layer concerns. The adapter provides the pipe; the application decides what flows through it.

**Design constraint.** Raw HTTP only. No provider SDKs. Every provider implementation uses `reqwest` (Rust), `httpx`/`aiohttp` (Python), or `fetch` (TypeScript) to call the provider's REST API directly. This eliminates SDK version conflicts, reduces dependency weight, and gives full control over retry, streaming, and error handling. The adapter owns the HTTP call — nothing between it and the wire.

---

## 2. Design Principles

Five principles govern the framework. They resolve the tension between provider abstraction and provider-specific fidelity.

**Principle 1: One trait, many providers.** The `LlmAdapter` trait is the single abstraction. Every provider implements it. The caller (agent SDK, local-sdk, coordinator) holds a `Box<dyn LlmAdapter>` and never knows which provider is behind it. The trait is narrow — `complete`, `complete_stream`, `models`, `health` — because a wide trait would leak provider specifics. Provider-specific features (Anthropic's extended thinking, OpenAI's logprobs) are passed through the `options` map, not the trait signature.

**Principle 2: Errors are classified, not hidden.** Every provider error is mapped to two axes: origin (structural, provider, transport, compute) and persistence (transient, permanent, unknown). The retry layer uses persistence to decide whether to retry. The caller uses origin to decide whether to log, escalate, or abort. Raw provider error codes and messages are preserved — classification adds information, it does not discard it.

**Principle 3: Streaming is first-class.** LLM streaming is not an optimization — it is the primary interface. Agents need time-to-first-token for responsiveness. The framework provides async streams that yield tokens incrementally. Non-streaming `complete` is built on top of streaming (collect all tokens, return the assembled result). The SSE parser handles all three provider formats: Anthropic's `content_block_delta`, OpenAI's `choices[].delta`, and NDJSON for local providers.

**Principle 4: Cost is always tracked.** Every completion result includes token usage (input + output) and estimated cost (from model pricing). Cost tracking is not optional — the runtime's budget enforcer (§12, resource enforcement) depends on it. If a provider does not report usage, the adapter estimates it from the message length. Underestimation is safer than no estimation.

**Principle 5: Credentials never leak.** API keys, bearer tokens, and auth headers are injected via configuration and never appear in: error messages, log output, trail entries, health reports, or completion results. The adapter strips credentials from HTTP error responses before wrapping them in `LlmError`. Every output path is treated as potentially externally visible.

---

## 3. Message Types

The adapter uses a common message format that maps to both Anthropic and OpenAI APIs without loss.

```rust
/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Content,
}

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Message content — text or structured.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    /// Plain text content.
    Text(String),
    /// Structured content blocks (for tool results, images, etc.).
    Blocks(Vec<ContentBlock>),
}

/// A content block within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String, is_error: bool },
}
```

**Why this structure.** Anthropic uses `content: [{"type": "text", "text": "..."}]` (array of blocks). OpenAI uses `content: "..."` (plain string) with a separate `tool_calls` array. The `Content` enum (untagged) handles both: `Text(String)` for OpenAI-style, `Blocks(Vec<ContentBlock>)` for Anthropic-style. Provider implementations translate between the common format and their wire format.

**System messages.** Both providers accept system messages, but with different mechanics. Anthropic uses a separate `system` field on the request. OpenAI uses a `system` role message in the array. The adapter normalizes: the caller always sends `Role::System` messages in the array. The provider implementation extracts and remaps as needed.

---

## 4. Adapter Trait

```rust
/// The provider-agnostic LLM adapter. One trait, any provider.
#[async_trait]
pub trait LlmAdapter: Send + Sync + 'static {
    /// Complete a conversation (non-streaming). Returns the full result.
    async fn complete(
        &self,
        messages: &[Message],
        options: &CompletionOptions,
    ) -> Result<CompletionResult, LlmError>;

    /// Stream a completion token-by-token. Returns an async stream of chunks.
    async fn complete_stream(
        &self,
        messages: &[Message],
        options: &CompletionOptions,
    ) -> Result<StreamHandle, LlmError>;

    /// List available models.
    async fn models(&self) -> Result<Vec<ModelInfo>, LlmError>;

    /// Check provider health (connectivity, auth, rate limits).
    async fn health(&self) -> ProviderHealth;
}
```

**CompletionOptions:**

```rust
#[derive(Debug, Clone, Default)]
pub struct CompletionOptions {
    /// Model to use. If None, uses the provider's default.
    pub model: Option<String>,
    /// Maximum tokens to generate. If None, uses model default.
    pub max_tokens: Option<u32>,
    /// Temperature (0.0–2.0). If None, uses model default.
    pub temperature: Option<f64>,
    /// Stop sequences.
    pub stop: Vec<String>,
    /// Available tools for function-calling.
    pub tools: Vec<ToolDefinition>,
    /// Per-request timeout in milliseconds. If None, uses provider default.
    pub timeout_ms: Option<u64>,
    /// Provider-specific options (extended thinking, logprobs, etc.).
    pub extra: serde_json::Value,
}
```

**Why `async_trait`.** The trait methods return futures. `async_trait` is the standard Rust approach for async trait methods. The `Send + Sync + 'static` bounds ensure the adapter can be shared across tokio tasks.

**Why `StreamHandle` instead of `Stream`.** Returning a `Pin<Box<dyn Stream>>` directly from an async trait method is ergonomically painful. `StreamHandle` wraps the stream and provides convenience methods (`next()`, `collect()`, `into_stream()`). It also carries metadata (request ID, model) for debugging.

---

## 5. Completion Result

```rust
#[derive(Debug, Clone, Serialize)]
pub struct CompletionResult {
    /// The model's text response.
    pub content: String,
    /// Tool calls the model wants to make (function-calling).
    pub tool_calls: Vec<ToolCall>,
    /// Token usage: prompt + completion.
    pub usage: TokenUsage,
    /// Estimated cost from model pricing.
    pub cost: Option<Cost>,
    /// Actual model used (may differ from requested if aliased).
    pub model: String,
    /// Request latency in milliseconds.
    pub latency_ms: u64,
    /// Whether the response was truncated (hit max_tokens).
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCall {
    /// Provider-assigned ID for this tool call.
    pub id: String,
    /// Tool name (maps to ToolDescriptor.name).
    pub name: String,
    /// Arguments as JSON (validated against tool's input_schema by the caller).
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl TokenUsage {
    pub fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Cost {
    /// Cost in the smallest currency unit (e.g., microdollars).
    pub amount_micros: u64,
    /// Currency code.
    pub currency: &'static str,
}
```

**Cost in microdollars.** LLM costs are fractions of cents. Floating-point arithmetic introduces rounding errors that accumulate over thousands of requests. Integer microdollars (`$1.00 = 1_000_000 microdollars`) eliminate this. Cost calculation: `input_tokens * input_price_micros / 1_000_000 + output_tokens * output_price_micros / 1_000_000`.

---

## 6. Streaming

```rust
/// A handle to an in-progress streaming completion.
pub struct StreamHandle {
    inner: Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>,
    pub model: String,
    pub request_id: Option<String>,
}

/// Events emitted during streaming.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// A text token.
    ContentDelta { delta: String },
    /// An incremental tool call fragment.
    ToolCallDelta {
        index: u32,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: Option<String>,
    },
    /// Token usage report (emitted before Done).
    Usage { usage: TokenUsage },
    /// Stream complete. MUST be the last event.
    Done,
}
```

**SSE parsing.** All three providers use Server-Sent Events for streaming, but with different data formats:

| Provider | SSE event type | Data format | Done signal |
|----------|---------------|-------------|-------------|
| Anthropic | `content_block_delta`, `message_delta`, `message_stop` | JSON with `type` field | `event: message_stop` |
| OpenAI | (none — all `data:` lines) | JSON with `choices[].delta` | `data: [DONE]` |
| Local/NDJSON | (none — newline-delimited JSON) | JSON with `done` field | `{"done": true}` |

The adapter includes a generic SSE line parser that splits the byte stream into events, and per-provider event parsers that extract `StreamEvent` from the provider's JSON format.

**Invariant: `Done` is last.** After emitting `StreamEvent::Done`, the stream yields no more items. This is enforced by the stream wrapper, not trusted from the provider. If the provider sends data after its done signal, the wrapper drops it.

**Usage before Done.** Providers typically report usage in the final message. The adapter emits `StreamEvent::Usage` before `StreamEvent::Done`. If the provider does not report streaming usage, the adapter estimates from accumulated content length.

**Error mid-stream.** If the provider connection drops or returns an error mid-stream, the adapter yields all buffered events, then yields `Err(LlmError)`. The caller receives partial content and can decide what to do (discard, retry, use partial result).

---

## 7. Anthropic Provider

Implements `LlmAdapter` for the Claude Messages API.

**Endpoint:** `POST https://api.anthropic.com/v1/messages`

**Authentication:** `x-api-key` header with API key from config.

**Request mapping:**

```
CompletionOptions + [Message] → Anthropic request:
  model: options.model or config.default_model
  max_tokens: options.max_tokens or 4096
  system: extracted from messages where role == System (joined as string)
  messages: remaining messages mapped to Anthropic format
  tools: options.tools mapped to Anthropic tool schema
  stream: true for complete_stream, false for complete
  temperature: options.temperature
  stop_sequences: options.stop
```

**Response mapping:**

```
Anthropic response → CompletionResult:
  content: content[0].text (first text block)
  tool_calls: content blocks where type == "tool_use"
  usage: usage.input_tokens, usage.output_tokens
  model: response.model
  truncated: stop_reason == "max_tokens"
```

**Streaming mapping:**

| Anthropic event | StreamEvent |
|-----------------|-------------|
| `content_block_delta` with `text_delta` | `ContentDelta { delta: text }` |
| `content_block_delta` with `input_json_delta` | `ToolCallDelta { ... }` |
| `message_delta` with `usage` | `Usage { usage }` |
| `message_stop` | `Done` |

**Cost table (hardcoded, updated per release):**

| Model family | Input ($/M tokens) | Output ($/M tokens) |
|-------------|-------------------|---------------------|
| claude-sonnet-4 | 3.00 | 15.00 |
| claude-haiku-4 | 0.80 | 4.00 |
| claude-opus-4 | 15.00 | 75.00 |

The cost table is a `HashMap<&str, (u64, u64)>` where values are microdollars per million tokens. Model names are matched by prefix (`claude-sonnet-4` matches `claude-sonnet-4-20250514`).

---

## 8. OpenAI Provider

Implements `LlmAdapter` for the Chat Completions API.

**Endpoint:** `POST https://api.openai.com/v1/chat/completions`

**Authentication:** `Authorization: Bearer <api_key>` header.

**Request mapping:**

```
CompletionOptions + [Message] → OpenAI request:
  model: options.model or config.default_model
  max_completion_tokens: options.max_tokens
  messages: all messages mapped to OpenAI format (system stays in array)
  tools: options.tools mapped to OpenAI function schema
  stream: true for complete_stream, false for complete
  temperature: options.temperature
  stop: options.stop
```

**Response mapping:**

```
OpenAI response → CompletionResult:
  content: choices[0].message.content
  tool_calls: choices[0].message.tool_calls
  usage: usage.prompt_tokens, usage.completion_tokens
  model: response.model
  truncated: choices[0].finish_reason == "length"
```

**Streaming mapping:**

| OpenAI event | StreamEvent |
|-------------|-------------|
| `choices[0].delta.content` | `ContentDelta { delta }` |
| `choices[0].delta.tool_calls[i]` | `ToolCallDelta { index, id, name, arguments_delta }` |
| `usage` in final chunk | `Usage { usage }` |
| `data: [DONE]` | `Done` |

**Model discovery:** `GET /v1/models` → filter to chat-capable models.

---

## 9. Generic Provider

Implements `LlmAdapter` for any OpenAI-compatible endpoint.

**Use cases:** Ollama, llama.cpp, vLLM, Together AI, Groq, any provider that implements the OpenAI Chat Completions API format.

**Configuration difference:** `base_url` is configurable (not hardcoded to `api.openai.com`). Authentication is optional (local providers like Ollama need none).

**Implementation:** Identical to the OpenAI provider except:
- `base_url` from config instead of `https://api.openai.com`.
- Auth header is optional (skip if no API key configured).
- Model discovery via `GET {base_url}/v1/models` (may fail — not all providers implement it).
- Cost tracking is optional (most local providers don't have pricing).
- Streaming format may be NDJSON instead of SSE (detected from content-type header).

---

## 10. Error Classification

Every error is classified on two axes.

```rust
#[derive(Debug, Clone, thiserror::Error)]
#[error("{origin}/{persistence}: {message}")]
pub struct LlmError {
    pub origin: ErrorOrigin,
    pub persistence: ErrorPersistence,
    /// Provider's error code (e.g., "rate_limit_error", "invalid_api_key").
    pub code: Option<String>,
    /// Human-readable message (credentials stripped).
    pub message: String,
    /// HTTP status code, if applicable.
    pub status: Option<u16>,
    /// Whether retry might succeed (derived from persistence).
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorOrigin {
    /// Malformed request (missing field, invalid JSON).
    Structural,
    /// Provider-side error (model not found, content filter, server error).
    Provider,
    /// Network error (timeout, DNS, TLS).
    Transport,
    /// Inference error (context exceeded, output truncated unexpectedly).
    Compute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorPersistence {
    /// Likely to succeed on retry (429, 503, timeout).
    Transient,
    /// Will not succeed on retry (401, 404, content policy).
    Permanent,
    /// Cannot determine.
    Unknown,
}
```

**Mapping table:**

| HTTP status | Origin | Persistence | Notes |
|:-:|:-:|:-:|---|
| 400 | Structural | Permanent | Invalid request format |
| 401 | Provider | Permanent | Bad API key |
| 403 | Provider | Permanent | Forbidden |
| 404 | Provider | Permanent | Model not found |
| 408 | Transport | Transient | Request timeout |
| 429 | Provider | Transient | Rate limited |
| 500 | Provider | Transient | Server error |
| 502 | Transport | Transient | Bad gateway |
| 503 | Provider | Transient | Overloaded |
| Connection refused | Transport | Transient | |
| DNS failure | Transport | Transient | |
| TLS error | Transport | Permanent | Certificate issue |
| Context exceeded | Compute | Permanent | Prompt too long |
| Content filtered | Provider | Permanent | Policy violation |

**`retryable` derivation:** `persistence == Transient`.

---

## 11. Retry Logic

The adapter retries transient failures with exponential backoff. Retry is transparent to the caller — a successful retry returns a normal result.

**Configuration:**

```rust
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum retry attempts (not counting the first try). Default: 2.
    pub max_retries: u32,
    /// Base delay before first retry. Default: 1000ms.
    pub base_delay_ms: u64,
    /// Backoff strategy. Default: Exponential.
    pub backoff: BackoffStrategy,
    /// Whether to honor provider's Retry-After header. Default: true.
    pub honor_retry_after: bool,
    /// Maximum total retry duration. Default: 30_000ms.
    pub max_retry_duration_ms: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum BackoffStrategy {
    /// Same delay every time.
    Fixed,
    /// Delay doubles each retry: base, base*2, base*4, ...
    Exponential,
}
```

**Retry sequence:**

1. Execute the request.
2. On success → return result.
3. On error → classify. If `persistence == Permanent` → return error immediately (no retry).
4. If `persistence == Transient` and retries remaining:
   a. Compute delay: `base_delay_ms * 2^attempt` for exponential, `base_delay_ms` for fixed.
   b. Add jitter: random ±25% of computed delay.
   c. If provider sent `Retry-After` header and `honor_retry_after` is true: use the larger of computed delay and `Retry-After`.
   d. Sleep for the delay.
   e. Execute the request again. Go to step 2.
5. If retries exhausted → return last error.

**Cap: 3 total attempts** (1 original + 2 retries by default). Configurable up to `max_retries: 5`. Beyond that is waste — if the provider hasn't recovered after 5 attempts, it won't recover soon.

---

## 12. Rate Limiting

A token bucket rate limiter prevents exceeding provider rate limits proactively.

```rust
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window. 0 = unlimited.
    pub max_requests_per_window: u32,
    /// Maximum tokens per window. 0 = unlimited.
    pub max_tokens_per_window: u32,
    /// Window duration in milliseconds.
    pub window_ms: u64,
}
```

**Mechanism.** The rate limiter tracks requests and tokens consumed within a sliding window. Before each request:

1. Check if adding this request would exceed `max_requests_per_window`. If yes, compute wait time until the window slides enough, and sleep.
2. Check if the estimated input tokens would exceed `max_tokens_per_window`. If yes, sleep similarly.
3. After the request completes, record actual token usage.

**Provider header feedback.** Anthropic and OpenAI return rate limit headers (`anthropic-ratelimit-*`, `x-ratelimit-*`). When present, the adapter updates its internal state from these headers — the provider's actual limits are more accurate than the configured limits.

---

## 13. Cost Model

```rust
#[derive(Debug, Clone)]
pub struct ModelPricing {
    /// Microdollars per million input tokens.
    pub input_price_micros_per_m: u64,
    /// Microdollars per million output tokens.
    pub output_price_micros_per_m: u64,
}
```

**Cost calculation:**

```rust
fn calculate_cost(usage: &TokenUsage, pricing: &ModelPricing) -> Cost {
    let input_cost = (usage.input_tokens as u64) * pricing.input_price_micros_per_m / 1_000_000;
    let output_cost = (usage.output_tokens as u64) * pricing.output_price_micros_per_m / 1_000_000;
    Cost {
        amount_micros: input_cost + output_cost,
        currency: "USD",
    }
}
```

**Pricing tables.** Each provider implementation includes a hardcoded pricing table (`HashMap<&str, ModelPricing>`) for known models. The table is matched by model prefix — `"claude-sonnet-4"` matches `"claude-sonnet-4-20250514"`. Unknown models → `cost: None` in the result (no estimation is better than wrong estimation).

**Aggregation.** The adapter crate provides a `CostTracker` that aggregates costs:

```rust
pub struct CostTracker {
    /// Per-workspace accumulated cost.
    workspace_costs: HashMap<String, u64>,
    /// Total accumulated cost.
    total_micros: u64,
}

impl CostTracker {
    pub fn record(&mut self, workspace_id: Option<&str>, cost: &Cost);
    pub fn workspace_total(&self, workspace_id: &str) -> u64;
    pub fn total(&self) -> u64;
}
```

---

## 14. Tool-Use Mapping

The adapter converts between `wacp-tools` `ToolDescriptor` and provider-specific function-calling formats.

```rust
/// Tool definition for LLM function-calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}
```

**From `ToolDescriptor`:**

```rust
impl From<&wacp_tools::ToolDescriptor> for Vec<ToolDefinition> {
    // For single-capability tools: name = tool_name
    // For multi-capability tools: name = tool_name.capability_name
}
```

**To Anthropic format:**

```json
{"name": "read_file", "description": "Read a file", "input_schema": {"type": "object", ...}}
```

Direct mapping — `ToolDefinition` maps 1:1 to Anthropic's tool schema.

**To OpenAI format:**

```json
{"type": "function", "function": {"name": "read_file", "description": "Read a file", "parameters": {"type": "object", ...}}}
```

The adapter wraps `ToolDefinition` in OpenAI's `{"type": "function", "function": ...}` envelope.

---

## 15. Crate Structure

```
crates/wacp-llm/
├── src/
│   ├── lib.rs              # Public exports
│   ├── adapter.rs          # LlmAdapter trait, CompletionOptions
│   ├── types.rs            # Message, Role, Content, ContentBlock, ToolDefinition
│   ├── result.rs           # CompletionResult, ToolCall, TokenUsage, Cost, ModelInfo
│   ├── stream.rs           # StreamHandle, StreamEvent, SSE parser
│   ├── error.rs            # LlmError, ErrorOrigin, ErrorPersistence
│   ├── retry.rs            # RetryConfig, BackoffStrategy, retry wrapper
│   ├── rate_limit.rs       # RateLimitConfig, token bucket
│   ├── cost.rs             # ModelPricing, CostTracker, calculate_cost
│   ├── providers/
│   │   ├── mod.rs          # Provider config, provider factory
│   │   ├── anthropic.rs    # Anthropic Messages API implementation
│   │   ├── openai.rs       # OpenAI Chat Completions implementation
│   │   └── generic.rs      # Generic OpenAI-compatible implementation
│   └── tool_mapping.rs     # ToolDescriptor → ToolDefinition conversion
├── Cargo.toml
└── tests/
    ├── types_tests.rs      # Message serde roundtrips
    ├── error_tests.rs      # Classification, retryable derivation
    ├── retry_tests.rs      # Backoff calculation, jitter, retry-after
    ├── rate_limit_tests.rs # Token bucket, window sliding
    ├── cost_tests.rs       # Cost calculation, tracker aggregation
    ├── stream_tests.rs     # SSE parsing, event extraction
    └── provider_tests.rs   # Request/response mapping (mock HTTP)
```

**Dependencies:**

| Crate | Purpose |
|-------|---------|
| `reqwest` | HTTP client (already in workspace) |
| `serde`, `serde_json` | JSON serialization |
| `tokio` | Async runtime, sleep, timeout |
| `tokio-util` | CancellationToken |
| `futures` | Stream trait, FutureExt |
| `thiserror` | Error derive |
| `tracing` | Structured logging |
| `uuid` | Request IDs |
| `rand` | Jitter for retry backoff |

**Optional dependency:** `wacp-tools` — for `ToolDescriptor` → `ToolDefinition` conversion. Optional because the adapter can be used without the tool framework (tools defined manually as `ToolDefinition`).

---

## 16. Test Requirements

| Module | Tests | Coverage target |
|--------|-------|----------------|
| `types.rs` | Message serde roundtrip (text, blocks, tool_use, tool_result). Role serde. Content untagged deserialization (string → Text, array → Blocks). | Every message variant serializes and deserializes correctly. |
| `result.rs` | TokenUsage::total(). Cost microdollar precision. CompletionResult serde roundtrip. ToolCall serde. | Arithmetic correctness. Serialization fidelity. |
| `error.rs` | Every ErrorOrigin × ErrorPersistence combination. retryable derivation (transient → true, permanent → false, unknown → false). Display format includes origin/persistence. HTTP status mapping (400→Structural/Permanent, 429→Provider/Transient, etc.). | Every variant tested. Mapping table exhaustively verified. |
| `retry.rs` | Exponential backoff: delays double. Fixed backoff: delays constant. Jitter within ±25%. Retry-after honored (larger wins). Max retries respected. Permanent error not retried. Transient error retried and succeeds. All retries exhausted → last error returned. Max retry duration cap. | Every configuration path tested. |
| `rate_limit.rs` | Under limit → passes. At limit → waits. Window slides → capacity restored. Tokens tracked independently of requests. Zero limits → unlimited. | Each limit type tested independently. |
| `cost.rs` | Known model → correct cost. Unknown model → None. Microdollar precision (no float). CostTracker: per-workspace, total. Multi-model mixed pricing. | Arithmetic verified against manual calculation. |
| `stream.rs` | SSE line parser: event + data extraction. Anthropic format: content_delta, tool_call_delta, usage, done. OpenAI format: delta content, tool_calls, [DONE]. NDJSON format: response, done. Error mid-stream: partial events + error. Done is last (no events after). | Every provider format parsed correctly. |
| `providers/*.rs` | Request body construction (Anthropic format, OpenAI format). System message extraction (Anthropic) vs. inline (OpenAI). Tool schema mapping per provider. Response parsing. Streaming event mapping. Error response parsing. Model discovery. | Each provider: success request, success response, error response, streaming. |
| `tool_mapping.rs` | Single-capability → name = tool_name. Multi-capability → name = tool_name.capability_name. Anthropic format output. OpenAI format output. | Both naming conventions. Both provider formats. |

**Total target: ~70 tests for the Rust crate.**

---

## 17. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §4.4 (checkpoint) | §1, §13 | resource_usage field for token tracking |
| §9 (trail) | §1 | Inference events recorded |
| §11 (security) | §2 | Credentials never in trail or errors |

### Implementation Specs

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Tool framework spec | §3, §14 | §14 | ToolDescriptor → ToolDefinition conversion |
| Runtime spec | §12 (resource enforcement) | §4, §13 | BudgetEnforcer inference dimension |
| SDK agent spec | §6 (LLM mapping) | §1 | How agents use LLM output |
| LAYER-MAPPING.md | M6 | §1 | Architectural position |

### Future Specs

| Spec | Referenced in | Topic |
|------|--------------|-------|
| Agent SDK v2 (Phase 22) | §1 | Agents call adapter through AgentContext |
| Local SDK (Phase 24) | §1 | Local agents hold adapter instance directly |
| Security (Phase 23) | §2 | Credential management, content filtering |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../protocol/PROTOCOL.md) | Implementation Plan: [IMPLEMENTATION.md](../IMPLEMENTATION.md)*
