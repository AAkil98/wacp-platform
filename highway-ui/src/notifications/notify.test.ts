import { describe, it, expect, vi, beforeEach } from "vitest";
import { notifyGate, notifyEscalation } from "./notify.js";
import type { PendingGate, ActiveEscalation } from "../store/index.js";

// Mock AudioContext
const mockOscillator = {
  connect: vi.fn(),
  type: "" as OscillatorType,
  frequency: { value: 0 },
  start: vi.fn(),
  stop: vi.fn(),
};

const mockGain = {
  connect: vi.fn(),
  gain: {
    value: 0,
    setValueAtTime: vi.fn(),
    exponentialRampToValueAtTime: vi.fn(),
  },
};

const mockAudioCtx = {
  createOscillator: vi.fn(() => mockOscillator),
  createGain: vi.fn(() => mockGain),
  destination: {},
  currentTime: 0,
};

globalThis.AudioContext = vi.fn(() => mockAudioCtx) as unknown as typeof AudioContext;

function makeGate(): PendingGate {
  return {
    gateId: "g-1",
    gateType: "task_approval",
    subject: new Uint8Array(),
    workspaceId: "ws-1",
    taskId: "t-1",
    timeoutMs: 60000n,
    fallbackAction: "approve",
    createdAt: BigInt(Date.now() * 1000),
  };
}

function makeEscalation(): ActiveEscalation {
  return {
    escalationId: "e-1",
    workspaceId: "ws-stuck",
    owner: "user-1",
    context: "Help needed",
    createdAt: BigInt(Date.now() * 1000),
  };
}

describe("notifyGate", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("plays an audio tone", () => {
    notifyGate(makeGate());
    expect(mockAudioCtx.createOscillator).toHaveBeenCalled();
    expect(mockOscillator.start).toHaveBeenCalled();
    expect(mockOscillator.stop).toHaveBeenCalled();
  });

  it("uses gate frequency (880 Hz)", () => {
    notifyGate(makeGate());
    expect(mockOscillator.frequency.value).toBe(880);
  });
});

describe("notifyEscalation", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("plays an audio tone", () => {
    notifyEscalation(makeEscalation());
    expect(mockAudioCtx.createOscillator).toHaveBeenCalled();
    expect(mockOscillator.start).toHaveBeenCalled();
  });

  it("uses escalation frequency (523 Hz)", () => {
    notifyEscalation(makeEscalation());
    expect(mockOscillator.frequency.value).toBe(523);
  });
});
