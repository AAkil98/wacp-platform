import type { LocalResources } from "./resources.js";
import type { AutonomyManager } from "./autonomy.js";
import type { SessionContext } from "./context.js";

// ---------------------------------------------------------------------------
// Workflow types (mirroring ecosystem vertical definitions)
// ---------------------------------------------------------------------------

/** A stage in a workflow. */
export interface WorkflowStage {
  id: string;
  name: string;
  roleId: string;
  dependsOn: readonly string[];
  gated: boolean;
}

/** A workflow — ordered stages forming a DAG. */
export interface Workflow {
  id: string;
  name: string;
  description: string;
  stages: readonly WorkflowStage[];
}

/** An agent profile — system prompt + tool whitelist. */
export interface AgentProfile {
  roleId: string;
  systemPrompt: string;
  tools: readonly string[];
  autonomy: "gated" | "autonomous";
}

// ---------------------------------------------------------------------------
// Execution types
// ---------------------------------------------------------------------------

/** Result of executing a single stage. */
export interface StageResult {
  stageId: string;
  roleId: string;
  content: string;
  success: boolean;
  /** Checkpoint: stage output preserved for downstream stages. */
  checkpoint: string;
}

/** Signal emitted at stage boundaries. */
export interface WorkflowSignal {
  type: "stage_started" | "stage_completed" | "stage_failed" | "gate_required" | "workflow_completed" | "workflow_failed";
  stageId?: string;
  roleId?: string;
  detail?: string;
}

/** Callbacks for the executor — how it interacts with the outside world. */
export interface ExecutorCallbacks {
  /** Execute a stage with the given profile and context. Returns the stage output. */
  executeStage: (
    profile: AgentProfile,
    goal: string,
    priorContext: string,
    stageId: string,
  ) => Promise<string>;

  /** Prompt the user for gate approval. Returns true if approved. */
  promptGate: (stage: WorkflowStage) => Promise<boolean>;

  /** Called when a signal is emitted. */
  onSignal: (signal: WorkflowSignal) => void;
}

/** Result of executing a full workflow. */
export interface WorkflowResult {
  workflowId: string;
  stages: StageResult[];
  success: boolean;
  /** Combined output from all stages. */
  finalOutput: string;
}

// ---------------------------------------------------------------------------
// WorkflowExecutor
// ---------------------------------------------------------------------------

/**
 * Executes a multi-stage workflow sequentially.
 *
 * Each stage:
 * 1. Resolves its profile (system prompt + tool whitelist)
 * 2. Checks gate (if gated → prompt human)
 * 3. Executes with the profile, passing prior stage output as context
 * 4. Records the result as a checkpoint
 * 5. Emits signals at boundaries
 */
export class WorkflowExecutor {
  constructor(
    private readonly profiles: AgentProfile[],
    private readonly context: SessionContext,
  ) {}

  /**
   * Execute a workflow from start to finish.
   *
   * Stages are executed in dependency order (topological sort).
   * Each stage receives the accumulated output from prior stages.
   */
  async execute(
    workflow: Workflow,
    goal: string,
    callbacks: ExecutorCallbacks,
  ): Promise<WorkflowResult> {
    const sorted = topologicalSort(workflow.stages);
    const stageResults: StageResult[] = [];
    let accumulatedContext = `Goal: ${goal}`;
    let allSucceeded = true;

    for (const stage of sorted) {
      // 1. Find profile for this stage's role
      const profile = this.profiles.find((p) => p.roleId === stage.roleId);
      if (!profile) {
        callbacks.onSignal({
          type: "stage_failed",
          stageId: stage.id,
          roleId: stage.roleId,
          detail: `No profile found for role '${stage.roleId}'`,
        });
        stageResults.push({
          stageId: stage.id,
          roleId: stage.roleId,
          content: "",
          success: false,
          checkpoint: "",
        });
        allSucceeded = false;
        break;
      }

      // 2. Gate check (if gated, prompt human)
      if (stage.gated) {
        callbacks.onSignal({
          type: "gate_required",
          stageId: stage.id,
          roleId: stage.roleId,
        });
        const approved = await callbacks.promptGate(stage);
        if (!approved) {
          callbacks.onSignal({
            type: "stage_failed",
            stageId: stage.id,
            detail: "Gate rejected by user",
          });
          stageResults.push({
            stageId: stage.id,
            roleId: stage.roleId,
            content: "",
            success: false,
            checkpoint: "",
          });
          allSucceeded = false;
          break;
        }
      }

      // 3. Signal stage start
      callbacks.onSignal({
        type: "stage_started",
        stageId: stage.id,
        roleId: stage.roleId,
      });

      // 4. Execute the stage
      try {
        const output = await callbacks.executeStage(
          profile,
          goal,
          accumulatedContext,
          stage.id,
        );

        // 5. Record checkpoint
        const checkpoint = `[${stage.name} (${stage.roleId})]\n${output}`;
        accumulatedContext += `\n\n${checkpoint}`;

        this.context.addHistory(`stage:${stage.id}: completed`);

        stageResults.push({
          stageId: stage.id,
          roleId: stage.roleId,
          content: output,
          success: true,
          checkpoint,
        });

        callbacks.onSignal({
          type: "stage_completed",
          stageId: stage.id,
          roleId: stage.roleId,
        });
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        callbacks.onSignal({
          type: "stage_failed",
          stageId: stage.id,
          roleId: stage.roleId,
          detail: message,
        });

        this.context.addHistory(`stage:${stage.id}: failed: ${message}`);

        stageResults.push({
          stageId: stage.id,
          roleId: stage.roleId,
          content: message,
          success: false,
          checkpoint: "",
        });

        allSucceeded = false;
        break;
      }
    }

    // Final signal
    callbacks.onSignal({
      type: allSucceeded ? "workflow_completed" : "workflow_failed",
    });

    return {
      workflowId: workflow.id,
      stages: stageResults,
      success: allSucceeded,
      finalOutput: stageResults
        .filter((s) => s.success)
        .map((s) => s.content)
        .join("\n\n"),
    };
  }
}

// ---------------------------------------------------------------------------
// Topological sort
// ---------------------------------------------------------------------------

function topologicalSort(stages: readonly WorkflowStage[]): WorkflowStage[] {
  const stageMap = new Map(stages.map((s) => [s.id, s]));
  const visited = new Set<string>();
  const result: WorkflowStage[] = [];

  function visit(id: string): void {
    if (visited.has(id)) return;
    visited.add(id);
    const stage = stageMap.get(id);
    if (!stage) return;
    for (const dep of stage.dependsOn) {
      visit(dep);
    }
    result.push(stage);
  }

  for (const stage of stages) {
    visit(stage.id);
  }

  return result;
}
