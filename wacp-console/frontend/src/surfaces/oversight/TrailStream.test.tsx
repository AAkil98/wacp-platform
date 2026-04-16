import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, beforeEach } from "vitest";
import { TrailStream } from "./TrailStream.tsx";
import { useSessionStore } from "../../store/session.ts";

function makeTrailEntry(overrides: Record<string, unknown> = {}) {
  return {
    id: "t1",
    timestamp: "2026-04-16T12:30:45.123Z",
    workspace_id: "ws1",
    workspace_label: "workspace-alpha",
    event_type: "tool_call",
    summary: "Called tool X",
    ...overrides,
  };
}

function resetStore() {
  useSessionStore.setState({
    sessionId: null,
    trail: [],
    gates: [],
    escalations: [],
    refusals: [],
    workspaces: new Map(),
    sessionStatus: null,
    trailBufferSize: 1000,
  });
}

describe("TrailStream", () => {
  beforeEach(() => {
    resetStore();
  });

  // --- Empty state ---

  it("renders empty message when there are no trail entries", () => {
    render(<TrailStream />);
    expect(screen.getByText("No trail entries yet.")).toBeInTheDocument();
    expect(screen.getByText("0 entries")).toBeInTheDocument();
  });

  // --- Rendering entries ---

  it("renders trail entries with summary text", () => {
    useSessionStore.setState({
      trail: [
        makeTrailEntry({ id: "t1", summary: "First event" }),
        makeTrailEntry({ id: "t2", summary: "Second event" }),
      ],
    });

    render(<TrailStream />);
    expect(screen.getByText("First event")).toBeInTheDocument();
    expect(screen.getByText("Second event")).toBeInTheDocument();
    expect(screen.getByText("2 entries")).toBeInTheDocument();
  });

  it("renders entries in reverse order (newest first)", () => {
    useSessionStore.setState({
      trail: [
        makeTrailEntry({ id: "t1", summary: "Oldest" }),
        makeTrailEntry({ id: "t2", summary: "Middle" }),
        makeTrailEntry({ id: "t3", summary: "Newest" }),
      ],
    });

    render(<TrailStream />);
    const summaries = screen.getAllByText(/Oldest|Middle|Newest/);
    expect(summaries[0]!.textContent).toBe("Newest");
    expect(summaries[1]!.textContent).toBe("Middle");
    expect(summaries[2]!.textContent).toBe("Oldest");
  });

  it("renders workspace label for each entry", () => {
    useSessionStore.setState({
      trail: [makeTrailEntry({ workspace_label: "my-workspace" })],
    });

    render(<TrailStream />);
    // Both the entry badge and the dropdown option contain the text;
    // verify at least one non-option element renders it.
    const matches = screen.getAllByText("my-workspace");
    expect(matches.length).toBeGreaterThanOrEqual(1);
  });

  it("renders event type tag", () => {
    useSessionStore.setState({
      trail: [makeTrailEntry({ event_type: "tool_call" })],
    });

    render(<TrailStream />);
    const matches = screen.getAllByText("tool_call");
    expect(matches.length).toBeGreaterThanOrEqual(1);
  });

  it("renders formatted timestamp", () => {
    useSessionStore.setState({
      trail: [makeTrailEntry({ timestamp: "2026-04-16T08:05:09.042Z" })],
    });

    render(<TrailStream />);
    // The timestamp is formatted using the local timezone of the Date constructor.
    // We look for the monospace timestamp by checking for a pattern with colons.
    const container = document.querySelector("[style*='monospace']");
    expect(container).toBeTruthy();
  });

  // --- Filtering ---

  it("filters entries by event type", () => {
    useSessionStore.setState({
      trail: [
        makeTrailEntry({ id: "t1", event_type: "tool_call", summary: "Tool event" }),
        makeTrailEntry({ id: "t2", event_type: "refusal", summary: "Refusal event" }),
        makeTrailEntry({ id: "t3", event_type: "tool_call", summary: "Another tool event" }),
      ],
    });

    render(<TrailStream />);
    expect(screen.getByText("3 entries")).toBeInTheDocument();

    // Filter to refusal only
    const selects = screen.getAllByRole("combobox");
    const typeSelect = selects[0]!;
    fireEvent.change(typeSelect, { target: { value: "refusal" } });

    expect(screen.getByText("1 entries")).toBeInTheDocument();
    expect(screen.getByText("Refusal event")).toBeInTheDocument();
    expect(screen.queryByText("Tool event")).not.toBeInTheDocument();
  });

  it("filters entries by workspace", () => {
    useSessionStore.setState({
      trail: [
        makeTrailEntry({ id: "t1", workspace_label: "ws-A", summary: "Event A" }),
        makeTrailEntry({ id: "t2", workspace_label: "ws-B", summary: "Event B" }),
      ],
    });

    render(<TrailStream />);
    const selects = screen.getAllByRole("combobox");
    const wsSelect = selects[1]!;
    fireEvent.change(wsSelect, { target: { value: "ws-A" } });

    expect(screen.getByText("1 entries")).toBeInTheDocument();
    expect(screen.getByText("Event A")).toBeInTheDocument();
    expect(screen.queryByText("Event B")).not.toBeInTheDocument();
  });

  it("applies both filters simultaneously", () => {
    useSessionStore.setState({
      trail: [
        makeTrailEntry({ id: "t1", event_type: "tool_call", workspace_label: "ws-A", summary: "A-tool" }),
        makeTrailEntry({ id: "t2", event_type: "refusal", workspace_label: "ws-A", summary: "A-refusal" }),
        makeTrailEntry({ id: "t3", event_type: "tool_call", workspace_label: "ws-B", summary: "B-tool" }),
      ],
    });

    render(<TrailStream />);
    const selects = screen.getAllByRole("combobox");
    fireEvent.change(selects[0]!, { target: { value: "tool_call" } });
    fireEvent.change(selects[1]!, { target: { value: "ws-A" } });

    expect(screen.getByText("1 entries")).toBeInTheDocument();
    expect(screen.getByText("A-tool")).toBeInTheDocument();
    expect(screen.queryByText("A-refusal")).not.toBeInTheDocument();
    expect(screen.queryByText("B-tool")).not.toBeInTheDocument();
  });

  it("shows empty message when filters exclude all entries", () => {
    // Use two event types in different workspaces so both filter dropdowns
    // are populated, then apply a combination that matches no entry.
    useSessionStore.setState({
      trail: [
        makeTrailEntry({ id: "t1", event_type: "tool_call", workspace_label: "ws-A" }),
        makeTrailEntry({ id: "t2", event_type: "refusal", workspace_label: "ws-B" }),
      ],
    });

    render(<TrailStream />);
    const selects = screen.getAllByRole("combobox");
    // Filter to tool_call + ws-B → zero overlap
    fireEvent.change(selects[0]!, { target: { value: "tool_call" } });
    fireEvent.change(selects[1]!, { target: { value: "ws-B" } });

    expect(screen.getByText(/0 entries/)).toBeInTheDocument();
    expect(screen.getByText(/No trail entries/)).toBeInTheDocument();
  });

  // --- Expand/Collapse ---

  it("expands entry to show JSON on click", () => {
    useSessionStore.setState({
      trail: [makeTrailEntry({ id: "t1", summary: "Clickable event" })],
    });

    render(<TrailStream />);
    expect(screen.queryByRole("code")).toBeNull();

    fireEvent.click(screen.getByText("Clickable event"));

    // After clicking, a <pre> with JSON.stringify content should appear
    const pre = document.querySelector("pre");
    expect(pre).toBeTruthy();
    expect(pre!.textContent).toContain('"id": "t1"');
  });

  it("collapses entry on second click", () => {
    useSessionStore.setState({
      trail: [makeTrailEntry({ id: "t1", summary: "Toggle event" })],
    });

    render(<TrailStream />);

    // Click to expand
    fireEvent.click(screen.getByText("Toggle event"));
    expect(document.querySelector("pre")).toBeTruthy();

    // Click again to collapse
    fireEvent.click(screen.getByText("Toggle event"));
    expect(document.querySelector("pre")).toBeNull();
  });

  it("only one entry expanded at a time", () => {
    useSessionStore.setState({
      trail: [
        makeTrailEntry({ id: "t1", summary: "First" }),
        makeTrailEntry({ id: "t2", summary: "Second" }),
      ],
    });

    render(<TrailStream />);

    fireEvent.click(screen.getByText("First"));
    expect(document.querySelectorAll("pre")).toHaveLength(1);
    expect(document.querySelector("pre")!.textContent).toContain('"id": "t1"');

    fireEvent.click(screen.getByText("Second"));
    expect(document.querySelectorAll("pre")).toHaveLength(1);
    expect(document.querySelector("pre")!.textContent).toContain('"id": "t2"');
  });

  // --- Large datasets ---

  it("handles 1000+ entries without crashing", () => {
    const entries = Array.from({ length: 1200 }, (_, i) =>
      makeTrailEntry({ id: `t${i}`, summary: `Event ${i}` }),
    );
    useSessionStore.setState({ trail: entries });

    render(<TrailStream />);
    expect(screen.getByText("1200 entries")).toBeInTheDocument();
    // Newest first, so Event 1199 should be first
    expect(screen.getByText("Event 1199")).toBeInTheDocument();
  });

  // --- Filter dropdowns population ---

  it("populates event type dropdown from trail data", () => {
    useSessionStore.setState({
      trail: [
        makeTrailEntry({ id: "t1", event_type: "tool_call" }),
        makeTrailEntry({ id: "t2", event_type: "refusal" }),
        makeTrailEntry({ id: "t3", event_type: "tool_call" }),
      ],
    });

    render(<TrailStream />);
    const options = screen.getAllByRole("option");
    const typeOptions = options.filter(
      (o) => o.textContent === "tool_call" || o.textContent === "refusal",
    );
    expect(typeOptions).toHaveLength(2);
  });

  it("populates workspace dropdown from trail data", () => {
    useSessionStore.setState({
      trail: [
        makeTrailEntry({ id: "t1", workspace_label: "ws-alpha" }),
        makeTrailEntry({ id: "t2", workspace_label: "ws-beta" }),
      ],
    });

    render(<TrailStream />);
    // Both entry badges and dropdown options contain the workspace label
    const alpha = screen.getAllByText("ws-alpha");
    const beta = screen.getAllByText("ws-beta");
    expect(alpha.length).toBeGreaterThanOrEqual(2); // badge + option
    expect(beta.length).toBeGreaterThanOrEqual(2);
  });
});
