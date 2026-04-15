import { datasciToolDefinitions, executeDatasciTools } from "./tools/datasci-tools.js";
import { DATASCI_PROFILES } from "./profiles/profiles.js";
import { DATASCI_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";
import { DATASCI_QUALITY_DIMENSIONS } from "./quality/quality.js";

export { DATASCI_ROLES, DATASCI_TASK_TYPES, getRole, getTaskType, type DatasciRole, type DatasciTaskType } from "./taxonomy.js";
export { datasciToolDefinitions, executeDatasciTools, validateHypothesisDeclaration, type ToolDefinition, type HypothesisDeclaration } from "./tools/datasci-tools.js";
export { DATASCI_PROFILES, getProfile, allProfiles, type AgentProfile } from "./profiles/profiles.js";
export { DATASCI_WORKFLOWS, getWorkflow, allWorkflows, validateWorkflow, type Workflow, type WorkflowStage } from "./workflows/workflows.js";
export { DATASCI_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type QualityDimension, type QualityLevel, type QualityReport, type EvaluationContext } from "./quality/quality.js";
export { detectTaskType, type DetectedTaskType } from "./detect.js";

const DATASCI_TOOL_OPERATIONS: Record<string, string> = {
  stat_summary: "data_read",
  correlation_analysis: "data_read",
  hypothesis_test: "data_read",
  feature_extract: "data_read",
  feature_transform: "data_write",
  model_fit: "compute_exec",
  diagnostic_plots: "file_write",
  bootstrap_sample: "compute_exec",
  causal_inference: "compute_exec",
  interpretation: "data_read",
};

/** Vertical descriptor consumed by the CLI ecosystem loader. */
export const DATASCI_VERTICAL = {
  id: "datasci",
  name: "Data Science",
  workflows: DATASCI_WORKFLOWS,
  profiles: DATASCI_PROFILES,
  toolDefinitions: datasciToolDefinitions(),
  detectTaskType,
  executeTool: executeDatasciTools,
  toolOperation: (name: string): string | null => DATASCI_TOOL_OPERATIONS[name] ?? null,
  defining_constraint:
    "Pre-declared hypotheses / statistical rigor — hypothesis_test refuses without a prior declared_hypothesis checkpoint; all point estimates must carry confidence intervals; assumption checks are required before parametric tests.",
  context_schema: {
    hypothesis_framework: {
      type: "string" as const,
      required: false,
      description: "Default hypothesis framework template for this session (optional; can be specified per-test).",
    },
  },
  tool_policies: {
    hypothesis_test: {
      kind: "requires_checkpoint" as const,
      description: "Refuses without a declared_hypothesis checkpoint providing null_hypothesis, alternative_hypothesis, alpha, and test_type.",
      checkpoint_type: "declared_hypothesis",
    },
  },
  checkpoint_types: {
    declared_hypothesis: {
      description: "Pre-declared statistical hypothesis — must exist before hypothesis_test can execute.",
      fields: [
        { name: "null_hypothesis", type: "string" as const, description: "The null hypothesis statement (H0)." },
        { name: "alternative_hypothesis", type: "string" as const, description: "The alternative hypothesis statement (H1)." },
        { name: "alpha", type: "number" as const, description: "Significance level (e.g. 0.05)." },
        { name: "test_type", type: "string" as const, description: "Statistical test to use (e.g. t-test, chi-square, anova)." },
        { name: "multiple_testing_correction", type: "string" as const, description: "Correction strategy (e.g. bonferroni, fdr-bh) if multiple comparisons are made." },
      ],
    },
  },
  quality_criteria: DATASCI_QUALITY_DIMENSIONS.map((d) => ({
    id: d.id,
    name: d.name,
    description: d.description,
    weight: 1.0,
  })),
  task_types: [
    { id: "datasci:test", name: "Hypothesis Test", description: "Execute a formal statistical hypothesis test.", workflow_id: "datasci:hypothesis-test", keywords: ["hypothesis test", "t-test", "chi-square", "anova", "p-value"] },
    { id: "datasci:hypothesize", name: "Declare Hypothesis", description: "Formulate and declare a testable hypothesis.", workflow_id: "datasci:hypothesis-test", keywords: ["test if", "test whether", "null hypothesis", "formulate hypothesis"] },
    { id: "datasci:model", name: "Build Model", description: "Fit a statistical or ML model to data.", workflow_id: "datasci:model-build", keywords: ["linear regression", "logistic regression", "glm", "gam", "model fit"] },
    { id: "datasci:explore", name: "Exploratory Analysis", description: "Profile and visualise a dataset.", workflow_id: "datasci:exploration", keywords: ["eda", "exploratory data analysis", "profile data", "describe data"] },
    { id: "datasci:feature", name: "Feature Engineering", description: "Extract or transform features for modelling.", workflow_id: "datasci:feature-only", keywords: ["feature extract", "feature transform", "standardize", "pca", "encoding"] },
    { id: "datasci:interpret", name: "Interpret Results", description: "Compute effect sizes, confidence intervals, and interpret outputs.", workflow_id: "datasci:interpret-only", keywords: ["interpret results", "effect size", "confidence interval", "odds ratio"] },
    { id: "datasci:report", name: "Statistical Report", description: "Produce a full statistical analysis report.", workflow_id: "datasci:full-analysis", keywords: ["statistical report", "statistical findings", "analysis report", "findings summary"] },
  ],
};
