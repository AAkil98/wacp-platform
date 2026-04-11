import type { LocalResources } from "@wacp/local";
import { SWE_VERTICAL } from "@wacp/swe";
import { DEVOPS_VERTICAL } from "@wacp/devops";
import { MLOPS_VERTICAL } from "@wacp/mlops";
import { FINANCE_VERTICAL } from "@wacp/finance";
import { HEALTHCARE_VERTICAL } from "@wacp/healthcare";
import { ANALYTICS_VERTICAL } from "@wacp/analytics";
import { DATASCI_VERTICAL } from "@wacp/datasci";

/** Tool definition shape — structurally compatible with each vertical's local declaration. */
export interface VerticalToolDefinition {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

/** Workflow stage shape — structurally compatible with each vertical's local declaration. */
export interface VerticalWorkflowStage {
  id: string;
  name: string;
  roleId: string;
  dependsOn: readonly string[];
  gated: boolean;
}

/** Workflow shape — structurally compatible with each vertical's local declaration. */
export interface VerticalWorkflow {
  id: string;
  name: string;
  description: string;
  stages: readonly VerticalWorkflowStage[];
}

/** Profile shape — structurally compatible with each vertical's local declaration. */
export interface VerticalProfile {
  roleId: string;
  systemPrompt: string;
  tools: readonly string[];
  autonomy: "gated" | "autonomous";
}

/** Detected task type returned by a vertical's detector. */
export interface DetectedTaskType {
  id: string;
  workflowId: string;
}

/** Result of executing a vertical tool. */
export interface VerticalToolResult {
  content: string;
  isError: boolean;
}

/** Policy kind discriminant for tool-layer enforcement rules. */
export type ToolPolicyKind =
  | "requires_checkpoint"    // prior checkpoint of a specific type must exist in trail or as arg
  | "requires_gate"          // protocol gate must be cleared (e.g. production env approval)
  | "budget_limited"         // execution blocked if a resource budget would be exceeded
  | "classification_gated";  // input is classified; blocked unless an override flag is set

/**
 * A tool-layer policy rule — describes how the vertical enforces a pre-condition
 * before allowing a tool to execute. Surfaced by the Console so users aren't
 * surprised at runtime.
 */
export interface ToolPolicy {
  kind: ToolPolicyKind;
  description: string;
  /** requires_checkpoint: the checkpoint type that must exist. */
  checkpoint_type?: string;
  /** requires_checkpoint: a tool-arg field that must match the checkpoint's recorded value. */
  matching_field?: string;
  /** requires_checkpoint: freshness window in milliseconds. */
  expires_after_ms?: number;
  /** requires_gate: human-readable condition that triggers the gate. */
  gate_condition?: string;
  /** budget_limited: the tool-arg field carrying the requested amount. */
  budget_field?: string;
  /** classification_gated: the classification values blocked by default. */
  blocked_classifications?: readonly string[];
  /** classification_gated: boolean arg flag that bypasses the block (with gate clearance). */
  override_flag?: string;
}

/** A typed field in a vertical's session-launch context schema. */
export interface ContextField {
  type: "string" | "number" | "boolean" | "enum";
  required: boolean;
  description: string;
  enum_values?: readonly string[];
  default?: string | number | boolean;
}

/** A field within a vertical-specific checkpoint type. */
export interface CheckpointField {
  name: string;
  type: "string" | "number" | "boolean" | "enum";
  description: string;
  enum_values?: readonly string[];
}

/** Schema for a vertical-specific checkpoint type. */
export interface CheckpointSchema {
  description: string;
  fields: readonly CheckpointField[];
}

/** Serializable quality criterion (without the evaluate function). */
export interface QualityCriterion {
  id: string;
  name: string;
  description: string;
  /** Relative weight — used by the Console when computing an aggregate score. 1.0 = equal weight. */
  weight: number;
}

/** Declarative task type with representative detection keywords. */
export interface TaskTypeDescriptor {
  id: string;
  name: string;
  description: string;
  workflow_id: string;
  /** Representative keywords (not full regex — for display and search purposes). */
  keywords: readonly string[];
}

/**
 * A loaded vertical descriptor — the runtime contract between the CLI and a
 * vertical package. Each vertical's index.ts exports a constant matching this
 * shape; the CLI consumes them via structural typing without importing from
 * any vertical-specific type module.
 */
export interface LoadedVertical {
  id: string;
  name: string;
  workflows: readonly VerticalWorkflow[];
  profiles: readonly VerticalProfile[];
  toolDefinitions: readonly VerticalToolDefinition[];
  detectTaskType: (goal: string) => DetectedTaskType | null;
  executeTool: (
    resources: LocalResources,
    name: string,
    args: Record<string, unknown>,
  ) => Promise<VerticalToolResult>;
  toolOperation: (name: string) => string | null;
  /** Human-readable description of the vertical's defining constraint. */
  defining_constraint: string;
  /** Typed schema for context tags the session launcher must collect before dispatch. */
  context_schema: Readonly<Record<string, ContextField>>;
  /** Tool-layer policy rules keyed by tool name. */
  tool_policies: Readonly<Record<string, ToolPolicy>>;
  /** Vertical-specific checkpoint types with field schemas. */
  checkpoint_types: Readonly<Record<string, CheckpointSchema>>;
  /** Serializable quality criteria (without evaluate functions). */
  quality_criteria: readonly QualityCriterion[];
  /** Declarative task type descriptors with representative keywords. */
  task_types: readonly TaskTypeDescriptor[];
}

/** A loaded ecosystem — collection of verticals with O(1) tool lookup. */
export interface LoadedEcosystem {
  verticals: LoadedVertical[];
  toolByName: Map<string, LoadedVertical>;
}

/** Registry of all available verticals, keyed by id. */
const REGISTRY: Record<string, LoadedVertical> = {
  swe: SWE_VERTICAL as LoadedVertical,
  devops: DEVOPS_VERTICAL as LoadedVertical,
  mlops: MLOPS_VERTICAL as LoadedVertical,
  finance: FINANCE_VERTICAL as LoadedVertical,
  healthcare: HEALTHCARE_VERTICAL as LoadedVertical,
  analytics: ANALYTICS_VERTICAL as LoadedVertical,
  datasci: DATASCI_VERTICAL as LoadedVertical,
};

/**
 * Default load order. Domain verticals are evaluated by the router BEFORE the
 * SWE catchall so a goal containing both a domain keyword and a SWE keyword
 * (e.g. "implement the trade execution flow") routes to the more specific
 * domain vertical (finance) rather than to SWE.
 */
export const DEFAULT_LOAD_ORDER: readonly string[] = [
  "devops",
  "mlops",
  "finance",
  "healthcare",
  "analytics",
  "datasci",
  "swe",
];

/** Look up the registry of known vertical IDs. */
export function knownVerticalIds(): string[] {
  return Object.keys(REGISTRY);
}

/**
 * Load an ecosystem of verticals.
 *
 * @param enabledIds — list of vertical IDs to load (default: all 7 in DEFAULT_LOAD_ORDER).
 *                    Unknown IDs are silently skipped; the caller is responsible for warning.
 * @throws if two loaded verticals declare a tool with the same name (collisions are illegal).
 */
export function loadEcosystem(enabledIds?: readonly string[]): LoadedEcosystem {
  const ids = (enabledIds && enabledIds.length > 0) ? enabledIds : DEFAULT_LOAD_ORDER;
  const verticals: LoadedVertical[] = [];
  const toolByName = new Map<string, LoadedVertical>();

  for (const id of ids) {
    const v = REGISTRY[id];
    if (!v) continue;
    verticals.push(v);

    for (const td of v.toolDefinitions) {
      const existing = toolByName.get(td.name);
      if (existing && existing.id !== v.id) {
        throw new Error(
          `Tool name collision: '${td.name}' is declared by both '${existing.id}' and '${v.id}'. ` +
          `Tool names must be globally unique across loaded verticals.`,
        );
      }
      toolByName.set(td.name, v);
    }
  }

  return { verticals, toolByName };
}

/**
 * Resolve a goal to its target vertical and workflow by trying each loaded
 * vertical's detector in load order. Returns the first match. If no detector
 * matches, falls back to the SWE catchall (which always returns a value).
 */
export interface RoutedGoal {
  verticalId: string;
  taskType: string;
  workflowId: string;
  workflow: VerticalWorkflow | undefined;
  profiles: readonly VerticalProfile[];
}

export function routeGoal(goal: string, ecosystem: LoadedEcosystem): RoutedGoal {
  for (const vertical of ecosystem.verticals) {
    const detected = vertical.detectTaskType(goal);
    if (detected) {
      return {
        verticalId: vertical.id,
        taskType: detected.id,
        workflowId: detected.workflowId,
        workflow: vertical.workflows.find((w) => w.id === detected.workflowId),
        profiles: vertical.profiles,
      };
    }
  }

  // No vertical claimed the goal — fall back to SWE's catchall.
  const swe = ecosystem.verticals.find((v) => v.id === "swe");
  if (swe) {
    const detected = swe.detectTaskType(goal) ?? { id: "swe:implement", workflowId: "swe:implement-feature" };
    return {
      verticalId: "swe",
      taskType: detected.id,
      workflowId: detected.workflowId,
      workflow: swe.workflows.find((w) => w.id === detected.workflowId),
      profiles: swe.profiles,
    };
  }

  // SWE is not loaded — return a synthetic fallback. The caller falls back to direct mode.
  return {
    verticalId: "swe",
    taskType: "swe:implement",
    workflowId: "swe:implement-feature",
    workflow: undefined,
    profiles: [],
  };
}

/** Compose the full tool definition list: built-in (passed in) + every loaded vertical's tools. */
export function composeToolDefinitions(
  builtin: readonly VerticalToolDefinition[],
  ecosystem: LoadedEcosystem,
): VerticalToolDefinition[] {
  const result: VerticalToolDefinition[] = [...builtin];
  for (const vertical of ecosystem.verticals) {
    for (const td of vertical.toolDefinitions) {
      result.push(td);
    }
  }
  return result;
}
