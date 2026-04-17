---
id: wcon-llm-stub
type: coding
status: final
created: 2026-04-17T00:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [llm, testing, integration, stub, fixture]
depends_on: [wcon-w7-integration-tests]
---

# Coding Spec — LLM Stub Provider

## Table of Contents

1. Purpose
2. Scope and non-scope
3. Configuration surface
4. Fixture format
5. Provider implementation
6. Test-side agent usage
7. Deliverables and file layout
8. Acceptance criteria
9. References

---

## 1. Purpose

The integration and E2E test suites need a deterministic LLM. `wacp-runtime` today ships only the Anthropic, OpenAI, and generic OpenAI-compatible providers in `wacp-llm`. None of them are usable in CI: Anthropic and OpenAI require real API keys and network calls; the generic provider requires a local OpenAI-compatible endpoint. As a consequence, six integration tests (`T7.2`, `T7.3`, `T7.5`, `T7.7`, `T7.8`, `T7.10`) are `#[ignore]`-ed with the reason "needs LLM stub", and the Playwright `golden-path` and `multi-user` scenarios in §13.7.7 inherit the same blocker.

This spec defines the stub provider — an `LlmAdapter` implementation that serves canned responses from a YAML fixture file, keyed either by message-prefix or by SHA-256 hash of the full input. It is entirely self-contained (no HTTP, no secrets, no tokio timer except for simulated inter-token delay) and safe to run in CI.

---

## 2. Scope and non-scope

**In scope (landed in this package).**
- `ProviderConfig::Stub` variant serializable via serde.
- `StubAdapter: LlmAdapter` implementation.
- YAML fixture format and loader.
- 27+ unit tests inside `wacp-llm::providers::stub` covering fixture load/parse/version, all three matcher kinds, default fallback, streaming ordering + timing, health, models, tool-call round-trips, base64 payload decoding, and factory precedence.
- An `llm_stub_e2e.rs` integration suite (§12.5 I6) that exercises the stub end-to-end against a live `wacp-runtime`: submit a goal, bind an agent via `wacp-sdk`, drive a stub-selected checkpoint + complete signal through real gRPC calls, and verify the streaming path.
- Tightened ignore reasons on `T7.2` / `T7.3` / `T7.5` / `T7.7` / `T7.8` / `T7.10` so the real blocker is recorded (runtime-side wiring gap, not the now-landed stub).

**Not in scope — follow-ups.**
- Un-ignoring `T7.2` / `T7.3`. Discovered during implementation that the runtime's `AgentService` handlers are shells: `Bind` does not return a real directive, `EmitSignal` does not advance the workspace FSM, and `CreateCheckpoint` does not fan into highway gates. All three are required for T7.2 / T7.3 to observe the behaviour they assert. The stub-provider side is ready; closing these tests needs a separate runtime-side package that wires `init.rs`'s `AgentRequest::*` handlers to the coordinator + workspace actor. Tracked as a successor to §13.7.6 in the audit.
- `T7.5` (partial-dispatch failure), `T7.7` (10-way concurrency), `T7.8` (slow-consumer pacing), `T7.10` (W4→W6 latency) — inherit the same runtime gap; un-ignore alongside the follow-up.
- Cost/pricing fidelity. The stub reports zero cost and a configurable token count per entry. Cost-path tests continue to use the real provider pricing tables on mocked `CompletionResult` values.
- Agent-side `StubAgent` helper in `console-integration`. The spec originally proposed one, but the existing `wacp-sdk::Agent` with the stub adapter composed inline is sufficient for I6 and — until the runtime wiring lands — provides no additional value over the direct pattern in `llm_stub_e2e.rs`. The helper is deferred to the runtime-side follow-up, where it becomes load-bearing for T7.2 / T7.3.

---

## 3. Configuration surface

Adds a fourth variant to `ProviderConfig`:

```rust
Stub {
    #[serde(default)]
    fixtures_path: Option<PathBuf>,
    #[serde(default)]
    fixtures_inline: Option<StubFixtures>,  // for unit tests
    #[serde(default)]
    default_model: Option<String>,
    #[serde(default)]
    token_delay_ms: u64,  // inter-token delay for streaming, default 0
}
```

Serialization is consistent with the other variants: `{"provider":"stub","fixtures_path":"…"}`. `fixtures_path` and `fixtures_inline` are mutually exclusive; if both are set, `fixtures_inline` wins (useful for unit tests that embed fixtures in code without a tempdir). If neither is set, the provider starts with an empty fixture set and every request falls through to the default response, if one was authored inline (see §4).

