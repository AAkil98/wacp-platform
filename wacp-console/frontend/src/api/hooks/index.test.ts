/**
 * Comprehensive tests for all API hooks in the WACP Console frontend.
 *
 * Covers every useQuery and useMutation wrapper: success, 401, 404, 500,
 * and network errors. Mutations also verify cache invalidation.
 *
 * Approach: mock global fetch (no MSW dependency), wrap hooks in
 * QueryClientProvider via renderHook, assert on data/error/loading states.
 */
import { renderHook, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createElement, type ReactNode } from "react";

import {
  useHealth,
  useRoles,
  useRole,
  useTools,
  useTool,
  useVerticals,
  useVertical,
  useEnvelopeTypes,
  useCheckpointTypes,
  useSearch,
  useVerticalWorkflows,
  useVerticalTaskTypes,
  useVerticalContextSchema,
  useProfiles,
  useProfile,
  useProfileVersions,
  useCreateProfile,
  useUpdateProfile,
  useDeleteProfile,
  useCloneProfile,
  useImportProfile,
  useUsers,
  useCreateUser,
  useSettings,
  useSetSetting,
  useAuditLog,
  useSessions,
  useSession,
  useReloadTaxonomy,
} from "./index";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Build a fresh QueryClient with retries disabled for deterministic tests. */
function makeClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
}

/** Wrapper component that provides a QueryClient. */
function makeWrapper(qc: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: qc }, children);
  };
}

/** Return a Response-like object that global fetch resolves with. */
function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function textResponse(body: string, status: number) {
  return new Response(body, { status, headers: { "content-type": "text/plain" } });
}

// ---------------------------------------------------------------------------
// Setup / teardown
// ---------------------------------------------------------------------------

let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  fetchMock = vi.fn();
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  vi.restoreAllMocks();
});

// ---------------------------------------------------------------------------
// Generic error-case factory — avoids repeating 5 identical blocks per hook
// ---------------------------------------------------------------------------

type QueryHookFactory = () => ReturnType<
  typeof renderHook<ReturnType<typeof useHealth>, unknown>
>;

/**
 * Shared assertions for every useQuery hook error scenario.
 * `setup` must call renderHook and return the result; fetch is already mocked.
 */
function describeQueryErrors(
  name: string,
  setup: () => QueryHookFactory,
) {
  describe(`${name} — error responses`, () => {
    let hookFactory: QueryHookFactory;

    beforeEach(() => {
      hookFactory = setup();
    });

    it("returns error on 401 response", async () => {
      fetchMock.mockResolvedValueOnce(jsonResponse({ error: "UNAUTHENTICATED" }, 401));
      const { result } = hookFactory();
      await waitFor(() => expect(result.current.isError).toBe(true));
      expect(result.current.error).toBeDefined();
      expect((result.current.error as any).status).toBe(401);
    });

    it("returns error on 404 response", async () => {
      fetchMock.mockResolvedValueOnce(jsonResponse({ error: "NOT_FOUND" }, 404));
      const { result } = hookFactory();
      await waitFor(() => expect(result.current.isError).toBe(true));
      expect((result.current.error as any).status).toBe(404);
    });

    it("returns error on 500 response", async () => {
      fetchMock.mockResolvedValueOnce(textResponse("Internal Server Error", 500));
      const { result } = hookFactory();
      await waitFor(() => expect(result.current.isError).toBe(true));
      expect((result.current.error as any).status).toBe(500);
    });

    it("returns error on network failure", async () => {
      fetchMock.mockRejectedValueOnce(new TypeError("Failed to fetch"));
      const { result } = hookFactory();
      await waitFor(() => expect(result.current.isError).toBe(true));
      expect(result.current.error).toBeInstanceOf(TypeError);
    });
  });
}

// ===========================================================================
//  QUERY HOOKS
// ===========================================================================

// --- useHealth ---

describe("useHealth", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ status: "ok" }));
    const qc = makeClient();
    const { result } = renderHook(() => useHealth(), { wrapper: makeWrapper(qc) });

    expect(result.current.isLoading).toBe(true);
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ status: "ok" });
    expect(result.current.error).toBeNull();
  });
});

