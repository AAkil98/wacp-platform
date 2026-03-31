/**
 * Integration: GatePanel + rpcs + store — 2 tests
 * Verifies gate approval flow from UI through RPC to store update.
 */
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { useStore } from "../store/index.js";
import { GateDecision } from "../gen/primitives_pb.js";

const mockRespondToGate = vi.fn();

vi.mock("../transport/client.js", () => ({
  getClient: () => ({
    respondToGate: mockRespondToGate,
  }),
}));

import { GatePanel } from "../components/gates/GatePanel.js";

beforeEach(() => {
  mockRespondToGate.mockReset();
  useStore.setState({
    session: { state: "connected", userId: "user-1", capabilities: [] },
    gates: {
      pending: new Map([
        ["gate-1", {
          gateId: "gate-1",
          gateType: "GATE_TYPE_TASK_APPROVAL",
          subject: new Uint8Array(),
          workspaceId: "ws-1",
          taskId: "t-1",
          timeoutMs: 30000n,
          fallbackAction: "cancel",
          createdAt: 100n,
        }],
      ]),
      resolved: new Map(),
      inFlight: new Set(),
    },
  });
});

describe("gate response flow integration", () => {
  it("approve button calls RPC and resolves gate in store", async () => {
    mockRespondToGate.mockResolvedValueOnce({ gateId: "gate-1", applied: true });

    render(
      <MemoryRouter>
        <GatePanel />
      </MemoryRouter>,
    );

    const approveBtn = screen.getByRole("button", { name: /approve/i });
    fireEvent.click(approveBtn);

    await waitFor(() => {
      expect(mockRespondToGate).toHaveBeenCalledOnce();
    });

    // Gate should be resolved in store
    const { gates } = useStore.getState();
    expect(gates.resolved.has("gate-1")).toBe(true);
    expect(gates.pending.has("gate-1")).toBe(false);
  });

  it("reject button calls RPC with reject decision", async () => {
    mockRespondToGate.mockResolvedValueOnce({ gateId: "gate-1", applied: true });

    render(
      <MemoryRouter>
        <GatePanel />
      </MemoryRouter>,
    );

    const rejectBtn = screen.getByRole("button", { name: /reject/i });
    fireEvent.click(rejectBtn);

    await waitFor(() => {
      expect(mockRespondToGate).toHaveBeenCalledOnce();
    });

    const arg = mockRespondToGate.mock.calls[0][0];
    expect(arg.decision).toBe(GateDecision.REJECT);
  });
});
