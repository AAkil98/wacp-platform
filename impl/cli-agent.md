# WACP Implementation: CLI Agent

```yaml
id: wacp-impl-cli-agent
type: implementation-spec
status: complete
created: 2026-04-02
revised: 2026-04-02
lineage: LAYER-MAPPING.md (A1)
depends_on:
  - wacp-impl-local-sdk
  - wacp-impl-tool-framework
  - wacp-impl-llm-adapters
  - wacp-impl-security
  - wacp-impl-coordinator-sdk
  - wacp-impl-runtime
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, application, cli, agent, repl, grpc, protocol]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Architecture](#2-architecture)
3. [Configuration](#3-configuration)
4. [Boot Sequence](#4-boot-sequence)
5. [REPL Loop](#5-repl-loop)
6. [Workflow Execution](#6-workflow-execution)
7. [Stage Agent Loop](#7-stage-agent-loop)
8. [Tool Integration](#8-tool-integration)
9. [Streaming Output](#9-streaming-output)
10. [Gate Prompts](#10-gate-prompts)
11. [Commands](#11-commands)
12. [Package Structure](#12-package-structure)
13. [Test Requirements](#13-test-requirements)
14. [References](#14-references)

---

## 1. Purpose

This spec defines the CLI agent — a terminal-based AI assistant that spawns the WACP runtime, connects via gRPC, and drives multi-stage workflows through the protocol. Every operation — workspace creation, signal emission, checkpoint recording, task decomposition — goes through the Rust runtime. The CLI is a protocol participant, not a standalone chatbot.

**Scope.** TypeScript package `@wacp/cli`. Runtime process management. gRPC clients for CoordinatorService and AgentService. Workflow-driven execution with SWE vertical profiles. REPL with input classification. Streaming LLM output. Gate prompts. Slash commands.

**Not in scope.** The Rust runtime internals (runtime spec). LLM provider APIs (llm-adapters spec). Tool framework mechanics (tool-framework spec). The CLI composes these — it does not implement them.

**Architectural constraint.** The CLI spawns `wacp-runtime serve` as a child process and communicates exclusively via gRPC. LLM calls use raw HTTP (the LLM is external to the protocol), but every result is checkpointed through the runtime. No local-only bypass.

---

## 2. Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Human (terminal)                                       │
│  types goals, approves gates, reads output              │
└─────────────┬───────────────────────────────────────────┘
              │ stdin/stdout
┌─────────────▼───────────────────────────────────────────┐
│  REPL (readline, input classification, Ctrl-C)          │
├─────────────────────────────────────────────────────────┤
│  Workflow Engine                                        │
│  ┌─────────────────┐  ┌────────────────────────────┐    │
│  │ Task Type       │  │ WorkflowExecutor           │    │
│  │ Detection       │→ │ (stages, profiles, gates)  │    │
│  └─────────────────┘  └────────────┬───────────────┘    │
│                                     │ per stage          │
│  ┌──────────────────────────────────▼───────────────┐   │
│  │  Stage Agent Loop                                │   │
│  │  ┌──────────┐  ┌──────────┐  ┌────────────────┐ │   │
│  │  │ LLM call │→ │ tool exec│→ │ checkpoint via │ │   │
│  │  │ (fetch)  │  │ (local)  │  │ AgentService   │ │   │
│  │  └──────────┘  └──────────┘  └────────────────┘ │   │
│  └──────────────────────────────────────────────────┘   │
├──────────────────────────┬──────────────────────────────┤
│  gRPC Clients            │  LocalSession               │
│  ┌───────────────────┐   │  (autonomy, resources,      │
│  │ CoordinatorClient │   │   context, interaction)      │
│  │ AgentClient       │   │                              │
│  └────────┬──────────┘   │                              │
├───────────┼──────────────┴──────────────────────────────┤
│           │ gRPC (ports 9090, 9092)                      │
│           ▼                                              │
│  ┌──────────────────────────────────────────────────┐   │
│  │  WACP Runtime (Rust child process)               │   │
│  │  Coordinator · Trail · Workspaces · Task Graph   │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

**Key distinction from previous architecture:** The runtime is a real process. Workspaces are real. Signals flow. Trail records. Checkpoints persist. The CLI does not simulate the protocol — it uses it.

---

## 3. Configuration

```yaml
# ~/.wacp/config.yaml
provider: anthropic
model: claude-sonnet-4-20250514
api_key: ${ANTHROPIC_API_KEY}
working_dir: .
autonomy: assisted
max_tokens: 16384
temperature: 0.0
system_prompt: |
  You are a helpful coding assistant.
