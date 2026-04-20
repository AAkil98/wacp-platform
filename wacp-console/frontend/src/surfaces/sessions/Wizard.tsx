import type React from "react";
import { useState, useEffect, useCallback } from "react";
import { useVerticals, useVertical, useProfiles } from "../../api/hooks/index";
import { api } from "../../api/client";
import type { ApiError } from "../../api/client";
import { ContextForm } from "./ContextForm";

// --- Types ---

interface VerticalSummary {
  id: string;
  name: string;
  defining_constraint: string;
  tool_count: number;
  workflow_count: number;
  role_count: number;
}

interface WorkflowSummary {
  id: string;
  name: string;
  description: string;
  stage_count: number;
  gated_stage_count: number;
}

interface RoleSummary {
  id: string;
  name: string;
}

interface VerticalDetail {
  id: string;
  name: string;
  defining_constraint: string;
  roles: RoleSummary[];
  workflows: WorkflowSummary[];
  context_schema: Record<string, FieldSchema>;
}

interface FieldSchema {
  field_type: string;
  required: boolean;
  description: string;
  enum_values?: string[];
  default?: unknown;
}

interface ProfileSummary {
  id: string;
  name: string;
  role_ref: string;
}

// Stable empty-array ref for React-Query fallback. Using `?? []` inline creates
// a fresh [] per render, which made the `useEffect([profiles, ...])` in
// `RoleSlot` fire every render during query loading. HEALTH-LOG §3.3.
const EMPTY_PROFILES: readonly ProfileSummary[] = [];

interface Assignment {
  role_ref: string;
  profile_id: string;
}

interface BudgetOverrides {
  max_cost_micros: number | null;
  max_tokens: number | null;
  max_wall_time_ms: number | null;
}

interface WizardProps {
  onClose: () => void;
}

// --- Styles ---

const overlay: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  height: "100%",
  background: "var(--color-bg)",
  color: "var(--color-text)",
};

const stepBar: React.CSSProperties = {
  display: "flex",
  gap: 0,
  padding: "16px 24px",
  borderBottom: "1px solid var(--color-border)",
  background: "var(--color-bg-secondary)",
};

const stepItem = (state: "completed" | "active" | "pending"): React.CSSProperties => ({
  flex: 1,
  textAlign: "center",
  padding: "8px 4px",
  fontSize: 13,
  fontWeight: state === "active" ? 700 : 400,
  color:
    state === "completed"
      ? "var(--color-accent)"
      : state === "active"
        ? "var(--color-text)"
        : "var(--color-text-secondary)",
  borderBottom:
    state === "active"
      ? "2px solid var(--color-accent)"
      : state === "completed"
        ? "2px solid var(--color-accent)"
        : "2px solid transparent",
  opacity: state === "pending" ? 0.5 : 1,
});

const body: React.CSSProperties = {
  flex: 1,
  overflowY: "auto",
  padding: 24,
};

const footer: React.CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  padding: "16px 24px",
  borderTop: "1px solid var(--color-border)",
  background: "var(--color-bg-secondary)",
};

const btnPrimary: React.CSSProperties = {
  padding: "8px 16px",
  background: "var(--color-accent)",
  color: "#fff",
  border: "none",
  borderRadius: 4,
  cursor: "pointer",
  fontWeight: 600,
};

const btnSecondary: React.CSSProperties = {
  ...btnPrimary,
  background: "transparent",
  border: "1px solid var(--color-border)",
  color: "var(--color-text)",
};

const btnDanger: React.CSSProperties = {
  ...btnPrimary,
  background: "var(--color-danger)",
};

const cardGrid: React.CSSProperties = {
  display: "grid",
  gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))",
  gap: 16,
};

const card = (selected: boolean): React.CSSProperties => ({
  padding: 16,
  border: selected ? "2px solid var(--color-accent)" : "1px solid var(--color-border)",
  borderRadius: 8,
  background: selected ? "var(--color-bg-secondary)" : "var(--color-bg)",
  cursor: "pointer",
  transition: "border-color 0.15s",
});

