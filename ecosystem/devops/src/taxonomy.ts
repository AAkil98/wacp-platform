/** Environment tiers — gate strictness scales with tier. */
export type EnvironmentTier = "dev" | "staging" | "production";

export const ENVIRONMENT_TIERS: EnvironmentTier[] = ["dev", "staging", "production"];

/** DevOps role definition. */
export interface DevopsRole {
  id: string;
  name: string;
  extends: "worker" | "observer";
  concern: string;
  toolAccess: "read-plan" | "execute" | "read-query" | "execute-query" | "read-scan";
  autonomy: "gated" | "autonomous";
}

/** DevOps task type definition. */
export interface DevopsTaskType {
  id: string;
  name: string;
  description: string;
  defaultWorkflow: string;
  roles: string[];
}

export const DEVOPS_ROLES: DevopsRole[] = [
  {
    id: "devops:architect",
    name: "Architect",
    extends: "worker",
    concern: "Design infrastructure, write IaC, plan changes",
    toolAccess: "read-plan",
    autonomy: "gated",
  },
  {
    id: "devops:deployer",
    name: "Deployer",
    extends: "worker",
    concern: "Execute deployments, manage rollbacks, verify health",
    toolAccess: "execute",
    autonomy: "gated",
  },
  {
    id: "devops:monitor",
    name: "Monitor",
    extends: "observer",
    concern: "Observe system health, query metrics/logs, manage alerts",
    toolAccess: "read-query",
    autonomy: "autonomous",
  },
  {
    id: "devops:responder",
    name: "Responder",
    extends: "worker",
    concern: "Triage incidents, perform mitigation, contain blast radius",
    toolAccess: "execute-query",
    autonomy: "gated",
  },
  {
    id: "devops:auditor",
    name: "Auditor",
    extends: "observer",
    concern: "Review compliance, security posture, policy adherence",
    toolAccess: "read-scan",
    autonomy: "autonomous",
  },
];

export const DEVOPS_TASK_TYPES: DevopsTaskType[] = [
  {
    id: "devops:provision",
    name: "Provision",
    description: "Set up new infrastructure",
    defaultWorkflow: "devops:provision",
    roles: ["devops:architect", "devops:deployer", "devops:auditor"],
  },
  {
    id: "devops:deploy",
    name: "Deploy",
    description: "Deploy application changes",
    defaultWorkflow: "devops:deploy",
    roles: ["devops:architect", "devops:auditor", "devops:deployer", "devops:monitor"],
  },
  {
    id: "devops:monitor",
    name: "Monitor",
    description: "Set up or review monitoring",
    defaultWorkflow: "devops:monitor-only",
    roles: ["devops:monitor"],
  },
  {
    id: "devops:respond",
    name: "Respond",
    description: "Incident response",
    defaultWorkflow: "devops:respond",
    roles: ["devops:responder", "devops:architect"],
  },
  {
    id: "devops:audit",
    name: "Audit",
    description: "Compliance/security audit",
    defaultWorkflow: "devops:audit",
    roles: ["devops:auditor"],
  },
  {
    id: "devops:migrate",
    name: "Migrate",
    description: "Migrate infrastructure or services",
    defaultWorkflow: "devops:deploy",
    roles: ["devops:architect", "devops:auditor", "devops:deployer", "devops:monitor"],
  },
  {
    id: "devops:configure",
    name: "Configure",
    description: "Configuration management",
    defaultWorkflow: "devops:provision",
    roles: ["devops:architect", "devops:deployer", "devops:auditor"],
  },
  {
    id: "devops:secure",
    name: "Secure",
    description: "Security hardening",
    defaultWorkflow: "devops:secure",
    roles: ["devops:auditor", "devops:architect"],
  },
  {
    id: "devops:optimize",
    name: "Optimize",
    description: "Performance/cost optimization",
    defaultWorkflow: "devops:optimize",
    roles: ["devops:monitor", "devops:architect", "devops:auditor"],
  },
];

/** Get a role by ID. */
export function getRole(id: string): DevopsRole | undefined {
  return DEVOPS_ROLES.find((r) => r.id === id);
}

/** Get a task type by ID. */
export function getTaskType(id: string): DevopsTaskType | undefined {
  return DEVOPS_TASK_TYPES.find((t) => t.id === id);
}

/** Check if an environment tier requires human approval for mutations. */
export function requiresHumanApproval(tier: EnvironmentTier): boolean {
  return tier === "production";
}
