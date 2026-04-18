import { describe, it, expect, vi, beforeEach } from "vitest";
import { ConnectError, Code } from "@connectrpc/connect";

// Mock all transport dependencies

const mockClient = {
  authenticate: vi.fn(),
  streamTrail: vi.fn(),
  streamWorkspaceChanges: vi.fn(),
  streamGates: vi.fn(),
  streamEscalations: vi.fn(),
  getTaskGraph: vi.fn(),
  getWorkspace: vi.fn(),
};

vi.mock("./client.js", () => ({
  getClient: () => mockClient,
}));

// Mock streams module — these are called by openStreams
vi.mock("./streams.js", () => ({
  startTrailStream: vi.fn(() => new Promise(() => {})),
  startWorkspaceStream: vi.fn(() => new Promise(() => {})),
  fetchTaskGraph: vi.fn(() => Promise.resolve()),
  fetchWorkspace: vi.fn(() => Promise.resolve()),
}));

vi.mock("../notifications/notify.js", () => ({
  notifyGate: vi.fn(),
  notifyEscalation: vi.fn(),
}));

let sessionState = "disconnected";
const mockSetSession = vi.fn((state: string) => { sessionState = state; });

vi.mock("../store/index.js", () => ({
  useStore: {
    getState: () => ({
      setSession: mockSetSession,
      session: { get state() { return sessionState; } },
      appendTrailEntry: vi.fn(),
      addWorkspaceChange: vi.fn(),
      upsertWorkspace: vi.fn(),
      setTaskGraph: vi.fn(),
      addPendingGate: vi.fn(),
      addEscalation: vi.fn(),
      setEscalationBanner: vi.fn(),
      workspaces: {},
    }),
  },
}));

import { SessionManager } from "./session.js";

beforeEach(() => {
  vi.clearAllMocks();
  sessionState = "disconnected";
});

describe("SessionManager", () => {
  it("connect sets state to connected on success", async () => {
    mockClient.authenticate.mockResolvedValueOnce({
      userId: "user-1",
      capabilities: ["observe", "inject"],
    });
    // Mock the gate/escalation streams to hang forever
    mockClient.streamGates.mockReturnValue((async function* () { await new Promise(() => {}); })());
    mockClient.streamEscalations.mockReturnValue((async function* () { await new Promise(() => {}); })());

    const sm = new SessionManager();
    await sm.connect("valid-token");

    expect(mockSetSession).toHaveBeenCalledWith(
      "connected",
      "user-1",
      ["observe", "inject"],
    );
  });

  it("connect with invalid token throws and sets disconnected", async () => {
    mockClient.authenticate.mockRejectedValueOnce(
      new ConnectError("bad token", Code.Unauthenticated),
    );

    const sm = new SessionManager();
    await expect(sm.connect("bad-token")).rejects.toThrow("Authentication failed");

    // setSession called with "authenticating" then "disconnected"
    const states = mockSetSession.mock.calls.map((c: unknown[]) => c[0]);
    expect(states).toContain("disconnected");
  });

  it("connect with server error throws connection failed", async () => {
    mockClient.authenticate.mockRejectedValueOnce(
      new ConnectError("server down", Code.Unavailable),
    );

    const sm = new SessionManager();
    await expect(sm.connect("token")).rejects.toThrow("Connection failed");
  });

  it("disconnect sets state to disconnected", async () => {
    mockClient.authenticate.mockResolvedValueOnce({
      userId: "user-1",
      capabilities: [],
    });
    mockClient.streamGates.mockReturnValue((async function* () { await new Promise(() => {}); })());
    mockClient.streamEscalations.mockReturnValue((async function* () { await new Promise(() => {}); })());

    const sm = new SessionManager();
    await sm.connect("token");
    sm.disconnect();

    const lastCall = mockSetSession.mock.calls[mockSetSession.mock.calls.length - 1]!;
    expect(lastCall[0]).toBe("disconnected");
  });

  it("getState returns current session state", async () => {
    const sm = new SessionManager();
    expect(sm.getState()).toBe("disconnected");
  });
});
