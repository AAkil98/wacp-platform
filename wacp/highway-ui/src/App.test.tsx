import { render, screen } from "@testing-library/react";
import { useStore } from "@/store";

// Mock sessionManager to avoid real connections
vi.mock("@/transport/session.js", () => ({
  sessionManager: { connect: vi.fn() },
}));

// Mock transport client for components that use it
vi.mock("@/transport/client.js", () => ({
  getClient: () => ({
    getCheckpoint: vi.fn(),
    getTaskGraph: vi.fn().mockResolvedValue({ tasks: [] }),
    getWorkspace: vi.fn(),
    streamTrail: vi.fn(),
    streamWorkspaceChanges: vi.fn(),
    streamGates: vi.fn(),
    streamEscalations: vi.fn(),
  }),
  initTransport: vi.fn(),
  getTransport: vi.fn(),
}));

import { App } from "./App.js";

function setConnected() {
  useStore.setState({
    session: { state: "connected", userId: "user-1", capabilities: [] },
    gates: { pending: new Map(), resolved: new Map(), inFlight: new Set() },
    escalations: { active: new Map(), resolved: new Map(), inFlight: new Set() },
    trail: { entries: [], paused: false, filters: { eventTypes: new Set(), workspaceId: "", actor: "" }, expandedEntryIndex: null },
    workspaces: { views: new Map(), changes: [], expandedNodes: new Set() },
    taskGraph: { tasks: [], lastFetched: null },
    notifications: { escalationBanner: null },
  });
}

describe("App routing", () => {
  it("shows LoginScreen when disconnected", () => {
    useStore.setState({
      session: { state: "disconnected", userId: "", capabilities: [] },
    });
    render(<App />);
    expect(screen.getByText("WACP Highway")).toBeInTheDocument();
    expect(screen.getByPlaceholderText(/paste token/i)).toBeInTheDocument();
  });

  it("renders layout when connected", () => {
    setConnected();
    render(<App />);
    // Sidebar nav items should be visible
    expect(screen.getByText("WACP Highway")).toBeInTheDocument();
    expect(screen.getByText("Gates")).toBeInTheDocument();
    expect(screen.getByText("Settings")).toBeInTheDocument();
  });

  it("default route renders trail view", () => {
    setConnected();
    render(<App />);
    // Both sidebar and TrailViewer have "Trail" text
    expect(screen.getAllByText("Trail").length).toBeGreaterThanOrEqual(2);
  });
});
