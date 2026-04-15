# WACP Layer Mapping — Middleware / Applications / Ecosystem

```yaml
created: 2026-04-01
status: design
authors:
  - Akil Abderrahim
  - Claude Opus 4.6
```

---

## Premise

mada-os defines three layers above the OS: **middleware** (6 frameworks), **applications** (6 apps), **ecosystem** (5 verticals). WACP's 12 Rust crates + proto definitions are the OS-equivalent. This document maps each mada-os component to its WACP counterpart — what exists, what's missing, and the design spec for each gap.

```
Ecosystem    (domain verticals — parameterize the platform)
─────────────────────────────────────────────────────── ecosystem boundary
Applications (CLI, SDK, API, IDE, dashboard, bridge)
─────────────────────────────────────────────────────── application boundary
Middleware   (7 frameworks — contracts for building on the runtime)
─────────────────────────────────────────────────────── middleware boundary
WACP Runtime (12 Rust crates + proto + protocol specs)
```

---

## 1 — Middleware Layer

Seven frameworks. Each defines a contract for building on the WACP runtime without being part of it. Remove middleware — runtime still boots, still accepts gRPC, still coordinates agents.

### Coverage Matrix

| # | Framework | mada-os | WACP exists | WACP gap |
|---|-----------|---------|-------------|----------|
| M1 | **agent-sdk** | WorkspaceContext, 20+ methods, directive/checkpoint/signal/tool/budget | `wacp-sdk` (Rust, thin), `sdk-python` (Python, thin) | Enrich both SDKs to full workspace context |
| M2 | **coordinator-sdk** | CoordinatorContext, goal/decompose/dispatch/integrate | None (coordinator is internal) | New: client-facing coordinator SDK |
| M3 | **local-sdk** | LocalSession, autonomy spectrum, interaction stream, nested sessions | None | New: session = root workspace composition |
| M4 | **transport** | HTTP + WebSocket + CLI bindings, auth providers, event delivery | `wacp-transport` (gRPC only, PSK auth) | REST gateway, auth provider plugins |
| M5 | **tool-framework** | ToolDescriptor, execution contract, packaging, discovery, sandboxing | None (tools are opaque to protocol) | New: full tool framework |
| M6 | **llm-adapters** | Provider adapters, streaming, cost, cognitive profiling, circuit breakers | None (WACP is LLM-agnostic) | New: LLM adapter framework |
| M7 | **security** | Cross-cutting threat model, trust boundaries, authorization | PSK + mTLS in transport | Elevate to cross-cutting security contract |

### M1 — Agent SDK (enrich existing)

**What exists:** `wacp-sdk` (Rust) — `Agent` struct, `AgentBuilder`, gRPC stream wrappers. `sdk-python` — `Agent` class, connect/signal/checkpoint/send_envelope/inbox.

**What's missing:** Both SDKs expose raw protocol operations. The middleware-level agent-sdk wraps these into ergonomic workspace context.

**Design spec:**

```
AgentContext (per-workspace, created on bind)
├── directive()           → structured task from coordinator
├── checkpoint(payload)   → CheckpointHandle (chain pointer, status)
├── complete(final?)      → emit complete signal + optional final checkpoint
├── blocked(reason)       → emit blocked signal
├── escalate(reason)      → create escalation via highway
├── query(content)        → send query envelope, await response
├── inbox()               → received envelopes since last read
├── send(target, content) → port-right-gated envelope delivery
├── tool(name, args)      → invoke registered tool (via M5)
├── tools()               → available tool descriptors
├── budget()              → remaining resources (5 dimensions)
├── trail(filter?)        → scoped trail entries
├── visible_workspaces()  → granted visibility set
├── read_workspace(id)    → read-only view of visible workspace
└── signal: CancellationToken
```

**Languages:** Rust crate (`wacp-agent-sdk`), Python package (`wacp.agent`). Both generated from same contract, diverge on idiom.

**Attachment surface:** `AgentService` RPCs (agent.proto) + local workspace state.

---

### M2 — Coordinator SDK (new)

**What exists:** Coordinator logic is internal to `wacp-coordinator` crate. No external SDK.

**What's missing:** A client-facing SDK for building custom coordinators or driving coordination externally.

**Design spec:**

