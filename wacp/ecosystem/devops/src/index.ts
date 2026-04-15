import { devopsToolDefinitions, executeDevopsTools } from "./tools/devops-tools.js";
import { DEVOPS_PROFILES } from "./profiles/profiles.js";
import { DEVOPS_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";
import { DEVOPS_QUALITY_DIMENSIONS } from "./quality/quality.js";

export { DEVOPS_ROLES, DEVOPS_TASK_TYPES, ENVIRONMENT_TIERS, getRole, getTaskType, requiresHumanApproval, type DevopsRole, type DevopsTaskType, type EnvironmentTier } from "./taxonomy.js";
export { devopsToolDefinitions, executeDevopsTools, type ToolDefinition } from "./tools/devops-tools.js";
export { DEVOPS_PROFILES, getProfile, allProfiles, type AgentProfile } from "./profiles/profiles.js";
export { DEVOPS_WORKFLOWS, getWorkflow, allWorkflows, validateWorkflow, type Workflow, type WorkflowStage } from "./workflows/workflows.js";
export { DEVOPS_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type QualityDimension, type QualityLevel, type QualityReport, type EvaluationContext } from "./quality/quality.js";
export { detectTaskType, type DetectedTaskType } from "./detect.js";

const DEVOPS_TOOL_OPERATIONS: Record<string, string> = {
  infra_plan: "compute_exec",
  deploy_execute: "deploy_exec",
  rollback: "deploy_exec",
  health_check: "data_read",
  log_query: "data_read",
  metric_query: "data_read",
  alert_manage: "data_write",
  config_validate: "compute_exec",
  secret_rotate: "secret_write",
  compliance_scan: "data_read",
};

/** Vertical descriptor consumed by the CLI ecosystem loader. */
export const DEVOPS_VERTICAL = {
  id: "devops",
  name: "DevOps",
  workflows: DEVOPS_WORKFLOWS,
  profiles: DEVOPS_PROFILES,
  toolDefinitions: devopsToolDefinitions(),
  detectTaskType,
  executeTool: executeDevopsTools,
  toolOperation: (name: string): string | null => DEVOPS_TOOL_OPERATIONS[name] ?? null,
  defining_constraint:
    "Environment-scaled blast-radius gating — deploy_execute, rollback, and secret_rotate require an environment context tag; production targets require human gate clearance before execution.",
  context_schema: {
    environment: {
      type: "enum" as const,
      required: true,
      description: "Target environment tier — determines blast radius and gate requirements.",
      enum_values: ["dev", "staging", "production"] as const,
    },
  },
  tool_policies: {
    deploy_execute: {
      kind: "requires_gate" as const,
      description: "Production deployments require human gate clearance before execution.",
      gate_condition: "environment === production",
    },
    rollback: {
      kind: "requires_gate" as const,
      description: "Production rollbacks require human gate clearance before execution.",
      gate_condition: "environment === production",
    },
    secret_rotate: {
      kind: "requires_gate" as const,
      description: "Production secret rotation requires human gate clearance before execution.",
      gate_condition: "environment === production",
    },
  },
  checkpoint_types: {},
  quality_criteria: DEVOPS_QUALITY_DIMENSIONS.map((d) => ({
    id: d.id,
    name: d.name,
    description: d.description,
    weight: 1.0,
  })),
  task_types: [
    { id: "devops:respond", name: "Incident Response", description: "Respond to a production incident — triage, mitigate, postmortem.", workflow_id: "devops:respond", keywords: ["rollback", "revert", "incident", "outage", "on-call"] },
    { id: "devops:deploy", name: "Deploy", description: "Plan and execute a deployment to a target environment.", workflow_id: "devops:deploy", keywords: ["deploy", "deployment", "release", "ship", "push"] },
    { id: "devops:provision", name: "Provision Infrastructure", description: "Provision or update infrastructure via IaC.", workflow_id: "devops:provision", keywords: ["provision", "terraform", "cloudformation", "infra", "vpc"] },
    { id: "devops:secure", name: "Harden / Secure", description: "Apply security hardening or rotate credentials.", workflow_id: "devops:secure", keywords: ["harden", "secure", "cve", "vulnerability", "secret rotate"] },
    { id: "devops:optimize", name: "Cost Optimize", description: "Analyze and reduce infrastructure costs.", workflow_id: "devops:optimize", keywords: ["optimize cost", "cost reduction", "right-size", "resource waste"] },
    { id: "devops:audit", name: "Compliance Audit", description: "Run a compliance or security audit.", workflow_id: "devops:audit", keywords: ["audit infra", "compliance scan", "security audit", "soc2"] },
    { id: "devops:monitor", name: "Monitor", description: "Review dashboards, alerts, and metrics.", workflow_id: "devops:monitor-only", keywords: ["monitor", "alert", "metric", "dashboard", "observability"] },
  ],
};
