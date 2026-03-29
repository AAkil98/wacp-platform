import { describe, it, expect, beforeEach } from "vitest";
import { useStore, type TrailEntry, type PendingGate, type ActiveEscalation, type WorkspaceView, type TaskView } from "./index.js";

function resetStore() {
  useStore.setState({
    session: { state: "disconnected", userId: null, capabilities: [] },
    trail: { entries: [], paused: false },
    gates: { pending: new Map(), resolved: new Map(), inFlight: new Set() },
    escalations: { active: new Map(), resolved: new Map(), inFlight: new Set() },
    workspaces: { views: new Map(), changes: [] },
    taskGraph: { tasks: [], lastFetched: null },
  });
}

function makeEntry(seq: number): TrailEntry {
  return {
    timestamp: BigInt(1000 + seq),
    eventType: "test",
    actor: "worker",
    workspaceId: `ws-${seq}`,
    body: "{}",
    sequenceNumber: BigInt(seq),
  };
}

describe("HighwayStore", () => {
  beforeEach(resetStore);

  // ── Session ──

  it("sets session state", () => {
    useStore.getState().setSession("connected", "user-1", ["inject"]);
    const { session } = useStore.getState();
    expect(session.state).toBe("connected");
    expect(session.userId).toBe("user-1");
    expect(session.capabilities).toEqual(["inject"]);
  });

  it("preserves userId when not provided", () => {
    useStore.getState().setSession("connected", "user-1");
    useStore.getState().setSession("reconnecting");
    expect(useStore.getState().session.userId).toBe("user-1");
  });

  // ── Trail ──

  it("appends trail entries", () => {
    useStore.getState().appendTrailEntry(makeEntry(1));
    useStore.getState().appendTrailEntry(makeEntry(2));
    expect(useStore.getState().trail.entries).toHaveLength(2);
  });

  it("caps trail at 10,000 entries", () => {
    for (let i = 0; i < 10_005; i++) {
      useStore.getState().appendTrailEntry(makeEntry(i));
    }
    expect(useStore.getState().trail.entries).toHaveLength(10_000);
    // Oldest evicted: entry 0-4 gone, entry 5 is first
    expect(useStore.getState().trail.entries[0]!.sequenceNumber).toBe(BigInt(5));
  });

  it("toggles trail pause", () => {
    useStore.getState().setTrailPaused(true);
    expect(useStore.getState().trail.paused).toBe(true);
    useStore.getState().setTrailPaused(false);
    expect(useStore.getState().trail.paused).toBe(false);
  });

  // ── Gates ──

  it("manages gate lifecycle: pending → inFlight → resolved", () => {
    const gate: PendingGate = {
      gateId: "g-1",
      gateType: "task_approval",
      subject: new Uint8Array(),
      workspaceId: "ws-1",
      taskId: "t-1",
      timeoutMs: 30000n,
      fallbackAction: "approve",
      createdAt: 1000n,
    };

    useStore.getState().addPendingGate(gate);
    expect(useStore.getState().gates.pending.has("g-1")).toBe(true);

    useStore.getState().markGateInFlight("g-1");
    expect(useStore.getState().gates.inFlight.has("g-1")).toBe(true);

    useStore.getState().resolveGate("g-1", true);
    expect(useStore.getState().gates.pending.has("g-1")).toBe(false);
    expect(useStore.getState().gates.inFlight.has("g-1")).toBe(false);
    expect(useStore.getState().gates.resolved.get("g-1")?.applied).toBe(true);
  });

  // ── Escalations ──

  it("manages escalation lifecycle", () => {
    const esc: ActiveEscalation = {
      escalationId: "e-1",
      workspaceId: "ws-1",
      owner: "user-1",
      context: "help needed",
      createdAt: 1000n,
    };

    useStore.getState().addEscalation(esc);
    expect(useStore.getState().escalations.active.has("e-1")).toBe(true);

    useStore.getState().resolveEscalation("e-1", true);
    expect(useStore.getState().escalations.active.has("e-1")).toBe(false);
    expect(useStore.getState().escalations.resolved.has("e-1")).toBe(true);
  });

  // ── Workspaces ──

  it("upserts workspace views", () => {
    const ws: WorkspaceView = {
      id: "ws-1",
      state: "WORKSPACE_STATE_ACTIVE",
      role: "worker",
      parent: "root",
      owner: "user-1",
      originator: "System",
      taskId: "t-1",
      checkpointCount: 3,
    };

    useStore.getState().upsertWorkspace(ws);
    expect(useStore.getState().workspaces.views.get("ws-1")?.state).toBe(
      "WORKSPACE_STATE_ACTIVE",
    );

    useStore.getState().upsertWorkspace({ ...ws, state: "WORKSPACE_STATE_BLOCKED" });
    expect(useStore.getState().workspaces.views.get("ws-1")?.state).toBe(
      "WORKSPACE_STATE_BLOCKED",
    );
  });

  // ── Task graph ──

  it("sets task graph", () => {
    const tasks: TaskView[] = [
      {
        id: "t-1",
        name: "task-1",
        status: "TASK_STATUS_PENDING",
        workspaceRef: "",
        dependsOn: [],
        parentTask: "",
      },
    ];
    useStore.getState().setTaskGraph(tasks);
    expect(useStore.getState().taskGraph.tasks).toHaveLength(1);
    expect(useStore.getState().taskGraph.lastFetched).not.toBeNull();
  });
});
