export function SettingsPanel() {
  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">Settings</h2>
      <div className="bg-zinc-900 rounded-lg p-4 border border-zinc-800 max-w-lg">
        <h3 className="text-sm font-medium mb-2">Autonomy Preset</h3>
        <p className="text-zinc-500 text-sm mb-4">
          Configuration changes require a runtime restart in the current version.
        </p>
        <select
          disabled
          className="w-full bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-500"
        >
          <option>supervised (read-only)</option>
          <option>autonomous</option>
          <option>gated</option>
          <option>custom</option>
        </select>
      </div>
    </div>
  );
}
