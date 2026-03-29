import { useState } from "react";

export function InjectionForm() {
  const [targetWorkspace, setTargetWorkspace] = useState("");
  const [envelopeType, setEnvelopeType] = useState("directive");
  const [priority, setPriority] = useState("Normal");
  const [payload, setPayload] = useState("");

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">Inject Envelope</h2>
      <form className="max-w-lg space-y-4">
        <div>
          <label className="block text-sm text-zinc-300 mb-1">
            Target Workspace
          </label>
          <input
            type="text"
            value={targetWorkspace}
            onChange={(e) => setTargetWorkspace(e.target.value)}
            placeholder="ws-..."
            className="w-full bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm text-white"
          />
        </div>

        <div>
          <label className="block text-sm text-zinc-300 mb-1">
            Envelope Type
          </label>
          <select
            value={envelopeType}
            onChange={(e) => setEnvelopeType(e.target.value)}
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
            className="w-full bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm text-white font-mono"
          />
        </div>

        <button
          type="submit"
          disabled={!targetWorkspace.trim()}
          className="bg-blue-600 hover:bg-blue-700 disabled:bg-zinc-700 disabled:text-zinc-500 text-white font-medium py-2 px-4 rounded text-sm"
        >
          Send
        </button>
      </form>
    </div>
  );
}
