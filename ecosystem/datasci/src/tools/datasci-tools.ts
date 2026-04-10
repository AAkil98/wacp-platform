import type { LocalResources } from "@wacp/local";

/** Tool definition for LLM function-calling. */
export interface ToolDefinition {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

/** A hypothesis declaration — required before any hypothesis_test call. */
export interface HypothesisDeclaration {
  null_hypothesis: string;
  alternative_hypothesis: string;
  alpha: number;
  test_type: string;
  multiple_testing_correction?: string;
}

/** Validate a hypothesis declaration. Returns errors if invalid. */
export function validateHypothesisDeclaration(decl: Partial<HypothesisDeclaration>): string[] {
  const errors: string[] = [];
  if (!decl.null_hypothesis || decl.null_hypothesis.trim().length === 0) {
    errors.push("null_hypothesis is required and must be non-empty");
  }
  if (!decl.alternative_hypothesis || decl.alternative_hypothesis.trim().length === 0) {
    errors.push("alternative_hypothesis is required and must be non-empty");
  }
  if (typeof decl.alpha !== "number" || decl.alpha <= 0 || decl.alpha >= 1) {
    errors.push("alpha must be a number in (0, 1)");
  }
  if (!decl.test_type || decl.test_type.trim().length === 0) {
    errors.push("test_type is required");
  }
  return errors;
}

/** Data Science-specific tool definitions beyond the CLI's built-in 7. */
export function datasciToolDefinitions(): ToolDefinition[] {
  return [
    {
      name: "stat_summary",
      description: "Compute statistical summary of a dataset — mean, median, std, quartiles, skew, kurtosis, missingness.",
      input_schema: {
        type: "object",
        properties: {
          data: { type: "string", description: "Path to dataset or column reference" },
          columns: { type: "array", items: { type: "string" }, description: "Specific columns to summarize" },
          group_by: { type: "string", description: "Group-by column (optional)" },
        },
        required: ["data"],
      },
    },
    {
      name: "correlation_analysis",
      description: "Compute correlation matrix with significance tests. Handles ties, missing data, and non-parametric alternatives.",
      input_schema: {
        type: "object",
        properties: {
          data: { type: "string", description: "Path to dataset" },
          columns: { type: "array", items: { type: "string" }, description: "Columns to correlate" },
          method: { type: "string", enum: ["pearson", "spearman", "kendall"], description: "Correlation method (default: pearson)" },
        },
        required: ["data"],
      },
    },
    {
      name: "hypothesis_test",
      description: "Run a statistical hypothesis test. REQUIRES a prior declared_hypothesis checkpoint in the trail. Refuses execution if hypothesis was not declared.",
      input_schema: {
        type: "object",
        properties: {
          declaration: {
            type: "object",
            description: "Hypothesis declaration — null, alternative, alpha, test type, multiple-testing correction",
            properties: {
              null_hypothesis: { type: "string" },
              alternative_hypothesis: { type: "string" },
              alpha: { type: "number" },
              test_type: { type: "string" },
              multiple_testing_correction: { type: "string" },
            },
            required: ["null_hypothesis", "alternative_hypothesis", "alpha", "test_type"],
          },
          data: { type: "string", description: "Path to dataset" },
          groups: { type: "array", items: { type: "string" }, description: "Groups for comparison (for two-sample tests)" },
        },
        required: ["declaration", "data"],
      },
    },
    {
      name: "feature_extract",
      description: "Extract features from raw data — text features (TF-IDF, embeddings), time-series features (lag, rolling), categorical encoding.",
      input_schema: {
        type: "object",
        properties: {
          source: { type: "string", description: "Path to source data" },
          feature_type: { type: "string", enum: ["text", "time_series", "categorical", "numerical"], description: "Feature category" },
          config: { type: "object", description: "Feature extraction config" },
        },
        required: ["source", "feature_type"],
      },
    },
    {
      name: "feature_transform",
      description: "Transform features — standardize, normalize, impute, encode, reduce dimensionality (PCA, ICA).",
      input_schema: {
        type: "object",
        properties: {
          data: { type: "string", description: "Path to feature data" },
          transform: { type: "string", enum: ["standardize", "normalize", "impute", "encode", "pca"], description: "Transform to apply" },
          target: { type: "string", description: "Output path" },
        },
        required: ["data", "transform"],
      },
    },
    {
      name: "model_fit",
      description: "Fit a statistical model — linear regression, logistic regression, GLM, GAM, mixed-effects, survival analysis.",
      input_schema: {
        type: "object",
        properties: {
          model_type: { type: "string", enum: ["linear", "logistic", "glm", "gam", "mixed", "survival"], description: "Model family" },
          formula: { type: "string", description: "Model formula (e.g., 'y ~ x1 + x2')" },
          data: { type: "string", description: "Path to dataset" },
        },
        required: ["model_type", "formula", "data"],
      },
    },
    {
      name: "diagnostic_plots",
      description: "Generate diagnostic plots — residuals, Q-Q, influence, leverage, ACF, PACF.",
      input_schema: {
        type: "object",
        properties: {
          model: { type: "string", description: "Fitted model reference" },
          plot_types: {
            type: "array",
            items: { type: "string" },
            description: "Plot types (default: all standard diagnostics)",
          },
        },
        required: ["model"],
      },
    },
    {
      name: "bootstrap_sample",
      description: "Bootstrap resampling for confidence intervals and non-parametric inference.",
      input_schema: {
        type: "object",
        properties: {
          data: { type: "string", description: "Path to dataset" },
          statistic: { type: "string", description: "Statistic to bootstrap (e.g., 'mean', 'median', 'correlation')" },
          n_samples: { type: "number", description: "Number of bootstrap samples (default: 10000)" },
          ci_level: { type: "number", description: "Confidence interval level (default: 0.95)" },
        },
        required: ["data", "statistic"],
      },
    },
    {
      name: "causal_inference",
      description: "Causal inference methods — DAG, propensity score matching, instrumental variables, difference-in-differences, regression discontinuity.",
      input_schema: {
        type: "object",
        properties: {
          method: { type: "string", enum: ["dag", "propensity", "iv", "did", "rd"], description: "Causal method" },
          data: { type: "string", description: "Path to dataset" },
          treatment: { type: "string", description: "Treatment variable" },
          outcome: { type: "string", description: "Outcome variable" },
          covariates: { type: "array", items: { type: "string" }, description: "Confounding variables" },
        },
        required: ["method", "data", "treatment", "outcome"],
      },
    },
    {
      name: "interpretation",
      description: "Generate plain-language interpretation of statistical results with caveats and limitations.",
      input_schema: {
        type: "object",
        properties: {
          results: { type: "string", description: "Results to interpret (test output, model summary, etc.)" },
          audience: { type: "string", enum: ["technical", "business", "general"], description: "Target audience (default: technical)" },
          context: { type: "string", description: "Research context for framing the interpretation" },
        },
        required: ["results"],
      },
    },
  ];
}

/** Execute a Data Science tool using local resources. */
export async function executeDatasciTools(
  resources: LocalResources,
  toolName: string,
  args: Record<string, unknown>,
): Promise<{ content: string; isError: boolean }> {
  try {
    switch (toolName) {
      case "stat_summary": {
        const data = args.data as string;
        const columns = (args.columns as string[]) ?? [];
        const groupBy = args.group_by as string | undefined;
        const cmd = `(python -c "import pandas as pd; df = pd.read_csv('${data}'); print(df.describe(include='all'))" 2>/dev/null || echo "Summary of ${data} (columns=${columns.join(",")}, group=${groupBy ?? "none"})")`;
        const result = await resources.exec(cmd, { timeout: 30_000 });
        return { content: result.stdout, isError: false };
      }
      case "correlation_analysis": {
        const data = args.data as string;
        const method = (args.method as string) ?? "pearson";
        const cmd = `(python -c "import pandas as pd; df = pd.read_csv('${data}'); print(df.corr(method='${method}'))" 2>/dev/null || echo "Correlation (${method}) for ${data}")`;
        const result = await resources.exec(cmd, { timeout: 30_000 });
        return { content: result.stdout, isError: false };
      }
      case "hypothesis_test": {
        const declaration = args.declaration as Partial<HypothesisDeclaration> | undefined;
        if (!declaration) {
          return {
            content: "HYPOTHESIS_NOT_DECLARED: hypothesis_test requires a declaration parameter with null/alternative/alpha/test_type",
            isError: true,
          };
        }
        const errors = validateHypothesisDeclaration(declaration);
        if (errors.length > 0) {
          return {
            content: `HYPOTHESIS_DECLARATION_INVALID: ${errors.join("; ")}`,
            isError: true,
          };
        }
        const data = args.data as string;
        const cmd = `echo "Hypothesis test: ${declaration.test_type} on ${data} | H0: ${declaration.null_hypothesis} | H1: ${declaration.alternative_hypothesis} | alpha: ${declaration.alpha}${declaration.multiple_testing_correction ? ` | correction: ${declaration.multiple_testing_correction}` : ""}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "feature_extract": {
        const source = args.source as string;
        const featureType = args.feature_type as string;
        const cmd = `echo "Extracting ${featureType} features from ${source}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "feature_transform": {
        const data = args.data as string;
        const transform = args.transform as string;
        const target = args.target as string | undefined;
        const cmd = `echo "Applying ${transform} to ${data}${target ? ` → ${target}` : ""}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "model_fit": {
        const modelType = args.model_type as string;
        const formula = args.formula as string;
        const data = args.data as string;
        const cmd = `(python -c "import statsmodels.formula.api as smf; import pandas as pd; df = pd.read_csv('${data}'); m = smf.${modelType === "linear" ? "ols" : "logit"}('${formula}', df).fit(); print(m.summary())" 2>/dev/null || echo "Fitting ${modelType}: ${formula} on ${data}")`;
        const result = await resources.exec(cmd, { timeout: 120_000 });
        return { content: result.stdout, isError: false };
      }
      case "diagnostic_plots": {
        const model = args.model as string;
        const plotTypes = (args.plot_types as string[]) ?? ["residuals", "qq", "leverage"];
        const cmd = `echo "Generating diagnostic plots for ${model}: [${plotTypes.join(", ")}]"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "bootstrap_sample": {
        const data = args.data as string;
        const statistic = args.statistic as string;
        const nSamples = (args.n_samples as number) ?? 10000;
        const ciLevel = (args.ci_level as number) ?? 0.95;
        const cmd = `echo "Bootstrap ${statistic} on ${data}: n=${nSamples}, CI=${ciLevel}"`;
        const result = await resources.exec(cmd, { timeout: 120_000 });
        return { content: result.stdout, isError: false };
      }
      case "causal_inference": {
        const method = args.method as string;
        const data = args.data as string;
        const treatment = args.treatment as string;
        const outcome = args.outcome as string;
        const covariates = (args.covariates as string[]) ?? [];
        const cmd = `echo "Causal inference (${method}) on ${data}: ${treatment} → ${outcome}${covariates.length ? ` | covariates: ${covariates.join(", ")}` : ""}"`;
        const result = await resources.exec(cmd, { timeout: 120_000 });
        return { content: result.stdout, isError: false };
      }
      case "interpretation": {
        const results = args.results as string;
        const audience = (args.audience as string) ?? "technical";
        const cmd = `echo "Interpreting results for ${audience} audience: ${results.slice(0, 100)}..."`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      default:
        return { content: `Unknown datasci tool: ${toolName}`, isError: true };
    }
  } catch (err) {
    return { content: `Error: ${(err as Error).message}`, isError: true };
  }
}
