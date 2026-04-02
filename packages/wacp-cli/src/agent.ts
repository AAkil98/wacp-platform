import type { LocalSession, AgentProfile } from "@wacp/local";
import type { CliConfig } from "./config.js";
import type { Message, ToolCall, LlmResponse } from "./llm.js";
import { streamCompletion, LlmCallError } from "./llm.js";
import { buildToolDefinitions, executeTool, type ToolDefinition, type ToolResult } from "./tools.js";
import { formatToolCall, formatToolResult, formatGatePrompt } from "./display.js";

export interface AgentCallbacks {
  /** Prompt the user for gate approval. Returns "y", "n", or "a". */
  promptGate: (prompt: string) => Promise<string>;
  /** Print a line to the terminal. */
  print: (text: string) => void;
  /** Write raw text (no newline) for streaming. */
  write: (text: string) => void;
}

/**
 * Run the agent loop for a single workflow stage.
 *
 * Uses the stage's profile (system prompt + tool whitelist) and receives
 * accumulated context from prior stages.
 */
export async function stageAgentLoop(
  session: LocalSession,
  config: CliConfig,
  goal: string,
  priorContext: string,
  profile: AgentProfile,
  callbacks: AgentCallbacks,
  signal?: AbortSignal,
): Promise<string> {
  // Build messages with the stage's system prompt + prior context
  const messages: Message[] = [
    { role: "system", content: profile.systemPrompt },
    { role: "user", content: `${priorContext}\n\nCurrent task: ${goal}` },
  ];

  // Filter tools to only those whitelisted for this role
  const allTools = buildToolDefinitions();
  const tools = filterToolsByProfile(allTools, profile);

  return runLoop(session, config, messages, tools, callbacks, signal);
}

/**
 * Run the agent loop without a workflow (direct execution).
 *
 * Uses the CLI's default system prompt and all available tools.
 * This is the fallback for goals that don't match a workflow.
 */
export async function agentLoop(
  session: LocalSession,
  config: CliConfig,
  goal: string,
  callbacks: AgentCallbacks,
  signal?: AbortSignal,
): Promise<string> {
  const messages: Message[] = [
    { role: "system", content: config.systemPrompt },
    { role: "user", content: goal },
  ];
  const tools = buildToolDefinitions();

  return runLoop(session, config, messages, tools, callbacks, signal);
}

/**
 * Core LLM + tool execution loop. Shared by stageAgentLoop and agentLoop.
 */
async function runLoop(
  session: LocalSession,
  config: CliConfig,
  messages: Message[],
  tools: ToolDefinition[],
  callbacks: AgentCallbacks,
  signal?: AbortSignal,
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
        if (err.retryable) {
          callbacks.print("Retrying in 2 seconds...");
          await sleep(2000);
          continue;
        }
        return "";
      }
      if ((err as Error).name === "AbortError") {
        callbacks.print("\n[cancelled]");
        return "";
      }
      throw err;
    }

    if (response.content) {
      callbacks.write("\n");
      lastContent = response.content;
    }

    if (response.toolCalls.length === 0) break;

    const toolMessages: Message[] = [];
    const assistantContent = buildAssistantContent(response);
    messages.push({ role: "assistant", content: assistantContent });

    for (const call of response.toolCalls) {
      callbacks.print(formatToolCall(call.name, call.arguments));

      const result = await executeWithGate(session, call, callbacks);
      callbacks.print(formatToolResult(result));

      if (config.provider === "anthropic") {
        toolMessages.push({
          role: "tool",
          content: [{ type: "tool_result", tool_use_id: call.id, content: result.content, is_error: result.isError }],
        });
      } else {
        toolMessages.push({
          role: "tool",
          content: result.content,
          tool_call_id: call.id,
        });
      }
    }

    messages.push(...toolMessages);
  }

  if (iterations >= maxIterations) {
    callbacks.print(`\n[stopped after ${maxIterations} iterations]`);
  }

  session.context.addHistory(`assistant: ${lastContent.slice(0, 200)}`);
  return lastContent;
}

// ---------------------------------------------------------------------------
// Tool filtering by profile
// ---------------------------------------------------------------------------

/** Filter tool definitions to only those in the profile's whitelist. */
export function filterToolsByProfile(
  allTools: ToolDefinition[],
  profile: AgentProfile,
): ToolDefinition[] {
  return allTools.filter((t) => profile.tools.includes(t.name));
}

// ---------------------------------------------------------------------------
// Gate checking (unchanged)
// ---------------------------------------------------------------------------

async function executeWithGate(
  session: LocalSession,
  call: ToolCall,
  callbacks: AgentCallbacks,
): Promise<ToolResult> {
  const opType = toolToOperation(call.name);

  if (opType && !session.autonomy.check(opType)) {
    const prompt = formatGatePrompt(
      call.name,
      opType,
      `${call.name}(${JSON.stringify(call.arguments).slice(0, 100)})`,
    );
    const answer = await callbacks.promptGate(prompt);

    switch (answer.toLowerCase().trim()) {
      case "y":
      case "yes": {
        session.autonomy.grant(opType);
        const result = await executeTool(session.resources, session.autonomy, call.name, call.id, call.arguments);
        session.autonomy.revoke(opType);
        session.context.recordTrust(opType, true);
        return result;
      }
      case "a":
      case "always":
        session.autonomy.grantAlways(opType);
        session.context.recordTrust(opType, true);
        return executeTool(session.resources, session.autonomy, call.name, call.id, call.arguments);

      default:
        session.context.recordTrust(opType, false);
        return {
          toolCallId: call.id,
          content: `Operation '${opType}' denied by user.`,
          isError: true,
        };
    }
  }

  return executeTool(session.resources, session.autonomy, call.name, call.id, call.arguments);
}

function toolToOperation(toolName: string): string | null {
  const mapping: Record<string, string> = {
    read_file: "file_read",
    write_file: "file_write",
    list_dir: "file_read",
    search_files: "file_read",
    code_search: "file_read",
    code_edit: "file_write",
    run_command: "shell_exec",
    test_run: "shell_exec",
    lint_check: "shell_exec",
    type_check: "shell_exec",
    git_status: "git_read",
    git_diff: "git_read",
    git_branch: "git_write",
    git_commit: "git_write",
    dependency_check: "shell_exec",
  };
  return mapping[toolName] ?? null;
}

function buildAssistantContent(response: LlmResponse): string | ContentBlock[] {
  if (response.toolCalls.length === 0) return response.content;
  const blocks: ContentBlock[] = [];
  if (response.content) blocks.push({ type: "text", text: response.content });
  for (const tc of response.toolCalls) {
    blocks.push({ type: "tool_use", id: tc.id, name: tc.name, input: tc.arguments });
  }
  return blocks;
}

type ContentBlock = { type: string; [key: string]: unknown };

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
