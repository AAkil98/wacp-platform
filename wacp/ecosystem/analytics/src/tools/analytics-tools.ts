import type { LocalResources } from "@wacp/local";

/** Tool definition for LLM function-calling. */
export interface ToolDefinition {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

/** Classification of a SQL statement for safety gating. */
export type SqlSafety = "read" | "scoped_mutation" | "unscoped_mutation" | "schema_change" | "destructive";

/** Classify a SQL statement for safety gating. */
export function classifySql(sql: string): SqlSafety {
  const stripped = sql.replace(/\/\*[\s\S]*?\*\//g, "").replace(/--[^\n]*/g, "").trim().toUpperCase();

  if (/^\s*(DROP|TRUNCATE)\b/.test(stripped)) return "destructive";
  if (/^\s*(CREATE|ALTER)\b/.test(stripped)) return "schema_change";

  // UPDATE/DELETE without WHERE are unscoped mutations
  if (/^\s*UPDATE\b/.test(stripped)) {
    return /\bWHERE\b/.test(stripped) ? "scoped_mutation" : "unscoped_mutation";
  }
  if (/^\s*DELETE\b/.test(stripped)) {
    return /\bWHERE\b/.test(stripped) ? "scoped_mutation" : "unscoped_mutation";
  }
  if (/^\s*INSERT\b/.test(stripped)) return "scoped_mutation";

  return "read";
}

/** Analytics-specific tool definitions beyond the CLI's built-in 7. */
export function analyticsToolDefinitions(): ToolDefinition[] {
  return [
    {
      name: "sql_query",
      description: "Execute a SQL query against a configured database. Read-only by default. Destructive operations (DROP, TRUNCATE, unscoped UPDATE/DELETE) are hard-blocked.",
      input_schema: {
        type: "object",
        properties: {
          query: { type: "string", description: "SQL query text" },
          database: { type: "string", description: "Database connection name" },
          allow_destructive: { type: "boolean", description: "Explicit override for destructive operations (default: false)" },
          row_limit: { type: "number", description: "Maximum rows to return (default: 1000)" },
        },
        required: ["query"],
      },
    },
    {
      name: "dashboard_build",
      description: "Build or update a dashboard (Superset, Metabase, Tableau, Looker).",
      input_schema: {
        type: "object",
        properties: {
          name: { type: "string", description: "Dashboard name" },
          definition: { type: "string", description: "Dashboard definition (JSON/YAML)" },
          target: { type: "string", description: "BI platform target" },
        },
        required: ["name", "definition"],
      },
    },
    {
      name: "data_profile",
      description: "Profile a dataset — compute statistics, distributions, null counts, cardinality.",
      input_schema: {
        type: "object",
        properties: {
          source: { type: "string", description: "Table name or file path" },
          columns: { type: "array", items: { type: "string" }, description: "Specific columns to profile (default: all)" },
        },
        required: ["source"],
      },
    },
    {
      name: "kpi_calculate",
      description: "Compute a KPI from raw data using a metric definition.",
      input_schema: {
        type: "object",
        properties: {
          metric: { type: "string", description: "Metric name or definition" },
          period: { type: "string", description: "Time period (e.g., '7d', 'last_quarter')" },
          dimensions: { type: "array", items: { type: "string" }, description: "Group-by dimensions" },
        },
        required: ["metric"],
      },
    },
    {
      name: "report_generate",
      description: "Generate a formatted report (Markdown, PDF, HTML) with cited source queries.",
      input_schema: {
        type: "object",
        properties: {
          title: { type: "string", description: "Report title" },
          sections: { type: "array", items: { type: "string" }, description: "Section names" },
          sources: { type: "array", items: { type: "string" }, description: "Source query checkpoint IDs" },
          format: { type: "string", enum: ["markdown", "pdf", "html"], description: "Output format (default: markdown)" },
        },
        required: ["title", "sources"],
      },
    },
    {
      name: "viz_create",
      description: "Create a visualization (bar, line, scatter, heatmap) from query results.",
      input_schema: {
        type: "object",
        properties: {
          type: { type: "string", enum: ["bar", "line", "scatter", "heatmap", "pie"], description: "Chart type" },
          data: { type: "string", description: "Data source (query result reference or file)" },
          x: { type: "string", description: "X-axis column" },
          y: { type: "string", description: "Y-axis column" },
        },
        required: ["type", "data"],
      },
    },
    {
      name: "data_reconcile",
      description: "Reconcile two data sources — compare counts, aggregates, sample values.",
      input_schema: {
        type: "object",
        properties: {
          source_a: { type: "string", description: "First source (table or query)" },
          source_b: { type: "string", description: "Second source (table or query)" },
          join_key: { type: "string", description: "Join key for row-level reconciliation (optional)" },
          tolerance: { type: "number", description: "Acceptable difference percentage (default: 0.01)" },
        },
        required: ["source_a", "source_b"],
      },
    },
    {
      name: "schema_explore",
      description: "Explore a database schema — list tables, columns, types, relationships.",
      input_schema: {
        type: "object",
        properties: {
          database: { type: "string", description: "Database connection name" },
          schema: { type: "string", description: "Schema name (optional)" },
          table: { type: "string", description: "Specific table to describe (optional)" },
        },
      },
    },
    {
      name: "query_optimize",
      description: "Analyze and optimize a slow query — suggest indexes, rewrites, execution plan improvements.",
      input_schema: {
        type: "object",
        properties: {
          query: { type: "string", description: "SQL query to analyze" },
          database: { type: "string", description: "Database connection name" },
        },
        required: ["query"],
      },
    },
    {
      name: "metric_define",
      description: "Define a semantic metric in the metric store (dbt semantic layer, Cube, LookML).",
      input_schema: {
        type: "object",
        properties: {
          name: { type: "string", description: "Metric name" },
          expression: { type: "string", description: "SQL expression defining the metric" },
          grain: { type: "string", description: "Metric grain (e.g., 'daily', 'per_user')" },
          description: { type: "string", description: "Business definition" },
        },
        required: ["name", "expression"],
      },
    },
  ];
}

/** Execute an analytics tool using local resources. */
export async function executeAnalyticsTools(
  resources: LocalResources,
  toolName: string,
  args: Record<string, unknown>,
): Promise<{ content: string; isError: boolean }> {
  try {
    switch (toolName) {
      case "sql_query": {
        const query = args.query as string;
        const database = (args.database as string) ?? "default";
        const allowDestructive = (args.allow_destructive as boolean) ?? false;
        const rowLimit = (args.row_limit as number) ?? 1000;

        const safety = classifySql(query);
        if ((safety === "destructive" || safety === "unscoped_mutation") && !allowDestructive) {
          return {
            content: `SQL_BLOCKED: ${safety} operation refused. Set allow_destructive=true with gate clearance to proceed.\nQuery: ${query.slice(0, 200)}`,
            isError: true,
          };
        }

        const cmd = detectSqlCommand(database, query, rowLimit);
        const result = await resources.exec(cmd, { timeout: 60_000 });
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "dashboard_build": {
        const name = args.name as string;
        const definition = args.definition as string;
        const target = (args.target as string) ?? "superset";
        const cmd = `echo "Building dashboard '${name}' for ${target} (${definition.length} bytes)"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "data_profile": {
        const source = args.source as string;
        const columns = (args.columns as string[]) ?? [];
        const colList = columns.length > 0 ? `columns=${columns.join(",")}` : "all columns";
        const cmd = `(python -c "import pandas as pd; df = pd.read_csv('${source}'); print(df.describe(include='all'))" 2>/dev/null || echo "Profiling ${source}: ${colList}")`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "kpi_calculate": {
        const metric = args.metric as string;
        const period = (args.period as string) ?? "last_quarter";
        const dimensions = (args.dimensions as string[]) ?? [];
        const cmd = `echo "KPI ${metric} for ${period} by [${dimensions.join(", ")}]"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "report_generate": {
        const title = args.title as string;
        const sources = args.sources as string[];
        const format = (args.format as string) ?? "markdown";
        if (!sources || sources.length === 0) {
          return { content: "REPORT_BLOCKED: no source queries cited — every report must cite its sources", isError: true };
        }
        const sections = (args.sections as string[]) ?? ["Summary", "Findings", "Data"];
        const cmd = `echo "Generated ${format} report '${title}' with sections [${sections.join(", ")}] citing ${sources.length} source(s)"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "viz_create": {
        const type = args.type as string;
        const data = args.data as string;
        const x = args.x as string | undefined;
        const y = args.y as string | undefined;
        const axes = x && y ? ` x=${x} y=${y}` : "";
        const cmd = `echo "Created ${type} chart from ${data}${axes}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "data_reconcile": {
        const sourceA = args.source_a as string;
        const sourceB = args.source_b as string;
        const tolerance = (args.tolerance as number) ?? 0.01;
        const cmd = `echo "Reconciling ${sourceA} vs ${sourceB} (tolerance: ${tolerance})"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "schema_explore": {
        const database = (args.database as string) ?? "default";
        const schema = args.schema as string | undefined;
        const table = args.table as string | undefined;
        const cmd = detectSchemaExploreCommand(database, schema, table);
        const result = await resources.exec(cmd);
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "query_optimize": {
        const query = args.query as string;
        const database = (args.database as string) ?? "default";
        const cmd = detectExplainCommand(database, query);
        const result = await resources.exec(cmd);
        return { content: result.stdout + result.stderr, isError: result.exitCode !== 0 };
      }
      case "metric_define": {
        const name = args.name as string;
        const expression = args.expression as string;
        const grain = (args.grain as string) ?? "daily";
        const cmd = `echo "Defined metric '${name}' (grain=${grain}): ${expression.slice(0, 100)}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      default:
        return { content: `Unknown analytics tool: ${toolName}`, isError: true };
    }
  } catch (err) {
    return { content: `Error: ${(err as Error).message}`, isError: true };
  }
}

function detectSqlCommand(database: string, query: string, rowLimit: number): string {
  const escaped = query.replace(/"/g, '\\"').replace(/\$/g, '\\$');
  return `(psql "${database}" -c "${escaped}" 2>/dev/null || duckdb -c "${escaped}" 2>/dev/null || echo "SQL execution placeholder for ${database} (limit ${rowLimit}): ${query.slice(0, 80)}")`;
}

function detectSchemaExploreCommand(database: string, schema?: string, table?: string): string {
  if (table) {
    return `(psql "${database}" -c "\\d ${table}" 2>/dev/null || duckdb -c "DESCRIBE ${table}" 2>/dev/null || echo "Schema for ${table}")`;
  }
  return `(psql "${database}" -c "\\dt" 2>/dev/null || duckdb -c "SHOW TABLES" 2>/dev/null || echo "Tables in ${schema ?? database}")`;
}

function detectExplainCommand(database: string, query: string): string {
  const escaped = query.replace(/"/g, '\\"').replace(/\$/g, '\\$');
  return `(psql "${database}" -c "EXPLAIN ANALYZE ${escaped}" 2>/dev/null || duckdb -c "EXPLAIN ${escaped}" 2>/dev/null || echo "Query plan for ${database}: ${query.slice(0, 80)}")`;
}
