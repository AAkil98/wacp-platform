import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { GateQueue } from "./GateQueue";
import { useSessionStore } from "../../store/session";

// ---- api mock ----

const apiPostMock = vi.fn();
vi.mock("../../api/client", () => ({
  api: {
    get: vi.fn(),
    post: (...args: unknown[]) => apiPostMock(...args),
    put: vi.fn(),
    patch: vi.fn(),
    delete: vi.fn(),
  },
}));

// ---- Fixtures ----

function makeGate(overrides: Record<string, unknown> = {}) {
  return {
    gate_id: "g1",
    type: "quality_gate",
    workspace_id: "ws1",
    workspace_label: "workspace-alpha",
    subject: { description: "Approve tool invocation?" },
    timeout_at: new Date(Date.now() + 5 * 60 * 1000).toISOString(),
    urgency: "medium",
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

// ---- Tests ----

describe("GateQueue", () => {
  beforeEach(() => {
    resetStore();
    apiPostMock.mockReset();
    apiPostMock.mockResolvedValue({});
  });

  afterEach(() => {
    resetStore();
  });

  it("renders empty state when there are no pending gates", () => {
    render(<GateQueue sessionId="s1" />);
    expect(screen.getByText(/No pending gates/i)).toBeInTheDocument();
  });

  it("renders a card for each gate", () => {
    useSessionStore.setState({
      gates: [
        makeGate({ gate_id: "g1", subject: { description: "Gate one" } }),
        makeGate({ gate_id: "g2", subject: { description: "Gate two" } }),
      ],
    });
    render(<GateQueue sessionId="s1" />);
    expect(screen.getByText("Gate one")).toBeInTheDocument();
    expect(screen.getByText("Gate two")).toBeInTheDocument();
  });

  it("sorts gates by urgency: critical first, then high, medium, low", () => {
    useSessionStore.setState({
      gates: [
        makeGate({ gate_id: "g-low", urgency: "low", subject: "LowSubject" }),
        makeGate({ gate_id: "g-crit", urgency: "critical", subject: "CritSubject" }),
        makeGate({ gate_id: "g-med", urgency: "medium", subject: "MedSubject" }),
        makeGate({ gate_id: "g-high", urgency: "high", subject: "HighSubject" }),
      ],
    });
    render(<GateQueue sessionId="s1" />);
    const subjects = screen.getAllByText(/(Low|Crit|Med|High)Subject/);
    expect(subjects[0]!.textContent).toBe("CritSubject");
    expect(subjects[1]!.textContent).toBe("HighSubject");
    expect(subjects[2]!.textContent).toBe("MedSubject");
    expect(subjects[3]!.textContent).toBe("LowSubject");
  });

  it("within the same urgency, sooner-timing-out gates come first", () => {
    const soon = new Date(Date.now() + 60_000).toISOString();
    const later = new Date(Date.now() + 600_000).toISOString();
    useSessionStore.setState({
      gates: [
        makeGate({ gate_id: "g-later", urgency: "medium", timeout_at: later, subject: "LaterSubject" }),
        makeGate({ gate_id: "g-soon", urgency: "medium", timeout_at: soon, subject: "SoonSubject" }),
      ],
    });
    render(<GateQueue sessionId="s1" />);
    const subjects = screen.getAllByText(/(Soon|Later)Subject/);
    expect(subjects[0]!.textContent).toBe("SoonSubject");
    expect(subjects[1]!.textContent).toBe("LaterSubject");
  });

  it("shows the gate's workspace label and type badge", () => {
    useSessionStore.setState({
      gates: [makeGate({ workspace_label: "ws-alpha-1", type: "quality_gate" })],
    });
    render(<GateQueue sessionId="s1" />);
    expect(screen.getByText("ws-alpha-1")).toBeInTheDocument();
    expect(screen.getByText("quality_gate")).toBeInTheDocument();
  });

  it("shows urgency label uppercased", () => {
    useSessionStore.setState({
      gates: [makeGate({ urgency: "critical" })],
    });
    render(<GateQueue sessionId="s1" />);
    expect(screen.getByText("critical")).toBeInTheDocument();
  });

  it("Approve button POSTs decision=approve with the entered reason", async () => {
    useSessionStore.setState({ gates: [makeGate({ gate_id: "g1" })] });
    render(<GateQueue sessionId="s1" />);
    const textarea = screen.getByPlaceholderText(/Reason \(optional\)/i) as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: "looks good" } });
    fireEvent.click(screen.getByText("Approve"));
    await waitFor(() => {
      expect(apiPostMock).toHaveBeenCalledWith("/api/sessions/s1/gates/g1", {
        decision: "approve",
        reason: "looks good",
      });
    });
  });

  it("Reject button POSTs decision=reject", async () => {
    useSessionStore.setState({ gates: [makeGate({ gate_id: "g1" })] });
    render(<GateQueue sessionId="s1" />);
    fireEvent.click(screen.getByText("Reject"));
    await waitFor(() => {
      expect(apiPostMock).toHaveBeenCalledWith("/api/sessions/s1/gates/g1", {
        decision: "reject",
        reason: "",
      });
    });
  });

  it("Approve/Reject buttons disabled while the request is in-flight", async () => {
    let resolveHold!: () => void;
    const hold = new Promise<void>((r) => { resolveHold = r; });
    apiPostMock.mockImplementation(() => hold);
    useSessionStore.setState({ gates: [makeGate({ gate_id: "g1" })] });
    render(<GateQueue sessionId="s1" />);
    fireEvent.click(screen.getByText("Approve"));
    await waitFor(() => {
      expect((screen.getByText("Approve") as HTMLButtonElement).disabled).toBe(true);
      expect((screen.getByText("Reject") as HTMLButtonElement).disabled).toBe(true);
    });
    resolveHold();
  });

  it("batch controls appear only after at least one gate is selected", () => {
    useSessionStore.setState({
      gates: [makeGate({ gate_id: "g1" }), makeGate({ gate_id: "g2" })],
    });
    render(<GateQueue sessionId="s1" />);
    expect(screen.queryByText(/Approve Selected/)).not.toBeInTheDocument();
    const checkboxes = screen.getAllByRole("checkbox");
    fireEvent.click(checkboxes[0]!);
    expect(screen.getByText(/Approve Selected/)).toBeInTheDocument();
    expect(screen.getByText(/1 selected/)).toBeInTheDocument();
  });

  it("selecting another gate updates the selected count", () => {
    useSessionStore.setState({
      gates: [makeGate({ gate_id: "g1" }), makeGate({ gate_id: "g2" })],
    });
    render(<GateQueue sessionId="s1" />);
    const checkboxes = screen.getAllByRole("checkbox");
    fireEvent.click(checkboxes[0]!);
    fireEvent.click(checkboxes[1]!);
    expect(screen.getByText(/2 selected/)).toBeInTheDocument();
  });

  it("Approve Selected fires one POST per selected gate with the shared batch reason", async () => {
    useSessionStore.setState({
      gates: [
        makeGate({ gate_id: "g1" }),
        makeGate({ gate_id: "g2" }),
        makeGate({ gate_id: "g3" }),
      ],
    });
    render(<GateQueue sessionId="s1" />);
    const checkboxes = screen.getAllByRole("checkbox");
    fireEvent.click(checkboxes[0]!);
    fireEvent.click(checkboxes[1]!);
    const batchReasonInput = screen.getByPlaceholderText(/Batch reason/i) as HTMLInputElement;
    fireEvent.change(batchReasonInput, { target: { value: "all clear" } });
    fireEvent.click(screen.getByText(/Approve Selected/i));
    await waitFor(() => {
      expect(apiPostMock).toHaveBeenCalledTimes(2);
    });
    const calls = apiPostMock.mock.calls;
    expect(calls.some((c) => c[0] === "/api/sessions/s1/gates/g1")).toBe(true);
    expect(calls.some((c) => c[0] === "/api/sessions/s1/gates/g2")).toBe(true);
    expect(calls.every((c) => (c[1] as Record<string, unknown>).reason === "all clear")).toBe(true);
  });

  it("deselecting via checkbox hides the batch controls when the last gate is deselected", () => {
    useSessionStore.setState({ gates: [makeGate({ gate_id: "g1" })] });
    render(<GateQueue sessionId="s1" />);
    const checkbox = screen.getByRole("checkbox");
    fireEvent.click(checkbox);
    expect(screen.getByText(/Approve Selected/i)).toBeInTheDocument();
    fireEvent.click(checkbox);
    expect(screen.queryByText(/Approve Selected/i)).not.toBeInTheDocument();
  });

  it("a resolved gate disappears when removed from the store", () => {
    useSessionStore.setState({
      gates: [makeGate({ gate_id: "g1", subject: "DisappearSubject" })],
    });
    const { rerender } = render(<GateQueue sessionId="s1" />);
    expect(screen.getByText("DisappearSubject")).toBeInTheDocument();
    useSessionStore.setState({ gates: [] });
    rerender(<GateQueue sessionId="s1" />);
    expect(screen.queryByText("DisappearSubject")).not.toBeInTheDocument();
    expect(screen.getByText(/No pending gates/i)).toBeInTheDocument();
  });

  it("formats expired timeout as 'expired'", () => {
    useSessionStore.setState({
      gates: [makeGate({ timeout_at: new Date(Date.now() - 60_000).toISOString() })],
    });
    render(<GateQueue sessionId="s1" />);
    expect(screen.getByText("expired")).toBeInTheDocument();
  });

  it("renders '--' when no timeout_at is set", () => {
    useSessionStore.setState({
      gates: [makeGate({ timeout_at: undefined })],
    });
    render(<GateQueue sessionId="s1" />);
    expect(screen.getByText("--")).toBeInTheDocument();
  });
});
