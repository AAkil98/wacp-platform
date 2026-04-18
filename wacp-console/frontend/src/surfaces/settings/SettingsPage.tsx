import type React from "react";
import { useState, useEffect } from "react";
import { useSettings, useSetSetting } from "../../api/hooks/index";
import { useUiStore } from "../../store/ui";

// --- Styles ---

const page: React.CSSProperties = {
  padding: 24,
  maxWidth: 800,
  color: "var(--color-text)",
  background: "var(--color-bg)",
};

const groupBox: React.CSSProperties = {
  marginBottom: 24,
  padding: 20,
  border: "1px solid var(--color-border)",
  borderRadius: 6,
  background: "var(--color-bg-secondary)",
};

const groupTitle: React.CSSProperties = {
  margin: "0 0 16px",
  fontSize: 16,
  fontWeight: 700,
};

const fieldRow: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 12,
  marginBottom: 12,
};

const fieldLabel: React.CSSProperties = {
  width: 200,
  fontSize: 13,
  fontWeight: 600,
  color: "var(--color-text-secondary)",
  flexShrink: 0,
};

const inputStyle: React.CSSProperties = {
  flex: 1,
  padding: "8px 10px",
  border: "1px solid var(--color-border)",
  borderRadius: 4,
  background: "var(--color-bg)",
  color: "var(--color-text)",
  fontSize: 14,
};

const btnPrimary: React.CSSProperties = {
  padding: "6px 14px",
  background: "var(--color-accent)",
  color: "#fff",
  border: "none",
  borderRadius: 4,
  cursor: "pointer",
  fontWeight: 600,
  fontSize: 13,
  flexShrink: 0,
};

const radioLabel: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 4,
  cursor: "pointer",
  fontSize: 14,
};

// --- Types ---

interface SettingEntry {
  key: string;
  value: unknown;
}

const GROUPS: Record<string, { title: string; keys: string[] }> = {
  runtime: {
    title: "Runtime Connection",
    keys: [
      "runtime.agent_address",
      "runtime.highway_address",
      "runtime.coordinator_address",
      "runtime.rest_address",
    ],
  },
  taxonomy: {
    title: "Taxonomy",
    keys: ["taxonomy.path"],
  },
  ui: {
    title: "User Interface",
    keys: ["ui.theme", "ui.trail_buffer"],
  },
  auth: {
    title: "Authentication",
    keys: ["auth.session_ttl"],
  },
};

const LABELS: Record<string, string> = {
  "runtime.agent_address": "Agent Service Address",
  "runtime.highway_address": "Highway Service Address",
  "runtime.coordinator_address": "Coordinator Service Address",
  "runtime.rest_address": "REST Gateway Address",
  "taxonomy.path": "Taxonomy Path",
  "ui.theme": "Theme",
  "ui.trail_buffer": "Trail Buffer Size",
  "auth.session_ttl": "Session TTL (seconds)",
};

export function SettingsPage() {
  const settingsQuery = useSettings();
  const setSetting = useSetSetting();
  const { theme, setTheme } = useUiStore();

  // Local editable state: key -> string value
  const [localValues, setLocalValues] = useState<Record<string, string>>({});
  // Track which keys have been modified
  const [dirty, setDirty] = useState<Record<string, boolean>>({});
  // Track save feedback
  const [saved, setSaved] = useState<Record<string, boolean>>({});

  // Initialize local values from server data
  const settings = (settingsQuery.data ?? []) as SettingEntry[];
  useEffect(() => {
    const vals: Record<string, string> = {};
    for (const s of settings) {
      vals[s.key] = String(s.value ?? "");
    }
    setLocalValues(vals);
    setDirty({});
    // Deliberate: re-init from server only when the query refreshes. Depending
    // on `settings` would reset user edits on every local change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settingsQuery.dataUpdatedAt]);

  function handleChange(key: string, value: string) {
    setLocalValues((prev) => ({ ...prev, [key]: value }));
    setDirty((prev) => ({ ...prev, [key]: true }));
    setSaved((prev) => ({ ...prev, [key]: false }));
  }

  function handleSave(key: string) {
    let parsedValue: unknown = localValues[key];
    // Attempt to parse as number if it looks numeric
    const num = Number(parsedValue);
    if (parsedValue !== "" && !isNaN(num)) {
      parsedValue = num;
    }

    setSetting.mutate(
      { key, value: parsedValue },
      {
        onSuccess: () => {
          setDirty((prev) => ({ ...prev, [key]: false }));
          setSaved((prev) => ({ ...prev, [key]: true }));
          // If theme changed, apply it
          if (key === "ui.theme") {
            const t = String(parsedValue);
            if (t === "light" || t === "dark" || t === "system") {
              setTheme(t);
            }
          }
        },
      },
    );
  }

  function handleThemeChange(t: "light" | "dark" | "system") {
    handleChange("ui.theme", t);
    setSetting.mutate(
      { key: "ui.theme", value: t },
      {
        onSuccess: () => {
          setTheme(t);
          setDirty((prev) => ({ ...prev, "ui.theme": false }));
          setSaved((prev) => ({ ...prev, "ui.theme": true }));
        },
      },
    );
  }

  if (settingsQuery.isLoading) {
    return (
      <div style={page}>
        <h1 style={{ margin: "0 0 16px", fontSize: 24, fontWeight: 700 }}>Settings</h1>
        <p style={{ color: "var(--color-text-secondary)" }}>Loading settings...</p>
      </div>
    );
  }

  return (
    <div style={page}>
      <h1 style={{ margin: "0 0 24px", fontSize: 24, fontWeight: 700 }}>Settings</h1>

      {Object.entries(GROUPS).map(([groupKey, group]) => (
        <div key={groupKey} style={groupBox}>
          <h2 style={groupTitle}>{group.title}</h2>
          {group.keys.map((key) => {
            // Special: theme uses radio buttons
            if (key === "ui.theme") {
              return (
                <div key={key} style={fieldRow}>
                  <span style={fieldLabel}>{LABELS[key] ?? key}</span>
                  <div style={{ display: "flex", gap: 16, flex: 1 }}>
                    {(["light", "dark", "system"] as const).map((opt) => (
                      <label key={opt} style={radioLabel}>
                        <input
                          type="radio"
                          name="theme"
                          value={opt}
                          checked={(localValues[key] ?? theme) === opt}
                          onChange={() => handleThemeChange(opt)}
                        />
                        {opt}
                      </label>
                    ))}
                  </div>
                  {saved[key] && <span style={{ fontSize: 12, color: "var(--color-accent)" }}>Saved</span>}
                </div>
              );
            }

            return (
              <div key={key} style={fieldRow}>
                <span style={fieldLabel}>{LABELS[key] ?? key}</span>
                <input
                  style={inputStyle}
                  value={localValues[key] ?? ""}
                  onChange={(e) => handleChange(key, e.target.value)}
                />
                <button
                  style={{ ...btnPrimary, opacity: dirty[key] ? 1 : 0.5 }}
                  disabled={!dirty[key]}
                  onClick={() => handleSave(key)}
                >
                  Save
                </button>
                {saved[key] && <span style={{ fontSize: 12, color: "var(--color-accent)" }}>Saved</span>}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}
