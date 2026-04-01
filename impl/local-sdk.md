# WACP Implementation: Local SDK

```yaml
id: wacp-impl-local-sdk
type: implementation-spec
status: draft
created: 2026-04-02
lineage: LAYER-MAPPING.md (M3)
protocol_sections:
  - §6 (workspace lifecycle — session maps to root workspace)
  - §8 (human highway — autonomy, gates)
depends_on:
  - wacp-impl-agent-sdk-v2
  - wacp-impl-coordinator-sdk
  - wacp-impl-tool-framework
  - wacp-impl-llm-adapters
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, middleware, local-sdk, session, autonomy, cli, ide]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Design Principles](#2-design-principles)
3. [Session Lifecycle](#3-session-lifecycle)
4. [Autonomy Manager](#4-autonomy-manager)
5. [Interaction Stream](#5-interaction-stream)
6. [Local Resources](#6-local-resources)
7. [Self-Orchestration](#7-self-orchestration)
8. [Boot Profile](#8-boot-profile)
9. [Session Context](#9-session-context)
10. [Package Structure](#10-package-structure)
11. [Test Requirements](#11-test-requirements)
12. [References](#12-references)

---

## 1. Purpose

This spec defines the local SDK — the composition layer for co-located human-agent systems (CLI, IDE, desktop agents). It answers "how does a local agent start, interact with the human, manage tools, and coordinate work" — not "how does the runtime enforce protocol rules" (that's the Rust runtime) or "how does the LLM think" (that's the application layer).

**Foundational rule:** Session = root workspace. Local agent = root coordinator.

The local SDK runs in the user's process (TypeScript/Bun for CLI and IDE). It is self-contained for local operations — tool execution, LLM calls, autonomy management happen in-process without requiring a separate Rust runtime. For multi-agent coordination (dispatching child workspaces), it connects to the runtime via gRPC.

**Scope.** TypeScript package `@wacp/local`. Session lifecycle (4 states). Autonomy manager (dynamic trust surface). Interaction stream (input classification). Local resources (filesystem, shell, git). Self-orchestration (coordinate + execute). Boot profile (fast startup). Session context (cross-task continuity).

**Not in scope.** The CLI REPL itself (Phase 25). IDE panels (Phase 28). LLM prompt engineering (application concern). The Rust runtime (exists).

**Language choice.** TypeScript, targeting Bun. The local SDK runs in the user's process — it must start fast, integrate with filesystem and shell APIs, and compose with the CLI and IDE, which are also TypeScript. Raw `fetch()` for LLM calls. Direct `fs`, `child_process`, `git` for local resources.

---

## 2. Design Principles

**Principle 1: Self-contained for single-agent tasks.** A CLI agent running a simple task (read file, edit code, run tests) should not need a separate Rust runtime process. The local SDK executes tools directly, calls LLMs directly, and manages the session locally. The runtime is only needed for multi-agent coordination.

**Principle 2: Session is the unit of interaction.** A session starts when the user opens the CLI, and ends when they close it. Within a session, multiple tasks may be executed. The session accumulates trust decisions, context, and history. Sessions are longer-lived than individual tasks.

**Principle 3: Autonomy evolves within a session.** The trust surface starts with a preset (supervised, assisted, autonomous) and evolves as the user grants or revokes permissions. "Always allow file reads" is a session-scoped decision. This differs from highway static presets — local autonomy is dynamic.

**Principle 4: Fast startup, not complete bootstrap.** The local SDK initializes lazily. The session object is created in <50ms. LLM adapter connects on first use. Tools load on first invocation. The user sees a prompt immediately, not after a 3-second boot sequence.

---

## 3. Session Lifecycle

```typescript
type SessionState = "open" | "active" | "suspended" | "closed";

class LocalSession {
  readonly id: string;
  state: SessionState;
  readonly config: SessionConfig;
  readonly autonomy: AutonomyManager;
  readonly resources: LocalResources;
  readonly context: SessionContext;

  static async create(config: SessionConfig): Promise<LocalSession>;
  async activate(): Promise<void>;
  async suspend(): Promise<void>;
  async close(): Promise<void>;
}
```

**State transitions:**

| From | To | Trigger |
|------|-----|---------|
| open | active | First goal submitted or `activate()` called |
| active | suspended | User pauses or `suspend()` called |
| suspended | active | User resumes or `activate()` called |
| active | closed | User quits or `close()` called |
| suspended | closed | User quits or `close()` called |

**SessionConfig:**

```typescript
interface SessionConfig {
  /** Working directory for local resources. */
  workingDir: string;
  /** Autonomy preset: supervised, assisted, autonomous. */
  autonomyPreset: AutonomyPreset;
  /** LLM provider configuration (optional — for single-agent mode). */
  llm?: LlmConfig;
  /** Runtime URL (optional — for multi-agent mode). */
  runtimeUrl?: string;
}

