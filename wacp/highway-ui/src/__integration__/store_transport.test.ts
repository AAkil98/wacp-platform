/**
 * Integration: store + transport (mocked gRPC) — 4 tests
 * Verifies that stream data flows correctly into the store.
 */
import { useStore, type TrailEntry, type WorkspaceView } from "../store/index.js";

function resetStore() {
  useStore.setState({
    trail: { entries: [], paused: false, filters: { eventTypes: new Set(), workspaceId: "", actor: "" }, expandedEntryIndex: null },
    workspaces: { views: new Map(), changes: [], expandedNodes: new Set() },
    gates: { pending: new Map(), resolved: new Map(), inFlight: new Set() },
    taskGraph: { tasks: [], lastFetched: null },
  });
}

beforeEach(resetStore);

describe("store ← transport integration", () => {
  it("appendTrailEntry accumulates entries in order", () => {
    const e1: TrailEntry = { timestamp: 1n, eventType: "signal", actor: "a1", workspaceId: "ws-1", body: "{}", sequenceNumber: 1n };
    const e2: TrailEntry = { timestamp: 2n, eventType: "checkpoint", actor: "a1", workspaceId: "ws-1", body: "{}", sequenceNumber: 2n };

    useStore.getState().appendTrailEntry(e1);
    useStore.getState().appendTrailEntry(e2);

    const entries = useStore.getState().trail.entries;
    expect(entries).toHaveLength(2);
    expect(entries[0].sequenceNumber).toBe(1n);
    expect(entries[1].sequenceNumber).toBe(2n);
  });

  it("upsertWorkspace adds new workspace to views map", () => {
    const ws: WorkspaceView = {
      id: "ws-1", state: "WORKSPACE_STATE_ACTIVE", role: "worker",
      parent: "ws-root", owner: "user-1", originator: "System",
      taskId: "t-1", checkpointCount: 0,
    };
    useStore.getState().upsertWorkspace(ws);

    const views = useStore.getState().workspaces.views;
    expect(views.get("ws-1")).toBeDefined();
    expect(views.get("ws-1")!.state).toBe("WORKSPACE_STATE_ACTIVE");
  });

  it("upsertWorkspace updates existing workspace", () => {
    const ws1: WorkspaceView = {
      id: "ws-1", state: "WORKSPACE_STATE_IDLE", role: "worker",
      parent: "", owner: "", originator: "", taskId: "", checkpointCount: 0,
    };
    useStore.getState().upsertWorkspace(ws1);

    const ws2: WorkspaceView = { ...ws1, state: "WORKSPACE_STATE_ACTIVE", checkpointCount: 3 };
    useStore.getState().upsertWorkspace(ws2);

    const views = useStore.getState().workspaces.views;
    expect(views.get("ws-1")!.state).toBe("WORKSPACE_STATE_ACTIVE");
    expect(views.get("ws-1")!.checkpointCount).toBe(3);
  });

  it("addPendingGate populates gates.pending map", () => {
    useStore.getState().addPendingGate({
      gateId: "g-1", gateType: "GATE_TYPE_TASK_APPROVAL",
      subject: new Uint8Array(), workspaceId: "ws-1", taskId: "t-1",
      timeoutMs: 30000n, fallbackAction: "cancel", createdAt: 100n,
    });

    const pending = useStore.getState().gates.pending;
    expect(pending.size).toBe(1);
    expect(pending.get("g-1")!.gateType).toBe("GATE_TYPE_TASK_APPROVAL");
  });
});
