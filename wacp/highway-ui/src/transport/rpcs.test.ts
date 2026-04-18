import { describe, it, expect, vi, beforeEach } from "vitest";
import { GateDecision } from "../gen/primitives_pb.js";

// Mock dependencies before imports
const mockClient = {
  injectEnvelope: vi.fn(),
  respondToGate: vi.fn(),
  respondToEscalation: vi.fn(),
};

vi.mock("./client.js", () => ({
  getClient: () => mockClient,
}));

const mockStore = {
  markGateInFlight: vi.fn(),
  resolveGate: vi.fn(),
  markEscalationInFlight: vi.fn(),
  resolveEscalation: vi.fn(),
};

vi.mock("../store/index.js", () => ({
  useStore: { getState: () => mockStore },
}));

import { injectEnvelope, respondToGate, respondToEscalation } from "./rpcs.js";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("injectEnvelope", () => {
  it("returns envelope ID on success", async () => {
    mockClient.injectEnvelope.mockResolvedValueOnce({ envelopeId: "env-123" });
    const result = await injectEnvelope({
      toWorkspace: "ws-target",
      type: "feedback",
      priority: "Normal",
      payload: "hello",
    });
    expect(result.envelopeId).toBe("env-123");
    expect(mockClient.injectEnvelope).toHaveBeenCalledOnce();
    const arg = mockClient.injectEnvelope.mock.calls[0]![0];
    expect(arg.toWorkspace).toBe("ws-target");
    expect(arg.type).toBe("feedback");
  });

  it("passes clientRequestId as UUID", async () => {
    mockClient.injectEnvelope.mockResolvedValueOnce({ envelopeId: "env-1" });
    await injectEnvelope({
      toWorkspace: "ws-1",
      type: "query",
      priority: "Normal",
      payload: "",
    });
    const arg = mockClient.injectEnvelope.mock.calls[0]![0];
    expect(arg.clientRequestId).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/
    );
  });
});

describe("respondToGate", () => {
  it("marks in-flight, calls RPC, resolves gate", async () => {
    mockClient.respondToGate.mockResolvedValueOnce({ applied: true });
    const result = await respondToGate("gate-1", GateDecision.APPROVE);
    expect(result.applied).toBe(true);
    expect(mockStore.markGateInFlight).toHaveBeenCalledWith("gate-1");
    expect(mockStore.resolveGate).toHaveBeenCalledWith("gate-1", true);
  });

  it("generates clientRequestId as UUID", async () => {
    mockClient.respondToGate.mockResolvedValueOnce({ applied: true });
    await respondToGate("gate-2", GateDecision.REJECT);
    const arg = mockClient.respondToGate.mock.calls[0]![0];
    expect(arg.clientRequestId).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/
    );
  });
});

describe("respondToEscalation", () => {
  it("sends abort action", async () => {
    mockClient.respondToEscalation.mockResolvedValueOnce({ applied: true });
    const result = await respondToEscalation("esc-1", { type: "abort" });
    expect(result.applied).toBe(true);
    const arg = mockClient.respondToEscalation.mock.calls[0]![0];
    expect(arg.action.case).toBe("abort");
    expect(arg.action.value).toBe(true);
    expect(mockStore.markEscalationInFlight).toHaveBeenCalledWith("esc-1");
    expect(mockStore.resolveEscalation).toHaveBeenCalledWith("esc-1", true);
  });

  it("sends delegate action", async () => {
    mockClient.respondToEscalation.mockResolvedValueOnce({ applied: true });
    await respondToEscalation("esc-2", { type: "delegate" });
    const arg = mockClient.respondToEscalation.mock.calls[0]![0];
    expect(arg.action.case).toBe("delegateToCoordinator");
    expect(arg.action.value).toBe(true);
  });

  it("sends feedback action with envelope", async () => {
    mockClient.respondToEscalation.mockResolvedValueOnce({ applied: true });
    await respondToEscalation("esc-3", {
      type: "feedback",
      envelope: {
        toWorkspace: "ws-target",
        envelopeType: "feedback",
        priority: "Urgent",
        payload: "try this approach",
      },
    });
    const arg = mockClient.respondToEscalation.mock.calls[0]![0];
    expect(arg.action.case).toBe("feedback");
    expect(arg.action.value.toWorkspace).toBe("ws-target");
  });
});
