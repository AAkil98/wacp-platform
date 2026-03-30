import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { WorkspaceDetailPanel } from "./WorkspaceDetailPanel.js";
import { useStore, type WorkspaceView } from "@/store";

function resetStore() {
  useStore.setState({
    session: { state: "connected", userId: "user-1", capabilities: ["inject"] },
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

function makeWorkspace(id: string, overrides?: Partial<WorkspaceView>): WorkspaceView {
  return {
    id,
    state: "WORKSPACE_STATE_ACTIVE",
    role: "worker",
    parent: "",
    owner: "user-1",
    originator: "System",
    taskId: "t-1",
    checkpointCount: 5,
    ...overrides,
  };
}

function renderPanel(workspaceId: string) {
  return render(
    <MemoryRouter initialEntries={[`/workspaces/${workspaceId}`]}>
      <Routes>
        <Route path="/workspaces/:workspaceId" element={<WorkspaceDetailPanel />} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("WorkspaceDetailPanel", () => {
  beforeEach(resetStore);

  it("shows not found for unknown workspace", () => {
    renderPanel("ws-unknown");
    expect(screen.getByText(/Workspace not found/)).toBeDefined();
  });

  it("renders workspace header with all fields", () => {
    useStore.getState().upsertWorkspace(
      makeWorkspace("ws-detail", {
        role: "worker",
        owner: "user-1",
        originator: "System",
      }),
    );
    renderPanel("ws-detail");

    expect(screen.getByText("ws-detail")).toBeDefined();
    expect(screen.getByText("active")).toBeDefined();
    expect(screen.getByText("worker")).toBeDefined();
    expect(screen.getByText("user-1")).toBeDefined();
    expect(screen.getByText("System")).toBeDefined();
  });

  it("shows checkpoint count", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("ws-cp", { checkpointCount: 7 }));
    renderPanel("ws-cp");
    expect(screen.getByText("7 checkpoints recorded")).toBeDefined();
  });

  it("shows inject envelope button", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("ws-inject"));
    renderPanel("ws-inject");
    expect(screen.getByText("Inject Envelope")).toBeDefined();
  });

  it("renders resource meter with budget", () => {
    useStore.getState().upsertWorkspace(
      makeWorkspace("ws-res", {
        currentUsage: {
          tokens: 5000n,
          wallTimeMs: 30000n,
          storageBytes: 512000n,
          networkBytes: 1024n,
          costMicros: 500000n,
        },
        budget: {
          maxTokens: 10000n,
          maxWallTimeMs: 60000n,
          maxStorageBytes: 1048576n,
          maxNetworkBytes: 1048576n,
          maxCostMicros: 1000000n,
          warningThreshold: 0.8,
        },
      }),
    );
    renderPanel("ws-res");
    expect(screen.getByText("Tokens")).toBeDefined();
    expect(screen.getByText("Wall time")).toBeDefined();
    expect(screen.getByText("Storage")).toBeDefined();
    expect(screen.getByText("Network")).toBeDefined();
    expect(screen.getByText("Cost")).toBeDefined();
  });

  it("shows no budget message when budget is absent", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("ws-nobudget"));
    renderPanel("ws-nobudget");
    expect(screen.getByText("No budget assigned")).toBeDefined();
  });
});