```
CoordinatorContext (per-session, created on goal receipt)
├── goal()                      → the input goal
├── decompose(tasks)            → create task graph (DAG)
├── ready_tasks()               → tasks with satisfied dependencies
├── dispatch(task, opts?)       → create workspace + assign
├── abort(workspace_id)         → terminate workspace
├── suspend(workspace_id)       → pause workspace
├── resume(workspace_id)        → unpause workspace
├── send_directive(ws, content) → send directive envelope
├── feedback(ws, content)       → send feedback envelope
├── signals(filter?)            → received signals since last read
├── wait_for_signal(filter?)    → block until matching signal
├── integrate(ws, opts?)        → trigger integration pipeline
├── escalate(reason)            → create highway escalation
├── allocatable()               → remaining budget for dispatch
└── signal: CancellationToken
```

**Languages:** Rust crate (`wacp-coordinator-sdk`), Python package (`wacp.coordinator`).

**Attachment surface:** `HighwayService` RPCs (highway.proto) + coordinator internal state exposed via new RPCs.

**Key decision:** Coordinator SDK is a *client* — it drives the runtime's coordinator via gRPC, it does not replace the internal coordinator. The internal coordinator remains the authority; SDK provides ergonomic access.

---

### M3 — Local SDK (new)

**What exists:** Nothing. WACP has no concept of co-located human-agent sessions.

**What's missing:** The composition layer for CLI, IDE, and desktop agents where human and agent share a process.

**Design spec:**

```
LocalSession
├── session lifecycle: OPEN → ACTIVE → SUSPENDED → CLOSED
├── maps to root workspace states
│
├── InteractionStream
│   ├── classify(input) → goal | amendment | query | approval | injection
│   └── bidirectional channel between human and root agent
│
├── AutonomyManager
│   ├── trust_surface: Set<OperationType>
│   ├── grant(op_type)
│   ├── revoke(op_type)
│   └── evolves dynamically within session (unlike highway static presets)
│
├── LocalResources
│   ├── filesystem (scoped to working directory)
│   ├── shell (subprocess execution)
│   └── git (repository state)
│
├── Composition
│   ├── embeds CoordinatorContext (dispatch children)
│   ├── embeds AgentContext (execute work directly)
│   └── self-orchestration: root agent is both coordinator and worker
│
├── SessionContext
│   ├── cross-task continuity (accumulated decisions, trust, history)
│   └── session checkpoints (capture/restore session-level state)
│
└── BootProfile
    ├── single-node RAL (no cluster)
    ├── minimal topology (root + children only)
    └── fast initialization target: <500ms to first interaction
```

**Language:** TypeScript (primary — CLI/IDE), Python (secondary). Not Rust — this runs in the user's process, not the server.

**Foundational rule:** Session = root workspace. Local agent = root coordinator.

---

### M4 — Transport Extensions (enrich existing)

**What exists:** `wacp-transport` crate — `Transport` trait, `InProcessTransport`, `GrpcTransport` (tonic), `Authenticator` trait, `PskProvider`, `AuthRateLimiter`.

**What's missing:** Non-gRPC bindings and pluggable auth.

**Design spec:**

```
Transport Bindings
├── gRPC (exists — tonic, agent:9090, highway:9091)
├── gRPC-Web (exists — highway-ui uses connect-web)
├── REST gateway
│   ├── maps ExternalAPI operations to HTTP verbs + paths
│   ├── SSE for event streaming
│   └── JSON request/response bodies
└── WebSocket
    ├── bidirectional event channel
    └── JSON-RPC over WebSocket frames

Auth Providers (pluggable behind Authenticator trait)
├── PSK (exists)
├── API key (lookup + rate limit)
├── OAuth 2.0 / OIDC (token validation)
├── mTLS client certificates (exists for TLS, needs identity extraction)
└── Session tokens (stateful, with expiry + renewal)
```

**Invariant:** Transport bindings add no logic. Translate wire format <-> runtime RPC. No state, no caching, no filtering.

---

### M5 — Tool Framework (new)

**What exists:** Nothing. WACP protocol treats tools as opaque — agents invoke them, protocol doesn't define how.

**What's missing:** Structured tool descriptors, execution contracts, packaging, discovery.

**Design spec:**