```

**Loading order:** CLI flags → env vars → config file → defaults.

**Env var substitution:** `${VAR_NAME}` resolved from `process.env`. Missing → error at boot.

---

## 4. Boot Sequence

```
1. Parse CLI flags (--config, --provider, --model, --working-dir, --autonomy)
2. Load config file (~/.wacp/config.yaml or --config path)
3. Merge: flags > env > file > defaults
4. Validate config (provider set, API key present)
5. Load SWE vertical (4 workflows, 4 profiles)
6. Spawn WACP runtime (wacp-runtime serve) as child process
7. Wait for runtime ready (TCP probe on agent port, up to 15s)
8. Connect gRPC clients (CoordinatorClient → port 9092, AgentClient → port 9090)
9. Create LocalSession (autonomy, resources, context)
10. Print banner (provider, model, autonomy, vertical)
11. Enter REPL loop
12. On exit: close gRPC clients, SIGTERM runtime, close session
```

**Runtime failure:** If the runtime binary is not available or fails to start within 15s, the CLI prints a warning and falls back to local-only mode (no protocol). This is a degraded mode, not the target architecture.

---

## 5. REPL Loop

Goals are routed through the SWE vertical's workflow engine:

1. Classify input (goal / amendment / query / approval / injection)
2. For goals: detect task type → select workflow → execute via WorkflowExecutor
3. For queries: direct agent loop (no workflow)
4. For slash commands: route to command handler

**Ctrl-C:** During agent work → cancels via AbortController. At prompt → re-prompt. Double Ctrl-C → exit.

---

## 6. Workflow Execution

When a goal is submitted:

```
1. detectTaskType(goal) → { id: "swe:implement", workflowId: "swe:implement-feature" }
2. CoordinatorClient.submitGoal(goal) → { goalId, rootWorkspaceId }
3. CoordinatorClient.decompose(workflow.stages as tasks) → [taskIds]
4. For each stage in topological order:
   a. If gated → prompt human for approval
   b. CoordinatorClient.dispatch(taskId, role, tools) → { workspaceId }
   c. Execute stage agent loop in that workspace
   d. Stage output flows as context to next stage
5. Workflow complete → print quality report
```

**Task type detection:** Keyword heuristics. "fix bug" → `swe:debug` → `swe:fix-bug` (3 stages). "add feature" → `swe:implement` → `swe:implement-feature` (4 stages). Default → implement.

**Profile switching:** Each stage uses the SWE profile for its role. Planner gets read-only tools + planning system prompt. Implementer gets read-write tools + implementation prompt. The LLM sees different tools and instructions per stage.

---

## 7. Stage Agent Loop

Each workflow stage is a real WACP workspace:

```
1. CoordinatorClient.dispatch(taskId, role) → workspaceId
2. AgentClient.bind(workspaceId, authToken) → bind response
3. AgentClient.emitSignal(STARTED)
4. LLM loop:
   a. Build messages (profile system prompt + prior context + goal)
   b. Filter tools to profile whitelist
   c. Call LLM (raw fetch + SSE streaming — LLM is external)
   d. Stream tokens to terminal
   e. For each tool call:
      - Autonomy gate check (prompt human if not trusted)
      - Execute tool via LocalResources
      - AgentClient.createCheckpoint(observation, tool result)
   f. Append tool results to messages
   g. Repeat until no tool calls
5. AgentClient.createCheckpoint(artifact, FINAL, stage output)
6. AgentClient.emitSignal(COMPLETE)
```

**On failure:** `AgentClient.emitSignal(FAILED, reason)`. Workflow stops.

**LLM is external:** The LLM call uses raw HTTP fetch to the provider API (Anthropic, OpenAI). This is correct — the LLM is not a WACP participant. But every LLM result is recorded in the protocol via checkpoints. The trail captures what happened; the LLM is the external compute resource.

---

## 8. Tool Integration

14 tools (7 built-in + 7 SWE-specific), each with JSON Schema for LLM function-calling:

| Tool | Operation | Description |
|------|-----------|-------------|
| `read_file` | `file_read` | Read file contents |
| `write_file` | `file_write` | Write file |
| `list_dir` | `file_read` | List directory |
| `search_files` | `file_read` | Glob pattern search |
| `run_command` | `shell_exec` | Shell command execution |
| `git_status` | `git_read` | Git status |
| `git_diff` | `git_read` | Git diff |
| `code_search` | `file_read` | Regex search in codebase |
| `code_edit` | `file_write` | Find-and-replace in file |
| `test_run` | `shell_exec` | Run test suite |
| `type_check` | `shell_exec` | Run type checker |
| `lint_check` | `shell_exec` | Run linter |
| `git_branch` | `git_write` | Create/switch branch |
| `git_commit` | `git_write` | Stage + commit |

Tools are filtered per stage by the profile's whitelist. Planner sees only read tools. Implementer sees read + write + exec.

---

## 9. Streaming Output

LLM responses stream token-by-token via SSE parsing. Tool calls display formatted summaries:

```
⟡ read_file(path: "src/auth.ts")
  → [742 bytes read]
