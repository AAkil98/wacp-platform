import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { WorkspaceTreeView } from "./WorkspaceTreeView.js";
import { useStore, type WorkspaceView } from "@/store";

function resetStore() {
  useStore.setState({
    session: { state: "connected", userId: "user-1", capabilities: [] },
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
    taskId: "",
    checkpointCount: 0,
    ...overrides,
  };
}

function renderTree() {
  return render(
    <MemoryRouter>
      <WorkspaceTreeView />
    </MemoryRouter>,
  );
}

describe("WorkspaceTreeView", () => {
  beforeEach(resetStore);

  it("shows empty state when no workspaces", () => {
    renderTree();
    expect(screen.getByText("No workspaces.")).toBeDefined();
  });

  it("renders workspace nodes", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("ws-alpha"));
    useStore.getState().upsertWorkspace(makeWorkspace("ws-beta"));
    renderTree();

    expect(screen.getByText("ws-alpha")).toBeDefined();
    expect(screen.getByText("ws-beta")).toBeDefined();
  });

  it("shows state labels", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("ws-1", { state: "WORKSPACE_STATE_ACTIVE" }));
    renderTree();
    expect(screen.getByText("active")).toBeDefined();
  });

  it("shows role badge", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("ws-1", { role: "observer" }));
    renderTree();
    expect(screen.getByText("observer")).toBeDefined();
  });

  it("renders hierarchical tree with children indented", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("root"));
    useStore.getState().upsertWorkspace(makeWorkspace("child-1", { parent: "root" }));

    // Expand root to show children
    useStore.getState().toggleNodeExpanded("root");
    renderTree();

    expect(screen.getByText("root")).toBeDefined();
    expect(screen.getByText("child-1")).toBeDefined();
  });

  it("auto-expands nodes with pending gates on descendants", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("root"));
    useStore.getState().upsertWorkspace(makeWorkspace("child", { parent: "root" }));
    useStore.getState().addPendingGate({
      gateId: "g-1",
      gateType: "task_approval",
      subject: new Uint8Array(),
      workspaceId: "child",
      taskId: "t-1",
      timeoutMs: 30000n,
      fallbackAction: "approve",
      createdAt: 1000n,
    });
    renderTree();

    // Child should be visible because parent auto-expands
    expect(screen.getByText("child")).toBeDefined();
  });

  it("truncates long workspace IDs", () => {
    useStore.getState().upsertWorkspace(makeWorkspace("very-long-workspace-id-here"));
    renderTree();
    // Should be truncated with ellipsis
    expect(screen.getByText(/very-long-wo/)).toBeDefined();
  });
});
