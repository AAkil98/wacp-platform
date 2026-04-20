import { useEffect } from "react";
import { useForm, Controller } from "react-hook-form";
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
  defaultValues: ProfileForm;
  roles: RoleSummary[];
  creating: boolean;
  selectedId: string | null;
  saving: boolean;
  cloning: boolean;
  showVersions: boolean;
  onSave: (values: ProfileForm) => void;
  onClone: () => void;
  onExport: (name: string) => void;
  onDeleteClick: (name: string) => void;
  onToggleVersions: () => void;
}

// HEALTH-LOG §3.4 / frontend-perf-plan F5 — `useForm`-driven editor. Field-level
// re-render radius: editing "Name" no longer re-renders "Description" or the
// radio groups. Container passes `defaultValues`; we `reset()` on change so
// loading a different profile swaps the whole form atomically.
export function ProfileEditor({
  defaultValues,
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
  const { register, control, handleSubmit, reset, watch } = useForm<ProfileForm>({
    defaultValues,
  });

  // Sync form when the container swaps the selected profile. Passing
  // `defaultValues` as the dep reference is safe because the container
  // computes it from a React-Query-stable `loadedProfile` (or the module-scope
  // `EMPTY_FORM` constant), so identity changes correspond to actual profile
  // swaps, not every render.
  useEffect(() => {
    reset(defaultValues);
  }, [defaultValues, reset]);

  const watchedName = watch("name");

  return (
    <>
      <h2 style={{ margin: "0 0 16px", fontSize: 20, fontWeight: 700 }}>
        {creating ? "New Profile" : `Edit: ${watchedName}`}
      </h2>

      <form onSubmit={handleSubmit(onSave)}>
        <div style={formGrid}>
          {/* Name */}
          <div style={fullSpan}>
            <label htmlFor="pf-name" style={fieldLabel}>Name</label>
            <input id="pf-name" style={inputStyle} {...register("name")} />
          </div>

          {/* Description */}
          <div style={fullSpan}>
            <label htmlFor="pf-description" style={fieldLabel}>Description</label>
            <textarea
              id="pf-description"
              style={{ ...inputStyle, minHeight: 60 }}
              {...register("description")}
            />
          </div>

          {/* Role */}
          <div>
            <label htmlFor="pf-role" style={fieldLabel}>Role</label>
            <select id="pf-role" style={inputStyle} {...register("role_ref")}>
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
              placeholder="e.g. anthropic"
              {...register("llm_provider")}
            />
          </div>

          {/* LLM Model */}
          <div>
            <label htmlFor="pf-model" style={fieldLabel}>LLM Model</label>
            <input
              id="pf-model"
              style={inputStyle}
              placeholder="e.g. claude-sonnet-4-20250514"
              {...register("llm_model")}
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
              {...register("temperature", { valueAsNumber: true })}
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
              {...register("max_tokens", { valueAsNumber: true })}
            />
          </div>

          {/* Autonomy — Controller wrapper so register + checked-radio semantics
              line up; react-hook-form binds radio groups by field name. */}
          <div>
            <div id="pf-autonomy-label" style={fieldLabel}>Autonomy</div>
            <div
              role="radiogroup"
              aria-labelledby="pf-autonomy-label"
              style={{ display: "flex", gap: 16, paddingTop: 6 }}
            >
              <Controller
                name="autonomy"
                control={control}
                render={({ field }) => (
                  <>
                    {(["autonomous", "assisted", "supervised"] as const).map((opt) => (
                      <label
                        key={opt}
                        style={{ display: "flex", alignItems: "center", gap: 4, cursor: "pointer" }}
                      >
                        <input
                          type="radio"
                          name={field.name}
                          value={opt}
                          checked={field.value === opt}
                          onChange={() => field.onChange(opt)}
                        />
                        {opt}
                      </label>
                    ))}
                  </>
                )}
              />
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
              <Controller
                name="visibility"
                control={control}
                render={({ field }) => (
                  <>
                    {(["private", "shared"] as const).map((opt) => (
                      <label
                        key={opt}
                        style={{ display: "flex", alignItems: "center", gap: 4, cursor: "pointer" }}
                      >
                        <input
                          type="radio"
                          name={field.name}
                          value={opt}
                          checked={field.value === opt}
                          onChange={() => field.onChange(opt)}
                        />
                        {opt}
                      </label>
                    ))}
                  </>
                )}
              />
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
              {...register("budget_limit", { valueAsNumber: true })}
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
              {...register("budget_window_secs", { valueAsNumber: true })}
            />
          </div>
        </div>

        {/* Save */}
        <div style={{ marginTop: 16 }}>
          <button type="submit" style={btnPrimary} disabled={saving}>
            {saving ? "Saving..." : creating ? "Create Profile" : "Save Changes"}
          </button>
        </div>
      </form>

      {/* Actions row (only for existing profiles). Outside the form so these
          buttons don't submit the form by default. */}
      {!creating && selectedId && (
        <div style={actionsRow}>
          <button style={btnSecondary} onClick={onClone} disabled={cloning}>
            {cloning ? "Cloning..." : "Clone"}
          </button>
          <button style={btnSecondary} onClick={() => onExport(watchedName)}>
            Export YAML
          </button>
          <button style={btnDanger} onClick={() => onDeleteClick(watchedName)}>
            Delete
          </button>
          <button style={btnSecondary} onClick={onToggleVersions}>
            {showVersions ? "Hide Versions" : "Version History"}
          </button>
        </div>
      )}
    </>
  );
}

export { EMPTY_FORM };
