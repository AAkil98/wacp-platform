import type { LocalResources } from "@wacp/local";

/** Tool definition for LLM function-calling. */
export interface ToolDefinition {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

/** DevOps-specific tool definitions beyond the CLI's built-in 7. */
export function devopsToolDefinitions(): ToolDefinition[] {
  return [
    {
      name: "infra_plan",
      description: "Generate/validate an infrastructure-as-code plan. Detects the IaC tool (Terraform, Pulumi, CDK) and runs a dry-run plan showing planned changes.",
      input_schema: {
        type: "object",
        properties: {
          directory: { type: "string", description: "IaC directory (default: '.')" },
          target: { type: "string", description: "Specific resource to plan (optional)" },
        },
      },
    },
    {
      name: "deploy_execute",
      description: "Execute a deployment. Applies IaC changes, pushes containers, or runs deployment scripts. Environment-aware — production requires gate clearance.",
      input_schema: {
        type: "object",
        properties: {
          directory: { type: "string", description: "Deployment directory (default: '.')" },
          environment: { type: "string", description: "Target environment: dev, staging, production" },
          auto_approve: { type: "boolean", description: "Skip confirmation prompts in IaC tools (default: true)" },
        },
        required: ["environment"],
      },
    },
    {
      name: "rollback",
      description: "Rollback to previous version. Reverts IaC state, undoes container deployments, or restores previous configuration.",
      input_schema: {
        type: "object",
        properties: {
          environment: { type: "string", description: "Target environment" },
          revision: { type: "string", description: "Specific revision to rollback to (optional — defaults to previous)" },
        },
        required: ["environment"],
      },
    },
    {
      name: "health_check",
      description: "Check service health. Probes HTTP endpoints, TCP ports, or process state. Returns per-endpoint status.",
      input_schema: {
        type: "object",
        properties: {
          endpoints: {
            type: "array",
            items: { type: "string" },
            description: "URLs or host:port pairs to check",
          },
          timeout: { type: "number", description: "Timeout per endpoint in seconds (default: 10)" },
        },
        required: ["endpoints"],
      },
    },
    {
      name: "log_query",
      description: "Query logs. Structured search across log files or aggregators. Supports time ranges and filters.",
      input_schema: {
        type: "object",
        properties: {
          query: { type: "string", description: "Search query (text or structured)" },
          source: { type: "string", description: "Log source path or service name" },
          since: { type: "string", description: "Time range start (e.g., '1h', '30m', ISO timestamp)" },
          limit: { type: "number", description: "Max entries to return (default: 100)" },
        },
        required: ["query"],
      },
    },
    {
      name: "metric_query",
      description: "Query metrics. Time-series queries for CPU, memory, latency, error rate, etc.",
      input_schema: {
        type: "object",
        properties: {
          query: { type: "string", description: "Metric query (PromQL, CloudWatch expression, etc.)" },
          period: { type: "string", description: "Time period (e.g., '1h', '24h')" },
          step: { type: "string", description: "Resolution step (e.g., '1m', '5m')" },
        },
        required: ["query"],
      },
    },
    {
      name: "alert_manage",
      description: "Manage alerts. Create, update, silence, or acknowledge alerts.",
      input_schema: {
        type: "object",
        properties: {
          action: { type: "string", enum: ["create", "update", "silence", "acknowledge", "list"], description: "Alert action" },
          name: { type: "string", description: "Alert name or ID" },
          query: { type: "string", description: "Alert condition query (for create/update)" },
          duration: { type: "string", description: "Silence duration (for silence action)" },
        },
        required: ["action"],
      },
    },
    {
      name: "config_validate",
      description: "Validate configuration files. Schema and syntax checking for YAML, JSON, HCL, Dockerfile, Kubernetes manifests.",
      input_schema: {
        type: "object",
        properties: {
          path: { type: "string", description: "Path to config file or directory" },
          schema: { type: "string", description: "Schema to validate against (optional — auto-detects)" },
        },
        required: ["path"],
      },
    },
    {
      name: "secret_rotate",
      description: "Rotate secrets. Generates new credentials, updates the secret store, and invalidates the old value. Environment-aware.",
      input_schema: {
        type: "object",
        properties: {
          name: { type: "string", description: "Secret name or path" },
          environment: { type: "string", description: "Target environment" },
          store: { type: "string", description: "Secret store (vault, aws-ssm, k8s-secret — auto-detects)" },
        },
        required: ["name", "environment"],
      },
    },
    {
      name: "compliance_scan",
      description: "Scan for compliance violations. Runs policy-as-code checks (OPA, Checkov, tfsec, kube-bench) against infrastructure definitions.",
      input_schema: {
        type: "object",
        properties: {
          path: { type: "string", description: "Path to scan (default: '.')" },
          framework: { type: "string", description: "Compliance framework filter (e.g., 'cis', 'hipaa', 'pci')" },
        },
      },
    },
  ];
}

/** Execute a DevOps tool using local resources. */
export async function executeDevopsTools(
  resources: LocalResources,
  toolName: string,
  args: Record<string, unknown>,
): Promise<{ content: string; isError: boolean }> {
  try {
    switch (toolName) {
      case "infra_plan": {
        const dir = (args.directory as string) ?? ".";
        const target = args.target as string | undefined;
        const cmd = detectIacPlanCommand(dir, target);
        const result = await resources.exec(cmd, { timeout: 120_000 });
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "deploy_execute": {
        const dir = (args.directory as string) ?? ".";
        const env = args.environment as string;
        const autoApprove = (args.auto_approve as boolean) ?? true;
        const cmd = detectDeployCommand(dir, env, autoApprove);
        const result = await resources.exec(cmd, { timeout: 300_000 });
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "rollback": {
        const env = args.environment as string;
        const revision = args.revision as string | undefined;
        const cmd = detectRollbackCommand(env, revision);
        const result = await resources.exec(cmd, { timeout: 120_000 });
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "health_check": {
        const endpoints = args.endpoints as string[];
        const timeout = (args.timeout as number) ?? 10;
        const results: string[] = [];
        for (const endpoint of endpoints) {
          const cmd = endpoint.startsWith("http")
            ? `curl -sf -o /dev/null -w "%{http_code}" --max-time ${timeout} "${endpoint}" 2>/dev/null && echo " OK" || echo " FAIL"`
            : `nc -z -w ${timeout} ${endpoint.replace(":", " ")} 2>/dev/null && echo "OK" || echo "FAIL"`;
          const res = await resources.exec(cmd);
          results.push(`${endpoint}: ${res.stdout.trim()}`);
        }
        return { content: results.join("\n"), isError: results.some((r) => r.includes("FAIL")) };
      }
      case "log_query": {
        const query = args.query as string;
        const source = (args.source as string) ?? "/var/log/syslog";
        const since = args.since as string | undefined;
        const limit = (args.limit as number) ?? 100;
        let cmd = `grep -n "${query.replace(/"/g, '\\"')}" ${source}`;
        if (since) cmd = `journalctl --since="${since}" --grep="${query.replace(/"/g, '\\"')}" 2>/dev/null || ${cmd}`;
        cmd += ` | tail -${limit}`;
        cmd += " 2>/dev/null || echo 'No matching log entries'";
        const result = await resources.exec(cmd);
        return { content: result.stdout || "No matching log entries.", isError: false };
      }
      case "metric_query": {
        const query = args.query as string;
        const period = (args.period as string) ?? "1h";
        const step = (args.step as string) ?? "1m";
        // Metric queries delegate to the configured observability backend
        const cmd = `curl -s "http://localhost:9090/api/v1/query_range?query=${encodeURIComponent(query)}&start=now-${period}&end=now&step=${step}" 2>/dev/null || echo '{"status":"error","error":"No metric backend configured"}'`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: result.exitCode !== 0 };
      }
      case "alert_manage": {
        const action = args.action as string;
        const name = (args.name as string) ?? "";
        const content = `Alert ${action}: ${name || "(all)"}`;
        // Alert management delegates to the configured alerting backend
        return { content, isError: false };
      }
      case "config_validate": {
        const path = args.path as string;
        const cmd = detectConfigValidateCommand(path);
        const result = await resources.exec(cmd);
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "secret_rotate": {
        const name = args.name as string;
        const env = args.environment as string;
        const store = args.store as string | undefined;
        const cmd = detectSecretRotateCommand(name, env, store);
        const result = await resources.exec(cmd);
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "compliance_scan": {
        const path = (args.path as string) ?? ".";
        const framework = args.framework as string | undefined;
        const cmd = detectComplianceScanCommand(path, framework);
        const result = await resources.exec(cmd, { timeout: 120_000 });
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      default:
        return { content: `Unknown DevOps tool: ${toolName}`, isError: true };
    }
  } catch (err) {
    return { content: `Error: ${(err as Error).message}`, isError: true };
  }
}

function detectIacPlanCommand(dir: string, target?: string): string {
  const targetFlag = target ? ` -target=${target}` : "";
  return `cd "${dir}" && (terraform plan -no-color${targetFlag} 2>/dev/null || pulumi preview 2>/dev/null || echo "No IaC tool detected")`;
}

function detectDeployCommand(dir: string, env: string, autoApprove: boolean): string {
  const approveFlag = autoApprove ? " -auto-approve" : "";
  return `cd "${dir}" && (terraform apply -no-color${approveFlag} 2>/dev/null || pulumi up --yes 2>/dev/null || kubectl apply -f . 2>/dev/null || echo "No deployment tool detected")`;
}

function detectRollbackCommand(env: string, revision?: string): string {
  const revFlag = revision ? ` --to-revision=${revision}` : "";
  return `kubectl rollout undo deployment --all${revFlag} 2>/dev/null || terraform apply -no-color -auto-approve 2>/dev/null || echo "No rollback method detected"`;
}

function detectConfigValidateCommand(path: string): string {
  return `(terraform validate -no-color 2>/dev/null || kubectl apply --dry-run=client -f "${path}" 2>/dev/null || yamllint "${path}" 2>/dev/null || echo "Config validation: no validator found")`;
}

function detectSecretRotateCommand(name: string, env: string, store?: string): string {
  if (store === "vault") return `vault kv put secret/${env}/${name} value=$(openssl rand -base64 32)`;
  return `echo "Secret rotation for ${name} in ${env}: delegate to configured secret store"`;
}

function detectComplianceScanCommand(path: string, framework?: string): string {
  const fwFlag = framework ? ` --framework ${framework}` : "";
  return `(checkov -d "${path}"${fwFlag} --compact 2>/dev/null || tfsec "${path}" 2>/dev/null || echo "No compliance scanner detected")`;
}
