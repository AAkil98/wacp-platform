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

export const MLOPS_WORKFLOWS: Workflow[] = [
  {
    id: "mlops:experiment",
    name: "Experiment",
    description: "Design → data-prep → train → evaluate",
    stages: [
      { id: "design", name: "Design", roleId: "mlops:researcher", dependsOn: [], gated: false },
      { id: "data-prep", name: "Data Prep", roleId: "mlops:researcher", dependsOn: ["design"], gated: false },
      { id: "train", name: "Train", roleId: "mlops:trainer", dependsOn: ["data-prep"], gated: true },
      { id: "evaluate", name: "Evaluate", roleId: "mlops:evaluator", dependsOn: ["train"], gated: false },
    ],
  },
  {
    id: "mlops:deploy",
    name: "Deploy",
    description: "Validate → deploy → monitor",
    stages: [
      { id: "validate", name: "Validate", roleId: "mlops:evaluator", dependsOn: [], gated: false },
      { id: "deploy", name: "Deploy", roleId: "mlops:deployer", dependsOn: ["validate"], gated: true },
      { id: "monitor", name: "Monitor", roleId: "mlops:monitor", dependsOn: ["deploy"], gated: false },
    ],
  },
  {
    id: "mlops:reproduce",
    name: "Reproduce",
    description: "Verify → execute",
    stages: [
      { id: "verify", name: "Verify", roleId: "mlops:researcher", dependsOn: [], gated: false },
      { id: "execute", name: "Execute", roleId: "mlops:trainer", dependsOn: ["verify"], gated: false },
    ],
  },
  {
    id: "mlops:optimize",
    name: "Optimize",
    description: "Analyze → propose → train",
    stages: [
      { id: "analyze", name: "Analyze", roleId: "mlops:evaluator", dependsOn: [], gated: false },
      { id: "propose", name: "Propose", roleId: "mlops:researcher", dependsOn: ["analyze"], gated: false },
      { id: "train", name: "Train", roleId: "mlops:trainer", dependsOn: ["propose"], gated: true },
    ],
  },
];

/** Get a workflow by ID. */
export function getWorkflow(id: string): Workflow | undefined {
  return MLOPS_WORKFLOWS.find((w) => w.id === id);
}

/** Get all workflows. */
export function allWorkflows(): Workflow[] {
  return [...MLOPS_WORKFLOWS];
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

  // Cycle detection via topological sort
  const visited = new Set<string>();
  const visiting = new Set<string>();

  function visit(id: string): boolean {
    if (visiting.has(id)) return false;
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
