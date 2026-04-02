/** A stage in a workflow. */
export interface WorkflowStage {
  id: string;
  name: string;
  roleId: string;
  dependsOn: string[];
  gated: boolean;
}

/** A workflow — ordered stages forming a DAG. */
export interface Workflow {
  id: string;
  name: string;
  description: string;
  stages: WorkflowStage[];
}

export const SWE_WORKFLOWS: Workflow[] = [
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
    description: "Plan → implement → test → review (behavior-preserving)",
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
    description: "Plan → implement → test (no review — urgency)",
    stages: [
      { id: "plan", name: "Plan", roleId: "swe:planner", dependsOn: [], gated: false },
      { id: "implement", name: "Implement", roleId: "swe:implementer", dependsOn: ["plan"], gated: true },
      { id: "test", name: "Test", roleId: "swe:tester", dependsOn: ["implement"], gated: false },
    ],
  },
  {
    id: "swe:write-tests",
    name: "Write Tests",
    description: "Plan → test (planner identifies gaps, tester fills them)",
    stages: [
      { id: "plan", name: "Plan", roleId: "swe:planner", dependsOn: [], gated: false },
      { id: "test", name: "Test", roleId: "swe:tester", dependsOn: ["plan"], gated: false },
    ],
  },
];

/** Get a workflow by ID. */
export function getWorkflow(id: string): Workflow | undefined {
  return SWE_WORKFLOWS.find((w) => w.id === id);
}

/** Get all workflows. */
export function allWorkflows(): Workflow[] {
  return [...SWE_WORKFLOWS];
}

/** Validate that a workflow's stages form a valid DAG (no cycles, deps exist). */
export function validateWorkflow(workflow: Workflow): string[] {
  const errors: string[] = [];
  const stageIds = new Set(workflow.stages.map((s) => s.id));

  for (const stage of workflow.stages) {
    for (const dep of stage.dependsOn) {
      if (!stageIds.has(dep)) {
        errors.push(`Stage '${stage.id}' depends on non-existent stage '${dep}'`);
      }
    }
  }

  // Simple cycle detection via topological sort
  const visited = new Set<string>();
  const visiting = new Set<string>();

  function visit(id: string): boolean {
    if (visiting.has(id)) return false; // cycle
    if (visited.has(id)) return true;
    visiting.add(id);
    const stage = workflow.stages.find((s) => s.id === id);
    if (stage) {
      for (const dep of stage.dependsOn) {
        if (!visit(dep)) {
          errors.push(`Cycle detected involving stage '${id}'`);
          return false;
        }
      }
    }
    visiting.delete(id);
    visited.add(id);
    return true;
  }

  for (const stage of workflow.stages) {
    visit(stage.id);
  }

  return errors;
}
