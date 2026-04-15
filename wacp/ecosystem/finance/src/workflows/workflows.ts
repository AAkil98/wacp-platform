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

export const FINANCE_WORKFLOWS: Workflow[] = [
  {
    id: "finance:trade-execution",
    name: "Trade Execution",
    description: "Analyze → compliance → execute → record",
    stages: [
      { id: "analyze", name: "Analyze", roleId: "finance:analyst", dependsOn: [], gated: false },
      { id: "compliance", name: "Compliance", roleId: "finance:compliance_officer", dependsOn: ["analyze"], gated: true },
      { id: "execute", name: "Execute", roleId: "finance:portfolio_manager", dependsOn: ["compliance"], gated: true },
      { id: "record", name: "Record", roleId: "finance:auditor", dependsOn: ["execute"], gated: false },
    ],
  },
  {
    id: "finance:portfolio-rebalance",
    name: "Portfolio Rebalance",
    description: "Assess → propose → compliance → execute",
    stages: [
      { id: "assess", name: "Assess", roleId: "finance:risk_officer", dependsOn: [], gated: false },
      { id: "propose", name: "Propose", roleId: "finance:portfolio_manager", dependsOn: ["assess"], gated: false },
      { id: "compliance", name: "Compliance", roleId: "finance:compliance_officer", dependsOn: ["propose"], gated: true },
      { id: "execute", name: "Execute", roleId: "finance:portfolio_manager", dependsOn: ["compliance"], gated: true },
    ],
  },
  {
    id: "finance:full-report",
    name: "Full Report",
    description: "Collect → analyze → risk → compliance → publish",
    stages: [
      { id: "collect", name: "Collect", roleId: "finance:analyst", dependsOn: [], gated: false },
      { id: "analyze", name: "Analyze", roleId: "finance:analyst", dependsOn: ["collect"], gated: false },
      { id: "risk", name: "Risk", roleId: "finance:risk_officer", dependsOn: ["analyze"], gated: false },
      { id: "compliance", name: "Compliance", roleId: "finance:compliance_officer", dependsOn: ["risk"], gated: true },
      { id: "publish", name: "Publish", roleId: "finance:auditor", dependsOn: ["compliance"], gated: true },
    ],
  },
  {
    id: "finance:client-onboarding",
    name: "Client Onboarding",
    description: "KYC → suitability → approve",
    stages: [
      { id: "kyc", name: "KYC", roleId: "finance:compliance_officer", dependsOn: [], gated: true },
      { id: "suitability", name: "Suitability", roleId: "finance:compliance_officer", dependsOn: ["kyc"], gated: true },
      { id: "approve", name: "Approve", roleId: "finance:portfolio_manager", dependsOn: ["suitability"], gated: true },
    ],
  },
];

/** Get a workflow by ID. */
export function getWorkflow(id: string): Workflow | undefined {
  return FINANCE_WORKFLOWS.find((w) => w.id === id);
}

/** Get all workflows. */
export function allWorkflows(): Workflow[] {
  return [...FINANCE_WORKFLOWS];
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
