import { describe, it, expect, beforeEach } from "vitest";
import {
  useStore,
  selectFilteredTrail,
  selectWorkspaceTree,
  type TrailEntry,
  type PendingGate,
  type ActiveEscalation,
  type WorkspaceView,
  type TaskView,
} from "./index.js";

function resetStore() {
  useStore.setState({
    session: { state: "disconnected", userId: null, capabilities: [] },
    trail: {
      entries: [],
      paused: false,
      filters: { eventTypes: new Set(), workspaceId: "", actor: "" },
      expandedEntryIndex: null,
    },
    gates: { pending: new Map(), resolved: new Map(), inFlight: new Set() },
    escalations: { active: new Map(), resolved: new Map(), inFlight: new Set() },
    workspaces: { views: new Map(), changes: [], expandedNodes: new Set() },
    taskGraph: { tasks: [], lastFetched: null },
  });
}

function makeEntry(seq: number, overrides?: Partial<TrailEntry>): TrailEntry {
  return {
    timestamp: BigInt(1000 + seq),
    eventType: "test",
    actor: "worker",
    workspaceId: `ws-${seq}`,
    body: "{}",
    sequenceNumber: BigInt(seq),
    ...overrides,
  };
}

function makeWorkspace(id: string, overrides?: Partial<WorkspaceView>): WorkspaceView {
  return {
    id,
    state: "WORKSPACE_STATE_ACTIVE",
    role: "worker",
    parent: "",
    owner: "user-1",
    originator: "System",
    taskId: "",
    checkpointCount: 0,
    ...overrides,
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
    expect(useStore.getState().trail.entries[0]!.sequenceNumber).toBe(BigInt(5));
  });

  it("toggles trail pause", () => {
    useStore.getState().setTrailPaused(true);
    expect(useStore.getState().trail.paused).toBe(true);
    useStore.getState().setTrailPaused(false);
    expect(useStore.getState().trail.paused).toBe(false);
  });

  // ── Trail Filters ──

  it("sets trail filter", () => {
    useStore.getState().setTrailFilter({ workspaceId: "ws-42" });
    expect(useStore.getState().trail.filters.workspaceId).toBe("ws-42");
    // Other filters unchanged
    expect(useStore.getState().trail.filters.actor).toBe("");
    expect(useStore.getState().trail.filters.eventTypes.size).toBe(0);
  });

  it("sets event type filter", () => {
    useStore.getState().setTrailFilter({ eventTypes: new Set(["gate_triggered", "signal_emitted"]) });
    const { eventTypes } = useStore.getState().trail.filters;
    expect(eventTypes.has("gate_triggered")).toBe(true);
    expect(eventTypes.has("signal_emitted")).toBe(true);
    expect(eventTypes.size).toBe(2);
  });

  it("clears all trail filters", () => {
    useStore.getState().setTrailFilter({ workspaceId: "ws-1", actor: "agent-A" });
    useStore.getState().clearTrailFilters();
    const { filters } = useStore.getState().trail;
    expect(filters.workspaceId).toBe("");
    expect(filters.actor).toBe("");
    expect(filters.eventTypes.size).toBe(0);
  });

  it("tracks expanded entry index", () => {
    useStore.getState().setExpandedEntry(5);
    expect(useStore.getState().trail.expandedEntryIndex).toBe(5);
    useStore.getState().setExpandedEntry(null);
    expect(useStore.getState().trail.expandedEntryIndex).toBeNull();
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
    const ws = makeWorkspace("ws-1");
    useStore.getState().upsertWorkspace(ws);
    expect(useStore.getState().workspaces.views.get("ws-1")?.state).toBe(
      "WORKSPACE_STATE_ACTIVE",
    );

    useStore.getState().upsertWorkspace({ ...ws, state: "WORKSPACE_STATE_BLOCKED" });
    expect(useStore.getState().workspaces.views.get("ws-1")?.state).toBe(
      "WORKSPACE_STATE_BLOCKED",
    );
  });

  it("stores workspace view with resource usage", () => {
    const ws = makeWorkspace("ws-res", {
      currentUsage: {
        tokens: 5000n,
        wallTimeMs: 12000n,
        storageBytes: 1024n,
        networkBytes: 2048n,
        costMicros: 100n,
      },
      budget: {
        maxTokens: 10000n,
        maxWallTimeMs: 60000n,
        maxStorageBytes: 1048576n,
        maxNetworkBytes: 1048576n,
        maxCostMicros: 1000000n,
        warningThreshold: 0.8,
      },
    });
    useStore.getState().upsertWorkspace(ws);
    const stored = useStore.getState().workspaces.views.get("ws-res")!;
    expect(stored.currentUsage?.tokens).toBe(5000n);
    expect(stored.budget?.maxTokens).toBe(10000n);
    expect(stored.budget?.warningThreshold).toBe(0.8);
  });

  it("toggles node expansion", () => {
    useStore.getState().toggleNodeExpanded("ws-1");
    expect(useStore.getState().workspaces.expandedNodes.has("ws-1")).toBe(true);
    useStore.getState().toggleNodeExpanded("ws-1");
    expect(useStore.getState().workspaces.expandedNodes.has("ws-1")).toBe(false);
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

// ── Selectors ──

describe("selectFilteredTrail", () => {
  beforeEach(resetStore);

  it("returns all entries when no filters active", () => {
    useStore.getState().appendTrailEntry(makeEntry(1));
    useStore.getState().appendTrailEntry(makeEntry(2));
    const result = selectFilteredTrail(useStore.getState().trail);
    expect(result).toHaveLength(2);
  });

  it("filters by event type", () => {
    useStore.getState().appendTrailEntry(makeEntry(1, { eventType: "gate_triggered" }));
    useStore.getState().appendTrailEntry(makeEntry(2, { eventType: "signal_emitted" }));
    useStore.getState().appendTrailEntry(makeEntry(3, { eventType: "gate_triggered" }));
    useStore.getState().setTrailFilter({ eventTypes: new Set(["gate_triggered"]) });

    const result = selectFilteredTrail(useStore.getState().trail);
    expect(result).toHaveLength(2);
    expect(result.every((e) => e.eventType === "gate_triggered")).toBe(true);
  });

  it("filters by workspace ID", () => {
    useStore.getState().appendTrailEntry(makeEntry(1, { workspaceId: "ws-alpha" }));
    useStore.getState().appendTrailEntry(makeEntry(2, { workspaceId: "ws-beta" }));
    useStore.getState().appendTrailEntry(makeEntry(3, { workspaceId: "ws-alpha" }));
    useStore.getState().setTrailFilter({ workspaceId: "ws-alpha" });

    const result = selectFilteredTrail(useStore.getState().trail);
    expect(result).toHaveLength(2);
    expect(result.every((e) => e.workspaceId === "ws-alpha")).toBe(true);
  });

  it("filters by actor", () => {
    useStore.getState().appendTrailEntry(makeEntry(1, { actor: "agent-A" }));
    useStore.getState().appendTrailEntry(makeEntry(2, { actor: "agent-B" }));
    useStore.getState().setTrailFilter({ actor: "agent-A" });

    const result = selectFilteredTrail(useStore.getState().trail);
    expect(result).toHaveLength(1);
    expect(result[0]!.actor).toBe("agent-A");
  });

  it("composes multiple filters with AND logic", () => {
    useStore.getState().appendTrailEntry(
      makeEntry(1, { eventType: "gate_triggered", workspaceId: "ws-1", actor: "agent-A" }),
    );
    useStore.getState().appendTrailEntry(
      makeEntry(2, { eventType: "gate_triggered", workspaceId: "ws-2", actor: "agent-A" }),
    );
    useStore.getState().appendTrailEntry(
      makeEntry(3, { eventType: "signal_emitted", workspaceId: "ws-1", actor: "agent-A" }),
    );
    useStore.getState().setTrailFilter({
      eventTypes: new Set(["gate_triggered"]),
      workspaceId: "ws-1",
    });

    const result = selectFilteredTrail(useStore.getState().trail);
    expect(result).toHaveLength(1);
    expect(result[0]!.sequenceNumber).toBe(1n);
  });
});

describe("selectWorkspaceTree", () => {
  beforeEach(resetStore);

  it("returns empty array for no workspaces", () => {
    const tree = selectWorkspaceTree(useStore.getState().workspaces);
    expect(tree).toEqual([]);
  });

  it("builds tree from parent-child relationships", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("root", { parent: "" }));
    useStore.getState().upsertWorkspace(makeWorkspace("child-1", { parent: "root" }));
    useStore.getState().upsertWorkspace(makeWorkspace("child-2", { parent: "root" }));

    const tree = selectWorkspaceTree(useStore.getState().workspaces);
    expect(tree).toHaveLength(1);
    expect(tree[0]!.workspace.id).toBe("root");
    expect(tree[0]!.children).toHaveLength(2);
    expect(tree[0]!.children.map((c) => c.workspace.id).sort()).toEqual(["child-1", "child-2"]);
  });

  it("treats workspaces with unknown parent as roots", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("orphan", { parent: "missing-parent" }));
    useStore.getState().upsertWorkspace(makeWorkspace("root", { parent: "" }));

    const tree = selectWorkspaceTree(useStore.getState().workspaces);
    expect(tree).toHaveLength(2);
    expect(tree.map((n) => n.workspace.id).sort()).toEqual(["orphan", "root"]);
  });

  it("builds deep tree (3 levels)", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("root", { parent: "" }));
    useStore.getState().upsertWorkspace(makeWorkspace("mid", { parent: "root" }));
    useStore.getState().upsertWorkspace(makeWorkspace("leaf", { parent: "mid" }));

    const tree = selectWorkspaceTree(useStore.getState().workspaces);
    expect(tree).toHaveLength(1);
    expect(tree[0]!.children).toHaveLength(1);
    expect(tree[0]!.children[0]!.children).toHaveLength(1);
    expect(tree[0]!.children[0]!.children[0]!.workspace.id).toBe("leaf");
  });

  it("reflects expanded state from store", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("root", { parent: "" }));
    useStore.getState().upsertWorkspace(makeWorkspace("child", { parent: "root" }));

    let tree = selectWorkspaceTree(useStore.getState().workspaces);
    expect(tree[0]!.expanded).toBe(false);

    useStore.getState().toggleNodeExpanded("root");
    tree = selectWorkspaceTree(useStore.getState().workspaces);
    expect(tree[0]!.expanded).toBe(true);
  });
});
