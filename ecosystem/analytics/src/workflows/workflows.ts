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

export const ANALYTICS_WORKFLOWS: Workflow[] = [
  {
    id: "analytics:query-and-report",
    name: "Query and Report",
    description: "Query → validate → report",
    stages: [
      { id: "query", name: "Query", roleId: "analytics:analyst", dependsOn: [], gated: false },
      { id: "validate", name: "Validate", roleId: "analytics:validator", dependsOn: ["query"], gated: false },
      { id: "report", name: "Report", roleId: "analytics:reporter", dependsOn: ["validate"], gated: true },
    ],
  },
  {
    id: "analytics:build-dashboard",
    name: "Build Dashboard",
    description: "Model → build → validate",
    stages: [
      { id: "model", name: "Model", roleId: "analytics:modeler", dependsOn: [], gated: false },
      { id: "build", name: "Build", roleId: "analytics:reporter", dependsOn: ["model"], gated: true },
      { id: "validate", name: "Validate", roleId: "analytics:validator", dependsOn: ["build"], gated: false },
    ],
  },
  {
    id: "analytics:investigate",
    name: "Investigate",
    description: "Explore → reconcile → report",
    stages: [
      { id: "explore", name: "Explore", roleId: "analytics:analyst", dependsOn: [], gated: false },
      { id: "reconcile", name: "Reconcile", roleId: "analytics:validator", dependsOn: ["explore"], gated: false },
      { id: "report", name: "Report", roleId: "analytics:insights", dependsOn: ["reconcile"], gated: false },
    ],
  },
  {
    id: "analytics:model-data",
    name: "Model Data",
    description: "Explore → model → validate",
    stages: [
      { id: "explore", name: "Explore", roleId: "analytics:analyst", dependsOn: [], gated: false },
      { id: "model", name: "Model", roleId: "analytics:modeler", dependsOn: ["explore"], gated: true },
      { id: "validate", name: "Validate", roleId: "analytics:validator", dependsOn: ["model"], gated: false },
    ],
  },
];

/** Get a workflow by ID. */
export function getWorkflow(id: string): Workflow | undefined {
  return ANALYTICS_WORKFLOWS.find((w) => w.id === id);
}

/** Get all workflows. */
export function allWorkflows(): Workflow[] {
  return [...ANALYTICS_WORKFLOWS];
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