describeQueryErrors("useHealth", () => {
  const qc = makeClient();
  return () => renderHook(() => useHealth(), { wrapper: makeWrapper(qc) });
});

// --- useRoles ---

describe("useRoles", () => {
  it("returns data on success (no params)", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ id: "r1" }]));
    const qc = makeClient();
    const { result } = renderHook(() => useRoles(), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ id: "r1" }]);
  });

  it("passes query params to fetch", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([]));
    const qc = makeClient();
    renderHook(() => useRoles({ base_role: "operator", vertical: "kyc" }), {
      wrapper: makeWrapper(qc),
    });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    const url: string = fetchMock.mock.calls[0][0];
    expect(url).toContain("base_role=operator");
    expect(url).toContain("vertical=kyc");
  });
});

describeQueryErrors("useRoles", () => {
  const qc = makeClient();
  return () => renderHook(() => useRoles(), { wrapper: makeWrapper(qc) });
});

// --- useRole ---

describe("useRole", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "r1", name: "admin" }));
    const qc = makeClient();
    const { result } = renderHook(() => useRole("r1"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ id: "r1", name: "admin" });
  });

  it("is disabled when id is empty string", async () => {
    const qc = makeClient();
    const { result } = renderHook(() => useRole(""), { wrapper: makeWrapper(qc) });
    // Should stay idle — never fires fetch
    expect(result.current.fetchStatus).toBe("idle");
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describeQueryErrors("useRole", () => {
  const qc = makeClient();
  return () => renderHook(() => useRole("r1"), { wrapper: makeWrapper(qc) });
});

// --- useTools ---

describe("useTools", () => {
  it("returns data on success (no params)", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ name: "t1" }]));
    const qc = makeClient();
    const { result } = renderHook(() => useTools(), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ name: "t1" }]);
  });

  it("passes query params to fetch", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([]));
    const qc = makeClient();
    renderHook(() => useTools({ vertical: "kyc", has_policy: true }), {
      wrapper: makeWrapper(qc),
    });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    const url: string = fetchMock.mock.calls[0][0];
    expect(url).toContain("vertical=kyc");
    expect(url).toContain("has_policy=true");
  });
});

describeQueryErrors("useTools", () => {
  const qc = makeClient();
  return () => renderHook(() => useTools(), { wrapper: makeWrapper(qc) });
});

// --- useTool ---

describe("useTool", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ name: "file_read" }));
    const qc = makeClient();
    const { result } = renderHook(() => useTool("file_read"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ name: "file_read" });
  });

  it("is disabled when name is empty string", async () => {
    const qc = makeClient();
    const { result } = renderHook(() => useTool(""), { wrapper: makeWrapper(qc) });
    expect(result.current.fetchStatus).toBe("idle");
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describeQueryErrors("useTool", () => {
  const qc = makeClient();
  return () => renderHook(() => useTool("file_read"), { wrapper: makeWrapper(qc) });
});

// --- useVerticals ---

describe("useVerticals", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ id: "kyc" }]));
    const qc = makeClient();
    const { result } = renderHook(() => useVerticals(), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ id: "kyc" }]);
  });
});

describeQueryErrors("useVerticals", () => {
  const qc = makeClient();
  return () => renderHook(() => useVerticals(), { wrapper: makeWrapper(qc) });
});

// --- useVertical ---

describe("useVertical", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "kyc", name: "KYC" }));
    const qc = makeClient();
    const { result } = renderHook(() => useVertical("kyc"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ id: "kyc", name: "KYC" });
  });

  it("is disabled when id is empty string", async () => {
    const qc = makeClient();
    const { result } = renderHook(() => useVertical(""), { wrapper: makeWrapper(qc) });
    expect(result.current.fetchStatus).toBe("idle");
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describeQueryErrors("useVertical", () => {
  const qc = makeClient();
  return () => renderHook(() => useVertical("kyc"), { wrapper: makeWrapper(qc) });
});

// --- useEnvelopeTypes ---

describe("useEnvelopeTypes", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ name: "env1" }]));
    const qc = makeClient();
    const { result } = renderHook(() => useEnvelopeTypes(), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ name: "env1" }]);
  });
});

