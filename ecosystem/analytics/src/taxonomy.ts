/** Analytics role definition. */
export interface AnalyticsRole {
  id: string;
  name: string;
  extends: "worker" | "observer";
  concern: string;
  toolAccess: "query-profile" | "schema-metric" | "reconcile-profile" | "report-viz" | "read-synthesize";
  autonomy: "gated" | "autonomous";
}

/** Analytics task type definition. */
export interface AnalyticsTaskType {
  id: string;
  name: string;
  description: string;
  defaultWorkflow: string;
  roles: string[];
}

export const ANALYTICS_ROLES: AnalyticsRole[] = [
  {
    id: "analytics:analyst",
    name: "Analyst",
    extends: "worker",
    concern: "Write queries, profile data, produce numbers",
    toolAccess: "query-profile",
    autonomy: "gated",
  },
  {
    id: "analytics:modeler",
    name: "Modeler",
    extends: "worker",
    concern: "Build data models, define metrics, design schemas",
    toolAccess: "schema-metric",
    autonomy: "gated",
  },
  {
    id: "analytics:validator",
    name: "Validator",
    extends: "observer",
    concern: "Reconcile sources, verify data integrity, check freshness",
    toolAccess: "reconcile-profile",
    autonomy: "autonomous",
  },
  {
    id: "analytics:reporter",
    name: "Reporter",
    extends: "worker",
    concern: "Generate reports, build visualizations, cite sources",
    toolAccess: "report-viz",
    autonomy: "gated",
  },
  {
    id: "analytics:insights",
    name: "Insights",
    extends: "observer",
    concern: "Synthesize findings, narrate implications, flag anomalies",
    toolAccess: "read-synthesize",
    autonomy: "autonomous",
  },
];

export const ANALYTICS_TASK_TYPES: AnalyticsTaskType[] = [
  {
    id: "analytics:query",
    name: "Query",
    description: "Write a SQL query and return results",
    defaultWorkflow: "analytics:query-and-report",
    roles: ["analytics:analyst", "analytics:validator", "analytics:reporter"],
  },
  {
    id: "analytics:report",
    name: "Report",
    description: "Produce a formatted report",
    defaultWorkflow: "analytics:query-and-report",
    roles: ["analytics:analyst", "analytics:validator", "analytics:reporter"],
  },
  {
    id: "analytics:dashboard",
    name: "Dashboard",
    description: "Build an interactive dashboard",
    defaultWorkflow: "analytics:build-dashboard",
    roles: ["analytics:modeler", "analytics:reporter", "analytics:validator"],
  },
  {
    id: "analytics:model",
    name: "Model",
    description: "Build a data model (facts, dimensions, metrics)",
    defaultWorkflow: "analytics:model-data",
    roles: ["analytics:analyst", "analytics:modeler", "analytics:validator"],
  },
  {
    id: "analytics:analyze",
    name: "Analyze",
    description: "Ad-hoc analysis",
    defaultWorkflow: "analytics:query-and-report",
    roles: ["analytics:analyst", "analytics:validator", "analytics:reporter"],
  },
  {
    id: "analytics:validate",
    name: "Validate",
    description: "Data validation and reconciliation",
    defaultWorkflow: "analytics:validate-only",
    roles: ["analytics:validator"],
  },
  {
    id: "analytics:monitor",
    name: "Monitor",
    description: "KPI monitoring",
    defaultWorkflow: "analytics:monitor-only",
    roles: ["analytics:insights"],
  },
  {
    id: "analytics:investigate",
    name: "Investigate",
    description: "Investigate a data anomaly",
    defaultWorkflow: "analytics:investigate",
    roles: ["analytics:analyst", "analytics:validator", "analytics:insights"],
  },
];

/** Get a role by ID. */
export function getRole(id: string): AnalyticsRole | undefined {
  return ANALYTICS_ROLES.find((r) => r.id === id);
}

/** Get a task type by ID. */
export function getTaskType(id: string): AnalyticsTaskType | undefined {
  return ANALYTICS_TASK_TYPES.find((t) => t.id === id);
}
