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
// `ProfilesSidebar` + `ProfileEditor` + `ProfileVersionsPanel` +
// `DeleteProfileModal`. Container keeps cross-subcomponent state + handlers.

const page: React.CSSProperties = {
  display: "flex",
  height: "100%",
  background: "var(--color-bg)",
  color: "var(--color-text)",
};

export function ProfilesPage() {
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [importYaml, setImportYaml] = useState("");
  const [showDelete, setShowDelete] = useState(false);
  const [showVersions, setShowVersions] = useState(false);
  const [form, setForm] = useState<ProfileForm>(EMPTY_FORM);

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
  // not a bare array. Accessing `.map` on the object (inside the form's
  // `<select>` render) was the §12.5 unmount trigger. Unwrap `.items` with a
  // defensive `Array.isArray` branch.
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

  function updateField<K extends keyof ProfileForm>(key: K, value: ProfileForm[K]) {
    setForm((prev) => ({ ...prev, [key]: value }));
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
              form={form}
              updateField={updateField}
              roles={roles}
              creating={creating}
              selectedId={selectedId}
              saving={saving}
              cloning={cloneMut.isPending}
              showVersions={showVersions}
              onSave={handleSave}
              onClone={handleClone}
              onExport={() => void handleExport()}
              onDeleteClick={() => setShowDelete(true)}
              onToggleVersions={() => setShowVersions(!showVersions)}
            />

            {showDelete && (
              <DeleteProfileModal
                profileName={form.name}
                onConfirm={handleDelete}
                onCancel={() => setShowDelete(false)}
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