describeQueryErrors("useEnvelopeTypes", () => {
  const qc = makeClient();
  return () => renderHook(() => useEnvelopeTypes(), { wrapper: makeWrapper(qc) });
});

// --- useCheckpointTypes ---

describe("useCheckpointTypes", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ name: "cp1" }]));
    const qc = makeClient();
    const { result } = renderHook(() => useCheckpointTypes(), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ name: "cp1" }]);
  });
});

describeQueryErrors("useCheckpointTypes", () => {
  const qc = makeClient();
  return () => renderHook(() => useCheckpointTypes(), { wrapper: makeWrapper(qc) });
});

// --- useSearch ---

describe("useSearch", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ results: [{ id: "1" }] }));
    const qc = makeClient();
    const { result } = renderHook(() => useSearch("test query"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ results: [{ id: "1" }] });
  });

  it("is disabled when query < 2 characters", async () => {
    const qc = makeClient();
    const { result } = renderHook(() => useSearch("a"), { wrapper: makeWrapper(qc) });
    expect(result.current.fetchStatus).toBe("idle");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("is enabled when query is exactly 2 characters", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ results: [] }));
    const qc = makeClient();
    const { result } = renderHook(() => useSearch("ab"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(fetchMock).toHaveBeenCalled();
  });

  it("passes type and vertical params", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ results: [] }));
    const qc = makeClient();
    renderHook(() => useSearch("test", "tool", "kyc"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    const url: string = fetchMock.mock.calls[0][0];
    expect(url).toContain("type=tool");
    expect(url).toContain("vertical=kyc");
  });
});

describeQueryErrors("useSearch", () => {
  const qc = makeClient();
  return () => renderHook(() => useSearch("test query"), { wrapper: makeWrapper(qc) });
});

// --- useVerticalWorkflows ---

describe("useVerticalWorkflows", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ id: "wf1" }]));
    const qc = makeClient();
    const { result } = renderHook(() => useVerticalWorkflows("kyc"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ id: "wf1" }]);
  });

  it("is disabled when id is empty string", async () => {
    const qc = makeClient();
    const { result } = renderHook(() => useVerticalWorkflows(""), { wrapper: makeWrapper(qc) });
    expect(result.current.fetchStatus).toBe("idle");
  });
});

describeQueryErrors("useVerticalWorkflows", () => {
  const qc = makeClient();
  return () => renderHook(() => useVerticalWorkflows("kyc"), { wrapper: makeWrapper(qc) });
});

// --- useVerticalTaskTypes ---

describe("useVerticalTaskTypes", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ id: "tt1" }]));
    const qc = makeClient();
    const { result } = renderHook(() => useVerticalTaskTypes("kyc"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ id: "tt1" }]);
  });

  it("is disabled when id is empty string", async () => {
    const qc = makeClient();
    const { result } = renderHook(() => useVerticalTaskTypes(""), { wrapper: makeWrapper(qc) });
    expect(result.current.fetchStatus).toBe("idle");
  });
});

describeQueryErrors("useVerticalTaskTypes", () => {
  const qc = makeClient();
  return () => renderHook(() => useVerticalTaskTypes("kyc"), { wrapper: makeWrapper(qc) });
});

// --- useVerticalContextSchema ---

describe("useVerticalContextSchema", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ type: "object" }));
    const qc = makeClient();
    const { result } = renderHook(() => useVerticalContextSchema("kyc"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ type: "object" });
  });

  it("is disabled when id is empty string", async () => {
    const qc = makeClient();
    const { result } = renderHook(() => useVerticalContextSchema(""), { wrapper: makeWrapper(qc) });
    expect(result.current.fetchStatus).toBe("idle");
  });
});

describeQueryErrors("useVerticalContextSchema", () => {
  const qc = makeClient();
  return () => renderHook(() => useVerticalContextSchema("kyc"), { wrapper: makeWrapper(qc) });
});

// --- useProfiles ---

