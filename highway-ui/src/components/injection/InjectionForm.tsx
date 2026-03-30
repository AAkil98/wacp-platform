import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { useStore } from "@/store";
import { injectEnvelope } from "@/transport/rpcs.js";
import { errorMessage } from "@/transport/errors.js";

type FormState = "idle" | "sending" | "success" | "error";

export function InjectionForm() {
  const [searchParams] = useSearchParams();
  const [targetWorkspace, setTargetWorkspace] = useState("");
  const [envelopeType, setEnvelopeType] = useState("directive");
  const [priority, setPriority] = useState("Normal");
  const [payload, setPayload] = useState("");
  const [formState, setFormState] = useState<FormState>("idle");
  const [resultMessage, setResultMessage] = useState("");

  const views = useStore((s) => s.workspaces.views);
  const workspaceIds = useMemo(() => [...views.keys()], [views]);

  // Pre-populate from query params (e.g., from escalation "Send Feedback")
  useEffect(() => {
    const ws = searchParams.get("workspace");
    const type = searchParams.get("type");
    if (ws) setTargetWorkspace(ws);
    if (type) setEnvelopeType(type);
  }, [searchParams]);

  // Clear success message and reset form after delay
  useEffect(() => {
    if (formState === "success") {
      const timer = setTimeout(() => {
        setFormState("idle");
        setResultMessage("");
        setTargetWorkspace("");
        setEnvelopeType("directive");
        setPriority("Normal");
        setPayload("");
      }, 2000);
      return () => clearTimeout(timer);
    }
  }, [formState]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!targetWorkspace.trim()) return;

    setFormState("sending");
    setResultMessage("");

    try {
      const result = await injectEnvelope({
        toWorkspace: targetWorkspace.trim(),
        type: envelopeType,
        priority,
        payload,
      });
      setFormState("success");
      setResultMessage(`Envelope injected — ID: ${result.envelopeId}`);
    } catch (err: unknown) {
      setFormState("error");
      setResultMessage(errorMessage(err));
    }
  };

  const canSend = targetWorkspace.trim() !== "" && formState !== "sending";

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">Inject Envelope</h2>
      <form className="max-w-lg space-y-4" onSubmit={handleSubmit}>
        <div>
          <label className="block text-sm text-zinc-300 mb-1">
            Target Workspace
          </label>
          <input
            type="text"
            value={targetWorkspace}
            onChange={(e) => setTargetWorkspace(e.target.value)}
            placeholder="ws-..."
            list="inject-workspace-ids"
            disabled={formState === "sending"}
            className="w-full bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm text-white"
          />
          <datalist id="inject-workspace-ids">
            {workspaceIds.map((id) => (
              <option key={id} value={id} />
            ))}
          </datalist>
        </div>

        <div>
          <label className="block text-sm text-zinc-300 mb-1">
            Envelope Type
          </label>
          <select
            value={envelopeType}
            onChange={(e) => setEnvelopeType(e.target.value)}
            disabled={formState === "sending"}
            className="w-full bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm text-white"
          >
            <option value="directive">directive</option>
            <option value="feedback">feedback</option>
            <option value="query">query</option>
          </select>
        </div>

        <div>
          <label className="block text-sm text-zinc-300 mb-1">Priority</label>
          <div className="flex gap-4">
            {["Normal", "Urgent", "Blocking"].map((p) => (
              <label key={p} className="flex items-center gap-1 text-sm">
                <input
                  type="radio"
                  name="priority"
                  value={p}
                  checked={priority === p}
                  onChange={() => setPriority(p)}
                  disabled={formState === "sending"}
                  className="accent-blue-500"
                />
                {p}
              </label>
            ))}
          </div>
        </div>

        <div>
          <label className="block text-sm text-zinc-300 mb-1">Payload</label>
          <textarea
            value={payload}
            onChange={(e) => setPayload(e.target.value)}
            rows={6}
            placeholder="Envelope payload (UTF-8 text)"
            disabled={formState === "sending"}
            className="w-full bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm text-white font-mono"
          />
        </div>

        {/* Result message */}
        {resultMessage && (
          <div
            className={`text-sm rounded px-3 py-2 ${
              formState === "success"
                ? "bg-green-900/50 text-green-300 border border-green-800"
                : "bg-red-900/50 text-red-300 border border-red-800"
            }`}
          >
            {resultMessage}
          </div>
        )}

        <button
          type="submit"
          disabled={!canSend}
          className="bg-blue-600 hover:bg-blue-700 disabled:bg-zinc-700 disabled:text-zinc-500 text-white font-medium py-2 px-4 rounded text-sm"
        >
          {formState === "sending" ? "Sending..." : "Send"}
        </button>
      </form>
    </div>
  );
}
