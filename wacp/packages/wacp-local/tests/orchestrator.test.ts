import { describe, it, expect } from "vitest";
import {
  WorkflowExecutor,
  type Workflow,
  type AgentProfile,
  type ExecutorCallbacks,
  type WorkflowSignal,
} from "../src/orchestrator.js";
import { SessionContext } from "../src/context.js";

// --- Test fixtures ---

const plannerProfile: AgentProfile = {
  roleId: "swe:planner",
  systemPrompt: "You are a planner. Analyze and plan.",
  tools: ["read_file", "code_search"],
  autonomy: "gated",
};

const implementerProfile: AgentProfile = {
  roleId: "swe:implementer",
  systemPrompt: "You are an implementer. Write code.",
  tools: ["read_file", "write_file", "code_edit"],
  autonomy: "gated",
};

const testerProfile: AgentProfile = {
  roleId: "swe:tester",
  systemPrompt: "You are a tester. Write and run tests.",
  tools: ["read_file", "write_file", "test_run"],
  autonomy: "gated",
};

const reviewerProfile: AgentProfile = {
  roleId: "swe:reviewer",
  systemPrompt: "You are a reviewer. Review changes.",
  tools: ["read_file", "git_diff"],
  autonomy: "autonomous",
};

const allProfiles = [plannerProfile, implementerProfile, testerProfile, reviewerProfile];

const twoStageWorkflow: Workflow = {
  id: "test:two-stage",
  name: "Two Stage",
  description: "Plan then implement",
  stages: [
    { id: "plan", name: "Plan", roleId: "swe:planner", dependsOn: [], gated: false },
    { id: "implement", name: "Implement", roleId: "swe:implementer", dependsOn: ["plan"], gated: true },
  ],
};

const fourStageWorkflow: Workflow = {
  id: "swe:implement-feature",
  name: "Implement Feature",
  description: "Plan → implement → test → review",
  stages: [
    { id: "plan", name: "Plan", roleId: "swe:planner", dependsOn: [], gated: false },
    { id: "implement", name: "Implement", roleId: "swe:implementer", dependsOn: ["plan"], gated: true },
    { id: "test", name: "Test", roleId: "swe:tester", dependsOn: ["implement"], gated: false },
    { id: "review", name: "Review", roleId: "swe:reviewer", dependsOn: ["test"], gated: true },
  ],
};

function makeCallbacks(
  options: {
    approveGates?: boolean;
    executeOutput?: string | ((profile: AgentProfile, stageId: string) => string);
    failStage?: string;
  } = {},
): { callbacks: ExecutorCallbacks; signals: WorkflowSignal[]; executedStages: string[] } {
  const signals: WorkflowSignal[] = [];
  const executedStages: string[] = [];

  const callbacks: ExecutorCallbacks = {
    executeStage: async (profile, _goal, _context, stageId) => {
      executedStages.push(stageId);

      if (options.failStage === stageId) {
        throw new Error(`Stage ${stageId} failed`);
      }

      if (typeof options.executeOutput === "function") {
        return options.executeOutput(profile, stageId);
      }
      return options.executeOutput ?? `output from ${stageId}`;
    },
    promptGate: async () => options.approveGates ?? true,
    onSignal: (signal) => signals.push(signal),
  };

  return { callbacks, signals, executedStages };
}

// --- Tests ---