const cardTitle: React.CSSProperties = {
  fontSize: 15,
  fontWeight: 700,
  marginBottom: 6,
};

const cardMeta: React.CSSProperties = {
  fontSize: 12,
  color: "var(--color-text-secondary)",
};

const cardDesc: React.CSSProperties = {
  fontSize: 13,
  color: "var(--color-text-secondary)",
  marginTop: 6,
  lineHeight: 1.4,
};

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "8px 10px",
  border: "1px solid var(--color-border)",
  borderRadius: 4,
  background: "var(--color-bg)",
  color: "var(--color-text)",
  fontSize: 14,
};

const fieldLabel: React.CSSProperties = {
  display: "block",
  marginBottom: 4,
  fontSize: 13,
  fontWeight: 600,
  color: "var(--color-text-secondary)",
};

const sectionHeading: React.CSSProperties = {
  fontSize: 18,
  fontWeight: 700,
  marginBottom: 16,
};

const summaryRow: React.CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  padding: "8px 0",
  borderBottom: "1px solid var(--color-border)",
  fontSize: 14,
};

const summaryLabel: React.CSSProperties = {
  fontWeight: 600,
  color: "var(--color-text-secondary)",
};

const errorBox: React.CSSProperties = {
  marginTop: 12,
  padding: 12,
  border: "1px solid var(--color-danger)",
  borderRadius: 6,
  background: "var(--color-bg-secondary)",
  color: "var(--color-danger)",
  fontSize: 13,
};

// --- Step labels ---

const STEPS = [
  "Select Vertical",
  "Select Workflow",
  "Assign Profiles",
  "Context",
  "Budget Overrides",
  "Review & Launch",
] as const;

// --- Component ---

