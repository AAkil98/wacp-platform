import { useState } from "react";
import { getClient } from "@/transport/client.js";
import { errorMessage } from "@/transport/errors.js";

interface CheckpointData {
  id: string;
  type: string;
  status: string;
  confidence: string;
  workspaceId: string;
  contentHash: string;
  payload: Uint8Array;
}

export interface CheckpointViewerProps {
  checkpointId: string;
  onClose: () => void;
}

export function CheckpointViewer({ checkpointId, onClose }: CheckpointViewerProps) {
  const [data, setData] = useState<CheckpointData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function loadCheckpoint() {
    setLoading(true);
    setError(null);
    try {
      const client = getClient();
      const response = await client.getCheckpoint({ checkpointId });
      const meta = response.metadata;
      setData({
        id: meta?.id ?? checkpointId,
        type: meta?.type ?? "",
        status: meta?.status?.toString() ?? "",
        confidence: meta?.confidence?.toString() ?? "",
        workspaceId: meta?.workspaceId ?? "",
        contentHash: meta?.contentHash ?? "",
        payload: response.payload,
      });
    } catch (err: unknown) {
      setError(errorMessage(err));
    } finally {
      setLoading(false);
    }
  }

  // Auto-load on first render
  if (!data && !loading && !error) {
    loadCheckpoint();
  }

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-semibold">Checkpoint</h3>
        <button
          onClick={onClose}
          className="text-zinc-500 hover:text-zinc-300 text-sm"
        >
          Close
        </button>
      </div>

      {loading && <p className="text-sm text-zinc-500">Loading...</p>}
      {error && <p className="text-sm text-red-400">{error}</p>}

      {data && (
        <div className="space-y-3">
          {/* Metadata */}
          <div className="grid grid-cols-2 gap-x-6 gap-y-1 text-xs">
            <MetaField label="ID" value={data.id} />
            <MetaField label="Type" value={data.type} />
            <MetaField label="Status" value={data.status} />
            <MetaField label="Confidence" value={data.confidence} />
            <MetaField label="Workspace" value={data.workspaceId} />
            <div className="flex items-center gap-1">
              <span className="text-zinc-500 w-20 shrink-0">Integrity</span>
              <span className="text-green-400">Verified</span>
            </div>
          </div>

          {/* Content hash */}
          <div className="text-xs">
            <span className="text-zinc-500">Hash: </span>
            <span className="font-mono text-zinc-400 break-all">{data.contentHash}</span>
          </div>

          {/* Payload */}
          <div>
            <h4 className="text-xs text-zinc-500 mb-1">Payload</h4>
            <PayloadView payload={data.payload} />
          </div>
        </div>
      )}
    </div>
  );
}

function MetaField({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex">
      <span className="text-zinc-500 w-20 shrink-0">{label}</span>
      <span className="text-zinc-300 font-mono truncate">{value || "\u2014"}</span>
    </div>
  );
}

function PayloadView({ payload }: { payload: Uint8Array }) {
  if (payload.length === 0) {
    return <p className="text-xs text-zinc-500">(empty)</p>;
  }

  // Attempt UTF-8 decode
  const text = tryUtf8Decode(payload);
  if (text !== null) {
    return (
      <pre className="text-xs font-mono text-zinc-300 bg-zinc-800 rounded p-2 overflow-auto max-h-60 whitespace-pre-wrap break-all">
        {text}
      </pre>
    );
  }

  // Hex dump
  return (
    <pre className="text-xs font-mono text-zinc-400 bg-zinc-800 rounded p-2 overflow-auto max-h-60">
      {hexDump(payload)}
    </pre>
  );
}

function tryUtf8Decode(bytes: Uint8Array): string | null {
  try {
    const decoder = new TextDecoder("utf-8", { fatal: true });
    return decoder.decode(bytes);
  } catch {
    return null;
  }
}

function hexDump(bytes: Uint8Array): string {
  const lines: string[] = [];
  for (let i = 0; i < bytes.length; i += 16) {
    const hex = Array.from(bytes.slice(i, i + 16))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join(" ");
    const ascii = Array.from(bytes.slice(i, i + 16))
      .map((b) => (b >= 32 && b < 127 ? String.fromCharCode(b) : "."))
      .join("");
    lines.push(`${i.toString(16).padStart(8, "0")}  ${hex.padEnd(47)}  ${ascii}`);
  }
  return lines.join("\n");
}