interface LlmConfig {
  provider: "anthropic" | "openai" | "generic";
  apiKey: string;
  model?: string;
  baseUrl?: string;
}
```

---

## 4. Autonomy Manager

The trust surface determines which operations the agent can perform without asking the human.

```typescript
type OperationType =
  | "file_read" | "file_write" | "file_delete"
  | "shell_exec" | "shell_exec_destructive"
  | "git_read" | "git_write"
  | "web_fetch"
  | "llm_call";

type AutonomyPreset = "supervised" | "assisted" | "autonomous";

class AutonomyManager {
  constructor(preset: AutonomyPreset);

  /** Check if an operation is currently trusted. */
  check(op: OperationType): boolean;

  /** Grant trust for an operation (for this session). */
  grant(op: OperationType): void;

  /** Grant trust permanently ("always allow"). */
  grantAlways(op: OperationType): void;

  /** Revoke trust for an operation. */
  revoke(op: OperationType): void;

  /** Current trust surface (set of trusted operations). */
  trustSurface(): Set<OperationType>;

  /** Reset to a preset. */
  resetToPreset(preset: AutonomyPreset): void;
}
```

**Preset defaults:**

| Operation | Supervised | Assisted | Autonomous |
|-----------|:-:|:-:|:-:|
| `file_read` | -- | yes | yes |
| `file_write` | -- | -- | yes |
| `file_delete` | -- | -- | -- |
| `shell_exec` | -- | -- | yes |
| `shell_exec_destructive` | -- | -- | -- |
| `git_read` | -- | yes | yes |
| `git_write` | -- | -- | yes |
| `web_fetch` | -- | yes | yes |
| `llm_call` | yes | yes | yes |

`--` means gated (requires human approval). `yes` means auto-approved.

**Gate resolution.** When the agent wants to perform an operation:
1. Check `autonomy.check(op)`. If trusted → proceed.
2. If not trusted → emit a gate event. The interaction stream presents it to the human.
3. Human approves → proceed + optionally `grant(op)` for future calls.
4. Human rejects → operation fails with a structured error.

---

## 5. Interaction Stream

Classifies human input into protocol-relevant categories.

```typescript
type InputType = "goal" | "amendment" | "query" | "approval" | "injection";

interface ClassifiedInput {
  type: InputType;
  content: string;
  /** For approval/rejection of pending gates. */
  gateId?: string;
  /** For injection into a specific workspace. */
  targetWorkspace?: string;
}

class InteractionStream {
  /** Classify raw human input. */
  classify(input: string, pendingGates: Gate[]): ClassifiedInput;

  /** Queue input during agent work (buffering). */
  buffer(input: string): void;

  /** Flush buffered input. */
  flush(): ClassifiedInput[];
}
```

**Classification rules (heuristic):**

| Pattern | Type | Example |
|---------|------|---------|
| Starts during idle | `goal` | "fix the bug in auth.ts" |
| Starts with "yes"/"no"/"approve"/"reject" and gate pending | `approval` | "yes, allow that" |
| Starts with "actually"/"instead"/"change" during active task | `amendment` | "actually, use Python instead" |
| Starts with "?"/"what"/"how"/"why" | `query` | "what files did you change?" |
| Explicit injection syntax (`@ws-id: message`) | `injection` | "@ws-child-1: focus on error handling" |
| Default | `goal` | anything else |

---

## 6. Local Resources

Direct access to the user's local environment, gated by the autonomy manager.

```typescript
class LocalResources {
  constructor(workingDir: string, autonomy: AutonomyManager);

  // --- Filesystem ---
  async readFile(path: string): Promise<string>;
  async writeFile(path: string, content: string): Promise<void>;
  async deleteFile(path: string): Promise<void>;
  async glob(pattern: string): Promise<string[]>;
  async readDir(path: string): Promise<string[]>;
  async fileExists(path: string): Promise<boolean>;

  // --- Shell ---
  async exec(command: string, opts?: ExecOptions): Promise<ExecResult>;

  // --- Git ---
  async gitStatus(): Promise<GitStatus>;
  async gitDiff(ref?: string): Promise<string>;
  async gitLog(limit?: number): Promise<GitLogEntry[]>;
  async gitStage(paths: string[]): Promise<void>;
  async gitCommit(message: string): Promise<string>;
}

interface ExecOptions {
  cwd?: string;
  timeout?: number;    // ms, default: 30_000
  stdin?: string;
}

interface ExecResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}
```

**Autonomy gating.** Every method checks the autonomy manager before executing:
- `readFile`, `glob`, `readDir`, `fileExists` → `file_read`
- `writeFile` → `file_write`
- `deleteFile` → `file_delete`
- `exec` → `shell_exec` (or `shell_exec_destructive` for known destructive patterns)
- `gitStatus`, `gitDiff`, `gitLog` → `git_read`
- `gitStage`, `gitCommit` → `git_write`

If the check fails, the method throws `AutonomyError` with the operation type and a human-readable message.

**Path scoping.** All filesystem operations are scoped to `workingDir`. Paths are resolved relative to it. Attempts to escape (via `../` traversal) are rejected.

---

## 7. Self-Orchestration

For single-agent tasks, the local agent executes work directly. For complex tasks, it dispatches child workspaces via the runtime.

```typescript
class SelfOrchestrator {
  constructor(
    session: LocalSession,
    resources: LocalResources,
    llmAdapter?: LlmAdapter,
  );

