import type { ProfileForm, RoleSummary } from "./ProfilesPage.types";
import { EMPTY_FORM } from "./ProfilesPage.types";
import {
  formGrid,
  fullSpan,
  fieldLabel,
  inputStyle,
  btnPrimary,
  btnSecondary,
  btnDanger,
  actionsRow,
} from "./ProfilesPage.styles";

interface ProfileEditorProps {
  form: ProfileForm;
  updateField: <K extends keyof ProfileForm>(key: K, value: ProfileForm[K]) => void;
  roles: RoleSummary[];
  creating: boolean;
  selectedId: string | null;
  saving: boolean;
  cloning: boolean;
  showVersions: boolean;
  onSave: () => void;
  onClone: () => void;
  onExport: () => void;
  onDeleteClick: () => void;
  onToggleVersions: () => void;
}

export function ProfileEditor({
  form,
  updateField,
  roles,
  creating,
  selectedId,
  saving,
  cloning,
  showVersions,
  onSave,
  onClone,
  onExport,
  onDeleteClick,
  onToggleVersions,
}: ProfileEditorProps) {
  return (
    <>
      <h2 style={{ margin: "0 0 16px", fontSize: 20, fontWeight: 700 }}>
        {creating ? "New Profile" : `Edit: ${form.name}`}
      </h2>

      <div style={formGrid}>
        {/* Name */}
        <div style={fullSpan}>
          <label htmlFor="pf-name" style={fieldLabel}>Name</label>
          <input
            id="pf-name"
            style={inputStyle}
            value={form.name}
            onChange={(e) => updateField("name", e.target.value)}
          />
        </div>

        {/* Description */}
        <div style={fullSpan}>
          <label htmlFor="pf-description" style={fieldLabel}>Description</label>
          <textarea
            id="pf-description"
            style={{ ...inputStyle, minHeight: 60 }}
            value={form.description}
            onChange={(e) => updateField("description", e.target.value)}
          />
        </div>

        {/* Role */}
        <div>
          <label htmlFor="pf-role" style={fieldLabel}>Role</label>
          <select
            id="pf-role"
            style={inputStyle}
            value={form.role_ref}
            onChange={(e) => updateField("role_ref", e.target.value)}
          >
            <option value="">-- Select role --</option>
            {roles.map((r) => (
              <option key={r.id} value={r.id}>
                {r.name || r.id}
              </option>
            ))}
          </select>
        </div>

        {/* LLM Provider */}
        <div>
          <label htmlFor="pf-provider" style={fieldLabel}>LLM Provider</label>
          <input
            id="pf-provider"
            style={inputStyle}
            value={form.llm_provider}
            onChange={(e) => updateField("llm_provider", e.target.value)}
            placeholder="e.g. anthropic"
          />
        </div>

        {/* LLM Model */}
        <div>
          <label htmlFor="pf-model" style={fieldLabel}>LLM Model</label>
          <input
            id="pf-model"
            style={inputStyle}
            value={form.llm_model}
            onChange={(e) => updateField("llm_model", e.target.value)}
            placeholder="e.g. claude-sonnet-4-20250514"
          />
        </div>

        {/* Temperature */}
        <div>
          <label htmlFor="pf-temperature" style={fieldLabel}>Temperature</label>
          <input
            id="pf-temperature"
            style={inputStyle}
            type="number"
            step={0.1}
            min={0}
            max={2}
            value={form.temperature}
            onChange={(e) =>
              updateField("temperature", parseFloat(e.target.value) || 0)
            }
          />
        </div>

        {/* Max Tokens */}
        <div>
          <label htmlFor="pf-max-tokens" style={fieldLabel}>Max Tokens</label>
          <input
            id="pf-max-tokens"
            style={inputStyle}
            type="number"
            min={1}
            value={form.max_tokens}
            onChange={(e) =>
              updateField("max_tokens", parseInt(e.target.value, 10) || 0)
            }
          />
        </div>

        {/* Autonomy */}
        <div>
          <div id="pf-autonomy-label" style={fieldLabel}>Autonomy</div>
          <div
            role="radiogroup"
            aria-labelledby="pf-autonomy-label"
            style={{ display: "flex", gap: 16, paddingTop: 6 }}
          >
            {(["autonomous", "assisted", "supervised"] as const).map((opt) => (
              <label
                key={opt}
                style={{ display: "flex", alignItems: "center", gap: 4, cursor: "pointer" }}
              >
                <input
                  type="radio"
                  name="autonomy"
                  value={opt}
                  checked={form.autonomy === opt}
                  onChange={() => updateField("autonomy", opt)}
                />
                {opt}
              </label>
            ))}
          </div>
        </div>

        {/* Visibility */}
        <div>
          <div id="pf-visibility-label" style={fieldLabel}>Visibility</div>
          <div
            role="radiogroup"
            aria-labelledby="pf-visibility-label"
            style={{ display: "flex", gap: 16, paddingTop: 6 }}
          >
            {(["private", "shared"] as const).map((opt) => (
              <label
                key={opt}
                style={{ display: "flex", alignItems: "center", gap: 4, cursor: "pointer" }}
              >
                <input
                  type="radio"
                  name="visibility"
                  value={opt}
                  checked={form.visibility === opt}
                  onChange={() => updateField("visibility", opt)}
                />
                {opt}
              </label>
            ))}
          </div>
        </div>

        {/* Budget Limit */}
        <div>
          <label htmlFor="pf-budget-limit" style={fieldLabel}>Budget Limit</label>
          <input
            id="pf-budget-limit"
            style={inputStyle}
            type="number"
            min={0}
            step={0.01}
            value={form.budget_limit}
            onChange={(e) =>
              updateField("budget_limit", parseFloat(e.target.value) || 0)
            }
          />
        </div>

        {/* Budget Window (seconds) */}
        <div>
          <label htmlFor="pf-budget-window" style={fieldLabel}>Budget Window (seconds)</label>
          <input
            id="pf-budget-window"
            style={inputStyle}
            type="number"
            min={0}
            value={form.budget_window_secs}
            onChange={(e) =>
              updateField("budget_window_secs", parseInt(e.target.value, 10) || 0)
            }
          />
        </div>
      </div>

      {/* Save */}
      <div style={{ marginTop: 16 }}>
        <button style={btnPrimary} onClick={onSave} disabled={saving}>
          {saving ? "Saving..." : creating ? "Create Profile" : "Save Changes"}
        </button>
      </div>

      {/* Actions row (only for existing profiles) */}
      {!creating && selectedId && (
        <div style={actionsRow}>
          <button style={btnSecondary} onClick={onClone} disabled={cloning}>
            {cloning ? "Cloning..." : "Clone"}
          </button>
          <button style={btnSecondary} onClick={onExport}>Export YAML</button>
          <button style={btnDanger} onClick={onDeleteClick}>Delete</button>
          <button style={btnSecondary} onClick={onToggleVersions}>
            {showVersions ? "Hide Versions" : "Version History"}
          </button>
        </div>
      )}
    </>
  );
}

// Re-export for callers that import from ProfileEditor expecting EMPTY_FORM
export { EMPTY_FORM };
