/**
 * Integration: session + store + client — 3 tests
 * Verifies session state transitions propagate through the store.
 */
import { useStore } from "../store/index.js";

const mockClient = {
  authenticate: vi.fn(),
  streamTrail: vi.fn(),
  streamWorkspaceChanges: vi.fn(),
  streamGates: vi.fn(),
  streamEscalations: vi.fn(),
  getTaskGraph: vi.fn(),
  getWorkspace: vi.fn(),
};

vi.mock("../transport/client.js", () => ({
  getClient: () => mockClient,
}));

vi.mock("../transport/streams.js", () => ({
  startTrailStream: vi.fn(() => new Promise(() => {})),
  startWorkspaceStream: vi.fn(() => new Promise(() => {})),
  fetchTaskGraph: vi.fn(() => Promise.resolve()),
  fetchWorkspace: vi.fn(() => Promise.resolve()),
}));

vi.mock("../notifications/notify.js", () => ({
  notifyGate: vi.fn(),
  notifyEscalation: vi.fn(),
}));

import { SessionManager } from "../transport/session.js";

beforeEach(() => {
  vi.clearAllMocks();
  useStore.setState({
    session: { state: "disconnected", userId: null, capabilities: [] },
  });
});

describe("session ↔ store integration", () => {
  it("connect updates store session to connected", async () => {
    mockClient.authenticate.mockResolvedValueOnce({
      userId: "user-1",
      capabilities: ["observe", "inject"],
    });
    mockClient.streamGates.mockReturnValue((async function* () { await new Promise(() => {}); })());
    mockClient.streamEscalations.mockReturnValue((async function* () { await new Promise(() => {}); })());

    const sm = new SessionManager();
    await sm.connect("valid-token");

    const { session } = useStore.getState();
    expect(session.state).toBe("connected");
    expect(session.userId).toBe("user-1");
    expect(session.capabilities).toContain("inject");
  });

  it("disconnect resets store session to disconnected", async () => {
    mockClient.authenticate.mockResolvedValueOnce({
      userId: "user-1",
      capabilities: [],
    });
    mockClient.streamGates.mockReturnValue((async function* () { await new Promise(() => {}); })());
    mockClient.streamEscalations.mockReturnValue((async function* () { await new Promise(() => {}); })());

    const sm = new SessionManager();
    await sm.connect("token");
    expect(useStore.getState().session.state).toBe("connected");

    sm.disconnect();
    expect(useStore.getState().session.state).toBe("disconnected");
  });

  it("failed connect leaves store disconnected", async () => {
    const { ConnectError, Code } = await import("@connectrpc/connect");
    mockClient.authenticate.mockRejectedValueOnce(
      new ConnectError("bad", Code.Unauthenticated),
    );

    const sm = new SessionManager();
    await expect(sm.connect("bad")).rejects.toThrow();

    expect(useStore.getState().session.state).toBe("disconnected");
  });
});
