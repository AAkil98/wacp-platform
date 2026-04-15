import type { LocalSession, AgentProfile } from "@wacp/local";
import type { CliConfig } from "./config.js";
import type { Message, ToolCall, LlmResponse } from "./llm.js";
import type { AgentClient } from "./protocol-client.js";
import type { LoadedEcosystem } from "./ecosystem.js";
import { SignalType, CheckpointStatus } from "./protocol-client.js";
import { streamCompletion, LlmCallError } from "./llm.js";
import { buildToolDefinitions, buildToolDefinitionsForEcosystem, executeTool, type ToolDefinition, type ToolResult } from "./tools.js";
import { formatToolCall, formatToolResult, formatGatePrompt } from "./display.js";

export interface AgentCallbacks {
  promptGate: (prompt: string) => Promise<string>;
  print: (text: string) => void;
  write: (text: string) => void;
}

/**
 * Run the agent loop for a single workflow stage — protocol-aware.
 *
 * Binds to the workspace via AgentService, emits signals, creates
 * checkpoints. LLM calls are still raw HTTP (LLM is external to
 * the protocol), but every result is checkpointed through the runtime.
 */
export async function stageAgentLoop(
  session: LocalSession,
  config: CliConfig,
  goal: string,
  priorContext: string,
  profile: AgentProfile,
  callbacks: AgentCallbacks,
  agentClient: AgentClient | null,
  workspaceId: string | null,
  authToken: string | null,
  signal?: AbortSignal,
  ecosystem?: LoadedEcosystem,
): Promise<string> {
  // Bind to workspace if we have protocol clients
  if (agentClient && workspaceId && authToken) {
    try {
      await agentClient.bind(workspaceId, authToken);
      await agentClient.emitSignal(SignalType.STARTED);
      callbacks.print(`  [protocol] bound to workspace ${workspaceId}`);
    } catch (err) {
      callbacks.print(`  [protocol] bind failed: ${(err as Error).message}`);
    }
  }

  const messages: Message[] = [
    { role: "system", content: profile.systemPrompt },
    { role: "user", content: `${priorContext}\n\nCurrent task: ${goal}` },
  ];

  const allTools = ecosystem ? buildToolDefinitionsForEcosystem(ecosystem) : buildToolDefinitions();
  const tools = filterToolsByProfile(allTools, profile);

  let lastContent = "";
  try {
    lastContent = await runLoop(session, config, messages, tools, callbacks, agentClient, signal, ecosystem);

    // Checkpoint the stage output
    if (agentClient && lastContent) {
      try {
        await agentClient.createCheckpoint(
          "artifact",
          Buffer.from(lastContent),
          `stage output for ${profile.roleId}`,
          CheckpointStatus.FINAL,
        );
      } catch { /* non-fatal */ }
    }

    // Signal completion
    if (agentClient) {
      try { await agentClient.emitSignal(SignalType.COMPLETE); } catch { /* non-fatal */ }
    }
  } catch (err) {
    if (agentClient) {
      try { await agentClient.emitSignal(SignalType.FAILED, (err as Error).message); } catch { /* best effort */ }
    }
    throw err;
  }

  return lastContent;
}

/**
 * Run the agent loop without a workflow (direct execution, no protocol).
 */
export async function agentLoop(
  session: LocalSession,
  config: CliConfig,
  goal: string,
  callbacks: AgentCallbacks,
  signal?: AbortSignal,
  ecosystem?: LoadedEcosystem,
): Promise<string> {
  const messages: Message[] = [
    { role: "system", content: config.systemPrompt },
    { role: "user", content: goal },
  ];
  const tools = ecosystem ? buildToolDefinitionsForEcosystem(ecosystem) : buildToolDefinitions();
  return runLoop(session, config, messages, tools, callbacks, null, signal, ecosystem);
}

