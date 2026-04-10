/** A detected Healthcare task type with its target workflow. */
export interface DetectedTaskType {
  id: string;
  workflowId: string;
}

/**
 * Classify a user goal into a Healthcare task type via keyword heuristics.
 * Returns null if the goal does not match the Healthcare domain.
 */
export function detectTaskType(goal: string): DetectedTaskType | null {
  const lower = goal.toLowerCase();
  if (/\b(differential\s+diagnos|ddx|diagnose|diagnosis)\b/.test(lower))
    return { id: "health:diagnose_support", workflowId: "health:patient-assessment" };
  if (/\b(h&p|history\s+(?:and|\&)\s+physical|admission\s+note|admit\s+(?:the\s+)?patient|intake)\b/.test(lower))
    return { id: "health:assess", workflowId: "health:patient-assessment" };
  if (/\b(progress\s+note|chart\s+note|discharge\s+summary|consult\s+note)\b/.test(lower))
    return { id: "health:report", workflowId: "health:patient-assessment" };
  if (/\b(literature\s+review|systematic\s+review|evidence\s+synthesis|pubmed|cochrane)\b/.test(lower))
    return { id: "health:research", workflowId: "health:literature-research" };
  if (/\b(phi|hipaa|de[-\s]?identif|safe\s+harbor|consent\s+verif|audit\s+(?:phi|trail))\b/.test(lower))
    return { id: "health:audit", workflowId: "health:phi-audit" };
  if (/\b(patient\s+education|teach\s+(?:the\s+)?patient|education\s+material|patient\s+handout)\b/.test(lower))
    return { id: "health:educate", workflowId: "health:patient-education" };
  if (/\b(cohort|de[-\s]?identified\s+data|population\s+health|epidemiolog)\b/.test(lower))
    return { id: "health:analyze", workflowId: "health:analyze-only" };
  if (/\b(monitor\s+(?:the\s+)?patient|vital\s+signs|follow[-\s]?up\s+visit|telemetry)\b/.test(lower))
    return { id: "health:monitor", workflowId: "health:monitor-only" };
  if (/\b(patient|clinical|clinician|nurse\s+(?:practitioner|case)|ward\s+round)\b/.test(lower))
    return { id: "health:assess", workflowId: "health:patient-assessment" };
  return null;
}
