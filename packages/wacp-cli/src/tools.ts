import type { LocalResources, AutonomyManager } from "@wacp/local";
import { AutonomyError } from "@wacp/local";

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

/** Build tool definitions for the LLM. */
export function buildToolDefinitions(): ToolDefinition[] {
  return [
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
}

/** Execute a tool call using local resources. Returns the result as a tool message. */
export async function executeTool(
  resources: LocalResources,
  autonomy: AutonomyManager,
  toolName: string,
  toolCallId: string,
  args: Record<string, unknown>,
): Promise<ToolResult> {
  try {
    let content: string;

    switch (toolName) {
      case "read_file":
        content = await resources.readFile(args.path as string);
        break;

      case "write_file":
        await resources.writeFile(args.path as string, args.content as string);
        content = `File written: ${args.path}`;
        break;

      case "list_dir":
        const entries = await resources.readDir(args.path as string);
        content = entries.join("\n");
        break;

      case "search_files":
        const matches = await resources.glob(args.pattern as string);
        content = matches.length > 0 ? matches.join("\n") : "No files found.";
        break;

      case "run_command":
        const result = await resources.exec(args.command as string);
        content = result.stdout;
        if (result.stderr) content += `\nstderr: ${result.stderr}`;
        if (result.exitCode !== 0) content += `\nexit code: ${result.exitCode}`;
        break;

      case "git_status":
        content = await resources.gitStatus();
        if (!content.trim()) content = "Working tree clean.";
        break;

      case "git_diff":
        content = await resources.gitDiff(args.ref as string | undefined);
        if (!content.trim()) content = "No changes.";
        break;

      default:
        return { toolCallId, content: `Unknown tool: ${toolName}`, isError: true };
    }

    return { toolCallId, content, isError: false };
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