describe("useProfiles", () => {
  it("returns data on success (no params)", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ id: "p1" }]));
    const qc = makeClient();
    const { result } = renderHook(() => useProfiles(), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ id: "p1" }]);
  });

  it("passes query params to fetch", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([]));
    const qc = makeClient();
    renderHook(() => useProfiles({ role_ref: "admin", search: "foo" }), {
      wrapper: makeWrapper(qc),
    });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    const url: string = fetchMock.mock.calls[0][0];
    expect(url).toContain("role_ref=admin");
    expect(url).toContain("search=foo");
  });
});

describeQueryErrors("useProfiles", () => {
  const qc = makeClient();
  return () => renderHook(() => useProfiles(), { wrapper: makeWrapper(qc) });
});

// --- useProfile ---

describe("useProfile", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "p1", name: "default" }));
    const qc = makeClient();
    const { result } = renderHook(() => useProfile("p1"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ id: "p1", name: "default" });
  });

  it("is disabled when id is empty string", async () => {
    const qc = makeClient();
    const { result } = renderHook(() => useProfile(""), { wrapper: makeWrapper(qc) });
    expect(result.current.fetchStatus).toBe("idle");
  });
});

describeQueryErrors("useProfile", () => {
  const qc = makeClient();
  return () => renderHook(() => useProfile("p1"), { wrapper: makeWrapper(qc) });
});

// --- useProfileVersions ---

describe("useProfileVersions", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ version: 1 }, { version: 2 }]));
    const qc = makeClient();
    const { result } = renderHook(() => useProfileVersions("p1"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ version: 1 }, { version: 2 }]);
  });

  it("is disabled when id is empty string", async () => {
    const qc = makeClient();
    const { result } = renderHook(() => useProfileVersions(""), { wrapper: makeWrapper(qc) });
    expect(result.current.fetchStatus).toBe("idle");
  });
});

describeQueryErrors("useProfileVersions", () => {
  const qc = makeClient();
  return () => renderHook(() => useProfileVersions("p1"), { wrapper: makeWrapper(qc) });
});

// --- useUsers ---

describe("useUsers", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ id: "u1", username: "admin" }]));
    const qc = makeClient();
    const { result } = renderHook(() => useUsers(), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ id: "u1", username: "admin" }]);
  });
});

describeQueryErrors("useUsers", () => {
  const qc = makeClient();
  return () => renderHook(() => useUsers(), { wrapper: makeWrapper(qc) });
});

// --- useSettings ---

describe("useSettings", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ max_sessions: 10 }));
    const qc = makeClient();
    const { result } = renderHook(() => useSettings(), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ max_sessions: 10 });
  });
});

describeQueryErrors("useSettings", () => {
  const qc = makeClient();
  return () => renderHook(() => useSettings(), { wrapper: makeWrapper(qc) });
});

// --- useAuditLog ---

describe("useAuditLog", () => {
  it("returns data on success (no params)", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ action: "login" }]));
    const qc = makeClient();
    const { result } = renderHook(() => useAuditLog(), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ action: "login" }]);
  });

  it("passes query params to fetch", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([]));
    const qc = makeClient();
    renderHook(() => useAuditLog({ user_id: "u1", action: "login", limit: 50 }), {
      wrapper: makeWrapper(qc),
    });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    const url: string = fetchMock.mock.calls[0][0];
    expect(url).toContain("user_id=u1");
    expect(url).toContain("action=login");
    expect(url).toContain("limit=50");
  });
});

describeQueryErrors("useAuditLog", () => {
  const qc = makeClient();
  return () => renderHook(() => useAuditLog(), { wrapper: makeWrapper(qc) });
});

// --- useSessions ---

describe("useSessions", () => {
  it("returns data on success (no params)", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([{ id: "s1" }]));
    const qc = makeClient();
    const { result } = renderHook(() => useSessions(), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([{ id: "s1" }]);
  });

  it("passes state param to fetch", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse([]));
    const qc = makeClient();
    renderHook(() => useSessions({ state: "active" }), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    const url: string = fetchMock.mock.calls[0][0];
    expect(url).toContain("state=active");
  });
});

describeQueryErrors("useSessions", () => {
  const qc = makeClient();
  return () => renderHook(() => useSessions(), { wrapper: makeWrapper(qc) });
});

