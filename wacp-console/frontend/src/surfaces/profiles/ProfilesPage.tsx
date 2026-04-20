import type React from "react";
import { useState, useEffect } from "react";
import {
  useProfiles,
  useProfile,
  useProfileVersions,
  useRoles,
  useCreateProfile,
  useUpdateProfile,
  useDeleteProfile,
  useCloneProfile,
  useImportProfile,
} from "../../api/hooks/index";
import { api } from "../../api/client";

// --- Styles ---

const page: React.CSSProperties = {
  display: "flex",
  height: "100%",
  background: "var(--color-bg)",
  color: "var(--color-text)",
};

const sidebar: React.CSSProperties = {
  width: 340,
  borderRight: "1px solid var(--color-border)",
  display: "flex",
  flexDirection: "column",
  background: "var(--color-bg-secondary)",
  overflow: "hidden",
};

const sidebarHeader: React.CSSProperties = {
  padding: "16px",
  borderBottom: "1px solid var(--color-border)",
  display: "flex",
  flexDirection: "column",
  gap: 8,
};

const listContainer: React.CSSProperties = {
  flex: 1,
  overflowY: "auto",
};

const listItem = (selected: boolean): React.CSSProperties => ({
  padding: "12px 16px",
  cursor: "pointer",
  borderBottom: "1px solid var(--color-border)",
  background: selected ? "var(--color-accent)" : "transparent",
  color: selected ? "#fff" : "var(--color-text)",
});

const badge: React.CSSProperties = {
  display: "inline-block",
  padding: "2px 8px",
  borderRadius: 4,
  fontSize: 11,
  fontWeight: 600,
  background: "var(--color-border)",
  color: "var(--color-text-secondary)",
  marginLeft: 6,
};

const editorPanel: React.CSSProperties = {
  flex: 1,
  overflowY: "auto",
  padding: 24,
};

const formGrid: React.CSSProperties = {
  display: "grid",
  gridTemplateColumns: "1fr 1fr",
  gap: 16,
};

const fullSpan: React.CSSProperties = { gridColumn: "1 / -1" };

