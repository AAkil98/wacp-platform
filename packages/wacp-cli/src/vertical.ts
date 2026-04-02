import type { LoadedVertical } from "./repl.js";
import type { Workflow, AgentProfile } from "@wacp/local";

/**
 * Load the SWE vertical — workflows + profiles.
 *
 * Defined inline (not imported from @wacp/swe) to avoid circular
 * dependency. The definitions match ecosystem/swe/ exactly.
 */
export function loadSweVertical(): LoadedVertical {
  return {
    workflows: SWE_WORKFLOWS,
    profiles: SWE_PROFILES,
  };
}

// ---------------------------------------------------------------------------
// SWE workflows (matches ecosystem/swe/src/workflows/workflows.ts)
// ---------------------------------------------------------------------------

const SWE_WORKFLOWS: Workflow[] = [
  {
    id: "swe:implement-feature",
    name: "Implement Feature",
    description: "Plan → implement → test → review",
    stages: [
      { id: "plan", name: "Plan", roleId: "swe:planner", dependsOn: [], gated: false },
      { id: "implement", name: "Implement", roleId: "swe:implementer", dependsOn: ["plan"], gated: true },
      { id: "test", name: "Test", roleId: "swe:tester", dependsOn: ["implement"], gated: false },
      { id: "review", name: "Review", roleId: "swe:reviewer", dependsOn: ["test"], gated: true },
    ],
  },
  {
    id: "swe:refactor",
    name: "Refactor",
    description: "Plan → implement → test → review",
    stages: [
      { id: "plan", name: "Plan", roleId: "swe:planner", dependsOn: [], gated: false },
      { id: "implement", name: "Implement", roleId: "swe:implementer", dependsOn: ["plan"], gated: true },
      { id: "test", name: "Test", roleId: "swe:tester", dependsOn: ["implement"], gated: false },
      { id: "review", name: "Review", roleId: "swe:reviewer", dependsOn: ["test"], gated: true },
    ],
  },
  {
    id: "swe:fix-bug",
    name: "Fix Bug",
    description: "Plan → implement → test",
    stages: [
      { id: "plan", name: "Plan", roleId: "swe:planner", dependsOn: [], gated: false },
      { id: "implement", name: "Implement", roleId: "swe:implementer", dependsOn: ["plan"], gated: true },
      { id: "test", name: "Test", roleId: "swe:tester", dependsOn: ["implement"], gated: false },
    ],
  },
  {
    id: "swe:write-tests",
    name: "Write Tests",
    description: "Plan → test",
    stages: [
      { id: "plan", name: "Plan", roleId: "swe:planner", dependsOn: [], gated: false },
      { id: "test", name: "Test", roleId: "swe:tester", dependsOn: ["plan"], gated: false },
    ],
  },
];

// ---------------------------------------------------------------------------
// SWE profiles (matches ecosystem/swe/src/profiles/profiles.ts)
// ---------------------------------------------------------------------------

const SWE_PROFILES: AgentProfile[] = [
  {
    roleId: "swe:planner",
    systemPrompt: `You are a software planning agent. Analyze the codebase, understand the requirements, and produce a structured implementation plan.

You have READ-ONLY access — do not modify any files.

Your plan should include:
1. Current state analysis — what exists, what's relevant
2. Files that need to be created or modified
3. Ordered steps with clear descriptions
4. Potential risks and edge cases
5. Test strategy

Output your plan in clear markdown with numbered steps.`,
    tools: ["read_file", "list_dir", "search_files", "code_search", "git_status", "git_diff"],
    autonomy: "gated",
  },
  {
    roleId: "swe:implementer",
    systemPrompt: `You are a software implementation agent. Write clean, correct code that follows existing project conventions.

Guidelines:
- Make minimal, focused changes — don't touch unrelated code
- Follow existing code style (naming, formatting, patterns)
- Write self-documenting code with clear variable names
- Use code_edit for small changes, write_file for new files
- Run the code after changes to verify it works
- Commit working changes with descriptive messages

If you encounter an error, fix it before moving on. Do not leave broken code.`,
    tools: [
      "read_file", "write_file", "list_dir", "search_files", "code_search",
      "code_edit", "run_command", "git_status", "git_diff", "git_branch",
      "git_commit", "test_run", "type_check",
    ],
    autonomy: "gated",
  },
  {
    roleId: "swe:tester",
    systemPrompt: `You are a software testing agent. Write and run tests that verify implementation correctness.

Guidelines:
- Test behavior through public interfaces, not implementation details
- Cover the happy path first, then edge cases and error paths
- Run existing tests to check for regressions
- Each test should have a clear, descriptive name
- Use the project's existing test framework and conventions
- Report results clearly: X passed, Y failed, with failure details

Do not modify the implementation code — only test files.`,
    tools: [
      "read_file", "write_file", "list_dir", "search_files", "code_search",
      "code_edit", "test_run", "run_command", "git_status", "git_diff",
    ],
    autonomy: "gated",
  },
  {
    roleId: "swe:reviewer",
    systemPrompt: `You are a code review agent. Evaluate changes for correctness, style, and design quality.

Review the diff and provide:
1. **Summary** — What the changes do (1-2 sentences)
2. **Issues** — Problems found, categorized:
   - Critical: bugs, security issues, data loss risks
   - Important: style violations, missing tests, poor naming
   - Suggestion: optional improvements
3. **Verdict** — APPROVE or REQUEST_CHANGES

You have READ-ONLY access. Do not modify any files.`,
    tools: ["read_file", "list_dir", "search_files", "code_search", "git_diff", "test_run"],
    autonomy: "autonomous",
  },
];