```
ToolDescriptor
├── name: string
├── version: semver
├── description: string
├── capabilities: Vec<Capability>
│   ├── name, description
│   ├── input_schema: JsonSchema (for validation + LLM tool-use)
│   ├── output_schema: JsonSchema
│   ├── timeout_ms: Option<u64>
│   ├── idempotent: bool
│   └── side_effects: bool
└── tags: Vec<string>

ToolPackage
├── descriptor: ToolDescriptor
├── handlers: Map<capability_name, Handler>
├── initialize(config) → Result
└── shutdown() → Result

ExecutionContract
├── input validation against schema before handler
├── timeout enforcement (capability default < invocation override < framework max)
├── structured errors: { code, message, retryable }
├── cancellation via AbortSignal / CancellationToken
├── result size limit (default 1 MB)
└── per-tool concurrency limit (default 10, queue 50)

Discovery
├── scan registered packages
├── validate descriptor + handler alignment
├── resolve configuration
└── register with runtime

Resilience
├── circuit breaker (per-tool, optional): closed → open → half-open
├── timeout hierarchy (3 levels)
└── concurrency limiting with backpressure

Sandboxing (3 levels)
├── none (in-process, trusted tools)
├── process (child process, code execution tools)
└── container (Docker, untrusted tools)
```

**Language:** Rust crate (`wacp-tools`). Python + TypeScript bindings for tool authoring.

---

### M6 — LLM Adapters (new)

**What exists:** Nothing. WACP is deliberately LLM-agnostic at protocol level.

**What's missing:** For agents to actually operate, they need LLM inference. This framework provides it.

**Design spec:**

```
LlmAdapter (trait / interface)
├── complete(messages, opts?) → CompletionResult
├── complete_stream(messages, opts?) → Stream<Token>
├── models() → Vec<ModelInfo>
└── health() → ProviderHealth

Providers (each implements LlmAdapter)
├── Anthropic (Claude — Messages API, raw HTTP)
├── OpenAI (GPT — Chat Completions API, raw HTTP)
├── Generic OpenAI-compatible (fallback for any provider)
└── Local (Ollama, llama.cpp — OpenAI-compatible endpoint)

CompletionResult
├── content: string
├── tool_calls: Vec<ToolCall>     # for tool-use / function-calling
├── usage: TokenUsage             # prompt + completion tokens
├── cost: Cost                    # estimated $ from model pricing
├── model: string                 # actual model used
└── latency_ms: u64

Cross-Cutting
├── retry: exponential backoff + jitter, classify transient vs permanent errors
├── rate limiting: token bucket per provider
├── circuit breaker: per provider, state machine
├── cost tracking: per-request, per-session, per-workspace aggregation
├── streaming: SSE parsing, partial token assembly
└── timeout: per-request, with AbortSignal propagation
```

**Design decision:** Raw HTTP (`fetch` / `reqwest`), no provider SDKs. Single adapter interface, provider-specific implementations.

**Language:** Rust crate (`wacp-llm`). Python package (`wacp.llm`). TypeScript package (`@wacp/llm`).

---

### M7 — Security (elevate existing)

**What exists:** PSK auth in `wacp-transport`, mTLS in `wacp-runtime`, rate limiter.

**What's missing:** Cross-cutting security contract spanning all 6 frameworks.

**Design spec:**

```
Security Contract
├── Trust boundaries
│   ├── runtime boundary: all inbound requests authenticated
│   ├── workspace boundary: agents see only granted visibility
│   ├── tool boundary: tools execute with scoped permissions
│   └── LLM boundary: no secrets/PII in LLM prompts (content filter)
│
├── Authorization model
│   ├── protocol-level: wacp-permissions (exists — role matrix, port rights)
│   ├── tool-level: per-tool capability permissions
│   └── resource-level: budget enforcement (exists — 5 dimensions)
│
├── Secret management
│   ├── LLM API keys: injected via config, never logged, never in trail
│   ├── auth tokens: session-scoped, expiry + rotation
│   └── tool credentials: scoped to tool, never exposed to agents
│
├── Content filtering
│   ├── PII redaction at LLM boundary
│   ├── secret scanning in checkpoint payloads
│   └── configurable per-workspace policy
│
└── Audit
    ├── trail (exists — hash-chained, tamper-evident)
    ├── auth events: login, failure, rate-limit, token refresh
    └── tool invocations: input hash, output hash, duration, errors
```

---

## 2 — Applications Layer

Six applications. Each composes middleware frameworks, owns its interaction surface, has no dependency on other apps.

### Coverage Matrix