A new factory function `providers::build_adapter(cfg: &ProviderConfig) -> Result<Arc<dyn LlmAdapter>, LlmError>` is introduced in `providers/mod.rs`. It is the first real consumer of the existing `ProviderConfig` enum — the Anthropic / OpenAI / generic branches return `LlmError::structural("not yet implemented")` until their respective adapters land. The stub branch returns a fully working adapter. This factory is the integration point for future runtime code that wants to pick a provider from config.

---

## 4. Fixture format

Fixtures live in a single YAML file loaded at startup.

```yaml
# wacp/crates/wacp-llm/tests/fixtures/stub_responses.yaml
version: 1
default:
  content: "stub default response"
  output_tokens: 4
  tool_calls: []

entries:
  - match:
      kind: prefix
      value: "You are a coordinator"
    response:
      content: "decompose-ok"
      output_tokens: 2
      tool_calls: []

  - match:
      kind: hash
      value: "3e2a…e8"   # SHA-256 of the exact rendered user message
    response:
      content: ""
      output_tokens: 3
      tool_calls:
        - id: "call_1"
          name: "propose_checkpoint"
          arguments:
            type: "task_approval"
            payload: "eyJvayI6dHJ1ZX0="   # base64 of any payload bytes

  - match:
      kind: contains
      value: "emit complete"
    response:
      content: "done"
      output_tokens: 1
      tool_calls: []
```

Three match kinds:
- `prefix` — the serialized message stream (`role: text\n…`) starts with `value`.
- `hash` — SHA-256 of the serialized message stream equals `value` (lowercase hex).
- `contains` — the serialized message stream contains `value` as a substring.

Matcher order is stable: the first matching entry wins. If nothing matches, the `default` entry is used. If there is no `default` and nothing matches, `complete` / `complete_stream` return `LlmError::structural("no fixture match")`.

`output_tokens` is authored per-entry. `input_tokens` is computed as the byte length of the serialized message stream / 4 (a rough heuristic; the stub is not used for cost-accuracy assertions). `latency_ms` is recorded as the actual wall time of the `complete` / `complete_stream` call — so tests that care about latency bounds still see meaningful numbers driven by `token_delay_ms`.

Streaming: the response `content` is chunked into single-character `ContentDelta` events (Unicode-aware via `.chars()`), followed by `ToolCallDelta` events for any tool calls (one `ToolCallDelta` with `id` + `name` + no args, then one with `arguments_delta` containing the JSON-serialized args in one chunk), then a `Usage` event, then `Done`. Inter-event delay is `token_delay_ms`; default 0 for fast tests.

---

## 5. Provider implementation

`wacp/crates/wacp-llm/src/providers/stub.rs` exports:

```rust
pub struct StubAdapter { … }
impl LlmAdapter for StubAdapter { … }

pub struct StubFixtures { /* public: tests construct inline */ }
impl StubFixtures {
    pub fn load(path: &Path) -> Result<Self, LlmError>;
    pub fn from_yaml(yaml: &str) -> Result<Self, LlmError>;
    pub fn matches(&self, messages: &[Message]) -> &StubResponse;
}
```

Serialization of a message stream (for matchers and hashing) is a stable, lossless rendering:

```
<role>:
<text>
---
```

System messages come first, then user / assistant / tool in order. `Content::Blocks` is flattened text-first, tool-use blocks serialized as `tool_use(name, json(input))`, tool-result blocks as `tool_result(id, content)`. `ToolDefinition`s passed via `CompletionOptions.tools` are appended as a trailer so tool-aware prompts hash differently from tool-less ones.

Concurrency: `StubAdapter` is `Send + Sync + 'static` (required by the trait). Fixtures are held behind an `Arc<StubFixtures>`; the adapter can be cloned cheaply and shared across tasks.

`models()` returns a single `ModelInfo` entry for "stub-model-1" with `supports_tools = true`, `supports_streaming = true`. `health()` always reports healthy with `models_available = 1`. No network activity anywhere in the provider.

---

## 6. Test-side agent usage

A reusable `StubAgent` helper lives in `console-integration/src/stub_agent.rs`. It wraps `wacp-sdk::Agent` with a preconfigured `StubAdapter`:

```rust
pub struct StubAgent {
    agent: wacp_sdk::Agent,
    adapter: Arc<dyn LlmAdapter>,
}

impl StubAgent {
    pub async fn connect(
        runtime_url: String,
        workspace_id: WorkspaceId,
        auth_token: String,
        fixtures: StubFixtures,
    ) -> Result<Self, Error>;

    /// Read the bind-time directive, match a fixture, emit Started,
    /// then act on the fixture's tool_calls (checkpoint / envelope)
    /// and finally emit the terminal signal dictated by the fixture.
    pub async fn run_once(&self) -> Result<RunOutcome, Error>;

    /// Drive the workspace forever — read from inbox, consult fixtures,
    /// respond. Stops on AgentComplete / AgentFailed.
    pub async fn run_to_completion(self) -> Result<(), Error>;
}
```