  /** Execute a task directly (single-agent path). */
  async execute(goal: string): Promise<ExecutionResult>;

  /** Dispatch to the runtime for multi-agent coordination. */
  async dispatch(goal: string, runtimeUrl: string): Promise<CoordinationResult>;
}
```

**Routing decision:** If `config.runtimeUrl` is set and the task is complex (multiple roles needed), use `dispatch()`. Otherwise, use `execute()`. The initial implementation always uses `execute()` — multi-agent dispatch is deferred to Phase 27 when the API server is wired.

**`execute()` flow:**
1. Call LLM with goal + available tools.
2. LLM returns text or tool calls.
3. For each tool call → execute via local resources (autonomy-gated).
4. Feed tool results back to LLM.
5. Repeat until LLM signals completion.
6. Return final result.

---

## 8. Boot Profile

Fast startup for CLI responsiveness.

```typescript
async function boot(config: SessionConfig): Promise<LocalSession> {
  // 1. Create session object (~1ms)
  const session = await LocalSession.create(config);

  // 2. Autonomy manager initialized from preset (~0ms)
  // Already done in constructor.

  // 3. Local resources initialized (~1ms)
  // Just stores working dir, no I/O.

  // 4. LLM adapter NOT initialized (lazy on first use)
  // 5. Runtime connection NOT established (lazy on first dispatch)

  return session; // Total: <50ms
}
```

LLM adapter connects on first `execute()` call. Runtime connects on first `dispatch()` call. Tool descriptors load on first `tools()` call.

---

## 9. Session Context

Cross-task continuity within a session.

```typescript
class SessionContext {
  /** Accumulated messages across tasks (for context window). */
  history: Message[];

  /** Trust decisions made during this session. */
  trustLog: TrustDecision[];

  /** Session-level metadata. */
  metadata: Record<string, unknown>;

  /** Take a snapshot of the current session state. */
  snapshot(): SessionSnapshot;

  /** Restore from a snapshot. */
  restore(snapshot: SessionSnapshot): void;

  /** Prune old history to stay within token budget. */
  prune(maxTokens: number): void;
}
```

---

## 10. Package Structure

```
packages/wacp-local/
├── package.json        # @wacp/local, TypeScript, Vitest
├── tsconfig.json
├── src/
│   ├── index.ts        # Public exports
│   ├── session.ts      # LocalSession, SessionConfig, SessionState
│   ├── autonomy.ts     # AutonomyManager, OperationType, presets
│   ├── interaction.ts  # InteractionStream, classify, InputType
│   ├── resources.ts    # LocalResources (fs, shell, git)
│   ├── orchestrator.ts # SelfOrchestrator (execute, dispatch)
│   ├── context.ts      # SessionContext, snapshot, prune
│   └── errors.ts       # AutonomyError, SessionError
└── tests/
    ├── session.test.ts
    ├── autonomy.test.ts
    ├── interaction.test.ts
    ├── resources.test.ts
    ├── orchestrator.test.ts
    └── context.test.ts
```

**Dependencies:** None external for the core. `@wacp/llm` (Phase 21 TS, future) for LLM calls — initially raw `fetch()` inline.

---

## 11. Test Requirements

| Module | Tests |
|--------|-------|
| `session.ts` | Create → state is open. Activate → active. Suspend → suspended. Resume → active. Close → closed. Invalid transitions rejected. Config validation. |
| `autonomy.ts` | Supervised preset: only llm_call trusted. Assisted: file_read + git_read + web_fetch + llm_call. Autonomous: most trusted. grant() adds. revoke() removes. grantAlways() persists. resetToPreset() clears. check() returns correct boolean. |
| `interaction.ts` | Goal classification. Approval with pending gate. Amendment during active. Query starts with "?". Injection with @ws syntax. Default → goal. Buffer and flush. |
| `resources.ts` | readFile succeeds when trusted. readFile throws AutonomyError when not trusted. writeFile gated. exec gated. Path scoping rejects traversal. Git operations gated. |
| `orchestrator.ts` | execute() calls LLM (mocked). Tool call → resource execution. Completion returns result. |
| `context.ts` | Snapshot captures state. Restore rebuilds state. Prune removes old history. Trust log accumulated. |

**Total target: ~40 tests.**

---

## 12. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Agent SDK v2 spec | §3 | §7 | AgentContext composition |
| Coordinator SDK spec | §4 | §7 | CoordinatorContext for dispatch |
| Tool framework spec | §10 | §6, §7 | Tool execution via local resources |
| LLM adapters spec | §4 | §7, §8 | LLM calls for self-orchestration |
| Security spec | §3 | §6 | Content filter at LLM boundary |
| LAYER-MAPPING.md | M3 | §1 | Local SDK design |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
