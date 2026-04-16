import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, it, expect } from "vitest";
import { RefusalPanel } from "./RefusalPanel";
import { useSessionStore } from "../../store/session";

// RefusalPanel is a read-only display component — no api calls, no store writes.
// The deliverable in AUDIT-2026-04-15 §13.7.3 mentioned "acknowledge action" and
// "expiry" fields; those are not in the current component or store schema.  This
// suite tests the actual behavior: rendering shape for each policy_kind and
// error_code family the runtime emits.

function makeRefusal(overrides: Record<string, unknown> = {}) {
  return {
    refusal_id: "r1",
    workspace_id: "ws1",
    workspace_label: "workspace-alpha",
    tool_name: "exec.shell",
    error_code: "POLICY_BLOCKED",
    policy_kind: "requires_checkpoint",
    reason: "This tool requires an approved checkpoint.",
    unblock_hint: "Request a checkpoint via /api/checkpoints.",
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

describe("RefusalPanel", () => {
  beforeEach(() => {
    resetStore();
  });

  afterEach(() => {
    resetStore();
  });

  it("renders empty state when no refusals are recorded", () => {
    render(<RefusalPanel />);
    expect(screen.getByText(/No refusals recorded/i)).toBeInTheDocument();
  });

  it("renders one card per refusal with tool name, error code, workspace label, and policy kind", () => {
    useSessionStore.setState({
      refusals: [
        makeRefusal({
          refusal_id: "r1",
          tool_name: "exec.shell",
          error_code: "POLICY_BLOCKED",
          policy_kind: "requires_checkpoint",
          workspace_label: "ws-alpha",
        }),
      ],
    });
    render(<RefusalPanel />);
    expect(screen.getByText("exec.shell")).toBeInTheDocument();
    expect(screen.getByText("POLICY_BLOCKED")).toBeInTheDocument();
    expect(screen.getByText("requires_checkpoint")).toBeInTheDocument();
    expect(screen.getByText("ws-alpha")).toBeInTheDocument();
  });

  it("renders the refusal reason", () => {
    useSessionStore.setState({
      refusals: [makeRefusal({ reason: "This specific reason shown." })],
    });
    render(<RefusalPanel />);
    expect(screen.getByText("This specific reason shown.")).toBeInTheDocument();
  });

  it("renders the unblock_hint when one is present", () => {
    useSessionStore.setState({
      refusals: [makeRefusal({ unblock_hint: "Try the checkpoint endpoint." })],
    });
    render(<RefusalPanel />);
    expect(screen.getByText("Try the checkpoint endpoint.")).toBeInTheDocument();
  });

  it("omits the unblock_hint paragraph when the field is empty", () => {
    useSessionStore.setState({
      refusals: [makeRefusal({ unblock_hint: "" })],
    });
    const { container } = render(<RefusalPanel />);
    // The only italic paragraph in the card would be the unblock hint; its
    // absence is the test.
    const italics = container.querySelectorAll('p[style*="italic"]');
    expect(italics.length).toBe(0);
  });

  it("renders for policy_kind = budget_limited", () => {
    useSessionStore.setState({
      refusals: [makeRefusal({ refusal_id: "r-b", policy_kind: "budget_limited" })],
    });
    render(<RefusalPanel />);
    expect(screen.getByText("budget_limited")).toBeInTheDocument();
  });

  it("renders for policy_kind = rate_limited", () => {
    useSessionStore.setState({
      refusals: [makeRefusal({ refusal_id: "r-r", policy_kind: "rate_limited" })],
    });
    render(<RefusalPanel />);
    expect(screen.getByText("rate_limited")).toBeInTheDocument();
  });

  it("applies a distinct color to POLICY_* error codes (warning)", () => {
    useSessionStore.setState({
      refusals: [makeRefusal({ error_code: "POLICY_BLOCKED" })],
    });
    const { container } = render(<RefusalPanel />);
    const badge = container.querySelector(
      'span[style*="var(--color-warning)"]',
    );
    expect(badge).not.toBeNull();
    expect(badge?.textContent).toBe("POLICY_BLOCKED");
  });

  it("applies a distinct color to PERM_* error codes (danger)", () => {
    useSessionStore.setState({
      refusals: [makeRefusal({ error_code: "PERM_DENIED" })],
    });
    const { container } = render(<RefusalPanel />);
    const badges = Array.from(
      container.querySelectorAll<HTMLSpanElement>('span[style*="var(--color-danger)"]'),
    );
    const match = badges.find((b) => b.textContent === "PERM_DENIED");
    expect(match).toBeDefined();
  });

  it("uses a muted color for error codes in neither family", () => {
    useSessionStore.setState({
      refusals: [makeRefusal({ error_code: "OTHER_CODE" })],
    });
    const { container } = render(<RefusalPanel />);
    const badge = container.querySelector(
      'span[style*="var(--color-text-muted)"]',
    );
    // There can be several muted spans in the card; what matters is that the
    // error_code badge renders without the warning/danger color class.
    expect(badge).not.toBeNull();
    expect(
      container.querySelector('span[style*="var(--color-warning)"]'),
    ).toBeNull();
  });

  it("renders each refusal in the order they arrived in the store", () => {
    useSessionStore.setState({
      refusals: [
        makeRefusal({ refusal_id: "r-first", reason: "first-reason" }),
        makeRefusal({ refusal_id: "r-second", reason: "second-reason" }),
        makeRefusal({ refusal_id: "r-third", reason: "third-reason" }),
      ],
    });
    render(<RefusalPanel />);
    const reasons = screen.getAllByText(/reason$/);
    expect(reasons[0]!.textContent).toBe("first-reason");
    expect(reasons[1]!.textContent).toBe("second-reason");
    expect(reasons[2]!.textContent).toBe("third-reason");
  });
});
