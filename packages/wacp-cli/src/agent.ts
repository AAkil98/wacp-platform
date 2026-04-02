import type { LocalSession } from "@wacp/local";
import type { CliConfig } from "./config.js";
import type { Message, ToolCall, LlmResponse } from "./llm.js";
import { streamCompletion, LlmCallError } from "./llm.js";
import { buildToolDefinitions, executeTool, type ToolResult } from "./tools.js";
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
 * Run the agent loop: call LLM → execute tools → repeat until done.
 *
 * Returns the final assistant content (for session history).
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

  let lastContent = "";
  let iterations = 0;
  const maxIterations = 25; // safety limit

  while (iterations < maxIterations) {
    iterations++;

    // 1. Call LLM with streaming
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

    // Newline after streamed content
    if (response.content) {
      callbacks.write("\n");
      lastContent = response.content;
    }

    // 2. No tool calls → done
    if (response.toolCalls.length === 0) break;

    // 3. Execute each tool call
    const toolMessages: Message[] = [];

    // Build the assistant message with tool calls for conversation history
    const assistantContent = buildAssistantContent(response);
    messages.push({ role: "assistant", content: assistantContent });

    for (const call of response.toolCalls) {
      // Display the tool call
      callbacks.print(formatToolCall(call.name, call.arguments));

      // Gate check
      const result = await executeWithGate(session, call, callbacks);

      // Display result
      callbacks.print(formatToolResult(result));

      // Build tool result message
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

    // 4. Append tool results to conversation
    messages.push(...toolMessages);

    // 5. Loop — LLM sees tool results and decides next action
  }

  if (iterations >= maxIterations) {
    callbacks.print(`\n[stopped after ${maxIterations} iterations]`);
  }

  // Record in session context
  session.context.addHistory(`user: ${goal}`);
  if (lastContent) {
    session.context.addHistory(`assistant: ${lastContent.slice(0, 200)}`);
  }

  return lastContent;
}

/**
 * Execute a tool call with autonomy gate checking.
 * If the operation is not trusted, prompts the user.
 */
async function executeWithGate(
  session: LocalSession,
  call: ToolCall,
  callbacks: AgentCallbacks,
): Promise<ToolResult> {
  // Map tool name to operation type for autonomy check
  const opType = toolToOperation(call.name);

  if (opType && !session.autonomy.check(opType)) {
    // Show gate prompt
    const prompt = formatGatePrompt(
      call.name,
      opType,
      `${call.name}(${JSON.stringify(call.arguments).slice(0, 100)})`,
    );
    const answer = await callbacks.promptGate(prompt);

    switch (answer.toLowerCase().trim()) {
      case "y":
      case "yes":
        // Allow this once — temporarily grant, execute, revoke
        session.autonomy.grant(opType);
        const result = await executeTool(session.resources, session.autonomy, call.name, call.id, call.arguments);
        session.autonomy.revoke(opType);
        session.context.recordTrust(opType, true);
        return result;

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

  // Already trusted — execute directly
  return executeTool(session.resources, session.autonomy, call.name, call.id, call.arguments);
}

function toolToOperation(toolName: string): string | null {
  const mapping: Record<string, string> = {
    read_file: "file_read",
    write_file: "file_write",
    list_dir: "file_read",
    search_files: "file_read",
    run_command: "shell_exec",
    git_status: "git_read",
    git_diff: "git_read",
  };
  return mapping[toolName] ?? null;
}

function buildAssistantContent(response: LlmResponse): string | ContentBlock[] {
  if (response.toolCalls.length === 0) return response.content;

  // For Anthropic, reconstruct the content blocks
  const blocks: ContentBlock[] = [];
  if (response.content) {
    blocks.push({ type: "text", text: response.content });
  }
  for (const tc of response.toolCalls) {
    blocks.push({
      type: "tool_use",
      id: tc.id,
      name: tc.name,
      input: tc.arguments,
    });
  }
  return blocks;
}

type ContentBlock = { type: string; [key: string]: unknown };

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
