# WACP Implementation: CLI Agent

```yaml
id: wacp-impl-cli-agent
type: implementation-spec
status: draft
created: 2026-04-02
lineage: LAYER-MAPPING.md (A1)
depends_on:
  - wacp-impl-local-sdk
  - wacp-impl-tool-framework
  - wacp-impl-llm-adapters
  - wacp-impl-security
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, application, cli, agent, repl]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Architecture](#2-architecture)
3. [Configuration](#3-configuration)
4. [Boot Sequence](#4-boot-sequence)
5. [REPL Loop](#5-repl-loop)
6. [Agent Loop](#6-agent-loop)
7. [Tool Integration](#7-tool-integration)
8. [Streaming Output](#8-streaming-output)
9. [Gate Prompts](#9-gate-prompts)
10. [Commands](#10-commands)
11. [Package Structure](#11-package-structure)
12. [Test Requirements](#12-test-requirements)
13. [References](#13-references)

---

## 1. Purpose

This spec defines the CLI agent — a terminal-based AI assistant that composes the local SDK, tool framework, and LLM adapters into an interactive REPL. It answers "how does a user interact with a WACP agent from the terminal" — not "how does the session manage state" (that's the local-sdk) or "how does the LLM think" (that's the adapter).

**Scope.** TypeScript package `@wacp/cli`. Configuration loading (YAML). Boot sequence. REPL loop with input classification. Agent loop (LLM call → tool execution → repeat). Streaming token output. Gate prompts for autonomy. Slash commands (`/trust`, `/revoke`, `/preset`, `/help`, `/exit`). Signal handling (Ctrl-C).

**Not in scope.** Multi-agent coordination (Phase 27 — API server). IDE integration (Phase 28). Ecosystem verticals (Phase 26). The CLI is a single-agent system — one LLM, one set of tools, one human.

---

## 2. Architecture

```
┌─────────────────────────────────────────────┐
│  Human (terminal)                           │
│  types goals, approves gates, reads output  │
└─────────────┬───────────────────────────────┘
              │ stdin/stdout
┌─────────────▼───────────────────────────────┐
│  REPL (readline, input classification)      │
├─────────────────────────────────────────────┤
│  Agent Loop                                 │
│  ┌──────────┐  ┌──────────┐  ┌───────────┐ │
│  │ LLM call │→ │ tool exec│→ │ gate check│ │
│  │ (fetch)  │  │ (local)  │  │ (autonomy)│ │
│  └──────────┘  └──────────┘  └───────────┘ │
├─────────────────────────────────────────────┤
│  LocalSession (autonomy, resources, context)│
└─────────────────────────────────────────────┘
```

The CLI is a thin composition layer. `LocalSession` manages state. The agent loop calls the LLM and executes tools. The REPL handles I/O.

---

## 3. Configuration

```yaml
# ~/.wacp/config.yaml
provider: anthropic          # anthropic | openai | generic
model: claude-sonnet-4-20250514
api_key: ${ANTHROPIC_API_KEY}  # env var substitution
working_dir: .               # default: current directory
autonomy: assisted           # supervised | assisted | autonomous

# Optional
base_url: https://api.anthropic.com  # for generic provider
max_tokens: 16384
temperature: 0.0
system_prompt: |
  You are a helpful coding assistant. You have access to filesystem,
  shell, and git tools. Use them to accomplish the user's goals.
