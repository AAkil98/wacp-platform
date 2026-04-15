/** SWE role definition. */
export interface SweRole {
  id: string;
  name: string;
  extends: "worker" | "observer";
  concern: string;
  toolAccess: "read-only" | "read-write" | "read-write-test";
  autonomy: "gated" | "autonomous";
}

/** SWE task type definition. */
export interface SweTaskType {
  id: string;
  name: string;
  description: string;
  defaultWorkflow: string;
  roles: string[];
}

export const SWE_ROLES: SweRole[] = [
  {
    id: "swe:planner",
    name: "Planner",
    extends: "worker",
    concern: "Analyze requirements, produce plans, assign scope",
    toolAccess: "read-only",
    autonomy: "gated",
  },
  {
    id: "swe:implementer",
    name: "Implementer",
    extends: "worker",
    concern: "Write/edit code, produce diffs",
    toolAccess: "read-write",
    autonomy: "gated",
  },
  {
    id: "swe:tester",
    name: "Tester",
    extends: "worker",
    concern: "Write tests, run tests, report results",
    toolAccess: "read-write-test",
    autonomy: "gated",
  },
  {
    id: "swe:reviewer",
    name: "Reviewer",
    extends: "observer",
    concern: "Review changes, evaluate quality",
    toolAccess: "read-only",
    autonomy: "autonomous",
  },
];

export const SWE_TASK_TYPES: SweTaskType[] = [
  {
    id: "swe:implement",
    name: "Implement Feature",
    description: "Build new functionality",
    defaultWorkflow: "swe:implement-feature",
    roles: ["swe:planner", "swe:implementer", "swe:tester", "swe:reviewer"],
  },
  {
    id: "swe:refactor",
    name: "Refactor",
    description: "Restructure existing code without changing behavior",
    defaultWorkflow: "swe:refactor",
    roles: ["swe:planner", "swe:implementer", "swe:tester", "swe:reviewer"],
  },
  {
    id: "swe:debug",
    name: "Debug",
    description: "Find and fix a bug",
    defaultWorkflow: "swe:fix-bug",
    roles: ["swe:planner", "swe:implementer", "swe:tester"],
  },
  {
    id: "swe:test",
    name: "Write Tests",
    description: "Write or improve test coverage",
    defaultWorkflow: "swe:write-tests",
    roles: ["swe:planner", "swe:tester"],
  },
  {
    id: "swe:review",
    name: "Review",
    description: "Review existing changes for quality",
    defaultWorkflow: "swe:review-only",
    roles: ["swe:reviewer"],
  },
  {
    id: "swe:document",
    name: "Document",
    description: "Write or update documentation",
    defaultWorkflow: "swe:document-only",
    roles: ["swe:implementer"],
  },
  {
    id: "swe:investigate",
    name: "Investigate",
    description: "Research a question about the codebase",
    defaultWorkflow: "swe:investigate-only",
    roles: ["swe:planner"],
  },
];

/** Get a role by ID. */
export function getRole(id: string): SweRole | undefined {
  return SWE_ROLES.find((r) => r.id === id);
}

/** Get a task type by ID. */
export function getTaskType(id: string): SweTaskType | undefined {
  return SWE_TASK_TYPES.find((t) => t.id === id);
}