// --- useSession ---

describe("useSession", () => {
  it("returns data on success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "s1", state: "active" }));
    const qc = makeClient();
    const { result } = renderHook(() => useSession("s1"), { wrapper: makeWrapper(qc) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ id: "s1", state: "active" });
  });

  it("is disabled when id is empty string", async () => {
    const qc = makeClient();
    const { result } = renderHook(() => useSession(""), { wrapper: makeWrapper(qc) });
    expect(result.current.fetchStatus).toBe("idle");
  });
});

describeQueryErrors("useSession", () => {
  const qc = makeClient();
  return () => renderHook(() => useSession("s1"), { wrapper: makeWrapper(qc) });
});

// ===========================================================================
//  QUERY — loading state transition
// ===========================================================================

describe("query loading state transitions", () => {
  it("transitions idle -> loading -> success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ status: "ok" }));
    const qc = makeClient();
    const { result } = renderHook(() => useHealth(), { wrapper: makeWrapper(qc) });

    // initially loading
    expect(result.current.isLoading).toBe(true);
    expect(result.current.isSuccess).toBe(false);
    expect(result.current.isError).toBe(false);

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.isLoading).toBe(false);
    expect(result.current.isError).toBe(false);
    expect(result.current.data).toEqual({ status: "ok" });
  });

  it("transitions idle -> loading -> error", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ error: "BOOM" }, 500));
    const qc = makeClient();
    const { result } = renderHook(() => useHealth(), { wrapper: makeWrapper(qc) });

    expect(result.current.isLoading).toBe(true);

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(result.current.isLoading).toBe(false);
    expect(result.current.isSuccess).toBe(false);
  });
});

// ===========================================================================
//  MUTATION HOOKS
// ===========================================================================

// --- Shared mutation error helper ---

function describeMutationErrors(
  name: string,
  setup: () => {
    qc: QueryClient;
    hook: ReturnType<typeof renderHook<ReturnType<typeof useCreateProfile>, unknown>>;
    trigger: () => void;
  },
) {
  describe(`${name} — error responses`, () => {
    it("returns error on 401 response", async () => {
      fetchMock.mockResolvedValueOnce(jsonResponse({ error: "UNAUTHENTICATED" }, 401));
      const { hook, trigger } = setup();
      await act(async () => { trigger(); });
      await waitFor(() => expect(hook.result.current.isError).toBe(true));
      expect((hook.result.current.error as any).status).toBe(401);
    });

    it("returns error on 404 response", async () => {
      fetchMock.mockResolvedValueOnce(jsonResponse({ error: "NOT_FOUND" }, 404));
      const { hook, trigger } = setup();
      await act(async () => { trigger(); });
      await waitFor(() => expect(hook.result.current.isError).toBe(true));
      expect((hook.result.current.error as any).status).toBe(404);
    });

    it("returns error on 500 response", async () => {
      fetchMock.mockResolvedValueOnce(textResponse("Internal Server Error", 500));
      const { hook, trigger } = setup();
      await act(async () => { trigger(); });
      await waitFor(() => expect(hook.result.current.isError).toBe(true));
      expect((hook.result.current.error as any).status).toBe(500);
    });

    it("returns error on network failure", async () => {
      fetchMock.mockRejectedValueOnce(new TypeError("Failed to fetch"));
      const { hook, trigger } = setup();
      await act(async () => { trigger(); });
      await waitFor(() => expect(hook.result.current.isError).toBe(true));
      expect(hook.result.current.error).toBeInstanceOf(TypeError);
    });
  });
}

// --- useCreateProfile ---

describe("useCreateProfile", () => {
  it("succeeds and invalidates profiles cache", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "p-new" }, 201));
    const qc = makeClient();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");
    const { result } = renderHook(() => useCreateProfile(), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate({ name: "new profile" }); });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(result.current.data).toEqual({ id: "p-new" });
    expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["profiles"] }));
  });

  it("sends JSON body via POST", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "p2" }, 201));
    const qc = makeClient();
    const { result } = renderHook(() => useCreateProfile(), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate({ name: "prof" }); });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const [url, opts] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/profiles");
    expect(opts.method).toBe("POST");
    expect(JSON.parse(opts.body)).toEqual({ name: "prof" });
  });
});

