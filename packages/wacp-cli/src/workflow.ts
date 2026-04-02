import type { LocalSession, AgentProfile, Workflow, WorkflowResult, WorkflowSignal, WorkflowStage } from "@wacp/local";
import type { CliConfig } from "./config.js";
import type { AgentCallbacks } from "./agent.js";
import type { CoordinatorClient, AgentClient } from "./protocol-client.js";
import { stageAgentLoop } from "./agent.js";
import { formatGatePrompt } from "./display.js";

export type { Workflow, AgentProfile, WorkflowResult };

export interface DetectedTaskType {
  id: string;
  workflowId: string;
}

/** Protocol clients — null when runtime is not available. */
export interface ProtocolClients {
  coordinator: CoordinatorClient | null;
  agent: AgentClient | null;
}

/**
 * Detect the SWE task type from a user goal using keyword heuristics.
 */
export function detectTaskType(goal: string): DetectedTaskType {
  const lower = goal.toLowerCase();
  if (lower.match(/\b(fix|bug|error|crash|broken|issue|debug)\b/))
    return { id: "swe:debug", workflowId: "swe:fix-bug" };
  if (lower.match(/\b(refactor|restructure|reorganize|simplify|clean\s*up)\b/))
    return { id: "swe:refactor", workflowId: "swe:refactor" };
  if (lower.match(/\b(test|tests|coverage|spec|specs)\b/) && !lower.match(/\b(implement|add|build|create)\b/))
    return { id: "swe:test", workflowId: "swe:write-tests" };
  if (lower.match(/\b(review|evaluate|assess|check\s+quality)\b/))
    return { id: "swe:review", workflowId: "swe:review-only" };
  if (lower.match(/\b(document|docs|readme|comments)\b/))
    return { id: "swe:document", workflowId: "swe:document-only" };
  if (lower.match(/\b(investigate|research|explore|understand|how\s+does)\b/))
    return { id: "swe:investigate", workflowId: "swe:investigate-only" };
  return { id: "swe:implement", workflowId: "swe:implement-feature" };
}

/**
 * Execute a goal through its detected workflow — protocol-aware.
 *
 * When protocol clients are available:
 * 1. SubmitGoal → creates root workspace in runtime
 * 2. Decompose → creates task graph from workflow stages
 * 3. Per stage: Dispatch → creates child workspace, agent binds + executes
 *
 * When protocol clients are null (no runtime):
 * Falls back to local-only execution via WorkflowExecutor.
 */
export async function executeGoalWithWorkflow(
  session: LocalSession,
  config: CliConfig,
  goal: string,
  workflow: Workflow,
  profiles: AgentProfile[],
  callbacks: AgentCallbacks,
  clients: ProtocolClients,
  signal?: AbortSignal,
): Promise<WorkflowResult> {
  callbacks.print(`\n━━━ Workflow: ${workflow.name} (${workflow.stages.length} stages) ━━━`);

  // Submit goal to runtime if connected
  let goalId: string | null = null;
  if (clients.coordinator) {
    try {
      const result = await clients.coordinator.submitGoal(goal);
      goalId = result.goalId;
      callbacks.print(`  [protocol] goal ${goalId} submitted, root workspace ${result.rootWorkspaceId}`);

      // Decompose into tasks
      const tasks = workflow.stages.map((stage) => ({
        name: stage.id,
        description: `${stage.name} (${stage.roleId})`,
        dependsOn: stage.dependsOn,
        role: stage.roleId,
        directivePayload: Buffer.from(goal),
        tools: profiles.find((p) => p.roleId === stage.roleId)?.tools ?? [],
      }));
      const taskIds = await clients.coordinator.decompose(tasks);
      callbacks.print(`  [protocol] decomposed into ${taskIds.length} tasks: ${taskIds.join(", ")}`);
    } catch (err) {
      callbacks.print(`  [protocol] decompose failed: ${(err as Error).message}, falling back to local`);
      clients = { coordinator: null, agent: null };
    }
  }

  // Execute stages via WorkflowExecutor with protocol integration
  const result = await session.executeWorkflow(workflow, profiles, goal, {
    executeStage: async (profile, stageGoal, priorContext, stageId) => {
      callbacks.print(`\n── Stage: ${stageId} (${profile.roleId}) ──`);

      // Dispatch workspace via coordinator
      let workspaceId: string | null = null;
      let authToken: string | null = null;
      if (clients.coordinator) {
        try {
          const dispatched = await clients.coordinator.dispatch(
            stageId,
            profile.roleId,
            Buffer.from(stageGoal),
            profile.tools,
          );
          workspaceId = dispatched.workspaceId;
          authToken = "cli-agent-token"; // runtime assigns token at dispatch
          callbacks.print(`  [protocol] workspace ${workspaceId} dispatched`);
        } catch (err) {
          callbacks.print(`  [protocol] dispatch failed: ${(err as Error).message}`);
        }
      }

      return stageAgentLoop(
        session, config, stageGoal, priorContext, profile, callbacks,
        clients.agent, workspaceId, authToken, signal,
      );
    },

    promptGate: async (stage: WorkflowStage) => {
      const prompt = formatGatePrompt(stage.name, `stage:${stage.id}`, `Proceed to ${stage.name} (${stage.roleId})?`);
      const answer = await callbacks.promptGate(prompt);
      const lower = answer.toLowerCase().trim();
      return lower === "y" || lower === "yes" || lower === "a" || lower === "always";
    },

    onSignal: (sig: WorkflowSignal) => {
      switch (sig.type) {
        case "stage_started": callbacks.print(`▶ ${sig.stageId} started (${sig.roleId})`); break;
        case "stage_completed": callbacks.print(`✓ ${sig.stageId} completed`); break;
        case "stage_failed": callbacks.print(`✗ ${sig.stageId} failed: ${sig.detail ?? "unknown"}`); break;
        case "workflow_completed": callbacks.print(`\n━━━ Workflow completed ━━━`); break;
        case "workflow_failed": callbacks.print(`\n━━━ Workflow failed ━━━`); break;
        default: break;
      }
    },
  });

  return result;
}