const fieldLabel: React.CSSProperties = {
  display: "block",
  marginBottom: 4,
  fontSize: 13,
  fontWeight: 600,
  color: "var(--color-text-secondary)",
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

const btnPrimary: React.CSSProperties = {
  padding: "8px 16px",
  background: "var(--color-accent)",
  color: "#fff",
  border: "none",
  borderRadius: 4,
  cursor: "pointer",
  fontWeight: 600,
};

const btnDanger: React.CSSProperties = {
  ...btnPrimary,
  background: "var(--color-danger)",
};

const btnSecondary: React.CSSProperties = {
  ...btnPrimary,
  background: "transparent",
  border: "1px solid var(--color-border)",
  color: "var(--color-text)",
};

const actionsRow: React.CSSProperties = {
  display: "flex",
  gap: 8,
  marginTop: 24,
  paddingTop: 16,
  borderTop: "1px solid var(--color-border)",
};

// --- Component ---

interface ProfileSummary {
  id: string;
  name: string;
  role_ref: string;
  autonomy: string;
  visibility: string;
}

interface ProfileDetail {
  id: string;
  name: string;
  description: string;
  role_ref: string;
  llm_provider: string;
  llm_model: string;
  temperature: number;
  max_tokens: number;
  autonomy: string;
  visibility: string;
  budget_limit: number;
  budget_window_secs: number;
}

interface RoleSummary {
  id: string;
  name: string;
}

interface VersionEntry {
  version: number;
  created_at: string;
  summary: string;
}

const EMPTY_FORM = {
  name: "",
  description: "",
  role_ref: "",
  llm_provider: "",
  llm_model: "",
  temperature: 0.7,
  max_tokens: 4096,
  autonomy: "supervised",
  visibility: "private",
  budget_limit: 0,
  budget_window_secs: 3600,
};

export function ProfilesPage() {
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [importYaml, setImportYaml] = useState("");
  const [showDelete, setShowDelete] = useState(false);
  const [showVersions, setShowVersions] = useState(false);
  const [form, setForm] = useState(EMPTY_FORM);

  const profilesQuery = useProfiles({ search: search || undefined });
  const profileQuery = useProfile(selectedId ?? "");
  const versionsQuery = useProfileVersions(selectedId ?? "");
  const rolesQuery = useRoles();

  const createMut = useCreateProfile();
  const updateMut = useUpdateProfile(selectedId ?? "");
  const deleteMut = useDeleteProfile(selectedId ?? "");
  const cloneMut = useCloneProfile(selectedId ?? "");
  const importMut = useImportProfile();

  const profiles = (profilesQuery.data ?? []) as ProfileSummary[];
  // `/api/roles` returns PaginatedResponse<RoleEntry> = `{items, cursor, has_more}`,
  // not a bare array. Accessing `.map` on the object later (inside the form's
  // `<select>` render) was the §12.5 unmount trigger — the form only renders
  // after Create New click, so the crash deferred until that moment.
  const rolesRaw = rolesQuery.data as
    | { items?: RoleSummary[] }
    | RoleSummary[]
    | undefined;
  const roles: RoleSummary[] = Array.isArray(rolesRaw) ? rolesRaw : (rolesRaw?.items ?? []);
  const versions = (versionsQuery.data ?? []) as VersionEntry[];

  // Sync form from loaded profile
  const loadedProfile = profileQuery.data as ProfileDetail | undefined;
  useEffect(() => {
    if (loadedProfile && !creating) {
      setForm({
        name: loadedProfile.name ?? "",
        description: loadedProfile.description ?? "",
        role_ref: loadedProfile.role_ref ?? "",
        llm_provider: loadedProfile.llm_provider ?? "",
        llm_model: loadedProfile.llm_model ?? "",
        temperature: loadedProfile.temperature ?? 0.7,
        max_tokens: loadedProfile.max_tokens ?? 4096,
        autonomy: loadedProfile.autonomy ?? "supervised",
        visibility: loadedProfile.visibility ?? "private",
        budget_limit: loadedProfile.budget_limit ?? 0,
        budget_window_secs: loadedProfile.budget_window_secs ?? 3600,
      });
    }
  }, [loadedProfile, creating]);

  function handleSelect(id: string) {
    setSelectedId(id);
    setCreating(false);
    setShowDelete(false);
    setShowVersions(false);
  }

  function handleNew() {
    setSelectedId(null);
    setCreating(true);
    setForm(EMPTY_FORM);
    setShowDelete(false);
    setShowVersions(false);
  }

  function handleSave() {
    if (creating) {
      createMut.mutate(form as unknown as Record<string, unknown>, {
        onSuccess: (result) => {
          const created = result as { id?: string };
          if (created?.id) setSelectedId(created.id);
          setCreating(false);
        },
      });
    } else if (selectedId) {
      updateMut.mutate(form as unknown as Record<string, unknown>);
    }
  }

  function handleClone() {
    if (!selectedId) return;
    cloneMut.mutate(undefined, {
      onSuccess: (result) => {
        const cloned = result as { id?: string };
        if (cloned?.id) setSelectedId(cloned.id);
      },
    });
  }

  async function handleExport() {
    if (!selectedId) return;
    try {
      const yaml = await api.get<string>(`/api/profiles/${selectedId}/export`);
      const blob = new Blob([yaml], { type: "application/x-yaml" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${form.name || "profile"}.yaml`;
      a.click();
      URL.revokeObjectURL(url);
    } catch {
      // export failed silently
    }
  }

  function handleDelete() {
    if (!selectedId) return;
    deleteMut.mutate(undefined, {
      onSuccess: () => {
        setSelectedId(null);
        setShowDelete(false);
      },
    });
  }

  function handleImport() {
    if (!importYaml.trim()) return;
    importMut.mutate(importYaml, {
      onSuccess: () => {
        setShowImport(false);
        setImportYaml("");
      },
    });
  }

  function updateField<K extends keyof typeof EMPTY_FORM>(key: K, value: (typeof EMPTY_FORM)[K]) {
    setForm((prev) => ({ ...prev, [key]: value }));
  }

  const saving = createMut.isPending || updateMut.isPending;

  return (
    <div style={page}>
      {/* Library sidebar */}
      <div style={sidebar}>
        <div style={sidebarHeader}>
          <h2 style={{ margin: 0, fontSize: 18, fontWeight: 700 }}>Profiles</h2>
          <input
            style={inputStyle}
            placeholder="Search profiles..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <div style={{ display: "flex", gap: 8 }}>
            <button style={btnPrimary} onClick={handleNew}>Create New</button>
            <button style={btnSecondary} onClick={() => setShowImport(true)}>Import YAML</button>
          </div>
        </div>

        <div style={listContainer}>
          {profilesQuery.isLoading && <div style={{ padding: 16, color: "var(--color-text-secondary)" }}>Loading...</div>}
          {profiles.map((p) => (
            <div
              key={p.id}
              style={listItem(selectedId === p.id)}
              onClick={() => handleSelect(p.id)}
            >
              <div style={{ fontWeight: 600, fontSize: 14 }}>{p.name}</div>
              <div style={{ fontSize: 12, color: selectedId === p.id ? "rgba(255,255,255,0.8)" : "var(--color-text-secondary)", marginTop: 2 }}>
                {p.role_ref}
                <span style={badge}>{p.autonomy}</span>
                <span style={{ ...badge, background: p.visibility === "shared" ? "var(--color-accent)" : "var(--color-border)", color: p.visibility === "shared" ? "#fff" : "var(--color-text-secondary)" }}>
                  {p.visibility}
                </span>
              </div>
            </div>
          ))}
          {!profilesQuery.isLoading && profiles.length === 0 && (
            <div style={{ padding: 16, color: "var(--color-text-secondary)" }}>No profiles found.</div>
          )}
        </div>
      </div>

      {/* Editor panel */}
      <div style={editorPanel}>
        {/* Import dialog */}
        {showImport && (
          <div style={{ marginBottom: 24, padding: 16, border: "1px solid var(--color-border)", borderRadius: 6, background: "var(--color-bg-secondary)" }}>
            <h3 style={{ margin: "0 0 8px" }}>Import Profile from YAML</h3>
            <textarea
              style={{ ...inputStyle, minHeight: 120, fontFamily: "monospace" }}
              placeholder="Paste YAML here..."
              value={importYaml}
              onChange={(e) => setImportYaml(e.target.value)}
            />
            <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
              <button style={btnPrimary} onClick={handleImport} disabled={importMut.isPending}>
                {importMut.isPending ? "Importing..." : "Import"}
              </button>
              <button style={btnSecondary} onClick={() => { setShowImport(false); setImportYaml(""); }}>Cancel</button>
            </div>
          </div>
        )}

        {!selectedId && !creating ? (
          <div style={{ color: "var(--color-text-secondary)", paddingTop: 64, textAlign: "center" }}>
            <p style={{ fontSize: 16 }}>Select a profile from the library or create a new one.</p>
          </div>
        ) : (
          <>
            <h2 style={{ margin: "0 0 16px", fontSize: 20, fontWeight: 700 }}>
              {creating ? "New Profile" : `Edit: ${form.name}`}
            </h2>

            <div style={formGrid}>
              {/* Name */}
              <div style={fullSpan}>
                <label htmlFor="pf-name" style={fieldLabel}>Name</label>
                <input id="pf-name" style={inputStyle} value={form.name} onChange={(e) => updateField("name", e.target.value)} />
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
                <select id="pf-role" style={inputStyle} value={form.role_ref} onChange={(e) => updateField("role_ref", e.target.value)}>
                  <option value="">-- Select role --</option>
                  {roles.map((r) => (
                    <option key={r.id} value={r.id}>{r.name || r.id}</option>
                  ))}
                </select>
              </div>

              {/* LLM Provider */}
              <div>
                <label htmlFor="pf-provider" style={fieldLabel}>LLM Provider</label>
                <input id="pf-provider" style={inputStyle} value={form.llm_provider} onChange={(e) => updateField("llm_provider", e.target.value)} placeholder="e.g. anthropic" />
              </div>

              {/* LLM Model */}
              <div>
                <label htmlFor="pf-model" style={fieldLabel}>LLM Model</label>
                <input id="pf-model" style={inputStyle} value={form.llm_model} onChange={(e) => updateField("llm_model", e.target.value)} placeholder="e.g. claude-sonnet-4-20250514" />
              </div>

              {/* Temperature */}
              <div>
                <label htmlFor="pf-temperature" style={fieldLabel}>Temperature</label>
                <input id="pf-temperature" style={inputStyle} type="number" step={0.1} min={0} max={2} value={form.temperature} onChange={(e) => updateField("temperature", parseFloat(e.target.value) || 0)} />
              </div>

              {/* Max Tokens */}
              <div>
                <label htmlFor="pf-max-tokens" style={fieldLabel}>Max Tokens</label>
                <input id="pf-max-tokens" style={inputStyle} type="number" min={1} value={form.max_tokens} onChange={(e) => updateField("max_tokens", parseInt(e.target.value, 10) || 0)} />
              </div>

              {/* Autonomy */}
              <div>
                <div id="pf-autonomy-label" style={fieldLabel}>Autonomy</div>
                <div role="radiogroup" aria-labelledby="pf-autonomy-label" style={{ display: "flex", gap: 16, paddingTop: 6 }}>
                  {(["autonomous", "assisted", "supervised"] as const).map((opt) => (
                    <label key={opt} style={{ display: "flex", alignItems: "center", gap: 4, cursor: "pointer" }}>
                      <input type="radio" name="autonomy" value={opt} checked={form.autonomy === opt} onChange={() => updateField("autonomy", opt)} />
                      {opt}
                    </label>
                  ))}
                </div>
              </div>

              {/* Visibility */}
              <div>
                <div id="pf-visibility-label" style={fieldLabel}>Visibility</div>
                <div role="radiogroup" aria-labelledby="pf-visibility-label" style={{ display: "flex", gap: 16, paddingTop: 6 }}>
                  {(["private", "shared"] as const).map((opt) => (
                    <label key={opt} style={{ display: "flex", alignItems: "center", gap: 4, cursor: "pointer" }}>
                      <input type="radio" name="visibility" value={opt} checked={form.visibility === opt} onChange={() => updateField("visibility", opt)} />
                      {opt}
                    </label>
                  ))}
                </div>
              </div>

              {/* Budget Limit */}
              <div>
                <label htmlFor="pf-budget-limit" style={fieldLabel}>Budget Limit</label>
                <input id="pf-budget-limit" style={inputStyle} type="number" min={0} step={0.01} value={form.budget_limit} onChange={(e) => updateField("budget_limit", parseFloat(e.target.value) || 0)} />
              </div>

              {/* Budget Window (seconds) */}
              <div>
                <label htmlFor="pf-budget-window" style={fieldLabel}>Budget Window (seconds)</label>
                <input id="pf-budget-window" style={inputStyle} type="number" min={0} value={form.budget_window_secs} onChange={(e) => updateField("budget_window_secs", parseInt(e.target.value, 10) || 0)} />
              </div>
            </div>

            {/* Save */}
            <div style={{ marginTop: 16 }}>
              <button style={btnPrimary} onClick={handleSave} disabled={saving}>
                {saving ? "Saving..." : creating ? "Create Profile" : "Save Changes"}
              </button>
            </div>

            {/* Actions row (only for existing profiles) */}
            {!creating && selectedId && (
              <div style={actionsRow}>
                <button style={btnSecondary} onClick={handleClone} disabled={cloneMut.isPending}>
                  {cloneMut.isPending ? "Cloning..." : "Clone"}
                </button>
                <button style={btnSecondary} onClick={() => void handleExport()}>Export YAML</button>
                <button style={btnDanger} onClick={() => setShowDelete(true)}>Delete</button>
                <button style={btnSecondary} onClick={() => setShowVersions(!showVersions)}>
                  {showVersions ? "Hide Versions" : "Version History"}
                </button>
              </div>
            )}

            {/* Delete confirmation */}
            {showDelete && (
              <div style={{ marginTop: 16, padding: 16, border: "1px solid var(--color-danger)", borderRadius: 6, background: "var(--color-bg-secondary)" }}>
                <p style={{ margin: "0 0 8px", fontWeight: 600, color: "var(--color-danger)" }}>
                  Delete profile "{form.name}"?
                </p>
                <p style={{ margin: "0 0 12px", fontSize: 13, color: "var(--color-warning)" }}>
                  Warning: This action cannot be undone. Any sessions using this profile may be affected.
                </p>
                <div style={{ display: "flex", gap: 8 }}>
                  <button style={btnDanger} onClick={handleDelete} disabled={deleteMut.isPending}>
                    {deleteMut.isPending ? "Deleting..." : "Confirm Delete"}
                  </button>
                  <button style={btnSecondary} onClick={() => setShowDelete(false)}>Cancel</button>
                </div>
              </div>
            )}

            {/* Version history panel */}
            {showVersions && (
              <div style={{ marginTop: 16, padding: 16, border: "1px solid var(--color-border)", borderRadius: 6, background: "var(--color-bg-secondary)" }}>
                <h3 style={{ margin: "0 0 12px", fontSize: 16 }}>Version History</h3>
                {versionsQuery.isLoading && <p style={{ color: "var(--color-text-secondary)" }}>Loading versions...</p>}
                {versions.length === 0 && !versionsQuery.isLoading && (
                  <p style={{ color: "var(--color-text-secondary)" }}>No version history available.</p>
                )}
                {versions.map((v) => (
                  <div key={v.version} style={{ padding: "8px 0", borderBottom: "1px solid var(--color-border)", fontSize: 13 }}>
                    <span style={{ fontWeight: 600 }}>v{v.version}</span>
                    <span style={{ marginLeft: 12, color: "var(--color-text-secondary)" }}>{v.created_at}</span>
                    {v.summary && <span style={{ marginLeft: 12 }}>{v.summary}</span>}
                  </div>
                ))}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