describeMutationErrors("useCreateProfile", () => {
  const qc = makeClient();
  const hook = renderHook(() => useCreateProfile(), { wrapper: makeWrapper(qc) });
  return { qc, hook, trigger: () => hook.result.current.mutate({ name: "x" }) };
});

// --- useUpdateProfile ---

describe("useUpdateProfile", () => {
  it("succeeds and invalidates profiles + profile cache", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "p1" }));
    const qc = makeClient();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");
    const { result } = renderHook(() => useUpdateProfile("p1"), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate({ name: "updated" }); });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["profiles"] }));
    expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["profile", "p1"] }));
  });

  it("sends PUT request to correct URL", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "p1" }));
    const qc = makeClient();
    const { result } = renderHook(() => useUpdateProfile("p1"), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate({ name: "updated" }); });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const [url, opts] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/profiles/p1");
    expect(opts.method).toBe("PUT");
  });
});

describeMutationErrors("useUpdateProfile", () => {
  const qc = makeClient();
  const hook = renderHook(() => useUpdateProfile("p1"), { wrapper: makeWrapper(qc) });
  return { qc, hook, trigger: () => hook.result.current.mutate({ name: "x" }) };
});

// --- useDeleteProfile ---

describe("useDeleteProfile", () => {
  it("succeeds and invalidates profiles cache", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ ok: true }));
    const qc = makeClient();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");
    const { result } = renderHook(() => useDeleteProfile("p1"), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate(); });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["profiles"] }));
  });

  it("sends DELETE request to correct URL", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ ok: true }));
    const qc = makeClient();
    const { result } = renderHook(() => useDeleteProfile("p1"), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate(); });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const [url, opts] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/profiles/p1");
    expect(opts.method).toBe("DELETE");
  });
});

describeMutationErrors("useDeleteProfile", () => {
  const qc = makeClient();
  const hook = renderHook(() => useDeleteProfile("p1"), { wrapper: makeWrapper(qc) });
  return { qc, hook, trigger: () => hook.result.current.mutate() };
});

// --- useCloneProfile ---

describe("useCloneProfile", () => {
  it("succeeds and invalidates profiles cache", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "p1-clone" }, 201));
    const qc = makeClient();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");
    const { result } = renderHook(() => useCloneProfile("p1"), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate(); });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(result.current.data).toEqual({ id: "p1-clone" });
    expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["profiles"] }));
  });

  it("sends POST request to correct URL", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "p1-clone" }, 201));
    const qc = makeClient();
    const { result } = renderHook(() => useCloneProfile("p1"), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate(); });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const [url, opts] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/profiles/p1/clone");
    expect(opts.method).toBe("POST");
  });
});

describeMutationErrors("useCloneProfile", () => {
  const qc = makeClient();
  const hook = renderHook(() => useCloneProfile("p1"), { wrapper: makeWrapper(qc) });
  return { qc, hook, trigger: () => hook.result.current.mutate() };
});

// --- useImportProfile ---

describe("useImportProfile", () => {
  it("succeeds and invalidates profiles cache", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "p-imported" }, 201));
    const qc = makeClient();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");
    const { result } = renderHook(() => useImportProfile(), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate("name: test\nroles: []"); });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["profiles"] }));
  });

  it("sends YAML body wrapped in JSON", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "pi" }, 201));
    const qc = makeClient();
    const { result } = renderHook(() => useImportProfile(), { wrapper: makeWrapper(qc) });
    const yamlStr = "name: test\nroles: []";

    await act(async () => { result.current.mutate(yamlStr); });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const [url, opts] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/profiles/import");
    expect(opts.method).toBe("POST");
    expect(JSON.parse(opts.body)).toEqual({ yaml: yamlStr });
  });
});

describeMutationErrors("useImportProfile", () => {
  const qc = makeClient();
  const hook = renderHook(() => useImportProfile(), { wrapper: makeWrapper(qc) });
  return { qc, hook, trigger: () => hook.result.current.mutate("yaml: test") };
});

