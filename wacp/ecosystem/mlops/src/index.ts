import { mlopsToolDefinitions, executeMlopsTools } from "./tools/mlops-tools.js";
import { MLOPS_PROFILES } from "./profiles/profiles.js";
import { MLOPS_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";
import { MLOPS_QUALITY_DIMENSIONS } from "./quality/quality.js";

export { MLOPS_ROLES, MLOPS_TASK_TYPES, getRole, getTaskType, type MlopsRole, type MlopsTaskType } from "./taxonomy.js";
export { mlopsToolDefinitions, executeMlopsTools, type ToolDefinition } from "./tools/mlops-tools.js";
export { MLOPS_PROFILES, getProfile, allProfiles, type AgentProfile } from "./profiles/profiles.js";
export { MLOPS_WORKFLOWS, getWorkflow, allWorkflows, validateWorkflow, type Workflow, type WorkflowStage } from "./workflows/workflows.js";
export { MLOPS_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type QualityDimension, type QualityLevel, type QualityReport, type EvaluationContext } from "./quality/quality.js";
export { detectTaskType, type DetectedTaskType } from "./detect.js";

const MLOPS_TOOL_OPERATIONS: Record<string, string> = {
  dataset_validate: "data_read",
  experiment_track: "data_write",
  train_launch: "compute_exec",
  eval_benchmark: "compute_exec",
  model_register: "data_write",
  model_deploy: "deploy_exec",
  drift_detect: "data_read",
  compute_budget: "data_read",
  reproduce_check: "compute_exec",
  data_lineage: "data_read",
};

/** Vertical descriptor consumed by the CLI ecosystem loader. */
export const MLOPS_VERTICAL = {
  id: "mlops",
  name: "MLOps",
  workflows: MLOPS_WORKFLOWS,
  profiles: MLOPS_PROFILES,
  toolDefinitions: mlopsToolDefinitions(),
  detectTaskType,
  executeTool: executeMlopsTools,
  toolOperation: (name: string): string | null => MLOPS_TOOL_OPERATIONS[name] ?? null,
  defining_constraint:
    "Compute-budget gating + reproducibility — train_launch refuses if max_hours would exceed the session budget; every experiment must record a reproducibility checkpoint (data hash, code version, random seed, hyperparameters).",
  context_schema: {
    compute_budget: {
      type: "number" as const,
      required: true,
      description: "Maximum GPU-hours available for training runs in this session.",
    },
  },
  tool_policies: {
    train_launch: {
      kind: "budget_limited" as const,
      description: "Training job refuses if max_hours would exceed the session compute_budget.",
      budget_field: "max_hours",
    },
  },
  checkpoint_types: {
    reproducibility_checkpoint: {
      description: "Records the full provenance of a training run — required for reproducibility.",
      fields: [
        { name: "data_hash", type: "string" as const, description: "SHA-256 of the training dataset." },
        { name: "code_version", type: "string" as const, description: "Git commit or image digest of the training code." },
        { name: "random_seed", type: "number" as const, description: "Global random seed used for the run." },
        { name: "hyperparameters", type: "string" as const, description: "JSON-serialised hyperparameter map." },
      ],
    },
  },
  quality_criteria: MLOPS_QUALITY_DIMENSIONS.map((d) => ({
    id: d.id,
    name: d.name,
    description: d.description,
    weight: 1.0,
  })),
  task_types: [
    { id: "mlops:reproduce", name: "Reproduce Experiment", description: "Re-run a prior experiment from its reproducibility checkpoint.", workflow_id: "mlops:reproduce", keywords: ["reproduce", "reproducibility", "rerun", "recreate"] },
    { id: "mlops:train", name: "Train Model", description: "Launch a training or fine-tuning run.", workflow_id: "mlops:experiment", keywords: ["train", "fine-tune", "pretrain", "sweep", "hyperparameter"] },
    { id: "mlops:experiment", name: "Run Experiment", description: "Design and run an experiment or ablation study.", workflow_id: "mlops:experiment", keywords: ["experiment", "ablation", "grid search", "study"] },
    { id: "mlops:deploy", name: "Deploy Model", description: "Register and deploy a trained model.", workflow_id: "mlops:deploy", keywords: ["deploy model", "model registry", "register", "serve"] },
    { id: "mlops:evaluate", name: "Evaluate Model", description: "Benchmark a model against an evaluation set.", workflow_id: "mlops:evaluate-only", keywords: ["evaluate", "benchmark", "leaderboard", "eval set"] },
    { id: "mlops:monitor", name: "Monitor Drift", description: "Detect distribution shift or model degradation in production.", workflow_id: "mlops:monitor-only", keywords: ["drift", "distribution shift", "production monitoring", "staleness"] },
    { id: "mlops:optimize", name: "Optimize Model", description: "Compress, quantize, or otherwise optimize a trained model.", workflow_id: "mlops:optimize", keywords: ["optimize model", "compression", "quantization", "onnx", "pruning"] },
    { id: "mlops:data_prep", name: "Prepare Dataset", description: "Validate, label, or preprocess a dataset.", workflow_id: "mlops:data-prep-only", keywords: ["dataset", "data prep", "preprocess", "labeling", "annotation"] },
    { id: "mlops:audit", name: "Audit Model Lineage", description: "Trace model provenance and data lineage.", workflow_id: "mlops:audit-only", keywords: ["model audit", "data lineage", "provenance", "traceability"] },
  ],
};
