import { useCallback, useMemo, useState } from "react";

// ── Types ──

type PresetName = "autonomous" | "supervised" | "gated" | "custom";
type FallbackAction = "approve" | "reject" | "escalate_to_coordinator";

interface GateConfig {
  enabled: boolean;
  timeoutS: number;
  fallback: FallbackAction;
}

interface HighwayConfig {
  preset: PresetName;
  visibility: boolean;
  injection: boolean;
  gates: Record<string, GateConfig>;
  gateDefaults: { timeoutS: number; fallback: FallbackAction };
  escalation: { enabled: boolean; timeoutS: number; fallback: FallbackAction };
}

const GATE_TYPES = [
  "task_approval",
  "workspace_create",
  "envelope_delivery",
  "integration",
  "conflict_resolution",
  "workspace_abort",
] as const;

const GATE_LABELS: Record<string, string> = {
  task_approval: "Task Approval",
  workspace_create: "Workspace Create",
  envelope_delivery: "Envelope Delivery",
  integration: "Integration",
  conflict_resolution: "Conflict Resolution",
  workspace_abort: "Workspace Abort",
};

// ── Presets (from human-highway spec §5) ──

function makeGates(enabled: boolean, timeoutS: number, fallback: FallbackAction): Record<string, GateConfig> {
  const gates: Record<string, GateConfig> = {};
  for (const type of GATE_TYPES) {
    gates[type] = { enabled, timeoutS, fallback };
  }
  return gates;
}

const PRESETS: Record<Exclude<PresetName, "custom">, Omit<HighwayConfig, "preset">> = {
  autonomous: {
    visibility: true,
    injection: true,
    gates: makeGates(false, 0, "approve"),
    gateDefaults: { timeoutS: 0, fallback: "approve" },
    escalation: { enabled: false, timeoutS: 0, fallback: "escalate_to_coordinator" },
  },
  supervised: {
    visibility: true,
    injection: true,
    gates: {
      ...makeGates(false, 0, "approve"),
      task_approval: { enabled: true, timeoutS: 30, fallback: "approve" },
    },
    gateDefaults: { timeoutS: 30, fallback: "approve" },
    escalation: { enabled: true, timeoutS: 60, fallback: "escalate_to_coordinator" },
  },
  gated: {
    visibility: true,
    injection: true,
    gates: makeGates(true, 0, "approve"),
    gateDefaults: { timeoutS: 0, fallback: "approve" },
    escalation: { enabled: true, timeoutS: 0, fallback: "escalate_to_coordinator" },
  },
};

function defaultConfig(): HighwayConfig {
  return { preset: "supervised", ...structuredClone(PRESETS.supervised) };
}

// ── Component ──