```

Stage boundaries display transitions:

```
━━━ Workflow: Implement Feature (4 stages) ━━━
  [protocol] goal goal-1 submitted, root workspace ws-root
  [protocol] decomposed into 4 tasks

── Stage: plan (swe:planner) ──
  [protocol] workspace ws-plan dispatched
  [protocol] bound to workspace ws-plan
▶ plan started (swe:planner)
  [LLM output streams here]
✓ plan completed
```

---

## 10. Gate Prompts

Two levels of gating:

**Autonomy gates** (tool-level): When a tool operation is not in the trust surface.
```
⚠ write_file wants to write src/auth.ts
  Operation: file_write
  [y]es / [n]o / [a]lways allow file_write: _
```

**Workflow gates** (stage-level): When a workflow stage is marked `gated: true`.
```
⚠ Implement wants to perform: Proceed to Implement (swe:implementer)?
  Operation: stage:implement
  [y]es / [n]o / [a]lways allow stage:implement: _
```

---

## 11. Commands

| Command | Action |
|---------|--------|
| `/help` | Print available commands |
| `/trust <op>` | Grant trust for an operation type |
| `/revoke <op>` | Revoke trust for an operation type |
| `/preset <name>` | Switch autonomy preset |
| `/surface` | Display current trust surface |
| `/exit` | Close session, kill runtime, exit |
| `/clear` | Clear terminal |

---

## 12. Package Structure

```
packages/wacp-cli/
├── package.json          # @wacp/cli, bin entry, @grpc/grpc-js
├── tsconfig.json
├── src/
│   ├── main.ts           # Entry: parse args, spawn runtime, connect gRPC, REPL
│   ├── index.ts          # Public exports
│   ├── config.ts         # YAML config loading, env vars, merge, validation
│   ├── runtime-manager.ts # Spawn/manage wacp-runtime child process
│   ├── protocol-client.ts # gRPC clients (CoordinatorClient, AgentClient)
│   ├── repl.ts           # REPL loop, input classification, slash commands
│   ├── workflow.ts       # Task type detection, workflow execution via gRPC
│   ├── agent.ts          # Stage agent loop (bind, signal, checkpoint, LLM)
│   ├── llm.ts            # Raw fetch to providers, SSE streaming
│   ├── tools.ts          # 14 tool definitions + executors
│   ├── vertical.ts       # SWE vertical loader (workflows + profiles)
│   ├── display.ts        # Terminal formatting (tools, gates, banner)
│   └── commands.ts       # Slash command handlers
└── tests/
    ├── config.test.ts
    ├── tools.test.ts
    ├── commands.test.ts
    ├── display.test.ts
    ├── llm.test.ts
    ├── streaming.test.ts
    ├── agent.test.ts
    ├── workflow.test.ts
    ├── runtime-manager.test.ts
    └── protocol-client.test.ts
```

**Dependencies:** `@wacp/local`, `@grpc/grpc-js`, `@grpc/proto-loader`, `yaml`.

---

## 13. Test Requirements

| Module | Tests |
|--------|-------|
| `config.ts` | YAML loading, env vars, merge order, validation |
| `tools.ts` | 14 tool definitions valid, executors map correctly |
| `commands.ts` | All slash commands, invalid operation, missing args |
| `display.ts` | Tool call format, gate prompt, banner, trust surface |
| `llm.ts` | Provider URLs, headers, body construction, response parsing |
| `streaming.ts` | SSE body construction, tool result messages |
| `agent.ts` | LlmCallError retryable classification |
| `workflow.ts` | Task type detection (10 cases), vertical loading (5), tool filtering (4) |
| `runtime-manager.ts` | Config defaults, URL generation, start failure, safe stop |
| `protocol-client.ts` | Signal/checkpoint constants match proto |

**Total: 97 tests.**

---

## 14. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Runtime spec | §3 (process model) | §2, §4 | Runtime as child process |
| Coordinator SDK spec | §3, §4 | §6 | CoordinatorService RPCs |
| Agent SDK v2 spec | §3 | §7 | AgentContext, bind, signal, checkpoint |
| Local SDK spec | §3–7 | §2, §7, §10 | Session, autonomy, resources, orchestrator |
| LLM adapters spec | §4, §6 | §7, §9 | Message types, streaming |
| Tool framework spec | §3 | §8 | ToolDefinition schema |
| SWE vertical spec | §2–8 | §6, §8 | Roles, workflows, profiles, quality |
| Security spec | §3 | §7 | Content filter at LLM boundary |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