| # | Application | mada-os | WACP exists | WACP gap |
|---|-------------|---------|-------------|----------|
| A1 | **CLI agent** | Terminal REPL, local-sdk, tool-framework, llm-adapters | None | New: primary user-facing product |
| A2 | **Embeddable SDK** | LocalSession API for host tools | `wacp-sdk` (protocol-level only) | Enrich: bundle local-sdk + tools |
| A3 | **API server** | REST + WebSocket, headless coordination | `wacp-runtime` (gRPC only) | Add REST/WS gateway |
| A4 | **IDE integration** | VS Code / JetBrains panels | None | New: extension using local-sdk |
| A5 | **Web dashboard** | Full management UI | `highway-ui` (highway-focused) | Expand to full dashboard |
| A6 | **Chat bridge** | Slack / Discord / Teams | None | New: platform message adapter |

### Composition Matrix

| Application | M1 agent | M2 coordinator | M3 local | M4 transport | M5 tools | M6 llm |
|-------------|:--------:|:--------------:|:--------:|:------------:|:--------:|:------:|
| **A1 CLI** | internal | internal | PRIMARY | -- | yes | yes |
| **A2 SDK** | internal | internal | PRIMARY | -- | yes | -- |
| **A3 API** | server | server | -- | PRIMARY | yes | yes |
| **A4 IDE** | internal | -- | PRIMARY | -- | yes | yes |
| **A5 Dashboard** | -- | -- | -- | PRIMARY | -- | -- |
| **A6 Bridge** | server | -- | -- | PRIMARY | yes | -- |

### A1 — CLI Agent (new, primary product)

```
CLI Agent
├── Composition root: LocalSession (M3)
├── Surface: terminal REPL (stdin/stdout/stderr)
├── LLM: configurable provider via M6
├── Tools: filesystem, shell, git, web search (via M5)
├── Autonomy: dynamic trust surface, starts supervised
├── Boot: <500ms to first prompt
├── Config: YAML/TOML — provider, model, working directory, trust presets
└── Output: streaming tokens, structured tool results, gate prompts
```

### A2 — Embeddable SDK (enrich existing)

```
Embeddable SDK
├── Composition root: LocalSession (M3)
├── Surface: TypeScript / Python API (programmatic, no UI)
├── LLM: bring-your-own (host provides adapter)
├── Tools: host registers tools via M5
├── Use case: other tools embed WACP coordination
└── Minimal dependency: local-sdk + agent-sdk + coordinator-sdk + tool-framework
```

### A3 — API Server (extend existing)

```
API Server (extends wacp-runtime)
├── Composition root: Transport binding (M4)
├── gRPC: exists (agent:9090, highway:9091)
├── REST: new — JSON API over HTTP, SSE for events
├── WebSocket: new — bidirectional event channel
├── Auth: pluggable providers (M4 auth)
├── Headless: pre-configured trust, no interactive approval
└── Multi-tenant: session isolation per client
```

### A4 — IDE Integration (new)

```
IDE Integration
├── Composition root: LocalSession (M3)
├── VS Code extension: webview panels, tree views, inline annotations
├── JetBrains plugin: tool windows, editor integration
├── Surface: IDE native UI elements
├── LLM + tools: same as CLI (M5, M6)
└── Specialty: file-scoped workspaces, diff preview, inline gate approval
```

### A5 — Web Dashboard (expand highway-ui)

```
Web Dashboard (extends highway-ui)
├── Existing: trail viewer, workspace tree, gate panel, escalation panel,
│             injection form, settings panel, checkpoint viewer
├── Add: session management (create, list, close)
├── Add: task graph visualization (DAG)
├── Add: resource/budget monitoring
├── Add: coordinator controls (decompose, dispatch, integrate)
├── Add: multi-session view
└── Add: audit log viewer
```

### A6 — Chat Bridge (new)

```
Chat Bridge
├── Composition root: Transport binding (M4)
├── Platforms: Slack, Discord, Teams (one adapter each)
├── Maps: platform messages → goal submissions
├── Maps: gate/escalation events → platform interactive messages
├── Maps: platform approvals → gate responses
└── Stateless per-message: session managed by runtime
```

---

## 3 — Ecosystem Layer

Domain verticals parameterize the platform through extension points. Each vertical provides: role taxonomy, task taxonomy, tool catalog, agent profiles, workflows, quality criteria.

### Extension Points (how verticals plug in)

