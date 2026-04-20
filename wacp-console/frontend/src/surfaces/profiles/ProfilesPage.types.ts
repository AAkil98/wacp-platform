// Shared types for the ProfilesPage subcomponents (F4 decomposition).

export interface ProfileSummary {
  id: string;
  name: string;
  role_ref: string;
  autonomy: string;
  visibility: string;
}

export interface ProfileDetail {
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

export interface RoleSummary {
  id: string;
  name: string;
}

export interface VersionEntry {
  version: number;
  created_at: string;
  summary: string;
}

export const EMPTY_FORM = {
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

export type ProfileForm = typeof EMPTY_FORM;
