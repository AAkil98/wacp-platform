import type React from "react";
import { useMemo, useState } from "react";
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
import { ProfilesSidebar } from "./ProfilesSidebar";
import { ProfileEditor } from "./ProfileEditor";
import { ProfileVersionsPanel } from "./ProfileVersionsPanel";
import { DeleteProfileModal } from "./DeleteProfileModal";
import { ImportYamlDialog } from "./ImportYamlDialog";
import type {
  ProfileSummary,
  ProfileDetail,
  RoleSummary,
  VersionEntry,
  ProfileForm,
} from "./ProfilesPage.types";
import { EMPTY_FORM } from "./ProfilesPage.types";
import { editorPanel } from "./ProfilesPage.styles";

// HEALTH-LOG §3.1 / frontend-perf-plan F4 — ProfilesPage decomposed into
// sidebar + editor + versions + delete + import-dialog subcomponents.
// F5 — form state now owned by `ProfileEditor` via react-hook-form; container
// passes `defaultValues` and receives `onSave(values)`.

const page: React.CSSProperties = {
  display: "flex",
  height: "100%",
  background: "var(--color-bg)",
  color: "var(--color-text)",
};

// Stable EMPTY_FORM reference for the "creating" case — avoids re-running the
// editor's `reset(defaultValues)` effect on every container re-render.
const CREATING_DEFAULTS: ProfileForm = { ...EMPTY_FORM };

function profileToForm(p: ProfileDetail | undefined): ProfileForm {
  if (!p) return CREATING_DEFAULTS;
  return {
    name: p.name ?? "",
    description: p.description ?? "",
    role_ref: p.role_ref ?? "",
    llm_provider: p.llm_provider ?? "",
    llm_model: p.llm_model ?? "",
    temperature: p.temperature ?? 0.7,
    max_tokens: p.max_tokens ?? 4096,
    autonomy: p.autonomy ?? "supervised",
    visibility: p.visibility ?? "private",
    budget_limit: p.budget_limit ?? 0,
    budget_window_secs: p.budget_window_secs ?? 3600,
  };
}

export function ProfilesPage() {
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [importYaml, setImportYaml] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [showVersions, setShowVersions] = useState(false);

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
  // not a bare array. Unwrap `.items` with a defensive `Array.isArray` branch
  // (§12.5 fix — otherwise roles.map inside the editor <select> throws on form mount).
  const rolesRaw = rolesQuery.data as
    | { items?: RoleSummary[] }
    | RoleSummary[]
    | undefined;
  const roles: RoleSummary[] = Array.isArray(rolesRaw) ? rolesRaw : (rolesRaw?.items ?? []);
  const versions = (versionsQuery.data ?? []) as VersionEntry[];

  const loadedProfile = profileQuery.data as ProfileDetail | undefined;

  // `defaultValues` for the editor — recomputed only when the loaded profile
  // ref changes or creating toggles. React-Query keeps `loadedProfile` stable
  // across renders with the same query key, so the memo matches reality.
  const editorDefaults = useMemo<ProfileForm>(
    () => (creating ? CREATING_DEFAULTS : profileToForm(loadedProfile)),
    [creating, loadedProfile],
  );

  function handleSelect(id: string) {
    setSelectedId(id);
    setCreating(false);
    setDeleteTarget(null);
    setShowVersions(false);
  }

  function handleNew() {
    setSelectedId(null);
    setCreating(true);
    setDeleteTarget(null);
    setShowVersions(false);
  }

  function handleSave(values: ProfileForm) {
    if (creating) {
      createMut.mutate(values as unknown as Record<string, unknown>, {
        onSuccess: (result) => {
          const created = result as { id?: string };
          if (created?.id) setSelectedId(created.id);
          setCreating(false);
        },
      });
    } else if (selectedId) {
      updateMut.mutate(values as unknown as Record<string, unknown>);
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

  async function handleExport(name: string) {
    if (!selectedId) return;
    try {
      const yaml = await api.get<string>(`/api/profiles/${selectedId}/export`);
      const blob = new Blob([yaml], { type: "application/x-yaml" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${name || "profile"}.yaml`;
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
        setDeleteTarget(null);
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

  const saving = createMut.isPending || updateMut.isPending;

  return (
    <div style={page}>
      <ProfilesSidebar
        profiles={profiles}
        isLoading={profilesQuery.isLoading}
        search={search}
        setSearch={setSearch}
        selectedId={selectedId}
        onSelect={handleSelect}
        onNew={handleNew}
        onImportClick={() => setShowImport(true)}
      />

      <div style={editorPanel}>
        {showImport && (
          <ImportYamlDialog
            yaml={importYaml}
            setYaml={setImportYaml}
            onImport={handleImport}
            onCancel={() => {
              setShowImport(false);
              setImportYaml("");
            }}
            isPending={importMut.isPending}
          />
        )}

        {!selectedId && !creating ? (
          <div
            style={{
              color: "var(--color-text-secondary)",
              paddingTop: 64,
              textAlign: "center",
            }}
          >
            <p style={{ fontSize: 16 }}>Select a profile from the library or create a new one.</p>
          </div>
        ) : (
          <>
            <ProfileEditor
              defaultValues={editorDefaults}
              roles={roles}
              creating={creating}
              selectedId={selectedId}
              saving={saving}
              cloning={cloneMut.isPending}
              showVersions={showVersions}
              onSave={handleSave}
              onClone={handleClone}
              onExport={(name) => void handleExport(name)}
              onDeleteClick={(name) => setDeleteTarget(name)}
              onToggleVersions={() => setShowVersions(!showVersions)}
            />

            {deleteTarget !== null && (
              <DeleteProfileModal
                profileName={deleteTarget}
                onConfirm={handleDelete}
                onCancel={() => setDeleteTarget(null)}
                isPending={deleteMut.isPending}
              />
            )}

            {showVersions && (
              <ProfileVersionsPanel versions={versions} isLoading={versionsQuery.isLoading} />
            )}
          </>
        )}
      </div>
    </div>
  );
}
