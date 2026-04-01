import type { ToolResult } from "./tools.js";

/** Format a tool call for terminal display. */
export function formatToolCall(name: string, args: Record<string, unknown>): string {
  const argStr = Object.entries(args)
    .map(([k, v]) => `${k}: ${JSON.stringify(v)}`)
    .join(", ");
  return `⟡ ${name}(${argStr})`;
}

/** Format a tool result for terminal display. */
export function formatToolResult(result: ToolResult): string {
  if (result.isError) {
    return `  ✗ ${result.content}`;
  }
  const preview = result.content.length > 200
    ? result.content.slice(0, 197) + "..."
    : result.content;
  const lines = preview.split("\n").length;
  if (lines > 5) {
    return `  → [${result.content.length} bytes, ${lines} lines]`;
  }
  return `  → ${preview}`;
}

/** Format a gate prompt for terminal display. */
export function formatGatePrompt(toolName: string, operation: string, detail: string): string {
  return [
    `⚠ ${toolName} wants to perform: ${detail}`,
    `  Operation: ${operation}`,
    `  [y]es / [n]o / [a]lways allow ${operation}: `,
  ].join("\n");
}

/** Print the welcome banner. */
export function formatBanner(workingDir: string, provider: string, model: string, autonomy: string): string {
  return [
    `WACP CLI Agent v0.1.0`,
    `  Provider: ${provider} (${model})`,
    `  Working dir: ${workingDir}`,
    `  Autonomy: ${autonomy}`,
    `  Type /help for commands, /exit to quit.`,
    "",
  ].join("\n");
}

/** Format the /surface command output. */
export function formatTrustSurface(trusted: Set<string>): string {
  if (trusted.size === 0) return "Trust surface: (empty — all operations gated)";
  const ops = [...trusted].sort().join(", ");
  return `Trust surface: ${ops}`;
}

/** Format the /help command output. */
export function formatHelp(): string {
  return [
    "Commands:",
    "  /help              Show this help",
    "  /trust <op>        Grant trust for an operation",
    "  /revoke <op>       Revoke trust for an operation",
    "  /preset <name>     Switch autonomy preset (supervised/assisted/autonomous)",
    "  /surface           Display current trust surface",
    "  /clear             Clear terminal",
    "  /exit              Exit the CLI",
    "",
    "Operations: file_read, file_write, file_delete, shell_exec, git_read, git_write, web_fetch, llm_call",
  ].join("\n");
}
