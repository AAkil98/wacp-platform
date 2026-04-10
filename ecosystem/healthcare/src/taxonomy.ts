/** Healthcare role definition. */
export interface HealthcareRole {
  id: string;
  name: string;
  extends: "worker" | "observer";
  concern: string;
  toolAccess: "clinical" | "search-deidentify" | "deidentified-analytics" | "read-audit" | "coordination-education";
  autonomy: "gated" | "autonomous";
}

/** Healthcare task type definition. */
export interface HealthcareTaskType {
  id: string;
  name: string;
  description: string;
  defaultWorkflow: string;
  roles: string[];
}

export const HEALTHCARE_ROLES: HealthcareRole[] = [
  {
    id: "health:clinician",
    name: "Clinician",
    extends: "worker",
    concern: "Patient assessment, clinical decision support, documentation",
    toolAccess: "clinical",
    autonomy: "gated",
  },
  {
    id: "health:researcher",
    name: "Researcher",
    extends: "worker",
    concern: "Literature review, evidence synthesis, study design",
    toolAccess: "search-deidentify",
    autonomy: "gated",
  },
  {
    id: "health:analyst",
    name: "Analyst",
    extends: "worker",
    concern: "Cohort analysis on de-identified data",
    toolAccess: "deidentified-analytics",
    autonomy: "gated",
  },
  {
    id: "health:compliance",
    name: "Compliance",
    extends: "observer",
    concern: "PHI audit, consent verification, regulatory adherence",
    toolAccess: "read-audit",
    autonomy: "autonomous",
  },
  {
    id: "health:coordinator",
    name: "Coordinator",
    extends: "worker",
    concern: "Care coordination, patient education, workflow scheduling",
    toolAccess: "coordination-education",
    autonomy: "gated",
  },
];

export const HEALTHCARE_TASK_TYPES: HealthcareTaskType[] = [
  {
    id: "health:assess",
    name: "Assess",
    description: "Patient assessment from intake to documented plan",
    defaultWorkflow: "health:patient-assessment",
    roles: ["health:clinician"],
  },
  {
    id: "health:diagnose_support",
    name: "Diagnose Support",
    description: "Differential diagnosis support",
    defaultWorkflow: "health:patient-assessment",
    roles: ["health:clinician"],
  },
  {
    id: "health:research",
    name: "Research",
    description: "Literature review and evidence synthesis",
    defaultWorkflow: "health:literature-research",
    roles: ["health:researcher", "health:compliance"],
  },
  {
    id: "health:analyze",
    name: "Analyze",
    description: "Cohort analysis on de-identified data",
    defaultWorkflow: "health:analyze-only",
    roles: ["health:analyst"],
  },
  {
    id: "health:monitor",
    name: "Monitor",
    description: "Ongoing patient monitoring",
    defaultWorkflow: "health:monitor-only",
    roles: ["health:clinician"],
  },
  {
    id: "health:report",
    name: "Report",
    description: "Patient-facing or clinician-facing report",
    defaultWorkflow: "health:patient-assessment",
    roles: ["health:clinician"],
  },
  {
    id: "health:audit",
    name: "Audit",
    description: "PHI audit, compliance review",
    defaultWorkflow: "health:phi-audit",
    roles: ["health:compliance"],
  },
  {
    id: "health:educate",
    name: "Educate",
    description: "Patient education materials",
    defaultWorkflow: "health:patient-education",
    roles: ["health:coordinator"],
  },
];

/** Get a role by ID. */
export function getRole(id: string): HealthcareRole | undefined {
  return HEALTHCARE_ROLES.find((r) => r.id === id);
}

/** Get a task type by ID. */
export function getTaskType(id: string): HealthcareTaskType | undefined {
  return HEALTHCARE_TASK_TYPES.find((t) => t.id === id);
}