describe("WorkflowExecutor", () => {
  it("executes a two-stage workflow in order", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const { callbacks, executedStages } = makeCallbacks({ approveGates: true });

    const result = await executor.execute(twoStageWorkflow, "implement auth", callbacks);

    expect(result.success).toBe(true);
    expect(result.stages).toHaveLength(2);
    expect(executedStages).toEqual(["plan", "implement"]);
  });

  it("executes a four-stage workflow in dependency order", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const { callbacks, executedStages } = makeCallbacks({ approveGates: true });

    const result = await executor.execute(fourStageWorkflow, "add login page", callbacks);

    expect(result.success).toBe(true);
    expect(result.stages).toHaveLength(4);
    expect(executedStages).toEqual(["plan", "implement", "test", "review"]);
  });

  it("passes correct profile to each stage", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const receivedProfiles: Record<string, string> = {};

    const { callbacks } = makeCallbacks({
      approveGates: true,
      executeOutput: (profile, stageId) => {
        receivedProfiles[stageId] = profile.roleId;
        return `done by ${profile.roleId}`;
      },
    });

    await executor.execute(fourStageWorkflow, "test", callbacks);

    expect(receivedProfiles["plan"]).toBe("swe:planner");
    expect(receivedProfiles["implement"]).toBe("swe:implementer");
    expect(receivedProfiles["test"]).toBe("swe:tester");
    expect(receivedProfiles["review"]).toBe("swe:reviewer");
  });

  it("passes prior stage output as context to next stage", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const receivedContexts: Record<string, string> = {};

    const { callbacks } = makeCallbacks({
      approveGates: true,
      executeOutput: (profile, stageId) => {
        return `[${stageId}-output]`;
      },
    });

    // Override executeStage to capture context
    callbacks.executeStage = async (_profile, _goal, context, stageId) => {
      receivedContexts[stageId] = context;
      return `[${stageId}-output]`;
    };

    await executor.execute(twoStageWorkflow, "build it", callbacks);

    // Plan stage gets the goal
    expect(receivedContexts["plan"]).toContain("build it");
    // Implement stage gets plan output
    expect(receivedContexts["implement"]).toContain("[plan-output]");
  });

  it("gated stage pauses and stops on rejection", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const { callbacks, executedStages, signals } = makeCallbacks({
      approveGates: false, // reject all gates
    });

    const result = await executor.execute(twoStageWorkflow, "test", callbacks);

    // Plan executes (not gated), implement is gated and rejected
    expect(executedStages).toEqual(["plan"]);
    expect(result.success).toBe(false);
    expect(result.stages).toHaveLength(2);
    expect(result.stages[1].success).toBe(false);
    expect(signals.some((s) => s.type === "gate_required")).toBe(true);
  });

  it("gated stage proceeds on approval", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const { callbacks, executedStages } = makeCallbacks({ approveGates: true });

    const result = await executor.execute(twoStageWorkflow, "test", callbacks);

    expect(executedStages).toEqual(["plan", "implement"]);
    expect(result.success).toBe(true);
  });

  it("emits correct signals at stage boundaries", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const { callbacks, signals } = makeCallbacks({ approveGates: true });

    await executor.execute(twoStageWorkflow, "test", callbacks);

    const types = signals.map((s) => s.type);
    expect(types).toContain("stage_started");
    expect(types).toContain("stage_completed");
    expect(types).toContain("gate_required");
    expect(types).toContain("workflow_completed");
    expect(types).not.toContain("workflow_failed");
  });

  it("failed stage stops workflow and emits failure signal", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const { callbacks, signals } = makeCallbacks({
      approveGates: true,
      failStage: "implement",
    });

    const result = await executor.execute(twoStageWorkflow, "test", callbacks);

    expect(result.success).toBe(false);
    expect(signals.some((s) => s.type === "stage_failed")).toBe(true);
    expect(signals.some((s) => s.type === "workflow_failed")).toBe(true);
  });

  it("missing profile for role stops workflow", async () => {
    const ctx = new SessionContext();
    // Only provide planner profile — implementer missing
    const executor = new WorkflowExecutor([plannerProfile], ctx);
    const { callbacks, signals } = makeCallbacks({ approveGates: true });

    const result = await executor.execute(twoStageWorkflow, "test", callbacks);

    // Plan succeeds, implement fails (no profile)
    expect(result.success).toBe(false);
    expect(signals.some((s) => s.type === "stage_failed" && s.detail?.includes("No profile"))).toBe(true);
  });

  it("records stage completions in session context", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const { callbacks } = makeCallbacks({ approveGates: true });

    await executor.execute(twoStageWorkflow, "test", callbacks);

    expect(ctx.history.some((h) => h.includes("stage:plan: completed"))).toBe(true);
    expect(ctx.history.some((h) => h.includes("stage:implement: completed"))).toBe(true);
  });

  it("records stage failure in session context", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const { callbacks } = makeCallbacks({ approveGates: true, failStage: "implement" });

    await executor.execute(twoStageWorkflow, "test", callbacks);

    expect(ctx.history.some((h) => h.includes("stage:implement: failed"))).toBe(true);
  });

  it("finalOutput combines successful stage outputs", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const { callbacks } = makeCallbacks({
      approveGates: true,
      executeOutput: (_p, stageId) => `${stageId} done`,
    });

    const result = await executor.execute(twoStageWorkflow, "test", callbacks);

    expect(result.finalOutput).toContain("plan done");
    expect(result.finalOutput).toContain("implement done");
  });

  it("stages have checkpoints with role annotations", async () => {
    const ctx = new SessionContext();
    const executor = new WorkflowExecutor(allProfiles, ctx);
    const { callbacks } = makeCallbacks({ approveGates: true });

    const result = await executor.execute(twoStageWorkflow, "test", callbacks);

    expect(result.stages[0].checkpoint).toContain("swe:planner");
    expect(result.stages[1].checkpoint).toContain("swe:implementer");
  });
});