| Extension point | Runtime attachment | Vertical provides |
|-----------------|-------------------|-------------------|
| Role registration | `wacp-taxonomy` | Derived role definitions |
| Tool registration | M5 tool-framework | Domain tool descriptors + handlers |
| Decomposition strategy | `wacp-coordinator` dispatch | Task type -> subtask DAG patterns |
| Quality evaluation | `wacp-coordinator` integration | Domain quality criteria |
| Gate policy | `wacp-coordinator` gate controller | Per-operation gate configuration |
| Agent profile | M1 agent-sdk | Role -> system prompt, tools, autonomy |

### Vertical Template

```
ecosystem/<vertical>/
├── <VERTICAL>.md        # Design spec
├── TOOLS.md             # Tool catalog
├── src/
│   ├── taxonomy.{rs,py,ts}   # Role + task type registrations
│   ├── tools/                 # Tool implementations
│   ├── profiles/              # Agent profiles (prompt, tools, autonomy)
│   ├── workflows/             # Decomposition patterns (task -> subtask DAG)
│   ├── quality/               # Quality criteria for integration
│   └── index.{rs,py,ts}      # Registration entry point
└── tests/
```

### Planned Verticals

| # | Vertical | Roles | Task types | Key constraint |
|---|----------|-------|------------|----------------|
| E1 | **SWE** | planner, implementer, tester, reviewer | implement, refactor, debug, test, review, document, investigate | Scope isolation, test coverage gates |
| E2 | **DevOps** | architect, deployer, monitor, responder, auditor | provision, deploy, monitor, respond, audit, migrate, configure, secure, optimize | Blast radius model, environment-scaled gating |
| E3 | **MLOps** | researcher, trainer, evaluator, deployer, monitor | experiment, train, evaluate, deploy, monitor, optimize, data-prep, reproduce, audit | Compute budget, reproducibility |
| E4 | **Finance** | analyst, risk-analyst, compliance-officer, reporter, portfolio-analyst | analyze, valuate, assess-risk, check-compliance, report, review-portfolio, rebalance, stress-test, investigate | Regulatory compliance, fiduciary model |
| E5 | **Healthcare** | clinician, researcher, analyst, compliance, coordinator | assess, diagnose-support, research, analyze, monitor, report, audit, educate | PHI/HIPAA, clinical validation |

---

## 4 — Build Order

Dependency-driven. Each phase unlocks the next.

| Phase | Deliverable | Depends on | Unlocks |
|-------|-------------|------------|---------|
| **L1** | M5 tool-framework, M6 llm-adapters | runtime (exists) | M1, M2, M3 |
| **L2** | M1 agent-sdk (enrich), M2 coordinator-sdk | L1 | M3, A1–A6 |
| **L3** | M7 security, M4 transport extensions | L2 | A3, A5, A6 |
| **L4** | M3 local-sdk | L2 | A1, A2, A4 |
| **L5** | A1 CLI agent | L4 | E1–E5 |
| **L6** | E1 SWE vertical | L5 | Ship |
| **L7** | A3 API server, A5 dashboard (expand) | L3 | -- |
| **L8** | A4 IDE, A6 chat bridge | L4, L3 | -- |
| **L9** | E2–E5 remaining verticals | L6 | -- |

---

## 5 — Design Specs Needed

Each item below requires a full implementation spec before coding begins.

| # | Spec | Scope | Priority |
|---|------|-------|----------|
| S1 | `impl/tool-framework.md` | M5 — descriptors, execution, packaging, discovery, sandboxing, resilience | L1 |
| S2 | `impl/llm-adapters.md` | M6 — adapter trait, providers, streaming, cost, retry, circuit breakers | L1 |
| S3 | `impl/agent-sdk-v2.md` | M1 — AgentContext contract, Rust + Python enrichment | L2 |
| S4 | `impl/coordinator-sdk.md` | M2 — CoordinatorContext, client-facing API, gRPC surface | L2 |
| S5 | `impl/security.md` | M7 — cross-cutting security contract, content filter, secret mgmt | L3 |
| S6 | `impl/transport-ext.md` | M4 — REST gateway, WebSocket, auth providers | L3 |
| S7 | `impl/local-sdk.md` | M3 — session lifecycle, autonomy, interaction stream, boot profile | L4 |
| S8 | `impl/cli-agent.md` | A1 — REPL, config, streaming output, tool integration | L5 |
| S9 | `ecosystem/swe/SWE.md` | E1 — roles, tasks, tools, profiles, workflows, quality | L6 |

---

*Layer mapping for WACP — Akil Abderrahim and Claude Opus 4.6*
