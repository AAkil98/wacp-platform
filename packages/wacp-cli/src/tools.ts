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
    // --- SWE-specific tools ---
    {
      name: "code_search",
      description: "Search the codebase for a pattern (regex supported). Returns matching lines with file paths.",
      input_schema: {
        type: "object",
        properties: {
          pattern: { type: "string", description: "Search pattern (regex)" },
          path: { type: "string", description: "Directory to search (default: '.')" },
          file_type: { type: "string", description: "File extension filter (e.g., 'ts', 'py')" },
        },
        required: ["pattern"],
      },
    },
    {
      name: "code_edit",
      description: "Apply a targeted edit to a file by replacing exact text. Preferred over write_file for small changes.",
      input_schema: {
        type: "object",
        properties: {
          path: { type: "string", description: "File path" },
          old_text: { type: "string", description: "Exact text to find" },
          new_text: { type: "string", description: "Replacement text" },
        },
        required: ["path", "old_text", "new_text"],
      },
    },
    {
      name: "test_run",
      description: "Run the project's test suite or a specific test file.",
      input_schema: {
        type: "object",
        properties: {
          command: { type: "string", description: "Explicit test command (overrides auto-detection)" },
          file: { type: "string", description: "Specific test file to run" },
        },
      },
    },
    {
      name: "type_check",
      description: "Run the type checker (tsc, mypy, cargo check).",
      input_schema: {
        type: "object",
        properties: {},
      },
    },
    {
      name: "lint_check",
      description: "Run the project linter.",
      input_schema: {
        type: "object",
        properties: {
          fix: { type: "boolean", description: "Auto-fix if supported" },
        },
      },
    },
    {
      name: "git_branch",
      description: "Create or switch to a git branch.",
      input_schema: {
        type: "object",
        properties: {
          name: { type: "string", description: "Branch name" },
          create: { type: "boolean", description: "Create if doesn't exist (default: true)" },
        },
        required: ["name"],
      },
    },
    {
      name: "git_commit",
      description: "Stage files and create a git commit.",
      input_schema: {
        type: "object",
        properties: {
          message: { type: "string", description: "Commit message" },
          paths: { type: "array", items: { type: "string" }, description: "Files to stage (default: all)" },
        },
        required: ["message"],
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

      // --- SWE tools ---
      case "code_search": {
        const pattern = args.pattern as string;
        const searchPath = (args.path as string) ?? ".";
        const fileType = args.file_type as string | undefined;
        let cmd = `grep -rn "${pattern.replace(/"/g, '\\"')}" ${searchPath}`;
        if (fileType) cmd += ` --include="*.${fileType}"`;
        cmd += " 2>/dev/null || true";
        const sr = await resources.exec(cmd);
        content = sr.stdout || "No matches found.";
        break;
      }
      case "code_edit": {
        const filePath = args.path as string;
        const oldText = args.old_text as string;
        const newText = args.new_text as string;
        const fileContent = await resources.readFile(filePath);
        if (!fileContent.includes(oldText)) {
          return { toolCallId, content: `Text not found in ${filePath}`, isError: true };
        }
        await resources.writeFile(filePath, fileContent.replace(oldText, newText));
        content = `Edited ${filePath}`;
        break;
      }
      case "test_run": {
        const testCmd = (args.command as string) ?? (args.file ? `npx vitest run ${args.file}` : "npm test");
        const tr = await resources.exec(testCmd, { timeout: 120_000 });
        content = tr.stdout;
        if (tr.stderr) content += `\n${tr.stderr}`;
        if (tr.exitCode !== 0) content += `\nexit code: ${tr.exitCode}`;
        break;
      }
      case "type_check": {
        const tc = await resources.exec("npx tsc --noEmit 2>&1 || true");
        content = tc.stdout;
        break;
      }
      case "lint_check": {
        const fix = (args.fix as boolean) ? " --fix" : "";
        const lc = await resources.exec(`npx eslint .${fix} 2>&1 || true`);
        content = lc.stdout;
        break;
      }
      case "git_branch": {
        const branchName = args.name as string;
        const create = (args.create as boolean) ?? true;
        const bc = create
          ? await resources.exec(`git checkout -b "${branchName}" 2>&1 || git checkout "${branchName}"`)
          : await resources.exec(`git checkout "${branchName}"`);
        content = bc.stdout + bc.stderr;
        break;
      }
      case "git_commit": {
        const msg = args.message as string;
        const paths = args.paths as string[] | undefined;
        const stageCmd = paths?.length ? `git add ${paths.map(p => `"${p}"`).join(" ")}` : "git add -A";
        await resources.exec(stageCmd);
        const gc = await resources.exec(`git commit -m "${msg.replace(/"/g, '\\"')}"`);
        content = gc.stdout + gc.stderr;
        break;
      }

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
