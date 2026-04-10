import { healthcareToolDefinitions, executeHealthcareTools } from "./tools/healthcare-tools.js";
import { HEALTHCARE_PROFILES } from "./profiles/profiles.js";
import { HEALTHCARE_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";

export { HEALTHCARE_ROLES, HEALTHCARE_TASK_TYPES, getRole, getTaskType, type HealthcareRole, type HealthcareTaskType } from "./taxonomy.js";
export {
  healthcareToolDefinitions,
  executeHealthcareTools,
  validatePhiAccessGrant,
  isPhiGrantFresh,
  grantCoversUse,
  detectPhi,
  HIPAA_SAFE_HARBOR_IDENTIFIERS,
  type ToolDefinition,
  type PhiAccessGrant,
  type HipaaIdentifier,
} from "./tools/healthcare-tools.js";
export { HEALTHCARE_PROFILES, getProfile, allProfiles, type AgentProfile } from "./profiles/profiles.js";
export { HEALTHCARE_WORKFLOWS, getWorkflow, allWorkflows, validateWorkflow, type Workflow, type WorkflowStage } from "./workflows/workflows.js";
export { HEALTHCARE_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type QualityDimension, type QualityLevel, type QualityReport, type EvaluationContext } from "./quality/quality.js";
export { detectTaskType, type DetectedTaskType } from "./detect.js";

const HEALTHCARE_TOOL_OPERATIONS: Record<string, string> = {
  clinical_search: "data_read",
  protocol_lookup: "data_read",
  lab_interpret: "phi_read",
  risk_score: "phi_read",
  phi_filter: "data_read",
  consent_verify: "phi_read",
  de_identify: "phi_write",
  clinical_report_generate: "phi_write",
  audit_export: "data_read",
  education_material: "file_write",
};

/** Vertical descriptor consumed by the CLI ecosystem loader. */
export const HEALTHCARE_VERTICAL = {
  id: "healthcare",
  name: "Healthcare",
  workflows: HEALTHCARE_WORKFLOWS,
  profiles: HEALTHCARE_PROFILES,
  toolDefinitions: healthcareToolDefinitions(),
  detectTaskType,
  executeTool: executeHealthcareTools,
  toolOperation: (name: string): string | null => HEALTHCARE_TOOL_OPERATIONS[name] ?? null,
};
