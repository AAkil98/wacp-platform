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

export const DEVOPS_WORKFLOWS: Workflow[] = [
  {
    id: "devops:provision",
    name: "Provision",
    description: "Plan → execute → verify",
    stages: [
      { id: "plan", name: "Plan", roleId: "devops:architect", dependsOn: [], gated: false },
      { id: "execute", name: "Execute", roleId: "devops:deployer", dependsOn: ["plan"], gated: true },
      { id: "verify", name: "Verify", roleId: "devops:auditor", dependsOn: ["execute"], gated: false },
    ],
  },
  {
    id: "devops:deploy",
    name: "Deploy",
    description: "Plan → validate → execute → verify",
    stages: [
      { id: "plan", name: "Plan", roleId: "devops:architect", dependsOn: [], gated: false },
      { id: "validate", name: "Validate", roleId: "devops:auditor", dependsOn: ["plan"], gated: false },
      { id: "execute", name: "Execute", roleId: "devops:deployer", dependsOn: ["validate"], gated: true },
      { id: "verify", name: "Verify", roleId: "devops:monitor", dependsOn: ["execute"], gated: false },
    ],
  },
  {
    id: "devops:respond",
    name: "Respond",
    description: "Triage → mitigate → postmortem",
    stages: [
      { id: "triage", name: "Triage", roleId: "devops:responder", dependsOn: [], gated: false },
      { id: "mitigate", name: "Mitigate", roleId: "devops:responder", dependsOn: ["triage"], gated: true },
      { id: "postmortem", name: "Postmortem", roleId: "devops:architect", dependsOn: ["mitigate"], gated: false },
    ],
  },
  {
    id: "devops:secure",
    name: "Secure",
    description: "Scan → remediate → verify",
    stages: [
      { id: "scan", name: "Scan", roleId: "devops:auditor", dependsOn: [], gated: false },
      { id: "remediate", name: "Remediate", roleId: "devops:architect", dependsOn: ["scan"], gated: true },
      { id: "verify", name: "Verify", roleId: "devops:auditor", dependsOn: ["remediate"], gated: false },
    ],
  },
  {
    id: "devops:optimize",
    name: "Optimize",
    description: "Analyze → propose → validate",
    stages: [
      { id: "analyze", name: "Analyze", roleId: "devops:monitor", dependsOn: [], gated: false },
      { id: "propose", name: "Propose", roleId: "devops:architect", dependsOn: ["analyze"], gated: false },
      { id: "validate", name: "Validate", roleId: "devops:auditor", dependsOn: ["propose"], gated: false },
    ],
  },
];

/** Get a workflow by ID. */
export function getWorkflow(id: string): Workflow | undefined {
  return DEVOPS_WORKFLOWS.find((w) => w.id === id);
}

/** Get all workflows. */
export function allWorkflows(): Workflow[] {
  return [...DEVOPS_WORKFLOWS];
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
