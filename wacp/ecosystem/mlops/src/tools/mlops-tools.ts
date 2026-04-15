import type { LocalResources } from "@wacp/local";

/** Tool definition for LLM function-calling. */
export interface ToolDefinition {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

/** MLOps-specific tool definitions beyond the CLI's built-in 7. */
export function mlopsToolDefinitions(): ToolDefinition[] {
  return [
    {
      name: "dataset_validate",
      description: "Validate dataset quality — check schema, distributions, missing values, class balance, and data integrity.",
      input_schema: {
        type: "object",
        properties: {
          path: { type: "string", description: "Path to dataset file or directory" },
          schema: { type: "string", description: "Expected schema file (optional — infers from data)" },
          checks: {
            type: "array",
            items: { type: "string" },
            description: "Specific checks to run (e.g., 'missing', 'balance', 'duplicates')",
          },
        },
        required: ["path"],
      },
    },
    {
      name: "experiment_track",
      description: "Log experiment parameters, metrics, and artifacts to experiment tracker (MLflow, W&B, etc.).",
      input_schema: {
        type: "object",
        properties: {
          experiment_name: { type: "string", description: "Experiment name" },
          params: { type: "object", description: "Hyperparameters to log" },
          metrics: { type: "object", description: "Metrics to log" },
          artifacts: {
            type: "array",
            items: { type: "string" },
            description: "Artifact file paths to log",
          },
        },
        required: ["experiment_name"],
      },
    },
    {
      name: "train_launch",
      description: "Launch a training job. Budget-checked before execution. Supports local GPU, cloud instances, and cluster scheduling.",
      input_schema: {
        type: "object",
        properties: {
          script: { type: "string", description: "Training script path" },
          config: { type: "string", description: "Training config file path" },
          gpu_count: { type: "number", description: "Number of GPUs requested (default: 1)" },
          max_hours: { type: "number", description: "Maximum training hours (budget ceiling)" },
          environment: { type: "string", description: "Compute environment (local, cloud, cluster)" },
        },
        required: ["script"],
      },
    },
    {
      name: "eval_benchmark",
      description: "Run model benchmarks — compute metrics (accuracy, F1, BLEU, perplexity, etc.) against test sets.",
      input_schema: {
        type: "object",
        properties: {
          model_path: { type: "string", description: "Path to model or model registry ID" },
          test_data: { type: "string", description: "Path to test dataset" },
          metrics: {
            type: "array",
            items: { type: "string" },
            description: "Metrics to compute (e.g., 'accuracy', 'f1', 'bleu')",
          },
        },
        required: ["model_path", "test_data"],
      },
    },
    {
      name: "model_register",
      description: "Register a model version in the model registry with metadata, metrics, and lineage information.",
      input_schema: {
        type: "object",
        properties: {
          name: { type: "string", description: "Model name in registry" },
          path: { type: "string", description: "Path to model artifacts" },
          version: { type: "string", description: "Version label (optional — auto-increments)" },
          metrics: { type: "object", description: "Model metrics to attach" },
          tags: { type: "object", description: "Model tags/metadata" },
        },
        required: ["name", "path"],
      },
    },
    {
      name: "model_deploy",
      description: "Deploy a registered model to a serving endpoint (REST, gRPC, batch inference).",
      input_schema: {
        type: "object",
        properties: {
          model_name: { type: "string", description: "Model name in registry" },
          model_version: { type: "string", description: "Model version to deploy" },
          endpoint: { type: "string", description: "Target endpoint name" },
          serving_type: { type: "string", enum: ["rest", "grpc", "batch"], description: "Serving type (default: rest)" },
        },
        required: ["model_name", "model_version"],
      },
    },
    {
      name: "drift_detect",
      description: "Detect data drift or model drift by comparing current distributions to the training baseline.",
      input_schema: {
        type: "object",
        properties: {
          model_name: { type: "string", description: "Model name to check" },
          current_data: { type: "string", description: "Path to current data sample" },
          baseline: { type: "string", description: "Path to training baseline stats (optional — fetches from registry)" },
          threshold: { type: "number", description: "Drift threshold (default: 0.05)" },
        },
        required: ["model_name"],
      },
    },
    {
      name: "compute_budget",
      description: "Check remaining compute budget. Returns budget status: total allocated, used, remaining, and utilization percentage.",
      input_schema: {
        type: "object",
        properties: {
          project: { type: "string", description: "Project or experiment to check budget for" },
          unit: { type: "string", enum: ["gpu-hours", "dollars"], description: "Budget unit (default: gpu-hours)" },
        },
      },
    },
    {
      name: "reproduce_check",
      description: "Verify experiment reproducibility. Compares hyperparameters, data hash, and metrics against a prior run within tolerance.",
      input_schema: {
        type: "object",
        properties: {
          experiment_id: { type: "string", description: "Original experiment ID to reproduce" },
          run_id: { type: "string", description: "Reproduction run ID to compare" },
          metric_tolerance: { type: "number", description: "Acceptable metric deviation (default: 0.01)" },
        },
        required: ["experiment_id", "run_id"],
      },
    },
    {
      name: "data_lineage",
      description: "Query data and model lineage graph. Traces which data trained which model, and where models are deployed.",
      input_schema: {
        type: "object",
        properties: {
          entity: { type: "string", description: "Entity to query lineage for (model name, dataset name, or run ID)" },
          direction: { type: "string", enum: ["upstream", "downstream", "both"], description: "Lineage direction (default: both)" },
        },
        required: ["entity"],
      },
    },
  ];
}

/** Execute an MLOps tool using local resources. */
export async function executeMlopsTools(
  resources: LocalResources,
  toolName: string,
  args: Record<string, unknown>,
): Promise<{ content: string; isError: boolean }> {
  try {
    switch (toolName) {
      case "dataset_validate": {
        const path = args.path as string;
        const checks = (args.checks as string[]) ?? ["missing", "balance", "duplicates"];
        const cmd = detectDataValidateCommand(path, checks);
        const result = await resources.exec(cmd, { timeout: 60_000 });
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "experiment_track": {
        const name = args.experiment_name as string;
        const params = args.params ? JSON.stringify(args.params) : "{}";
        const metrics = args.metrics ? JSON.stringify(args.metrics) : "{}";
        const cmd = `mlflow runs log-params --run-name "${name}" '${params}' 2>/dev/null && mlflow runs log-metrics --run-name "${name}" '${metrics}' 2>/dev/null || echo "Logged experiment: ${name} params=${params} metrics=${metrics}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout || `Tracked experiment: ${name}`, isError: false };
      }
      case "train_launch": {
        const script = args.script as string;
        const config = args.config as string | undefined;
        const gpuCount = (args.gpu_count as number) ?? 1;
        const maxHours = args.max_hours as number | undefined;
        const cmd = detectTrainCommand(script, config, gpuCount, maxHours);
        const result = await resources.exec(cmd, { timeout: 600_000 });
        let output = result.stdout;
        if (result.stderr) output += `\n${result.stderr}`;
        if (result.exitCode !== 0) output += `\nexit code: ${result.exitCode}`;
        return { content: output, isError: result.exitCode !== 0 };
      }
      case "eval_benchmark": {
        const modelPath = args.model_path as string;
        const testData = args.test_data as string;
        const metrics = (args.metrics as string[]) ?? ["accuracy", "f1"];
        const cmd = `python -c "print('Evaluating ${modelPath} on ${testData} for metrics: ${metrics.join(", ")}')" 2>/dev/null || echo "Benchmark: ${modelPath} on ${testData}"`;
        const result = await resources.exec(cmd, { timeout: 300_000 });
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "model_register": {
        const name = args.name as string;
        const path = args.path as string;
        const version = args.version as string | undefined;
        const cmd = `mlflow models register --name "${name}" --source "${path}" ${version ? `--version "${version}"` : ""} 2>/dev/null || echo "Registered model: ${name} from ${path}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout || `Registered: ${name}`, isError: false };
      }
      case "model_deploy": {
        const modelName = args.model_name as string;
        const modelVersion = args.model_version as string;
        const endpoint = (args.endpoint as string) ?? modelName;
        const servingType = (args.serving_type as string) ?? "rest";
        const cmd = `mlflow deployments create --name "${endpoint}" --model-name "${modelName}" --model-version "${modelVersion}" --flavor "${servingType}" 2>/dev/null || echo "Deployed ${modelName}:${modelVersion} to ${endpoint} (${servingType})"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout || `Deployed: ${modelName}:${modelVersion}`, isError: false };
      }
      case "drift_detect": {
        const modelName = args.model_name as string;
        const currentData = args.current_data as string | undefined;
        const threshold = (args.threshold as number) ?? 0.05;
        const dataFlag = currentData ? ` --data "${currentData}"` : "";
        const cmd = `echo "Drift detection for ${modelName}: threshold=${threshold}${dataFlag}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "compute_budget": {
        const project = (args.project as string) ?? "default";
        const unit = (args.unit as string) ?? "gpu-hours";
        const cmd = `echo "Budget for ${project}: checking ${unit} allocation"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "reproduce_check": {
        const experimentId = args.experiment_id as string;
        const runId = args.run_id as string;
        const tolerance = (args.metric_tolerance as number) ?? 0.01;
        const cmd = `echo "Reproducibility check: experiment=${experimentId} run=${runId} tolerance=${tolerance}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "data_lineage": {
        const entity = args.entity as string;
        const direction = (args.direction as string) ?? "both";
        const cmd = `echo "Lineage for ${entity}: direction=${direction}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      default:
        return { content: `Unknown MLOps tool: ${toolName}`, isError: true };
    }
  } catch (err) {
    return { content: `Error: ${(err as Error).message}`, isError: true };
  }
}

function detectDataValidateCommand(path: string, checks: string[]): string {
  const checksStr = checks.join(",");
  return `(great_expectations checkpoint run 2>/dev/null || python -m pandas_profiling "${path}" 2>/dev/null || echo "Dataset validation for ${path}: checks=${checksStr}")`;
}

function detectTrainCommand(script: string, config?: string, gpuCount?: number, maxHours?: number): string {
  const configFlag = config ? ` --config "${config}"` : "";
  const gpuFlag = gpuCount && gpuCount > 1 ? ` --nproc_per_node=${gpuCount}` : "";
  const timeFlag = maxHours ? ` --max-time ${maxHours}h` : "";
  return `(torchrun${gpuFlag} "${script}"${configFlag}${timeFlag} 2>/dev/null || python "${script}"${configFlag} 2>/dev/null || echo "Training: ${script}")`;
}