```

**Loading order:** CLI flags → env vars → config file → defaults.

**Config resolution:**
```typescript
interface CliConfig {
  provider: "anthropic" | "openai" | "generic";
  model: string;
  apiKey: string;
  workingDir: string;
  autonomy: AutonomyPreset;
  baseUrl?: string;
  maxTokens: number;
  temperature: number;
  systemPrompt: string;
}
```

**Env var substitution:** Values starting with `${` are resolved from environment variables. `${ANTHROPIC_API_KEY}` → `process.env.ANTHROPIC_API_KEY`. Missing env var → error at boot.

---

## 4. Boot Sequence

```
1. Parse CLI flags (--config, --provider, --model, --working-dir)
2. Load config file (~/.wacp/config.yaml or --config path)
3. Merge: flags > env > file > defaults
4. Validate config (provider set, API key present)
5. Create LocalSession(config)
6. Register built-in tools
7. Print welcome banner
8. Enter REPL loop
```

Target: prompt visible in <200ms. LLM connects lazily on first goal.

---

## 5. REPL Loop

```typescript
async function repl(session: LocalSession, config: CliConfig): Promise<void> {
  const rl = createInterface({ input: stdin, output: stdout });
  const stream = new InteractionStream();

  while (session.state !== "closed") {
    const input = await prompt(rl, "wacp> ");
    if (input === null) { await session.close(); break; } // EOF

    // Classify
    const classified = stream.classify(input.trim(), pendingGates);

    switch (classified.type) {
      case "goal":
      case "amendment":
        await agentLoop(session, config, classified.content);
        break;
      case "approval":
        resolveGate(classified);
        break;
      case "query":
        await handleQuery(session, classified.content);
        break;
      case "injection":
        // Not implemented in single-agent CLI
        print("Injection not supported in single-agent mode.");
        break;
    }
  }
}
```

**Slash commands** are intercepted before classification: input starting with `/` is routed to the command handler (§10).

**Ctrl-C handling:** During agent loop, Ctrl-C cancels the current LLM call (via cancellation token). During prompt, Ctrl-C prints a blank line and re-prompts. Double Ctrl-C exits.

---

## 6. Agent Loop

The core execution cycle: call LLM → process response → execute tools → repeat.

```typescript
async function agentLoop(
  session: LocalSession,
  config: CliConfig,
  goal: string,
): Promise<void> {
  const messages: Message[] = [
    { role: "system", content: config.systemPrompt },
    { role: "user", content: goal },
  ];

  const tools = buildToolDefinitions(session);

  while (true) {
    // 1. Call LLM
    const response = await callLlm(config, messages, tools);

    // 2. Print text content
    if (response.content) {
      printStreaming(response.content);
    }

    // 3. If no tool calls → done
    if (response.toolCalls.length === 0) break;

    // 4. Execute each tool call
    const toolResults = [];
    for (const call of response.toolCalls) {
      const result = await executeTool(session, call);
      toolResults.push(result);
    }

    // 5. Append assistant message + tool results to conversation
    messages.push({ role: "assistant", content: response.raw });
    for (const result of toolResults) {
      messages.push(result.message);
    }

    // 6. Loop — LLM sees tool results and continues
  }

  // Record in session context
  session.context.addHistory(`user: ${goal}`);
  session.context.addHistory(`assistant: [completed]`);
}
```

**Tool execution with autonomy gating:** Each tool call checks the autonomy manager. If the operation is not trusted, a gate prompt is shown to the user (§9). If approved, execution proceeds. If rejected, a tool error is returned to the LLM.

---

## 7. Tool Integration

Built-in tools registered at boot, backed by `LocalResources`:

| Tool name | Maps to | Operation type |
|-----------|---------|----------------|
| `read_file` | `resources.readFile(path)` | `file_read` |
| `write_file` | `resources.writeFile(path, content)` | `file_write` |
| `list_dir` | `resources.readDir(path)` | `file_read` |
| `search_files` | `resources.glob(pattern)` | `file_read` |
| `run_command` | `resources.exec(command)` | `shell_exec` |
| `git_status` | `resources.gitStatus()` | `git_read` |
| `git_diff` | `resources.gitDiff(ref?)` | `git_read` |

Each tool has a JSON Schema `input_schema` for LLM function-calling and an executor function that calls the corresponding `LocalResources` method.

```typescript
function buildToolDefinitions(session: LocalSession): ToolDefinition[] {
  return [
    {
      name: "read_file",
      description: "Read the contents of a file",
      input_schema: {
        type: "object",
        properties: { path: { type: "string", description: "File path relative to working dir" } },
        required: ["path"],
      },
    },
    // ... other tools
  ];
}
```

---

## 8. Streaming Output

LLM responses are streamed token-by-token to the terminal for responsiveness.

```typescript
async function callLlm(
  config: CliConfig,
  messages: Message[],
  tools: ToolDefinition[],
): Promise<LlmResponse> {
  // Raw fetch to provider API with streaming
  const response = await fetch(providerUrl(config), {
    method: "POST",
    headers: providerHeaders(config),
    body: JSON.stringify(providerBody(config, messages, tools)),
  });

  // Stream SSE response, print tokens as they arrive
  for await (const chunk of parseSSE(response.body)) {
    if (chunk.type === "content_delta") {
      process.stdout.write(chunk.delta);
    }
  }
  process.stdout.write("\n");
  // ... assemble full response
}
```

**Tool call display:** When the LLM makes a tool call, print a formatted summary:

```
⟡ read_file(path: "src/auth.ts")
  → [742 bytes read]
```

---

## 9. Gate Prompts

When a tool operation is not trusted by the autonomy manager:

```
⚠ read_file wants to read src/auth.ts
  Operation: file_read
  [y]es / [n]o / [a]lways allow file_read: _
```

- `y` → allow this one invocation, continue.
- `n` → return error to LLM ("operation denied by user").
- `a` → `autonomy.grantAlways("file_read")`, allow this and all future.

The gate prompt blocks the agent loop until the user responds.

---

## 10. Commands

Slash commands intercepted by the REPL:

| Command | Action |
|---------|--------|
| `/help` | Print available commands |
| `/trust <op>` | Grant trust for an operation type |
| `/revoke <op>` | Revoke trust for an operation type |
| `/preset <name>` | Switch autonomy preset |
| `/surface` | Display current trust surface |
| `/exit` | Close session and exit |
| `/clear` | Clear terminal |

---

## 11. Package Structure

```
packages/wacp-cli/
├── package.json        # @wacp/cli, TypeScript, bin entry
├── tsconfig.json
├── src/
│   ├── index.ts        # Entry point: parse args, load config, boot, repl
│   ├── config.ts       # Config loading (YAML, env vars, defaults, merge)
│   ├── repl.ts         # REPL loop, readline, input routing
│   ├── agent.ts        # Agent loop: LLM call → tool exec → repeat
│   ├── llm.ts          # Raw fetch to Anthropic/OpenAI, SSE parsing
│   ├── tools.ts        # Built-in tool definitions + executors
│   ├── display.ts      # Streaming output, tool call display, gate prompts
│   └── commands.ts     # Slash command handlers
└── tests/
    ├── config.test.ts
    ├── tools.test.ts
    ├── commands.test.ts
    └── display.test.ts
```

**Dependencies:** `@wacp/local`, `yaml` (YAML parsing).

**Bin entry:** `"bin": { "wacp": "./dist/index.js" }` — installed globally via `npm install -g @wacp/cli`.

---

## 12. Test Requirements

| Module | Tests |
|--------|-------|
| `config.ts` | Load from YAML string. Env var substitution. Missing env var → error. Defaults applied. Merge order (flags > env > file). Invalid provider → error. |
| `tools.ts` | All 7 tool definitions have valid schemas. Tool executor maps to correct resource method. Tool result formatted as message. |
| `commands.ts` | /help prints list. /trust grants. /revoke revokes. /preset switches. /surface shows set. Unknown command → error message. |
| `display.ts` | Tool call formatted. Gate prompt formatted. |

**Total target: ~25 tests.** Agent loop and LLM integration tested via E2E with mock LLM responses.

---

## 13. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Local SDK spec | §3–9 | §2, §5, §6 | LocalSession, AutonomyManager, resources |
| LLM adapters spec | §3–8 | §6, §8 | Message types, streaming, provider APIs |
| Tool framework spec | §3 | §7 | ToolDefinition schema |
| Security spec | §3 | §6 | Content filter before LLM calls |
| LAYER-MAPPING.md | A1 | §1 | CLI agent design |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
