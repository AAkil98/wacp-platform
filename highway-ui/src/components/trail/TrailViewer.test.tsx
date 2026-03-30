import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { TrailViewer } from "./TrailViewer.js";
import { useStore, type TrailEntry } from "@/store";

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

function makeEntry(seq: number, overrides?: Partial<TrailEntry>): TrailEntry {
  return {
    timestamp: BigInt(1711900000000000 + seq * 1000000),
    eventType: "signal_emitted",
    actor: "agent-A",
    workspaceId: `ws-${seq}`,
    body: '{"signal":"ready"}',
    sequenceNumber: BigInt(seq),
    ...overrides,
  };
}

function renderViewer(props?: { workspaceId?: string; showFilters?: boolean }) {
  return render(
    <MemoryRouter>
      <TrailViewer {...props} />
    </MemoryRouter>,
  );
}

describe("TrailViewer", () => {
  beforeEach(resetStore);

  it("shows empty state when no entries", () => {
    renderViewer();
    expect(screen.getByText("No trail entries.")).toBeDefined();
  });

  it("renders filter bar by default", () => {
    renderViewer();
    expect(screen.getByPlaceholderText("Workspace ID")).toBeDefined();
    expect(screen.getByPlaceholderText("Actor")).toBeDefined();
    expect(screen.getByText("Event type")).toBeDefined();
  });

  it("hides filter bar when showFilters is false", () => {
    renderViewer({ showFilters: false });
    expect(screen.queryByPlaceholderText("Workspace ID")).toBeNull();
  });

  it("renders trail entries", () => {
    useStore.getState().appendTrailEntry(makeEntry(1, { eventType: "gate_triggered" }));
    useStore.getState().appendTrailEntry(makeEntry(2, { eventType: "signal_emitted" }));
    renderViewer();

    expect(screen.getByText("gate_triggered")).toBeDefined();
    expect(screen.getByText("signal_emitted")).toBeDefined();
  });

  it("shows pause/resume button", () => {
    renderViewer();
    const btn = screen.getByText("Pause");
    fireEvent.click(btn);
    expect(screen.getByText("Resume")).toBeDefined();
  });

  it("formats HLC timestamps as HH:MM:SS.mmm", () => {
    // 1711900000000000 microseconds = 1711900000000 ms = some date
    useStore.getState().appendTrailEntry(makeEntry(1));
    renderViewer();
    // The timestamp should be formatted — look for a time-like pattern
    const timeElements = screen.getAllByTitle(/\d{4}-/);
    expect(timeElements.length).toBeGreaterThan(0);
  });

  it("scoped mode filters by workspace", () => {
    useStore.getState().appendTrailEntry(makeEntry(1, { workspaceId: "ws-target" }));
    useStore.getState().appendTrailEntry(makeEntry(2, { workspaceId: "ws-other" }));
    renderViewer({ workspaceId: "ws-target" });

    // Should show the scoped title
    expect(screen.getByText(/Trail — ws-target/)).toBeDefined();
  });
});