export function Wizard({ onClose }: WizardProps) {
  const [step, setStep] = useState(0);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [selectedVertical, setSelectedVertical] = useState("");
  const [selectedWorkflow, setSelectedWorkflow] = useState("");
  const [assignments, setAssignments] = useState<Assignment[]>([]);
  const [contextValues, setContextValues] = useState<Record<string, unknown>>({});
  const [budgets, setBudgets] = useState<BudgetOverrides>({
    max_cost_micros: null,
    max_tokens: null,
    max_wall_time_ms: null,
  });
  const [sessionName, setSessionName] = useState("");
  const [launching, setLaunching] = useState(false);
  const [launchError, setLaunchError] = useState<string | null>(null);
  const [stepError, setStepError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // Data queries
  const verticalsQuery = useVerticals();
  const verticalQuery = useVertical(selectedVertical);

  const verticals = (verticalsQuery.data ?? []) as VerticalSummary[];
  const verticalDetail = verticalQuery.data as VerticalDetail | undefined;

  // Initialize context defaults when vertical detail loads
  useEffect(() => {
    if (!verticalDetail?.context_schema) return;
    const defaults: Record<string, unknown> = {};
    for (const [key, field] of Object.entries(verticalDetail.context_schema)) {
      if (field?.default !== undefined) {
        defaults[key] = field.default;
      }
    }
    setContextValues(defaults);
  }, [verticalDetail]);

  // Initialize assignments when vertical detail loads
  useEffect(() => {
    if (!verticalDetail?.roles) return;
    setAssignments(
      verticalDetail.roles.map((r) => ({ role_ref: r.id, profile_id: "" })),
    );
  }, [verticalDetail]);

  const contextSchema = verticalDetail?.context_schema ?? {};
  const hasContext = Object.keys(contextSchema).length > 0;

  // Step navigation
  const canNext = useCallback((): boolean => {
    switch (step) {
      case 0: return selectedVertical !== "";
      case 1: return selectedWorkflow !== "";
      case 2: return assignments.every((a) => a.profile_id !== "");
      case 3: return true; // context is optional-ish, validated on server
      case 4: return true; // budgets are optional
      case 5: return true;
      default: return false;
    }
  }, [step, selectedVertical, selectedWorkflow, assignments]);

  async function handleNext() {
    setStepError(null);
    setSaving(true);

    try {
      if (step === 0) {
        // Create session
        const result = await api.post<{ id: string }>("/api/sessions", {
          vertical: selectedVertical,
          workflow: "",
        });
        setSessionId(result.id);
      } else if (step === 1 && sessionId) {
        // Patch workflow
        await api.patch(`/api/sessions/${sessionId}`, {
          workflow: selectedWorkflow,
        });
      } else if (step === 2 && sessionId) {
        // Save assignments
        await api.put(`/api/sessions/${sessionId}/assignments`, assignments);
      } else if (step === 3 && sessionId) {
        // Save context
        await api.patch(`/api/sessions/${sessionId}`, {
          context: contextValues,
        });
      }
    } catch (err: unknown) {
      const apiErr = err as ApiError;
      setStepError(apiErr.message ?? "An error occurred");
      setSaving(false);
      return;
    }

    setSaving(false);

    // Auto-skip context step if schema is empty
    let nextStep = step + 1;
    if (nextStep === 3 && !hasContext) {
      nextStep = 4;
    }
    setStep(nextStep);
  }

  function handleBack() {
    setStepError(null);
    let prevStep = step - 1;
    if (prevStep === 3 && !hasContext) {
      prevStep = 2;
    }
    setStep(prevStep);
  }

  async function handleDiscard() {
    if (sessionId) {
      try {
        await api.post(`/api/sessions/${sessionId}/cancel`);
      } catch {
        // Best effort
      }
    }
    onClose();
  }

  async function handleLaunch() {
    if (!sessionId) return;
    setLaunching(true);
    setLaunchError(null);

    try {
      // Patch name + budgets
      const patch: Record<string, unknown> = {};
      if (sessionName) patch.name = sessionName;
      if (budgets.max_cost_micros != null) patch.max_cost_micros = budgets.max_cost_micros;
      if (budgets.max_tokens != null) patch.max_tokens = budgets.max_tokens;
      if (budgets.max_wall_time_ms != null) patch.max_wall_time_ms = budgets.max_wall_time_ms;

      if (Object.keys(patch).length > 0) {
        await api.patch(`/api/sessions/${sessionId}`, patch);
      }

      await api.post(`/api/sessions/${sessionId}/launch`);
      onClose();
    } catch (err: unknown) {
      const apiErr = err as ApiError;
      const detail = apiErr.detail as { violations?: string[] } | undefined;
      if (detail?.violations) {
        setLaunchError(detail.violations.join("\n"));
      } else {
        setLaunchError(apiErr.message ?? "Launch failed");
      }
    } finally {
      setLaunching(false);
    }
  }

  // --- Step renderers ---

  function renderStepContent(): React.ReactNode {
    switch (step) {
      case 0: return <SelectVerticalStep />;
      case 1: return <SelectWorkflowStep />;
      case 2: return <AssignProfilesStep />;
      case 3: return <ContextStep />;
      case 4: return <BudgetOverridesStep />;
      case 5: return <ReviewLaunchStep />;
      default: return null;
    }
  }

  function SelectVerticalStep() {
    if (verticalsQuery.isLoading) {
      return <p style={{ color: "var(--color-text-secondary)" }}>Loading verticals...</p>;
    }

    return (
      <div>
        <h2 style={sectionHeading}>Select a Vertical</h2>
        <div style={cardGrid}>
          {verticals.map((v) => (
            <div
              key={v.id}
              style={card(selectedVertical === v.id)}
              onClick={() => setSelectedVertical(v.id)}
            >
              <div style={cardTitle}>{v.name}</div>
              <div style={cardDesc}>{v.defining_constraint}</div>
              <div style={{ ...cardMeta, marginTop: 8 }}>
                {v.role_count} roles / {v.workflow_count} workflows / {v.tool_count} tools
              </div>
            </div>
          ))}
        </div>
        {verticals.length === 0 && !verticalsQuery.isLoading && (
          <p style={{ color: "var(--color-text-secondary)" }}>No verticals available.</p>
        )}
      </div>
    );
  }

  function SelectWorkflowStep() {
    if (verticalQuery.isLoading) {
      return <p style={{ color: "var(--color-text-secondary)" }}>Loading workflows...</p>;
    }

    const workflows = verticalDetail?.workflows ?? [];

    return (
      <div>
        <h2 style={sectionHeading}>Select a Workflow</h2>
        <div style={cardGrid}>
          {workflows.map((w) => (
            <div
              key={w.id}
              style={card(selectedWorkflow === w.id)}
              onClick={() => setSelectedWorkflow(w.id)}
            >
              <div style={cardTitle}>{w.name}</div>
              <div style={cardDesc}>{w.description}</div>
              <div style={{ ...cardMeta, marginTop: 8 }}>
                {w.stage_count} stages ({w.gated_stage_count} gated)
              </div>
            </div>
          ))}
        </div>
        {workflows.length === 0 && (
          <p style={{ color: "var(--color-text-secondary)" }}>No workflows available for this vertical.</p>
        )}
      </div>
    );
  }

  function AssignProfilesStep() {
    const roles = verticalDetail?.roles ?? [];

    return (
      <div>
        <h2 style={sectionHeading}>Assign Profiles to Roles</h2>
        {roles.map((role) => {
          const assignment = assignments.find((a) => a.role_ref === role.id);
          return (
            <RoleSlot
              key={role.id}
              roleId={role.id}
              roleName={role.name}
              selectedProfileId={assignment?.profile_id ?? ""}
              onSelect={(profileId) => {
                setAssignments((prev) =>
                  prev.map((a) =>
                    a.role_ref === role.id ? { ...a, profile_id: profileId } : a,
                  ),
                );
              }}
            />
          );
        })}
        {roles.length === 0 && (
          <p style={{ color: "var(--color-text-secondary)" }}>No roles defined for this vertical.</p>
        )}
      </div>
    );
  }

  function ContextStep() {
    return (
      <div>
        <h2 style={sectionHeading}>Session Context</h2>
        <ContextForm
          schema={contextSchema}
          values={contextValues}
          onChange={setContextValues}
        />
      </div>
    );
  }

  function BudgetOverridesStep() {
    return (
      <div>
        <h2 style={sectionHeading}>Budget Overrides (Optional)</h2>
        <p style={{ color: "var(--color-text-secondary)", marginBottom: 16, fontSize: 13 }}>
          Leave blank to use the default budget from the vertical definition.
        </p>

        <div style={{ display: "flex", flexDirection: "column", gap: 16, maxWidth: 400 }}>
          <div>
            <label style={fieldLabel} htmlFor="budget-max-cost">Max Cost (micros)</label>
            <input
              id="budget-max-cost"
              type="number"
              style={inputStyle}
              placeholder="e.g. 5000000"
              value={budgets.max_cost_micros ?? ""}
              onChange={(e) => {
                const v = e.target.value;
                setBudgets((b) => ({
                  ...b,
                  max_cost_micros: v === "" ? null : parseInt(v, 10),
                }));
              }}
            />
          </div>

          <div>
            <label style={fieldLabel} htmlFor="budget-max-tokens">Max Tokens</label>
            <input
              id="budget-max-tokens"
              type="number"
              style={inputStyle}
              placeholder="e.g. 100000"
              value={budgets.max_tokens ?? ""}
              onChange={(e) => {
                const v = e.target.value;
                setBudgets((b) => ({
                  ...b,
                  max_tokens: v === "" ? null : parseInt(v, 10),
                }));
              }}
            />
          </div>

          <div>
            <label style={fieldLabel} htmlFor="budget-max-wall-time">Max Wall Time (ms)</label>
            <input
              id="budget-max-wall-time"
              type="number"
              style={inputStyle}
              placeholder="e.g. 3600000"
              value={budgets.max_wall_time_ms ?? ""}
              onChange={(e) => {
                const v = e.target.value;
                setBudgets((b) => ({
                  ...b,
                  max_wall_time_ms: v === "" ? null : parseInt(v, 10),
                }));
              }}
            />
          </div>
        </div>
      </div>
    );
  }

  function ReviewLaunchStep() {
    const workflows = verticalDetail?.workflows ?? [];
    const wf = workflows.find((w) => w.id === selectedWorkflow);
    const roles = verticalDetail?.roles ?? [];

    return (
      <div>
        <h2 style={sectionHeading}>Review & Launch</h2>

        <div style={{ maxWidth: 560, border: "1px solid var(--color-border)", borderRadius: 8, padding: 20, background: "var(--color-bg-secondary)" }}>
          <div style={summaryRow}>
            <span style={summaryLabel}>Vertical</span>
            <span>{verticalDetail?.name ?? selectedVertical}</span>
          </div>
          <div style={summaryRow}>
            <span style={summaryLabel}>Workflow</span>
            <span>{wf?.name ?? selectedWorkflow}</span>
          </div>

          <div style={{ ...summaryRow, flexDirection: "column", gap: 4 }}>
            <span style={summaryLabel}>Assignments</span>
            {assignments.map((a) => {
              const role = roles.find((r) => r.id === a.role_ref);
              return (
                <div key={a.role_ref} style={{ fontSize: 13, paddingLeft: 8 }}>
                  {role?.name ?? a.role_ref} → <ProfileNameLabel profileId={a.profile_id} />
                </div>
              );
            })}
          </div>

          {Object.keys(contextValues).length > 0 && (
            <div style={{ ...summaryRow, flexDirection: "column", gap: 4 }}>
              <span style={summaryLabel}>Context</span>
              {Object.entries(contextValues).map(([k, v]) => (
                <div key={k} style={{ fontSize: 13, paddingLeft: 8 }}>
                  {k}: {String(v)}
                </div>
              ))}
            </div>
          )}

          {(budgets.max_cost_micros != null || budgets.max_tokens != null || budgets.max_wall_time_ms != null) && (
            <div style={{ ...summaryRow, flexDirection: "column", gap: 4 }}>
              <span style={summaryLabel}>Budget Overrides</span>
              {budgets.max_cost_micros != null && (
                <div style={{ fontSize: 13, paddingLeft: 8 }}>Max Cost: {budgets.max_cost_micros} micros</div>
              )}
              {budgets.max_tokens != null && (
                <div style={{ fontSize: 13, paddingLeft: 8 }}>Max Tokens: {budgets.max_tokens}</div>
              )}
              {budgets.max_wall_time_ms != null && (
                <div style={{ fontSize: 13, paddingLeft: 8 }}>Max Wall Time: {budgets.max_wall_time_ms} ms</div>
              )}
            </div>
          )}

          <div style={{ marginTop: 16 }}>
            <label style={fieldLabel} htmlFor="session-name">Session Name (optional)</label>
            <input
              id="session-name"
              type="text"
              style={inputStyle}
              placeholder="My session"
              value={sessionName}
              onChange={(e) => setSessionName(e.target.value)}
            />
          </div>

          <div style={{ marginTop: 16 }}>
            <button
              style={{ ...btnPrimary, width: "100%", padding: "10px 16px", fontSize: 15 }}
              onClick={() => void handleLaunch()}
              disabled={launching}
            >
              {launching ? "Launching..." : "Launch Session"}
            </button>
          </div>

          {launchError && (
            <div style={errorBox}>
              {launchError.split("\n").map((line, i) => (
                <div key={i}>{line}</div>
              ))}
            </div>
          )}
        </div>
      </div>
    );
  }

  // Step indicator state
  function stepState(idx: number): "completed" | "active" | "pending" {
    if (idx < step) return "completed";
    if (idx === step) return "active";
    return "pending";
  }

  return (
    <div style={overlay}>
      {/* Step indicator bar */}
      <div style={stepBar}>
        {STEPS.map((label, i) => (
          <div key={label} style={stepItem(stepState(i))}>
            {i + 1}. {label}
          </div>
        ))}
      </div>

      {/* Body */}
      <div style={body}>
        {renderStepContent()}
        {stepError && <div style={errorBox}>{stepError}</div>}
      </div>

      {/* Footer */}
      <div style={footer}>
        <button
          style={btnDanger}
          onClick={() => void handleDiscard()}
        >
          Discard
        </button>
        <div style={{ display: "flex", gap: 8 }}>
          {step > 0 && (
            <button style={btnSecondary} onClick={handleBack}>
              Back
            </button>
          )}
          {step < STEPS.length - 1 && (
            <button
              style={btnPrimary}
              onClick={() => void handleNext()}
              disabled={!canNext() || saving}
            >
              {saving ? "Saving..." : "Next"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

// --- Helper: Profile picker per role slot ---

function RoleSlot({
  roleId,
  roleName,
  selectedProfileId,
  onSelect,
}: {
  roleId: string;
  roleName: string;
  selectedProfileId: string;
  onSelect: (profileId: string) => void;
}) {
  const profilesQuery = useProfiles({ role_ref: roleId });
  const profiles = (profilesQuery.data ?? EMPTY_PROFILES) as ProfileSummary[];

  // Auto-select first match when profiles load. Dep'd on the first profile's
  // id (primitive) + selectedProfileId — not on `profiles` (array identity) or
  // `onSelect` (caller-owned closure). HEALTH-LOG §3.3 recommendation 3:
  // value-compare-via-key over object/callback identity.
  const firstProfileId = profiles[0]?.id;
  useEffect(() => {
    if (firstProfileId && !selectedProfileId) {
      onSelect(firstProfileId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [firstProfileId, selectedProfileId]);

  const slotStyle: React.CSSProperties = {
    display: "flex",
    alignItems: "center",
    gap: 16,
    padding: "12px 0",
    borderBottom: "1px solid var(--color-border)",
  };

  const roleLabel: React.CSSProperties = {
    fontWeight: 600,
    fontSize: 14,
    minWidth: 160,
  };

  const selectStyle: React.CSSProperties = {
    flex: 1,
    maxWidth: 320,
    padding: "8px 10px",
    border: "1px solid var(--color-border)",
    borderRadius: 4,
    background: "var(--color-bg)",
    color: "var(--color-text)",
    fontSize: 14,
  };

  return (
    <div style={slotStyle}>
      <span style={roleLabel}>{roleName}</span>
      {profilesQuery.isLoading ? (
        <span style={{ color: "var(--color-text-secondary)", fontSize: 13 }}>Loading profiles...</span>
      ) : profiles.length === 0 ? (
        <span style={{ color: "var(--color-danger)", fontSize: 13 }}>No matching profiles</span>
      ) : (
        <select
          style={selectStyle}
          value={selectedProfileId}
          onChange={(e) => onSelect(e.target.value)}
        >
          <option value="">-- Select profile --</option>
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>{p.name}</option>
          ))}
        </select>
      )}
    </div>
  );
}

// --- Helper: Display profile name by ID ---

function ProfileNameLabel({ profileId }: { profileId: string }) {
  const profilesQuery = useProfiles();
  const profiles = (profilesQuery.data ?? []) as ProfileSummary[];
  const profile = profiles.find((p) => p.id === profileId);
  return <span>{profile?.name ?? profileId}</span>;
}
