import { healthcareToolDefinitions, executeHealthcareTools } from "./tools/healthcare-tools.js";
import { HEALTHCARE_PROFILES } from "./profiles/profiles.js";
import { HEALTHCARE_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";
import { HEALTHCARE_QUALITY_DIMENSIONS } from "./quality/quality.js";

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
  defining_constraint:
    "HIPAA PHI access grant — clinical_report_generate, lab_interpret, and risk_score refuse without a valid phi_access_grant argument (consent or de-identification basis); 18 HIPAA Safe Harbor identifiers screened by phi_filter; patient-assessment workflow is fully gated for clinician sign-off.",
  context_schema: {
    phi_access_basis: {
      type: "enum" as const,
      required: true,
      description: "Permitted basis under which PHI may be accessed in this session.",
      enum_values: ["consent", "de_identified"] as const,
    },
  },
  tool_policies: {
    clinical_report_generate: {
      kind: "requires_checkpoint" as const,
      description: "Requires a valid phi_access_grant argument (consent or de-identification basis) from a prior consent_verify or de_identify call.",
      checkpoint_type: "phi_access_grant",
    },
    lab_interpret: {
      kind: "requires_checkpoint" as const,
      description: "Requires a valid phi_access_grant argument (consent or de-identification basis).",
      checkpoint_type: "phi_access_grant",
    },
    risk_score: {
      kind: "requires_checkpoint" as const,
      description: "Requires a valid phi_access_grant argument (consent or de-identification basis).",
      checkpoint_type: "phi_access_grant",
    },
  },
  checkpoint_types: {
    phi_access_grant: {
      description: "PHI access authorisation — consent or de-identification basis — required before clinical tools execute.",
      fields: [
        { name: "basis", type: "enum" as const, description: "Authorisation basis.", enum_values: ["consent", "de_identified"] as const },
        { name: "patient_id", type: "string" as const, description: "Patient identifier (consent basis only)." },
        { name: "consent_id", type: "string" as const, description: "Consent document identifier (consent basis only)." },
        { name: "consent_scope", type: "string" as const, description: "JSON array of permitted use scopes (consent basis)." },
        { name: "deidentification_method", type: "string" as const, description: "Method used to de-identify data (de_identified basis)." },
        { name: "deidentified_data_hash", type: "string" as const, description: "SHA-256 of the de-identified dataset (de_identified basis)." },
        { name: "expires_at", type: "number" as const, description: "Unix timestamp (ms) after which this grant expires." },
      ],
    },
  },
  quality_criteria: HEALTHCARE_QUALITY_DIMENSIONS.map((d) => ({
    id: d.id,
    name: d.name,
    description: d.description,
    weight: 1.0,
  })),
  task_types: [
    { id: "health:diagnose_support", name: "Differential Diagnosis", description: "Generate a differential diagnosis list to support a clinician.", workflow_id: "health:patient-assessment", keywords: ["differential diagnosis", "ddx", "diagnose", "differential"] },
    { id: "health:assess", name: "Patient Assessment", description: "Complete a history and physical or admission note.", workflow_id: "health:patient-assessment", keywords: ["h&p", "history and physical", "admission note", "assessment"] },
    { id: "health:report", name: "Clinical Note", description: "Write a progress note, chart note, or discharge summary.", workflow_id: "health:patient-assessment", keywords: ["progress note", "chart note", "discharge summary", "soap note"] },
    { id: "health:research", name: "Literature Research", description: "Conduct a systematic or narrative literature review.", workflow_id: "health:literature-research", keywords: ["literature review", "systematic review", "pubmed", "evidence review"] },
    { id: "health:audit", name: "PHI Audit", description: "Audit PHI access, consent records, or HIPAA compliance.", workflow_id: "health:phi-audit", keywords: ["phi", "hipaa", "de-identify", "consent verify", "audit"] },
    { id: "health:educate", name: "Patient Education", description: "Generate patient education materials at an appropriate reading level.", workflow_id: "health:patient-education", keywords: ["patient education", "teach patient", "handout", "patient materials"] },
    { id: "health:analyze", name: "Cohort Analysis", description: "Analyze de-identified population health data.", workflow_id: "health:analyze-only", keywords: ["cohort", "de-identified data", "population health", "epidemiology"] },
    { id: "health:monitor", name: "Monitor Patient", description: "Review vital sign trends or follow-up on a care plan.", workflow_id: "health:monitor-only", keywords: ["monitor patient", "vital signs", "follow-up", "care plan"] },
  ],
};