// --- useCreateUser ---

describe("useCreateUser", () => {
  it("succeeds and invalidates users cache", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "u-new" }, 201));
    const qc = makeClient();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");
    const { result } = renderHook(() => useCreateUser(), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate({ username: "newuser" }); });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(result.current.data).toEqual({ id: "u-new" });
    expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["users"] }));
  });

  it("sends JSON body via POST to /api/users", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "u2" }, 201));
    const qc = makeClient();
    const { result } = renderHook(() => useCreateUser(), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate({ username: "alice" }); });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const [url, opts] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/users");
    expect(opts.method).toBe("POST");
    expect(JSON.parse(opts.body)).toEqual({ username: "alice" });
  });
});

describeMutationErrors("useCreateUser", () => {
  const qc = makeClient();
  const hook = renderHook(() => useCreateUser(), { wrapper: makeWrapper(qc) });
  return { qc, hook, trigger: () => hook.result.current.mutate({ username: "x" }) };
});

// --- useSetSetting ---

describe("useSetSetting", () => {
  it("succeeds and invalidates settings cache", async () => {
    // 204 No Content
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }));
    const qc = makeClient();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");
    const { result } = renderHook(() => useSetSetting(), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate({ key: "max_sessions", value: 42 }); });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["settings"] }));
  });

  it("sends PUT request to /api/settings/:key with value body", async () => {
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }));
    const qc = makeClient();
    const { result } = renderHook(() => useSetSetting(), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate({ key: "theme", value: "dark" }); });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const [url, opts] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/settings/theme");
    expect(opts.method).toBe("PUT");
    expect(JSON.parse(opts.body)).toBe("dark");
  });
});

describeMutationErrors("useSetSetting", () => {
  const qc = makeClient();
  const hook = renderHook(() => useSetSetting(), { wrapper: makeWrapper(qc) });
  return { qc, hook, trigger: () => hook.result.current.mutate({ key: "k", value: "v" }) };
});

// --- useReloadTaxonomy ---

describe("useReloadTaxonomy", () => {
  it("succeeds and invalidates roles, tools, verticals caches", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ reloaded: true }));
    const qc = makeClient();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");
    const { result } = renderHook(() => useReloadTaxonomy(), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate(); });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["roles"] }));
    expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["tools"] }));
    expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["verticals"] }));
  });

  it("sends POST request to /api/taxonomy/reload", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ reloaded: true }));
    const qc = makeClient();
    const { result } = renderHook(() => useReloadTaxonomy(), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate(); });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const [url, opts] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/taxonomy/reload");
    expect(opts.method).toBe("POST");
  });
});

describeMutationErrors("useReloadTaxonomy", () => {
  const qc = makeClient();
  const hook = renderHook(() => useReloadTaxonomy(), { wrapper: makeWrapper(qc) });
  return { qc, hook, trigger: () => hook.result.current.mutate() };
});

// ===========================================================================
//  MUTATION — loading state transitions
// ===========================================================================

describe("mutation loading state transitions", () => {
  it("transitions idle -> pending -> success", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "p1" }, 201));
    const qc = makeClient();
    const { result } = renderHook(() => useCreateProfile(), { wrapper: makeWrapper(qc) });

    // initially idle
    expect(result.current.isPending).toBe(false);
    expect(result.current.isSuccess).toBe(false);
    expect(result.current.isError).toBe(false);

    await act(async () => { result.current.mutate({ name: "test" }); });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(result.current.isPending).toBe(false);
    expect(result.current.isError).toBe(false);
  });

  it("transitions idle -> pending -> error", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ error: "FORBIDDEN" }, 403));
    const qc = makeClient();
    const { result } = renderHook(() => useCreateProfile(), { wrapper: makeWrapper(qc) });

    await act(async () => { result.current.mutate({ name: "test" }); });
    await waitFor(() => expect(result.current.isError).toBe(true));

    expect(result.current.isPending).toBe(false);
    expect(result.current.isSuccess).toBe(false);
    expect((result.current.error as any).status).toBe(403);
  });
});
