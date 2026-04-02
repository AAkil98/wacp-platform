import type { LocalSession, AgentProfile, Workflow, WorkflowResult, WorkflowSignal, WorkflowStage } from "@wacp/local";
import type { CliConfig } from "./config.js";
import type { AgentCallbacks } from "./agent.js";
import { stageAgentLoop } from "./agent.js";
import { formatGatePrompt } from "./display.js";

// Re-export SWE vertical types for convenience
export type { Workflow, AgentProfile, WorkflowResult };

/** Task type detection result. */
export interface DetectedTaskType {
  id: string;
  workflowId: string;
}

/**
 * Detect the SWE task type from a user goal using keyword heuristics.
 * Returns the task type ID and its default workflow ID.
 */
export function detectTaskType(goal: string): DetectedTaskType {
  const lower = goal.toLowerCase();

  if (lower.match(/\b(fix|bug|error|crash|broken|issue|debug)\b/)) {
    return { id: "swe:debug", workflowId: "swe:fix-bug" };
  }
  if (lower.match(/\b(refactor|restructure|reorganize|simplify|clean\s*up)\b/)) {
    return { id: "swe:refactor", workflowId: "swe:refactor" };
  }
  if (lower.match(/\b(test|tests|coverage|spec|specs)\b/) && !lower.match(/\b(implement|add|build|create)\b/)) {
    return { id: "swe:test", workflowId: "swe:write-tests" };
  }
  if (lower.match(/\b(review|evaluate|assess|check\s+quality)\b/)) {
    return { id: "swe:review", workflowId: "swe:review-only" };
  }
  if (lower.match(/\b(document|docs|readme|comments)\b/)) {
    return { id: "swe:document", workflowId: "swe:document-only" };
  }
  if (lower.match(/\b(investigate|research|explore|understand|how\s+does)\b/)) {
    return { id: "swe:investigate", workflowId: "swe:investigate-only" };
  }

  // Default: implement feature
  return { id: "swe:implement", workflowId: "swe:implement-feature" };
}

/**
 * Execute a goal through its detected workflow.
 *
 * This is the protocol-aware path: goal → task type → workflow → stages with profiles.
 */
export async function executeGoalWithWorkflow(
  session: LocalSession,
  config: CliConfig,
  goal: string,
  workflow: Workflow,
  profiles: AgentProfile[],
  callbacks: AgentCallbacks,
  signal?: AbortSignal,
): Promise<WorkflowResult> {
  callbacks.print(`\n━━━ Workflow: ${workflow.name} (${workflow.stages.length} stages) ━━━`);

  const result = await session.executeWorkflow(workflow, profiles, goal, {
    executeStage: async (profile, stageGoal, priorContext, stageId) => {
      // Filter tools to only those whitelisted for this role
      callbacks.print(`\n── Stage: ${stageId} (${profile.roleId}) ──`);

      const output = await stageAgentLoop(
        session,
        config,
        stageGoal,
        priorContext,
        profile,
        callbacks,
        signal,
      );

      return output;
    },

    promptGate: async (stage: WorkflowStage) => {
      const prompt = formatGatePrompt(
        stage.name,
        `stage:${stage.id}`,
        `Proceed to ${stage.name} (${stage.roleId})?`,
      );
      const answer = await callbacks.promptGate(prompt);
      const lower = answer.toLowerCase().trim();
      return lower === "y" || lower === "yes" || lower === "a" || lower === "always";
    },

    onSignal: (signal: WorkflowSignal) => {
      switch (signal.type) {
        case "stage_started":
          callbacks.print(`▶ ${signal.stageId} started (${signal.roleId})`);
          break;
        case "stage_completed":
          callbacks.print(`✓ ${signal.stageId} completed`);
          break;
        case "stage_failed":
          callbacks.print(`✗ ${signal.stageId} failed: ${signal.detail ?? "unknown"}`);
          break;
        case "gate_required":
          // Gate prompt is handled by promptGate callback
          break;
        case "workflow_completed":
          callbacks.print(`\n━━━ Workflow completed ━━━`);
          break;
        case "workflow_failed":
          callbacks.print(`\n━━━ Workflow failed ━━━`);
          break;
      }
    },
  });

  return result;
}
