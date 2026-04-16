import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { EscalationInbox } from "./EscalationInbox";
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

function makeEscalation(overrides: Record<string, unknown> = {}) {
  return {
    escalation_id: "esc1",
    workspace_id: "ws1",
    workspace_label: "workspace-alpha",
    reason: "Unable to resolve without operator input",
    timestamp: new Date(Date.now() - 10_000).toISOString(),
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

describe("EscalationInbox", () => {
  beforeEach(() => {
    resetStore();
    apiPostMock.mockReset();
    apiPostMock.mockResolvedValue({});
  });

  afterEach(() => {
    resetStore();
  });

  it("renders empty state when the inbox is empty", () => {
    render(<EscalationInbox sessionId="s1" />);
    expect(screen.getByText(/No escalations/i)).toBeInTheDocument();
  });

  it("renders one row per escalation with reason + workspace label", () => {
    useSessionStore.setState({
      escalations: [
        makeEscalation({ escalation_id: "esc1", reason: "Reason one", workspace_label: "ws-alpha" }),
        makeEscalation({ escalation_id: "esc2", reason: "Reason two", workspace_label: "ws-beta" }),
      ],
    });
    render(<EscalationInbox sessionId="s1" />);
    expect(screen.getByText("Reason one")).toBeInTheDocument();
    expect(screen.getByText("Reason two")).toBeInTheDocument();
    expect(screen.getByText("ws-alpha")).toBeInTheDocument();
    expect(screen.getByText("ws-beta")).toBeInTheDocument();
  });

  it("does not render the response textarea until a row is expanded", () => {
    useSessionStore.setState({ escalations: [makeEscalation()] });
    render(<EscalationInbox sessionId="s1" />);
    expect(
      screen.queryByPlaceholderText(/Type your response/i),
    ).not.toBeInTheDocument();
  });

  it("clicking the summary row expands detail + response form", () => {
    useSessionStore.setState({ escalations: [makeEscalation({ reason: "expand me" })] });
    render(<EscalationInbox sessionId="s1" />);
    fireEvent.click(screen.getByText("expand me"));
    expect(screen.getByPlaceholderText(/Type your response/i)).toBeInTheDocument();
    expect(screen.getByText("Submit")).toBeInTheDocument();
  });

  it("clicking the summary row a second time collapses detail", () => {
    useSessionStore.setState({ escalations: [makeEscalation({ reason: "collapse me" })] });
    render(<EscalationInbox sessionId="s1" />);
    fireEvent.click(screen.getByText("collapse me"));
    expect(screen.getByPlaceholderText(/Type your response/i)).toBeInTheDocument();
    fireEvent.click(screen.getByText("collapse me"));
    expect(
      screen.queryByPlaceholderText(/Type your response/i),
    ).not.toBeInTheDocument();
  });

  it("Submit is disabled when the response is empty or whitespace-only", () => {
    useSessionStore.setState({ escalations: [makeEscalation()] });
    render(<EscalationInbox sessionId="s1" />);
    fireEvent.click(screen.getByText(/resolve without operator/));
    const submit = screen.getByText("Submit") as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    const textarea = screen.getByPlaceholderText(/Type your response/i);
    fireEvent.change(textarea, { target: { value: "   " } });
    expect(submit.disabled).toBe(true);
  });

  it("Submit is enabled once a non-empty response is typed", () => {
    useSessionStore.setState({ escalations: [makeEscalation()] });
    render(<EscalationInbox sessionId="s1" />);
    fireEvent.click(screen.getByText(/resolve without operator/));
    const textarea = screen.getByPlaceholderText(/Type your response/i);
    fireEvent.change(textarea, { target: { value: "here is my reply" } });
    expect((screen.getByText("Submit") as HTMLButtonElement).disabled).toBe(false);
  });

  it("Submit POSTs the response and clears the textarea on success", async () => {
    useSessionStore.setState({ escalations: [makeEscalation({ escalation_id: "esc1" })] });
    render(<EscalationInbox sessionId="s1" />);
    fireEvent.click(screen.getByText(/resolve without operator/));
    const textarea = screen.getByPlaceholderText(/Type your response/i) as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: "acknowledged" } });
    fireEvent.click(screen.getByText("Submit"));
    await waitFor(() => {
      expect(apiPostMock).toHaveBeenCalledWith(
        "/api/sessions/s1/escalations/esc1",
        { response: "acknowledged" },
      );
    });
    await waitFor(() => {
      expect(textarea.value).toBe("");
    });
  });

  it("shows 'Sending...' while the submit is in-flight", async () => {
    let resolveHold!: () => void;
    const hold = new Promise<void>((r) => { resolveHold = r; });
    apiPostMock.mockImplementation(() => hold);
    useSessionStore.setState({ escalations: [makeEscalation()] });
    render(<EscalationInbox sessionId="s1" />);
    fireEvent.click(screen.getByText(/resolve without operator/));
    fireEvent.change(screen.getByPlaceholderText(/Type your response/i), {
      target: { value: "sending" },
    });
    fireEvent.click(screen.getByText("Submit"));
    await waitFor(() => {
      expect(screen.getByText(/Sending\.\.\./)).toBeInTheDocument();
    });
    resolveHold();
  });

  it("shows a relative 'Xs ago' timestamp when timestamp is present", () => {
    useSessionStore.setState({
      escalations: [
        makeEscalation({
          timestamp: new Date(Date.now() - 45_000).toISOString(),
        }),
      ],
    });
    render(<EscalationInbox sessionId="s1" />);
    expect(screen.getByText(/\d+s ago/)).toBeInTheDocument();
  });

  it("renders '--' when timestamp is undefined", () => {
    useSessionStore.setState({
      escalations: [makeEscalation({ timestamp: undefined })],
    });
    render(<EscalationInbox sessionId="s1" />);
    expect(screen.getByText("--")).toBeInTheDocument();
  });

  it("a resolved escalation disappears when removed from the store", () => {
    useSessionStore.setState({
      escalations: [makeEscalation({ escalation_id: "esc1", reason: "bye" })],
    });
    const { rerender } = render(<EscalationInbox sessionId="s1" />);
    expect(screen.getByText("bye")).toBeInTheDocument();
    useSessionStore.setState({ escalations: [] });
    rerender(<EscalationInbox sessionId="s1" />);
    expect(screen.queryByText("bye")).not.toBeInTheDocument();
    expect(screen.getByText(/No escalations/i)).toBeInTheDocument();
  });
});
