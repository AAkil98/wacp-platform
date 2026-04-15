/** MLOps role definition. */
export interface MlopsRole {
  id: string;
  name: string;
  extends: "worker" | "observer";
  concern: string;
  toolAccess: "read-experiment" | "compute-exec" | "read-benchmark" | "execute-registry" | "read-monitor";
  autonomy: "gated" | "autonomous";
}

/** MLOps task type definition. */
export interface MlopsTaskType {
  id: string;
  name: string;
  description: string;
  defaultWorkflow: string;
  roles: string[];
}

export const MLOPS_ROLES: MlopsRole[] = [
  {
    id: "mlops:researcher",
    name: "Researcher",
    extends: "worker",
    concern: "Design experiments, select architectures, analyze results",
    toolAccess: "read-experiment",
    autonomy: "gated",
  },
  {
    id: "mlops:trainer",
    name: "Trainer",
    extends: "worker",
    concern: "Launch training runs, manage compute, track experiments",
    toolAccess: "compute-exec",
    autonomy: "gated",
  },
  {
    id: "mlops:evaluator",
    name: "Evaluator",
    extends: "observer",
    concern: "Benchmark models, run evaluations, compare metrics",
    toolAccess: "read-benchmark",
    autonomy: "autonomous",
  },
  {
    id: "mlops:deployer",
    name: "Deployer",
    extends: "worker",
    concern: "Deploy models, manage registry, serve endpoints",
    toolAccess: "execute-registry",
    autonomy: "gated",
  },
  {
    id: "mlops:monitor",
    name: "Monitor",
    extends: "observer",
    concern: "Monitor model performance, detect drift, track metrics",
    toolAccess: "read-monitor",
    autonomy: "autonomous",
  },
];

export const MLOPS_TASK_TYPES: MlopsTaskType[] = [
  {
    id: "mlops:experiment",
    name: "Experiment",
    description: "Design and run an experiment",
    defaultWorkflow: "mlops:experiment",
    roles: ["mlops:researcher", "mlops:trainer", "mlops:evaluator"],
  },
  {
    id: "mlops:train",
    name: "Train",
    description: "Train a model",
    defaultWorkflow: "mlops:optimize",
    roles: ["mlops:evaluator", "mlops:researcher", "mlops:trainer"],
  },
  {
    id: "mlops:evaluate",
    name: "Evaluate",
    description: "Evaluate model performance",
    defaultWorkflow: "mlops:evaluate-only",
    roles: ["mlops:evaluator"],
  },
  {
    id: "mlops:deploy",
    name: "Deploy",
    description: "Deploy model to serving",
    defaultWorkflow: "mlops:deploy",
    roles: ["mlops:evaluator", "mlops:deployer", "mlops:monitor"],
  },
  {
    id: "mlops:monitor",
    name: "Monitor",
    description: "Monitor model performance",
    defaultWorkflow: "mlops:monitor-only",
    roles: ["mlops:monitor"],
  },
  {
    id: "mlops:optimize",
    name: "Optimize",
    description: "Optimize model/training performance",
    defaultWorkflow: "mlops:optimize",
    roles: ["mlops:evaluator", "mlops:researcher", "mlops:trainer"],
  },
  {
    id: "mlops:data-prep",
    name: "Data Prep",
    description: "Prepare/validate datasets",
    defaultWorkflow: "mlops:data-prep-only",
    roles: ["mlops:researcher"],
  },
  {
    id: "mlops:reproduce",
    name: "Reproduce",
    description: "Reproduce an experiment",
    defaultWorkflow: "mlops:reproduce",
    roles: ["mlops:researcher", "mlops:trainer"],
  },
  {
    id: "mlops:audit",
    name: "Audit",
    description: "Audit model lineage/compliance",
    defaultWorkflow: "mlops:audit-only",
    roles: ["mlops:evaluator"],
  },
];

/** Get a role by ID. */
export function getRole(id: string): MlopsRole | undefined {
  return MLOPS_ROLES.find((r) => r.id === id);
}

/** Get a task type by ID. */
export function getTaskType(id: string): MlopsTaskType | undefined {
  return MLOPS_TASK_TYPES.find((t) => t.id === id);
}
