import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { CheckpointViewer } from "./CheckpointViewer.js";

const mockGetCheckpoint = vi.fn();

vi.mock("@/transport/client.js", () => ({
  getClient: () => ({
    getCheckpoint: mockGetCheckpoint,
  }),
}));

vi.mock("@/transport/errors.js", () => ({
  errorMessage: (err: unknown) =>
    err instanceof Error ? err.message : "Unknown error",
}));

beforeEach(() => {
  mockGetCheckpoint.mockReset();
});

const onClose = vi.fn();

function successResponse(payload: Uint8Array = new TextEncoder().encode("output data")) {
  return {
    metadata: {
      id: "cp-1",
      type: "artifact",
      status: { toString: () => "CHECKPOINT_STATUS_FINAL" },
      confidence: { toString: () => "CONFIDENCE_HIGH" },
      workspaceId: "ws-1",
      contentHash: "sha256-abc123",
    },
    payload,
  };
}

describe("CheckpointViewer", () => {
  it("shows loading state initially", () => {
    mockGetCheckpoint.mockReturnValue(new Promise(() => {})); // never resolves
    render(<CheckpointViewer checkpointId="cp-1" onClose={onClose} />);
    expect(screen.getByText("Loading...")).toBeInTheDocument();
  });

  it("renders metadata fields after load", async () => {
    mockGetCheckpoint.mockResolvedValueOnce(successResponse());
    render(<CheckpointViewer checkpointId="cp-1" onClose={onClose} />);
    await waitFor(() => {
      expect(screen.getByText("cp-1")).toBeInTheDocument();
    });
    expect(screen.getByText("artifact")).toBeInTheDocument();
    expect(screen.getByText("Verified")).toBeInTheDocument();
    expect(screen.getByText("sha256-abc123")).toBeInTheDocument();
  });

  it("renders text payload", async () => {
    mockGetCheckpoint.mockResolvedValueOnce(
      successResponse(new TextEncoder().encode("hello world")),
    );
    render(<CheckpointViewer checkpointId="cp-1" onClose={onClose} />);
    await waitFor(() => {
      expect(screen.getByText("hello world")).toBeInTheDocument();
    });
  });

  it("renders hex dump for binary payload", async () => {
    // Use invalid UTF-8 sequence
    const binary = new Uint8Array([0xff, 0xfe, 0x00, 0x01, 0x80]);
    mockGetCheckpoint.mockResolvedValueOnce(successResponse(binary));
    render(<CheckpointViewer checkpointId="cp-1" onClose={onClose} />);
    await waitFor(() => {
      expect(screen.getByText(/ff fe 00 01/)).toBeInTheDocument();
    });
  });

  it("shows error message on failure", async () => {
    mockGetCheckpoint.mockRejectedValueOnce(new Error("not found"));
    render(<CheckpointViewer checkpointId="cp-missing" onClose={onClose} />);
    await waitFor(() => {
      expect(screen.getByText("not found")).toBeInTheDocument();
    });
  });

  it("close button calls onClose", async () => {
    mockGetCheckpoint.mockResolvedValueOnce(successResponse());
    render(<CheckpointViewer checkpointId="cp-1" onClose={onClose} />);
    fireEvent.click(screen.getByText("Close"));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
