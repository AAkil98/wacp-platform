/** Data Science role definition. */
export interface DatasciRole {
  id: string;
  name: string;
  extends: "worker" | "observer";
  concern: string;
  toolAccess: "read-summary" | "test-inference" | "extract-transform" | "fit-diagnostics" | "read-only";
  autonomy: "gated" | "autonomous";
}

/** Data Science task type definition. */
export interface DatasciTaskType {
  id: string;
  name: string;
  description: string;
  defaultWorkflow: string;
  roles: string[];
}

export const DATASCI_ROLES: DatasciRole[] = [
  {
    id: "datasci:explorer",
    name: "Explorer",
    extends: "worker",
    concern: "Exploratory data analysis, statistical summaries, visualization",
    toolAccess: "read-summary",
    autonomy: "gated",
  },
  {
    id: "datasci:statistician",
    name: "Statistician",
    extends: "worker",
    concern: "Hypothesis testing, statistical inference, causal reasoning",
    toolAccess: "test-inference",
    autonomy: "gated",
  },
  {
    id: "datasci:feature_engineer",
    name: "Feature Engineer",
    extends: "worker",
    concern: "Feature extraction, transformation, selection",
    toolAccess: "extract-transform",
    autonomy: "gated",
  },
  {
    id: "datasci:modeler",
    name: "Modeler",
    extends: "worker",
    concern: "Fit statistical models, diagnose fit, check assumptions",
    toolAccess: "fit-diagnostics",
    autonomy: "gated",
  },
  {
    id: "datasci:reviewer",
    name: "Reviewer",
    extends: "observer",
    concern: "Peer review, methodology check, reproducibility audit",
    toolAccess: "read-only",
    autonomy: "autonomous",
  },
];

export const DATASCI_TASK_TYPES: DatasciTaskType[] = [
  {
    id: "datasci:explore",
    name: "Explore",
    description: "Exploratory data analysis",
    defaultWorkflow: "datasci:exploration",
    roles: ["datasci:explorer", "datasci:reviewer"],
  },
  {
    id: "datasci:hypothesize",
    name: "Hypothesize",
    description: "Formulate a hypothesis",
    defaultWorkflow: "datasci:hypothesis-test",
    roles: ["datasci:statistician", "datasci:reviewer"],
  },
  {
    id: "datasci:test",
    name: "Test",
    description: "Run a hypothesis test",
    defaultWorkflow: "datasci:hypothesis-test",
    roles: ["datasci:statistician", "datasci:reviewer"],
  },
  {
    id: "datasci:model",
    name: "Model",
    description: "Build a statistical model",
    defaultWorkflow: "datasci:model-build",
    roles: ["datasci:explorer", "datasci:feature_engineer", "datasci:modeler", "datasci:reviewer"],
  },
  {
    id: "datasci:feature",
    name: "Feature",
    description: "Feature engineering",
    defaultWorkflow: "datasci:feature-only",
    roles: ["datasci:feature_engineer"],
  },
  {
    id: "datasci:validate",
    name: "Validate",
    description: "Validate a model or inference",
    defaultWorkflow: "datasci:validate-only",
    roles: ["datasci:reviewer"],
  },
  {
    id: "datasci:interpret",
    name: "Interpret",
    description: "Interpret statistical results",
    defaultWorkflow: "datasci:interpret-only",
    roles: ["datasci:statistician"],
  },
  {
    id: "datasci:report",
    name: "Report",
    description: "Report findings",
    defaultWorkflow: "datasci:full-analysis",
    roles: ["datasci:explorer", "datasci:statistician", "datasci:reviewer"],
  },
  {
    id: "datasci:review",
    name: "Review",
    description: "Peer review an analysis",
    defaultWorkflow: "datasci:review-only",
    roles: ["datasci:reviewer"],
  },
];

/** Get a role by ID. */
export function getRole(id: string): DatasciRole | undefined {
  return DATASCI_ROLES.find((r) => r.id === id);
}

/** Get a task type by ID. */
export function getTaskType(id: string): DatasciTaskType | undefined {
  return DATASCI_TASK_TYPES.find((t) => t.id === id);
}
