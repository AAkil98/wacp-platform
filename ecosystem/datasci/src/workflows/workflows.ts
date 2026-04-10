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

export const DATASCI_WORKFLOWS: Workflow[] = [
  {
    id: "datasci:full-analysis",
    name: "Full Analysis",
    description: "Explore → hypothesize → test → interpret → review",
    stages: [
      { id: "explore", name: "Explore", roleId: "datasci:explorer", dependsOn: [], gated: false },
      { id: "hypothesize", name: "Hypothesize", roleId: "datasci:statistician", dependsOn: ["explore"], gated: false },
      { id: "test", name: "Test", roleId: "datasci:statistician", dependsOn: ["hypothesize"], gated: true },
      { id: "interpret", name: "Interpret", roleId: "datasci:statistician", dependsOn: ["test"], gated: false },
      { id: "review", name: "Review", roleId: "datasci:reviewer", dependsOn: ["interpret"], gated: true },
    ],
  },
  {
    id: "datasci:hypothesis-test",
    name: "Hypothesis Test",
    description: "Declare → test → interpret",
    stages: [
      { id: "declare", name: "Declare", roleId: "datasci:statistician", dependsOn: [], gated: false },
      { id: "test", name: "Test", roleId: "datasci:statistician", dependsOn: ["declare"], gated: true },
      { id: "interpret", name: "Interpret", roleId: "datasci:statistician", dependsOn: ["test"], gated: false },
    ],
  },
  {
    id: "datasci:model-build",
    name: "Model Build",
    description: "Explore → feature → fit → validate",
    stages: [
      { id: "explore", name: "Explore", roleId: "datasci:explorer", dependsOn: [], gated: false },
      { id: "feature", name: "Feature", roleId: "datasci:feature_engineer", dependsOn: ["explore"], gated: false },
      { id: "fit", name: "Fit", roleId: "datasci:modeler", dependsOn: ["feature"], gated: true },
      { id: "validate", name: "Validate", roleId: "datasci:reviewer", dependsOn: ["fit"], gated: false },
    ],
  },
  {
    id: "datasci:exploration",
    name: "Exploration",
    description: "Profile → visualize",
    stages: [
      { id: "profile", name: "Profile", roleId: "datasci:explorer", dependsOn: [], gated: false },
      { id: "visualize", name: "Visualize", roleId: "datasci:explorer", dependsOn: ["profile"], gated: false },
    ],
  },
];

/** Get a workflow by ID. */
export function getWorkflow(id: string): Workflow | undefined {
  return DATASCI_WORKFLOWS.find((w) => w.id === id);
}

/** Get all workflows. */
export function allWorkflows(): Workflow[] {
  return [...DATASCI_WORKFLOWS];
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