export function SettingsPanel() {
  const [config, setConfig] = useState<HighwayConfig>(defaultConfig);

  const updateField = useCallback(<K extends keyof HighwayConfig>(key: K, value: HighwayConfig[K]) => {
    setConfig((c) => ({ ...c, [key]: value, preset: "custom" as PresetName }));
  }, []);

  const handlePresetChange = useCallback((preset: PresetName) => {
    if (preset === "custom") {
      setConfig((c) => ({ ...c, preset: "custom" }));
    } else {
      setConfig({ preset, ...structuredClone(PRESETS[preset]) });
    }
  }, []);

  const updateGate = useCallback((gateType: string, patch: Partial<GateConfig>) => {
    setConfig((c) => ({
      ...c,
      preset: "custom",
      gates: { ...c.gates, [gateType]: { ...c.gates[gateType]!, ...patch } },
    }));
  }, []);

  const updateEscalation = useCallback((patch: Partial<HighwayConfig["escalation"]>) => {
    setConfig((c) => ({
      ...c,
      preset: "custom",
      escalation: { ...c.escalation, ...patch },
    }));
  }, []);

  const updateGateDefaults = useCallback((patch: Partial<HighwayConfig["gateDefaults"]>) => {
    setConfig((c) => ({
      ...c,
      preset: "custom",
      gateDefaults: { ...c.gateDefaults, ...patch },
    }));
  }, []);

  // ── Warnings ──
  const enabledGates = useMemo(
    () => GATE_TYPES.filter((t) => config.gates[t]?.enabled),
    [config.gates],
  );

  const allTimeoutsZero = useMemo(() => {
    if (enabledGates.length === 0) return false;
    return enabledGates.every((t) => config.gates[t]!.timeoutS === 0);
  }, [enabledGates, config.gates]);

  return (
    <div className="max-w-2xl">
      <h2 className="text-lg font-semibold mb-4">Settings</h2>

      {/* Preset selector */}
      <div className="bg-zinc-900 rounded-lg p-4 border border-zinc-800 mb-4">
        <h3 className="text-sm font-medium mb-2">Autonomy Preset</h3>
        <select
          value={config.preset}
          onChange={(e) => handlePresetChange(e.target.value as PresetName)}
          className="w-full bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm text-white"
        >
          <option value="autonomous">autonomous</option>
          <option value="supervised">supervised</option>
          <option value="gated">gated</option>
          <option value="custom">custom</option>
        </select>
      </div>

      {/* Visibility */}
      <Section title="Visibility">
        <Toggle
          label="Trail visibility"
          checked={config.visibility}
          onChange={(v) => updateField("visibility", v)}
        />
      </Section>

      {/* Injection */}
      <Section title="Injection">
        <Toggle
          label="Allow envelope injection"
          checked={config.injection}
          onChange={(v) => updateField("injection", v)}
        />
      </Section>

      {/* Gate defaults */}
      <Section title="Gate Defaults">
        <div className="flex gap-4 items-center">
          <label className="text-xs text-zinc-400 w-20">Timeout (s)</label>
          <input
            type="number"
            min={0}
            value={config.gateDefaults.timeoutS}
            onChange={(e) => updateGateDefaults({ timeoutS: Number(e.target.value) })}
            className="bg-zinc-800 border border-zinc-700 rounded px-2 py-1 text-sm text-white w-24"
          />
          <label className="text-xs text-zinc-400 w-16">Fallback</label>
          <FallbackSelect
            value={config.gateDefaults.fallback}
            onChange={(f) => updateGateDefaults({ fallback: f })}
          />
        </div>
      </Section>

      {/* Gates */}
      <Section title="Gates">
        {enabledGates.length === 0 && (
          <p className="text-xs text-amber-400 mb-2">No gates enabled — all transitions will proceed without approval.</p>
        )}
        <div className="space-y-2">
          {GATE_TYPES.map((type) => {
            const gate = config.gates[type]!;
            return (
              <div key={type} className="flex items-center gap-3">
                <Toggle
                  label={GATE_LABELS[type] ?? type}
                  checked={gate.enabled}
                  onChange={(v) => updateGate(type, { enabled: v })}
                  labelWidth="w-40"
                />
                {gate.enabled && (
                  <>
                    <input
                      type="number"
                      min={0}
                      value={gate.timeoutS}
                      onChange={(e) => updateGate(type, { timeoutS: Number(e.target.value) })}
                      className="bg-zinc-800 border border-zinc-700 rounded px-2 py-1 text-xs text-white w-20"
                      title="Timeout (seconds). 0 = indefinite."
                    />
                    <FallbackSelect
                      value={gate.fallback}
                      onChange={(f) => updateGate(type, { fallback: f })}
                    />
                  </>
                )}
              </div>
            );
          })}
        </div>
      </Section>

      {/* Escalation */}
      <Section title="Escalation">
        <div className="space-y-2">
          <Toggle
            label="Enable escalation"
            checked={config.escalation.enabled}
            onChange={(v) => updateEscalation({ enabled: v })}
          />
          {config.escalation.enabled && (
            <div className="flex gap-4 items-center ml-4">
              <label className="text-xs text-zinc-400 w-20">Timeout (s)</label>
              <input
                type="number"
                min={0}
                value={config.escalation.timeoutS}
                onChange={(e) => updateEscalation({ timeoutS: Number(e.target.value) })}
                className="bg-zinc-800 border border-zinc-700 rounded px-2 py-1 text-sm text-white w-24"
              />
              <label className="text-xs text-zinc-400 w-16">Fallback</label>
              <FallbackSelect
                value={config.escalation.fallback}
                onChange={(f) => updateEscalation({ fallback: f })}
              />
            </div>
          )}
        </div>
      </Section>

      {/* Warnings */}
      {allTimeoutsZero && (
        <div className="bg-amber-900/30 border border-amber-800 rounded-lg p-3 mb-4 text-sm text-amber-300">
          Indefinite timeouts on all gates. A missing human will deadlock the system.
        </div>
      )}

      {/* Apply button — disabled per spec §12 */}
      <button
        disabled
        title="Configuration changes require a runtime restart in the current version."
        className="bg-zinc-700 text-zinc-500 font-medium py-2 px-4 rounded text-sm cursor-not-allowed"
      >
        Apply
      </button>
    </div>
  );
}

// ── Sub-components ──

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="bg-zinc-900 rounded-lg p-4 border border-zinc-800 mb-4">
      <h3 className="text-sm font-medium mb-3">{title}</h3>
      {children}
    </div>
  );
}

function Toggle({
  label,
  checked,
  onChange,
  labelWidth = "w-40",
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  labelWidth?: string;
}) {
  return (
    <label className="flex items-center gap-3 cursor-pointer">
      <span className={`text-sm text-zinc-300 ${labelWidth}`}>{label}</span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`relative inline-flex h-5 w-9 shrink-0 rounded-full transition-colors ${
          checked ? "bg-blue-600" : "bg-zinc-700"
        }`}
      >
        <span
          className={`inline-block h-4 w-4 rounded-full bg-white transition-transform mt-0.5 ${
            checked ? "translate-x-4 ml-0.5" : "translate-x-0.5"
          }`}
        />
      </button>
    </label>
  );
}

function FallbackSelect({
  value,
  onChange,
}: {
  value: FallbackAction;
  onChange: (v: FallbackAction) => void;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value as FallbackAction)}
      className="bg-zinc-800 border border-zinc-700 rounded px-2 py-1 text-xs text-white"
    >
      <option value="approve">approve</option>
      <option value="reject">reject</option>
      <option value="escalate_to_coordinator">escalate</option>
    </select>
  );
}
