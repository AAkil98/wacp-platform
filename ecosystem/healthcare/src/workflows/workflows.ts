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

export const HEALTHCARE_WORKFLOWS: Workflow[] = [
  {
    id: "health:patient-assessment",
    name: "Patient Assessment",
    description: "Consent → gather → analyze → report",
    stages: [
      { id: "consent", name: "Consent", roleId: "health:clinician", dependsOn: [], gated: true },
      { id: "gather", name: "Gather", roleId: "health:clinician", dependsOn: ["consent"], gated: true },
      { id: "analyze", name: "Analyze", roleId: "health:clinician", dependsOn: ["gather"], gated: true },
      { id: "report", name: "Report", roleId: "health:clinician", dependsOn: ["analyze"], gated: true },
    ],
  },
  {
    id: "health:literature-research",
    name: "Literature Research",
    description: "Question → search → synthesize → review",
    stages: [
      { id: "question", name: "Question", roleId: "health:researcher", dependsOn: [], gated: false },
      { id: "search", name: "Search", roleId: "health:researcher", dependsOn: ["question"], gated: false },
      { id: "synthesize", name: "Synthesize", roleId: "health:researcher", dependsOn: ["search"], gated: false },
      { id: "review", name: "Review", roleId: "health:compliance", dependsOn: ["synthesize"], gated: true },
    ],
  },
  {
    id: "health:phi-audit",
    name: "PHI Audit",
    description: "Scan → report",
    stages: [
      { id: "scan", name: "Scan", roleId: "health:compliance", dependsOn: [], gated: false },
      { id: "report", name: "Report", roleId: "health:compliance", dependsOn: ["scan"], gated: false },
    ],
  },
  {
    id: "health:patient-education",
    name: "Patient Education",
    description: "Assess level → generate",
    stages: [
      { id: "assess_level", name: "Assess Level", roleId: "health:coordinator", dependsOn: [], gated: true },
      { id: "generate", name: "Generate", roleId: "health:coordinator", dependsOn: ["assess_level"], gated: true },
    ],
  },
];

/** Get a workflow by ID. */
export function getWorkflow(id: string): Workflow | undefined {
  return HEALTHCARE_WORKFLOWS.find((w) => w.id === id);
}

/** Get all workflows. */
export function allWorkflows(): Workflow[] {
  return [...HEALTHCARE_WORKFLOWS];
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
