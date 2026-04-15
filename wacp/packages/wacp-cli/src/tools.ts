import type { LocalResources, AutonomyManager } from "@wacp/local";
import { AutonomyError } from "@wacp/local";
import { sweToolDefinitions, executeSweTools } from "@wacp/swe";
import type { LoadedEcosystem, VerticalToolDefinition } from "./ecosystem.js";

/** Tool definition for LLM function-calling. */
export interface ToolDefinition {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

/** Result of a tool execution. */
export interface ToolResult {
  toolCallId: string;
  content: string;
  isError: boolean;
}

/** The 7 universal built-in tools available in every workspace. */
const BUILTIN_TOOL_DEFINITIONS: ToolDefinition[] = [
  {
    name: "read_file",
    description: "Read the contents of a file. Returns the full file content as a string.",
    input_schema: {
      type: "object",
      properties: {
        path: { type: "string", description: "File path relative to working directory" },
      },
      required: ["path"],
    },
  },
  {
    name: "write_file",
    description: "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Creates parent directories as needed.",
    input_schema: {
      type: "object",
      properties: {
        path: { type: "string", description: "File path relative to working directory" },
        content: { type: "string", description: "Content to write" },
      },
      required: ["path", "content"],
    },
  },
  {
    name: "list_dir",
    description: "List the contents of a directory.",
    input_schema: {
      type: "object",
      properties: {
        path: { type: "string", description: "Directory path relative to working directory. Use '.' for current directory." },
      },
      required: ["path"],
    },
  },
  {
    name: "search_files",
    description: "Search for files matching a pattern in the working directory.",
    input_schema: {
      type: "object",
      properties: {
        pattern: { type: "string", description: "Search pattern (substring match)" },
      },
      required: ["pattern"],
    },
  },
  {
    name: "run_command",
    description: "Execute a shell command and return its output. Use for running tests, builds, linting, etc.",
    input_schema: {
      type: "object",
      properties: {
        command: { type: "string", description: "Shell command to execute" },
      },
      required: ["command"],
    },
  },
  {
    name: "git_status",
    description: "Show the current git status (modified, added, deleted files).",
    input_schema: {
      type: "object",
      properties: {},
    },
  },
  {
    name: "git_diff",
    description: "Show git diff of current changes, or diff against a specific ref.",
    input_schema: {
      type: "object",
      properties: {
        ref: { type: "string", description: "Git ref to diff against (optional, defaults to HEAD)" },
      },
    },
  },
];

const BUILTIN_TOOL_NAMES = new Set(BUILTIN_TOOL_DEFINITIONS.map((t) => t.name));

/** The 7 universal built-in tools — usable from any vertical. */
export function builtinToolDefinitions(): ToolDefinition[] {
  return [...BUILTIN_TOOL_DEFINITIONS];
}

/**
 * Backward-compat: returns built-in + SWE-specific tools (14 total) when no
 * ecosystem is supplied. Equivalent to the pre-27R behavior. Existing tests
 * (and any caller that hasn't migrated) get the legacy SWE-only tool list.
 *
 * For multi-vertical composition, callers should use buildToolDefinitionsForEcosystem.
 */
export function buildToolDefinitions(): ToolDefinition[] {
  return [...BUILTIN_TOOL_DEFINITIONS, ...(sweToolDefinitions() as ToolDefinition[])];
}

/** Compose built-in + every loaded vertical's tools. Used by the multi-vertical CLI runtime. */
export function buildToolDefinitionsForEcosystem(ecosystem: LoadedEcosystem): ToolDefinition[] {
  const result: ToolDefinition[] = [...BUILTIN_TOOL_DEFINITIONS];
  for (const vertical of ecosystem.verticals) {
    for (const td of vertical.toolDefinitions as readonly VerticalToolDefinition[]) {
      result.push(td as ToolDefinition);
    }
  }
  return result;
}

/**
 * Execute a tool call.
 *
 * Dispatch order:
 *   1. Built-in 7 tools — handled inline.
 *   2. If an ecosystem is supplied, dispatch via ecosystem.toolByName to the
 *      owning vertical's executor (this is the multi-vertical path).
 *   3. Legacy fallback: try the SWE vertical's executor directly. This keeps
 *      pre-27R callers (and existing CLI tests) working without modification.
 *   4. Unknown tool — structured error.
 */
export async function executeTool(
  resources: LocalResources,
  autonomy: AutonomyManager,
  toolName: string,
  toolCallId: string,
  args: Record<string, unknown>,
  ecosystem?: LoadedEcosystem,
): Promise<ToolResult> {
  try {
    // 1. Built-in tools.
    if (BUILTIN_TOOL_NAMES.has(toolName)) {
      const content = await executeBuiltinTool(resources, toolName, args);
      return { toolCallId, content, isError: false };
    }

    // 2. Ecosystem dispatch — multi-vertical path.
    if (ecosystem) {
      const owner = ecosystem.toolByName.get(toolName);
      if (owner) {
        const result = await owner.executeTool(resources, toolName, args);
        return { toolCallId, content: result.content, isError: result.isError };
      }
    }

    // 3. Legacy fallback: SWE vertical's executor.
    const sweResult = await executeSweTools(resources, toolName, args);
    if (!sweResult.content.startsWith("Unknown SWE tool:")) {
      return { toolCallId, content: sweResult.content, isError: sweResult.isError };
    }

    // 4. Unknown.
    return { toolCallId, content: `Unknown tool: ${toolName}`, isError: true };
  } catch (err) {
    if (err instanceof AutonomyError) {
      return {
        toolCallId,
        content: `Operation denied: ${err.operation} requires human approval`,
        isError: true,
      };
    }
    const message = err instanceof Error ? err.message : String(err);
    return { toolCallId, content: `Error: ${message}`, isError: true };
  }
}

/** Execute one of the 7 built-in tools. */
async function executeBuiltinTool(
  resources: LocalResources,
  toolName: string,
  args: Record<string, unknown>,
): Promise<string> {
  switch (toolName) {
    case "read_file":
      return resources.readFile(args.path as string);

    case "write_file":
      await resources.writeFile(args.path as string, args.content as string);
      return `File written: ${args.path}`;

    case "list_dir": {
      const entries = await resources.readDir(args.path as string);
      return entries.join("\n");
    }

    case "search_files": {
      const matches = await resources.glob(args.pattern as string);
      return matches.length > 0 ? matches.join("\n") : "No files found.";
    }

    case "run_command": {
      const result = await resources.exec(args.command as string);
      let content = result.stdout;
      if (result.stderr) content += `\nstderr: ${result.stderr}`;
      if (result.exitCode !== 0) content += `\nexit code: ${result.exitCode}`;
      return content;
    }

    case "git_status": {
      const content = await resources.gitStatus();
      return content.trim() ? content : "Working tree clean.";
    }

    case "git_diff": {
      const content = await resources.gitDiff(args.ref as string | undefined);
      return content.trim() ? content : "No changes.";
    }

    default:
      throw new Error(`Built-in tool dispatch reached unreachable case: ${toolName}`);
  }
}
