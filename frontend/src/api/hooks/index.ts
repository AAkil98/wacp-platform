/**
 * TanStack Query hooks for all endpoint families.
 */

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../client";

// --- Health ---
export function useHealth() {
  return useQuery({ queryKey: ["health"], queryFn: () => api.get("/api/health"), refetchInterval: 30000 });
}

// --- Discovery ---
export function useRoles(params?: { base_role?: string; vertical?: string }) {
  const qs = new URLSearchParams();
  if (params?.base_role) qs.set("base_role", params.base_role);
  if (params?.vertical) qs.set("vertical", params.vertical);
  const q = qs.toString();
  return useQuery({ queryKey: ["roles", params], queryFn: () => api.get(`/api/roles${q ? `?${q}` : ""}`) });
}

export function useRole(id: string) {
  return useQuery({ queryKey: ["role", id], queryFn: () => api.get(`/api/roles/${id}`), enabled: !!id });
}

export function useTools(params?: { vertical?: string; has_policy?: boolean }) {
  const qs = new URLSearchParams();
  if (params?.vertical) qs.set("vertical", params.vertical);
  if (params?.has_policy != null) qs.set("has_policy", String(params.has_policy));
  const q = qs.toString();
  return useQuery({ queryKey: ["tools", params], queryFn: () => api.get(`/api/tools${q ? `?${q}` : ""}`) });
}

export function useTool(name: string) {
  return useQuery({ queryKey: ["tool", name], queryFn: () => api.get(`/api/tools/${name}`), enabled: !!name });
}

export function useVerticals() {
  return useQuery({ queryKey: ["verticals"], queryFn: () => api.get("/api/verticals") });
}

export function useVertical(id: string) {
  return useQuery({ queryKey: ["vertical", id], queryFn: () => api.get(`/api/verticals/${id}`), enabled: !!id });
}

export function useEnvelopeTypes() {
  return useQuery({ queryKey: ["envelopeTypes"], queryFn: () => api.get("/api/envelope-types") });
}

export function useCheckpointTypes() {
  return useQuery({ queryKey: ["checkpointTypes"], queryFn: () => api.get("/api/checkpoint-types") });
}

export function useSearch(q: string, type?: string, vertical?: string) {
  const qs = new URLSearchParams({ q });
  if (type) qs.set("type", type);
  if (vertical) qs.set("vertical", vertical);
  return useQuery({
    queryKey: ["search", q, type, vertical],
    queryFn: () => api.get(`/api/search?${qs}`),
    enabled: q.length >= 2,
  });
}

// --- Per-vertical ---
export function useVerticalWorkflows(id: string) {
  return useQuery({ queryKey: ["vertical", id, "workflows"], queryFn: () => api.get(`/api/verticals/${id}/workflows`), enabled: !!id });
}
export function useVerticalTaskTypes(id: string) {
  return useQuery({ queryKey: ["vertical", id, "taskTypes"], queryFn: () => api.get(`/api/verticals/${id}/task-types`), enabled: !!id });
}
export function useVerticalContextSchema(id: string) {
  return useQuery({ queryKey: ["vertical", id, "contextSchema"], queryFn: () => api.get(`/api/verticals/${id}/context-schema`), enabled: !!id });
}

// --- Profiles ---
export function useProfiles(params?: { role_ref?: string; search?: string }) {
  const qs = new URLSearchParams();
  if (params?.role_ref) qs.set("role_ref", params.role_ref);
  if (params?.search) qs.set("search", params.search);
  const q = qs.toString();
  return useQuery({ queryKey: ["profiles", params], queryFn: () => api.get(`/api/profiles${q ? `?${q}` : ""}`) });
}

export function useProfile(id: string) {
  return useQuery({ queryKey: ["profile", id], queryFn: () => api.get(`/api/profiles/${id}`), enabled: !!id });
}

export function useProfileVersions(id: string) {
  return useQuery({ queryKey: ["profile", id, "versions"], queryFn: () => api.get(`/api/profiles/${id}/versions`), enabled: !!id });
}

export function useCreateProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (data: Record<string, unknown>) => api.post("/api/profiles", data),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["profiles"] }); },
  });
}

export function useUpdateProfile(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (data: Record<string, unknown>) => api.put(`/api/profiles/${id}`, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["profiles"] });
      qc.invalidateQueries({ queryKey: ["profile", id] });
    },
  });
}

export function useDeleteProfile(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.delete(`/api/profiles/${id}`),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["profiles"] }); },
  });
}

export function useCloneProfile(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post(`/api/profiles/${id}/clone`),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["profiles"] }); },
  });
}

export function useImportProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (yaml: string) => api.post("/api/profiles/import", { yaml }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["profiles"] }); },
  });
}

// --- Users ---
export function useUsers() {
  return useQuery({ queryKey: ["users"], queryFn: () => api.get("/api/users") });
}

export function useCreateUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (data: Record<string, unknown>) => api.post("/api/users", data),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["users"] }); },
  });
}

// --- Settings ---
export function useSettings() {
  return useQuery({ queryKey: ["settings"], queryFn: () => api.get("/api/settings") });
}

export function useSetSetting() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ key, value }: { key: string; value: unknown }) => api.put(`/api/settings/${key}`, value),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["settings"] }); },
  });
}

// --- Audit Log ---
export function useAuditLog(params?: { user_id?: string; action?: string; limit?: number }) {
  const qs = new URLSearchParams();
  if (params?.user_id) qs.set("user_id", params.user_id);
  if (params?.action) qs.set("action", params.action);
  if (params?.limit) qs.set("limit", String(params.limit));
  const q = qs.toString();
  return useQuery({ queryKey: ["auditLog", params], queryFn: () => api.get(`/api/audit-log${q ? `?${q}` : ""}`) });
}

// --- Sessions ---
export function useSessions(params?: { state?: string }) {
  const qs = new URLSearchParams();
  if (params?.state) qs.set("state", params.state);
  const q = qs.toString();
  return useQuery({ queryKey: ["sessions", params], queryFn: () => api.get(`/api/sessions${q ? `?${q}` : ""}`) });
}

export function useSession(id: string) {
  return useQuery({ queryKey: ["session", id], queryFn: () => api.get(`/api/sessions/${id}`), enabled: !!id });
}

// --- Taxonomy ---
export function useReloadTaxonomy() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post("/api/taxonomy/reload"),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["roles"] });
      qc.invalidateQueries({ queryKey: ["tools"] });
      qc.invalidateQueries({ queryKey: ["verticals"] });
    },
  });
}