async function runLoop(
  session: LocalSession,
  config: CliConfig,
  messages: Message[],
  tools: ToolDefinition[],
  callbacks: AgentCallbacks,
  agentClient: AgentClient | null,
  signal?: AbortSignal,
  ecosystem?: LoadedEcosystem,
): Promise<string> {
  let lastContent = "";
  let iterations = 0;
  const maxIterations = 25;

  while (iterations < maxIterations) {
    iterations++;

    let response: LlmResponse;
    try {
      response = await streamCompletion(config, messages, tools, {
        onToken: (token) => callbacks.write(token),
        onToolCallStart: (name) => callbacks.print(`\n⟡ calling ${name}...`),
      }, signal);
    } catch (err) {
      if (err instanceof LlmCallError) {
        callbacks.print(`\nLLM error (${err.status}): ${err.message}`);
        if (err.retryable) { callbacks.print("Retrying in 2s..."); await sleep(2000); continue; }
        return "";
      }
      if ((err as Error).name === "AbortError") { callbacks.print("\n[cancelled]"); return ""; }
      throw err;
    }

    if (response.content) { callbacks.write("\n"); lastContent = response.content; }
    if (response.toolCalls.length === 0) break;

    const toolMessages: Message[] = [];
    messages.push({ role: "assistant", content: buildAssistantContent(response) });

    for (const call of response.toolCalls) {
      callbacks.print(formatToolCall(call.name, call.arguments));
      const result = await executeWithGate(session, call, callbacks, ecosystem);
      callbacks.print(formatToolResult(result));

      // Checkpoint tool results through the protocol
      if (agentClient && !result.isError) {
        try {
          await agentClient.createCheckpoint(
            "observation", Buffer.from(result.content.slice(0, 4096)),
            `tool:${call.name}`, CheckpointStatus.PROVISIONAL,
          );
        } catch { /* non-fatal */ }
      }

      if (config.provider === "anthropic") {
        toolMessages.push({
          role: "tool",
          content: [{ type: "tool_result", tool_use_id: call.id, content: result.content, is_error: result.isError }],
        });
      } else {
        toolMessages.push({ role: "tool", content: result.content, tool_call_id: call.id });
      }
    }
    messages.push(...toolMessages);
  }

  if (iterations >= maxIterations) callbacks.print(`\n[stopped after ${maxIterations} iterations]`);
  session.context.addHistory(`assistant: ${lastContent.slice(0, 200)}`);
  return lastContent;
}

export function filterToolsByProfile(allTools: ToolDefinition[], profile: AgentProfile): ToolDefinition[] {
  return allTools.filter((t) => profile.tools.includes(t.name));
}

async function executeWithGate(
  session: LocalSession,
  call: ToolCall,
  callbacks: AgentCallbacks,
  ecosystem?: LoadedEcosystem,
): Promise<ToolResult> {
  const opType = toolToOperation(call.name, ecosystem);
  if (opType && !session.autonomy.check(opType)) {
    const prompt = formatGatePrompt(call.name, opType, `${call.name}(${JSON.stringify(call.arguments).slice(0, 100)})`);
    const answer = await callbacks.promptGate(prompt);
    switch (answer.toLowerCase().trim()) {
      case "y": case "yes": {
        session.autonomy.grant(opType);
        const r = await executeTool(session.resources, session.autonomy, call.name, call.id, call.arguments, ecosystem);
        session.autonomy.revoke(opType);
        session.context.recordTrust(opType, true);
        return r;
      }
      case "a": case "always":
        session.autonomy.grantAlways(opType);
        session.context.recordTrust(opType, true);
        return executeTool(session.resources, session.autonomy, call.name, call.id, call.arguments, ecosystem);
      default:
        session.context.recordTrust(opType, false);
        return { toolCallId: call.id, content: `Operation '${opType}' denied by user.`, isError: true };
    }
  }
  return executeTool(session.resources, session.autonomy, call.name, call.id, call.arguments, ecosystem);
}

const BUILTIN_TOOL_OPERATIONS: Record<string, string> = {
  read_file: "file_read",
  write_file: "file_write",
  list_dir: "file_read",
  search_files: "file_read",
  run_command: "shell_exec",
  git_status: "git_read",
  git_diff: "git_read",
};

function toolToOperation(name: string, ecosystem?: LoadedEcosystem): string | null {
  // Built-in tools first.
  const builtin = BUILTIN_TOOL_OPERATIONS[name];
  if (builtin) return builtin;

  // Ecosystem-aware: ask the owning vertical.
  if (ecosystem) {
    const owner = ecosystem.toolByName.get(name);
    if (owner) {
      return owner.toolOperation(name);
    }
  }

  // Legacy SWE-only fallback (for callers that don't pass an ecosystem).
  const sweLegacy: Record<string, string> = {
    code_search: "file_read",
    code_edit: "file_write",
    test_run: "shell_exec",
    type_check: "shell_exec",
    lint_check: "shell_exec",
    git_branch: "git_write",
    git_commit: "git_write",
    dependency_check: "shell_exec",
  };
  return sweLegacy[name] ?? null;
}

function buildAssistantContent(response: LlmResponse): string | ContentBlock[] {
  if (response.toolCalls.length === 0) return response.content;
  const blocks: ContentBlock[] = [];
  if (response.content) blocks.push({ type: "text", text: response.content });
  for (const tc of response.toolCalls) blocks.push({ type: "tool_use", id: tc.id, name: tc.name, input: tc.arguments });
  return blocks;
}

type ContentBlock = { type: string; [key: string]: unknown };
function sleep(ms: number): Promise<void> { return new Promise((r) => setTimeout(r, ms)); }
