import { vi } from "vitest";
import type React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";

export const SAMPLE_PROFILES = [
  { id: "p1", name: "Alpha Agent", role_ref: "analyst", autonomy: "supervised", visibility: "private" },
  { id: "p2", name: "Beta Bot", role_ref: "operator", autonomy: "autonomous", visibility: "shared" },
  { id: "p3", name: "Gamma Guard", role_ref: "reviewer", autonomy: "assisted", visibility: "private" },
];

export const SAMPLE_PROFILE_DETAIL = {
  id: "p1",
  name: "Alpha Agent",
  description: "Primary analyst profile",
  role_ref: "analyst",
  llm_provider: "anthropic",
  llm_model: "claude-sonnet-4-20250514",
  temperature: 0.7,
  max_tokens: 4096,
  autonomy: "supervised",
  visibility: "private",
  budget_limit: 10,
  budget_window_secs: 3600,
};

export const SAMPLE_ROLES = [
  { id: "analyst", name: "Analyst" },
  { id: "operator", name: "Operator" },
  { id: "reviewer", name: "Reviewer" },
];

export const SAMPLE_VERSIONS = [
  { version: 1, created_at: "2026-01-01T00:00:00Z", summary: "Initial version" },
  { version: 2, created_at: "2026-02-15T12:30:00Z", summary: "Updated model" },
];

export function queryWrapper() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return (
      <QueryClientProvider client={qc}>
        <MemoryRouter>
          {children}
        </MemoryRouter>
      </QueryClientProvider>
    );
  };
}

export function makeMutationMock(result?: unknown, opts?: { isPending?: boolean }) {
  const mutateFn = vi.fn((_data: unknown, callbacks?: { onSuccess?: (r: unknown) => void }) => {
    callbacks?.onSuccess?.(result);
  });
  return {
    mutate: mutateFn,
    isPending: opts?.isPending ?? false,
    isError: false,
    error: null,
  };
}

export function defaultQueryResult(data: unknown, overrides?: Record<string, unknown>) {
  return {
    data,
    isLoading: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
    ...overrides,
  };
}
