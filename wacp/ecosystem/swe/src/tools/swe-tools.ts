import type { LocalResources } from "@wacp/local";

/** Tool definition for LLM function-calling. */
export interface ToolDefinition {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

/** SWE-specific tool definitions beyond the CLI's built-in 7. */
export function sweToolDefinitions(): ToolDefinition[] {
  return [
    {
      name: "code_search",
      description: "Search the codebase for a pattern (regex supported). Returns matching lines with file paths and line numbers.",
      input_schema: {
        type: "object",
        properties: {
          pattern: { type: "string", description: "Search pattern (regex)" },
          path: { type: "string", description: "Directory to search in (default: '.')" },
          file_type: { type: "string", description: "File extension filter (e.g., 'ts', 'py', 'rs')" },
        },
        required: ["pattern"],
      },
    },
    {
      name: "code_edit",
      description: "Apply a targeted edit to a file by replacing an exact string match. Use this instead of write_file when making small, precise changes.",
      input_schema: {
        type: "object",
        properties: {
          path: { type: "string", description: "File path" },
          old_text: { type: "string", description: "Exact text to find and replace" },
          new_text: { type: "string", description: "Replacement text" },
        },
        required: ["path", "old_text", "new_text"],
      },
    },
    {
      name: "test_run",
      description: "Run the test suite or a specific test file. Detects the test framework from the project (vitest, jest, pytest, cargo test, etc.).",
      input_schema: {
        type: "object",
        properties: {
          command: { type: "string", description: "Explicit test command (overrides auto-detection)" },
          file: { type: "string", description: "Specific test file to run" },
          filter: { type: "string", description: "Test name filter pattern" },
        },
      },
    },
    {
      name: "lint_check",
      description: "Run the project linter. Detects the linter from config files (eslint, biome, ruff, clippy, etc.).",
      input_schema: {
        type: "object",
        properties: {
          path: { type: "string", description: "Path to lint (default: '.')" },
          fix: { type: "boolean", description: "Auto-fix issues if supported" },
        },
      },
    },
    {
      name: "type_check",
      description: "Run the type checker (tsc, mypy, cargo check, etc.). Returns type errors if any.",
      input_schema: {
        type: "object",
        properties: {
          path: { type: "string", description: "Path to check (default: '.')" },
        },
      },
    },
    {
      name: "git_branch",
      description: "Create a new git branch or switch to an existing one.",
      input_schema: {
        type: "object",
        properties: {
          name: { type: "string", description: "Branch name" },
          create: { type: "boolean", description: "Create the branch if it doesn't exist (default: true)" },
        },
        required: ["name"],
      },
    },
    {
      name: "git_commit",
      description: "Stage specified files (or all changes) and create a git commit.",
      input_schema: {
        type: "object",
        properties: {
          message: { type: "string", description: "Commit message" },
          paths: {
            type: "array",
            items: { type: "string" },
            description: "Files to stage (default: all modified files)",
          },
        },
        required: ["message"],
      },
    },
    {
      name: "dependency_check",
      description: "Check for outdated or vulnerable dependencies in the project.",
      input_schema: {
        type: "object",
        properties: {},
      },
    },
  ];
}

/** Execute a SWE tool using local resources. */
export async function executeSweTools(
  resources: LocalResources,
  toolName: string,
  args: Record<string, unknown>,
): Promise<{ content: string; isError: boolean }> {
  try {
    switch (toolName) {
      case "code_search": {
        const pattern = args.pattern as string;
        const path = (args.path as string) ?? ".";
        const fileType = args.file_type as string | undefined;
        let cmd = `grep -rn "${pattern.replace(/"/g, '\\"')}" ${path}`;
        if (fileType) cmd += ` --include="*.${fileType}"`;
        cmd += " 2>/dev/null || true";
        const result = await resources.exec(cmd);
        return { content: result.stdout || "No matches found.", isError: false };
      }
      case "code_edit": {
        const filePath = args.path as string;
        const oldText = args.old_text as string;
        const newText = args.new_text as string;
        const content = await resources.readFile(filePath);
        if (!content.includes(oldText)) {
          return { content: `Text not found in ${filePath}`, isError: true };
        }
        const updated = content.replace(oldText, newText);
        await resources.writeFile(filePath, updated);
        return { content: `Edited ${filePath}: replaced ${oldText.length} chars`, isError: false };
      }
      case "test_run": {
        const cmd = (args.command as string)
          ?? detectTestCommand(args.file as string | undefined, args.filter as string | undefined);
        const result = await resources.exec(cmd, { timeout: 120_000 });
        let output = result.stdout;
        if (result.stderr) output += `\n${result.stderr}`;
        if (result.exitCode !== 0) output += `\nexit code: ${result.exitCode}`;
        return { content: output, isError: result.exitCode !== 0 };
      }
      case "lint_check": {
        const fix = (args.fix as boolean) ?? false;
        const cmd = detectLintCommand(args.path as string | undefined, fix);
        const result = await resources.exec(cmd);
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "type_check": {
        const cmd = detectTypeCheckCommand(args.path as string | undefined);
        const result = await resources.exec(cmd);
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "git_branch": {
        const name = args.name as string;
        const create = (args.create as boolean) ?? true;
        const cmd = create ? `git checkout -b "${name}" 2>&1 || git checkout "${name}"` : `git checkout "${name}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "git_commit": {
        const message = args.message as string;
        const paths = args.paths as string[] | undefined;
        const stageCmd = paths?.length ? `git add ${paths.map(p => `"${p}"`).join(" ")}` : "git add -A";
        await resources.exec(stageCmd);
        const result = await resources.exec(`git commit -m "${message.replace(/"/g, '\\"')}"`);
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "dependency_check": {
        const result = await resources.exec("npm outdated 2>/dev/null || pip list --outdated 2>/dev/null || cargo outdated 2>/dev/null || echo 'No package manager detected'");
        return { content: result.stdout, isError: false };
      }
      default:
        return { content: `Unknown SWE tool: ${toolName}`, isError: true };
    }
  } catch (err) {
    return { content: `Error: ${(err as Error).message}`, isError: true };
  }
}

function detectTestCommand(file?: string, filter?: string): string {
  // Simple heuristic — can be extended
  let cmd = "npm test 2>/dev/null || pytest 2>/dev/null || cargo test 2>/dev/null";
  if (file) cmd = `npx vitest run ${file} 2>/dev/null || pytest ${file} 2>/dev/null`;
  if (filter) cmd += ` -t "${filter}" 2>/dev/null || --filter "${filter}"`;
  return cmd;
}

function detectLintCommand(path?: string, fix?: boolean): string {
  const target = path ?? ".";
  const fixFlag = fix ? " --fix" : "";
  return `npx eslint ${target}${fixFlag} 2>/dev/null || npx biome check ${target} 2>/dev/null || ruff check ${target}${fixFlag} 2>/dev/null || echo "No linter found"`;
}

function detectTypeCheckCommand(path?: string): string {
  return `npx tsc --noEmit 2>/dev/null || mypy ${path ?? "."} 2>/dev/null || cargo check 2>/dev/null || echo "No type checker found"`;
}
