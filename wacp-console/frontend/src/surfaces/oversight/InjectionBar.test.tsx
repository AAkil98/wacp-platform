import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { InjectionBar } from "./InjectionBar";
import { useSessionStore } from "../../store/session";

// NOTE on deliverable drift: AUDIT §13.7.3 item 4 described sending to
// workspaces in each state (active/paused/completed/failed).  The current
// `InjectionBar` only lists workspaces whose state uppercases to "ACTIVE";
// other states are filtered out client-side and never offered as targets.
// There is also no client-side oversize check — the server owns that.
// This suite tests actual behavior.

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

describe("InjectionBar", () => {
  beforeEach(() => {
    resetStore();
    apiPostMock.mockReset();
    apiPostMock.mockResolvedValue({});
  });

  afterEach(() => {
    resetStore();
  });

  it("renders intro copy and form controls", () => {
    render(<InjectionBar sessionId="s1" />);
    expect(screen.getByText(/Inject a directive into an active workspace/i)).toBeInTheDocument();
    expect(screen.getByText(/Target workspace/i)).toBeInTheDocument();
    expect(screen.getByText(/Directive payload/i)).toBeInTheDocument();
  });

  it("shows 'No active workspaces available' when the session has none", () => {
    render(<InjectionBar sessionId="s1" />);
    expect(screen.getByText(/No active workspaces available/i)).toBeInTheDocument();
  });

  it("lists only ACTIVE workspaces in the target dropdown", () => {
    useSessionStore.setState({
      workspaces: new Map([
        ["ws-active-1", "ACTIVE"],
        ["ws-paused-1", "PAUSED"],
        ["ws-completed-1", "COMPLETED"],
        ["ws-failed-1", "FAILED"],
        ["ws-active-2", "active"], // case-insensitive match
      ]),
    });
    render(<InjectionBar sessionId="s1" />);
    const select = screen.getByRole("combobox") as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toContain("ws-active-1");
    expect(values).toContain("ws-active-2");
    expect(values).not.toContain("ws-paused-1");
    expect(values).not.toContain("ws-completed-1");
    expect(values).not.toContain("ws-failed-1");
  });

  it("Send button is disabled when payload is empty", () => {
    useSessionStore.setState({ workspaces: new Map([["ws1", "ACTIVE"]]) });
    render(<InjectionBar sessionId="s1" />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "ws1" } });
    const btn = screen.getByRole("button", { name: /Send Directive/i }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it("Send button is disabled when payload is whitespace only", () => {
    useSessionStore.setState({ workspaces: new Map([["ws1", "ACTIVE"]]) });
    render(<InjectionBar sessionId="s1" />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "ws1" } });
    fireEvent.change(screen.getByPlaceholderText(/Enter directive text/i), {
      target: { value: "   \n  " },
    });
    expect(
      (screen.getByRole("button", { name: /Send Directive/i }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("Send button is disabled when no workspace is selected", () => {
    useSessionStore.setState({ workspaces: new Map([["ws1", "ACTIVE"]]) });
    render(<InjectionBar sessionId="s1" />);
    fireEvent.change(screen.getByPlaceholderText(/Enter directive text/i), {
      target: { value: "do the thing" },
    });
    expect(
      (screen.getByRole("button", { name: /Send Directive/i }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("Send is enabled once both workspace and non-empty payload are set", () => {
    useSessionStore.setState({ workspaces: new Map([["ws1", "ACTIVE"]]) });
    render(<InjectionBar sessionId="s1" />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "ws1" } });
    fireEvent.change(screen.getByPlaceholderText(/Enter directive text/i), {
      target: { value: "do the thing" },
    });
    expect(
      (screen.getByRole("button", { name: /Send Directive/i }) as HTMLButtonElement).disabled,
    ).toBe(false);
  });

  it("clicking Send posts to /api/sessions/<id>/inject with the payload + workspace_id", async () => {
    useSessionStore.setState({ workspaces: new Map([["ws1", "ACTIVE"]]) });
    render(<InjectionBar sessionId="s42" />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "ws1" } });
    fireEvent.change(screen.getByPlaceholderText(/Enter directive text/i), {
      target: { value: "pause and wait" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Send Directive/i }));
    await waitFor(() => {
      expect(apiPostMock).toHaveBeenCalledWith("/api/sessions/s42/inject", {
        workspace_id: "ws1",
        content: { text: "pause and wait" },
      });
    });
  });

  it("success clears the payload and shows a success message", async () => {
    useSessionStore.setState({ workspaces: new Map([["ws1", "ACTIVE"]]) });
    render(<InjectionBar sessionId="s1" />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "ws1" } });
    const textarea = screen.getByPlaceholderText(/Enter directive text/i) as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: "payload" } });
    fireEvent.click(screen.getByRole("button", { name: /Send Directive/i }));
    await waitFor(() => {
      expect(screen.getByText(/Directive sent successfully/i)).toBeInTheDocument();
    });
    expect(textarea.value).toBe("");
  });

  it("failure keeps the payload and shows a Failed: <message>", async () => {
    apiPostMock.mockRejectedValue(new Error("network down"));
    useSessionStore.setState({ workspaces: new Map([["ws1", "ACTIVE"]]) });
    render(<InjectionBar sessionId="s1" />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "ws1" } });
    const textarea = screen.getByPlaceholderText(/Enter directive text/i) as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: "will fail" } });
    fireEvent.click(screen.getByRole("button", { name: /Send Directive/i }));
    await waitFor(() => {
      expect(screen.getByText(/Failed: network down/)).toBeInTheDocument();
    });
    expect(textarea.value).toBe("will fail");
  });

  it("shows 'Sending...' while the request is in-flight", async () => {
    let resolveHold!: () => void;
    const hold = new Promise<void>((r) => { resolveHold = r; });
    apiPostMock.mockImplementation(() => hold);
    useSessionStore.setState({ workspaces: new Map([["ws1", "ACTIVE"]]) });
    render(<InjectionBar sessionId="s1" />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "ws1" } });
    fireEvent.change(screen.getByPlaceholderText(/Enter directive text/i), {
      target: { value: "in-flight" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Send Directive/i }));
    await waitFor(() => {
      expect(screen.getByText(/Sending\.\.\./)).toBeInTheDocument();
    });
    resolveHold();
  });

  it("does not fire a duplicate POST if Send is clicked twice quickly", async () => {
    let resolveHold!: () => void;
    const hold = new Promise<void>((r) => { resolveHold = r; });
    apiPostMock.mockImplementation(() => hold);
    useSessionStore.setState({ workspaces: new Map([["ws1", "ACTIVE"]]) });
    render(<InjectionBar sessionId="s1" />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "ws1" } });
    fireEvent.change(screen.getByPlaceholderText(/Enter directive text/i), {
      target: { value: "once" },
    });
    const btn = screen.getByRole("button", { name: /Send Directive/i });
    fireEvent.click(btn);
    fireEvent.click(btn);
    await waitFor(() => {
      expect(apiPostMock).toHaveBeenCalledTimes(1);
    });
    resolveHold();
  });
});