The fixture's `tool_calls` field carries the action(s) the agent should take:
- `name: "emit_signal"` — one of `Started`, `Blocked`, `Complete`, `Failed`, `Escalation` via `arguments.type`.
- `name: "create_checkpoint"` — passes `type`, `payload` (base64-decoded), `status` (`"provisional"` or `"final"`) through to `CheckpointBuilder`.
- `name: "send_envelope"` — passes `to` (workspace id), `type`, `payload` through to `EnvelopeBuilder`.

`run_once` is called in a tokio task per workspace by the integration tests. The test launches a session via the console API, receives workspace IDs from the launch response, and spawns one `StubAgent` per workspace.

---

## 7. Deliverables and file layout

| # | File | Purpose |
|---|------|---------|
| 1 | `wacp-console/specs/coding/wcon-llm-stub.md` | This spec. |
| 2 | `wacp/crates/wacp-llm/src/providers/stub.rs` | Stub `LlmAdapter` impl + `StubFixtures` loader. |
| 3 | `wacp/crates/wacp-llm/src/providers/mod.rs` | Add `ProviderConfig::Stub`, `build_adapter`, `sha2` + `base64` + `serde_yaml` deps. |
| 4 | `wacp/crates/wacp-llm/Cargo.toml` | `sha2`, `serde_yaml`, `base64` added. |
| 5 | `wacp/crates/wacp-llm/tests/fixtures/stub_responses.yaml` | Baseline fixture used by both unit tests and integration tests. |
| 6 | `wacp-console/integration/src/stub_agent.rs` + `lib.rs` export | Test-side agent helper. |
| 7 | `wacp-console/integration/Cargo.toml` | Add `wacp-sdk`, `wacp-llm`, `wacp-types` dev-deps. |
| 8 | `wacp-console/integration/tests/llm_stub_e2e.rs` | §12.5 I6. Exercises the stub end-to-end. |
| 9 | `wacp-console/integration/tests/lifecycle.rs` | Un-ignore + author `T7.2`, `T7.3`. |
| 10 | `wacp-console/integration/tests/chaos.rs` | Tighten `T7.5` ignore reason (still deferred — needs dispatch-failure injection on top of the stub). |
| 11 | `wacp-console/integration/tests/cross_session.rs` | Tighten `T7.7`, `T7.8`, `T7.10` ignore reasons. |
| 12 | `wacp-console/specs/coding/wcon-w7-integration-tests.md` | Remove "awaiting LLM stub" from the §5.1 deviation note. |
| 13 | `AUDIT-2026-04-15.md` | Mark §13.7.6 landed in the §13.8 tracking table; drop the LLM-stub mention from §13.1 item 6 / §13.5. |
| 14 | `SEED.md` | Refresh the post-session strike-through list + resumption-point ordering. |

---

## 8. Acceptance criteria

1. `cargo test -p wacp-llm` green, including the new `providers::stub` module's 27 unit tests and the `providers::tests::build_adapter_*` factory tests. **Met.**
2. `cargo test -p console-integration --test llm_stub_e2e` green — both scenarios pass. **Met.**
3. `cargo test -p console-integration` green as a whole; T7.1 / T7.4 / T7.6 / T7.9 pass; T7.2 / T7.3 / T7.5 / T7.7 / T7.8 / T7.10 remain `#[ignore]`-ed with reasons that point at the specific runtime-side wiring gap rather than the now-landed stub. **Met.**
4. `wacp-console/specs/coding/wcon-w7-integration-tests.md` §5.1 deviation note updated — the "awaiting LLM stub" mention is replaced with a pointer to the runtime-side follow-up. **Met.**
5. `AUDIT-2026-04-15.md` §13.7.6 marked as partial-landed (stub provider + I6) with the runtime-side follow-up recorded in §13.5 / §13.8. **Met.**

---

## 9. References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-w7-integration-tests | W7 Integration Tests | implements the deferred work called out in §5.1 |
| wacp-impl-llm-adapters | WACP LLM Adapter Framework | extends with a fourth provider |
| wacp-impl-sdk-agent | WACP Agent SDK | consumed by the stub-agent helper |

*WACP Platform — authored by Akil Abderrahim and Claude Opus 4.7 (1M context).*
