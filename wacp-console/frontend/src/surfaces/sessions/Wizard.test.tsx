import { cleanup, render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import type React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { Wizard } from "./Wizard";

// ---- Fixtures (module-scoped, stable refs) ----

interface VerticalSummaryShape {
  id: string;
  name: string;
  defining_constraint: string;
  tool_count: number;
  workflow_count: number;
  role_count: number;
}

const SAMPLE_VERTICALS: VerticalSummaryShape[] = [
  {
    id: "v1",
    name: "SWE Vertical",
    defining_constraint: "Ships passing tests",
    tool_count: 5,
    workflow_count: 2,
    role_count: 2,
  },
  {
    id: "v-no-ctx",
    name: "Minimal Vertical",
    defining_constraint: "Bare-bones",
    tool_count: 1,
    workflow_count: 1,
    role_count: 1,
  },
];

const V1_DETAIL = {
  id: "v1",
  name: "SWE Vertical",
  defining_constraint: "Ships passing tests",
  roles: [
    { id: "analyst", name: "Analyst" },
    { id: "operator", name: "Operator" },
  ],
  workflows: [
    {
      id: "wf1",
      name: "Golden Path",
      description: "Analyze then execute",
      stage_count: 3,
      gated_stage_count: 1,
    },
    {
      id: "wf2",
      name: "Fast Path",
      description: "Single-stage autonomy",
      stage_count: 1,
      gated_stage_count: 0,
    },
  ],
  context_schema: {
    goal: {
      field_type: "string",
      required: true,
      description: "What the session should achieve",
    },
    max_turns: {
      field_type: "number",
      required: false,
      description: "Upper bound on coordination turns",
      default: 10,
    },
    dry_run: {
      field_type: "boolean",
      required: false,
      description: "Plan only",
    },
    priority: {
      field_type: "string",
      required: false,
      description: "Queue priority",
      enum_values: ["low", "normal", "high"],
    },
  },
};

const V_NO_CTX_DETAIL = {
  id: "v-no-ctx",
  name: "Minimal Vertical",
  defining_constraint: "Bare-bones",
  roles: [{ id: "analyst", name: "Analyst" }],
  workflows: [
    {
      id: "wf-min",
      name: "Minimal Flow",
      description: "Single stage",
      stage_count: 1,
      gated_stage_count: 0,
    },
  ],
  context_schema: {},
};

const V_NO_WORKFLOWS_DETAIL = {
  id: "v-empty-wf",
  name: "Empty Vertical",
  defining_constraint: "none",
  roles: [{ id: "analyst", name: "Analyst" }],
  workflows: [],
  context_schema: {},
};

const PROFILES_ANALYST = [
  { id: "prof-analyst-1", name: "Alpha Analyst", role_ref: "analyst" },
];
const PROFILES_OPERATOR = [
  { id: "prof-operator-1", name: "Beta Operator", role_ref: "operator" },
];
const PROFILES_ALL = [...PROFILES_ANALYST, ...PROFILES_OPERATOR];
const PROFILES_NONE: typeof PROFILES_ALL = [];

function stableQueryResult<T>(data: T, overrides?: Record<string, unknown>) {
  return {
    data,
    isLoading: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
    ...overrides,
  };
}

// Pre-allocate every mock return that the hooks will yield.  Every render
// must see the **same** object reference for a given query key, otherwise
// the top-level useEffect(…, [verticalDetail]) hooks re-fire on every
// render and drive an infinite loop.  See AUDIT-2026-04-15 §13.6.
const EMPTY_VERTICAL_QUERY = stableQueryResult(undefined);
const V1_VERTICAL_QUERY = stableQueryResult(V1_DETAIL);
const V_NO_CTX_VERTICAL_QUERY = stableQueryResult(V_NO_CTX_DETAIL);
const V_NO_WF_VERTICAL_QUERY = stableQueryResult(V_NO_WORKFLOWS_DETAIL);

const VERTICALS_QUERY_OK = stableQueryResult(SAMPLE_VERTICALS);
const VERTICALS_QUERY_EMPTY = stableQueryResult([]);
const VERTICALS_QUERY_LOADING = stableQueryResult(undefined, { isLoading: true });

const PROFILES_ANALYST_QUERY = stableQueryResult(PROFILES_ANALYST);
const PROFILES_OPERATOR_QUERY = stableQueryResult(PROFILES_OPERATOR);
const PROFILES_NONE_QUERY = stableQueryResult(PROFILES_NONE);
const PROFILES_ALL_QUERY = stableQueryResult(PROFILES_ALL);

// ---- File-scoped mocks ----

const mockVerticals = vi.fn();
const mockVertical = vi.fn();
const mockProfiles = vi.fn();

vi.mock("../../api/hooks/index", () => ({
  useVerticals: () => mockVerticals(),
  useVertical: (id: string) => mockVertical(id),
  useProfiles: (params?: { role_ref?: string }) => mockProfiles(params),
}));

const apiPostMock = vi.fn();
const apiPatchMock = vi.fn();
const apiPutMock = vi.fn();
const apiGetMock = vi.fn();
const apiDeleteMock = vi.fn();

vi.mock("../../api/client", () => ({
  api: {
    get: (...args: unknown[]) => apiGetMock(...args),
    post: (...args: unknown[]) => apiPostMock(...args),
    patch: (...args: unknown[]) => apiPatchMock(...args),
    put: (...args: unknown[]) => apiPutMock(...args),
    delete: (...args: unknown[]) => apiDeleteMock(...args),
  },
}));

function setupHappyMocks() {
  mockVerticals.mockReturnValue(VERTICALS_QUERY_OK);
  mockVertical.mockImplementation((id: string) => {
    if (id === "v1") return V1_VERTICAL_QUERY;
    if (id === "v-no-ctx") return V_NO_CTX_VERTICAL_QUERY;
    if (id === "v-empty-wf") return V_NO_WF_VERTICAL_QUERY;
    return EMPTY_VERTICAL_QUERY;
  });
  mockProfiles.mockImplementation((params?: { role_ref?: string }) => {
    if (!params || !params.role_ref) return PROFILES_ALL_QUERY;
    if (params.role_ref === "analyst") return PROFILES_ANALYST_QUERY;
    if (params.role_ref === "operator") return PROFILES_OPERATOR_QUERY;
    return PROFILES_NONE_QUERY;
  });
  apiPostMock.mockResolvedValue({ id: "s1" });
  apiPatchMock.mockResolvedValue({});
  apiPutMock.mockResolvedValue({});
  apiGetMock.mockResolvedValue({});
  apiDeleteMock.mockResolvedValue({});
}

// ---- Shared QueryClient + wrapper (AUDIT §13.6 pattern) ----

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false, gcTime: 0, staleTime: Infinity },
    mutations: { retry: false },
  },
});

function wrapper() {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <MemoryRouter>{children}</MemoryRouter>
      </QueryClientProvider>
    );
  };
}

afterEach(() => {
  cleanup();
  queryClient.clear();
});

// ---- Navigation helpers ----

function renderWizard(onClose = vi.fn()) {
  const result = render(<Wizard onClose={onClose} />, { wrapper: wrapper() });
  return { onClose, ...result };
}

async function advanceToWorkflowStep() {
  const utils = renderWizard();
  fireEvent.click(screen.getByText("SWE Vertical"));
  fireEvent.click(screen.getByRole("button", { name: /Next/i }));
  await waitFor(() => {
    expect(screen.getByText("Select a Workflow")).toBeInTheDocument();
  });
  return utils;
}

async function advanceToAssignStep() {
  const utils = await advanceToWorkflowStep();
  fireEvent.click(screen.getByText("Golden Path"));
  fireEvent.click(screen.getByRole("button", { name: /Next/i }));
  await waitFor(() => {
    expect(screen.getByText("Assign Profiles to Roles")).toBeInTheDocument();
  });
  return utils;
}

async function advanceToContextStep() {
  const utils = await advanceToAssignStep();
  await waitFor(() => {
    const nextBtn = screen.getByRole("button", { name: /Next/i });
    expect(nextBtn).not.toBeDisabled();
  });
  fireEvent.click(screen.getByRole("button", { name: /Next/i }));
  await waitFor(() => {
    expect(screen.getByText("Session Context")).toBeInTheDocument();
  });
  return utils;
}

async function advanceToBudgetStep() {
  const utils = await advanceToContextStep();
  fireEvent.click(screen.getByRole("button", { name: /Next/i }));
  await waitFor(() => {
    expect(screen.getByText(/Budget Overrides/i)).toBeInTheDocument();
  });
  return utils;
}

async function advanceToReviewStep() {
  const utils = await advanceToBudgetStep();
  fireEvent.click(screen.getByRole("button", { name: /Next/i }));
  await waitFor(() => {
    expect(screen.getByText("Review & Launch")).toBeInTheDocument();
  });
  return utils;
}

// ---- Step 1: Select Vertical ----

describe("Wizard step 1 — Select Vertical", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupHappyMocks();
  });

  it("shows loading state while verticals query is pending", () => {
    mockVerticals.mockReturnValue(VERTICALS_QUERY_LOADING);
    renderWizard();
    expect(screen.getByText(/Loading verticals/i)).toBeInTheDocument();
  });

  it("renders empty state when no verticals are available", () => {
    mockVerticals.mockReturnValue(VERTICALS_QUERY_EMPTY);
    renderWizard();
    expect(screen.getByText(/No verticals available/i)).toBeInTheDocument();
  });

  it("renders a card for each vertical", () => {
    renderWizard();
    expect(screen.getByText("SWE Vertical")).toBeInTheDocument();
    expect(screen.getByText("Minimal Vertical")).toBeInTheDocument();
  });

  it("shows vertical metadata (roles / workflows / tools counts)", () => {
    renderWizard();
    expect(
      screen.getByText(/2 roles \/ 2 workflows \/ 5 tools/),
    ).toBeInTheDocument();
  });

  it("Next is disabled until a vertical is selected", () => {
    renderWizard();
    const nextBtn = screen.getByRole("button", { name: /Next/i });
    expect(nextBtn).toBeDisabled();
  });

  it("clicking a vertical card enables Next", () => {
    renderWizard();
    fireEvent.click(screen.getByText("SWE Vertical"));
    const nextBtn = screen.getByRole("button", { name: /Next/i });
    expect(nextBtn).not.toBeDisabled();
  });

  it("Next click posts to /api/sessions and advances to step 2", async () => {
    renderWizard();
    fireEvent.click(screen.getByText("SWE Vertical"));
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(apiPostMock).toHaveBeenCalledWith("/api/sessions", {
        vertical: "v1",
        workflow: "",
      });
    });
    expect(screen.getByText("Select a Workflow")).toBeInTheDocument();
  });
});

// ---- Step 2: Select Workflow ----

describe("Wizard step 2 — Select Workflow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupHappyMocks();
  });

  it("renders each workflow of the selected vertical", async () => {
    await advanceToWorkflowStep();
    expect(screen.getByText("Golden Path")).toBeInTheDocument();
    expect(screen.getByText("Fast Path")).toBeInTheDocument();
  });

  it("shows workflow stage metadata", async () => {
    await advanceToWorkflowStep();
    expect(screen.getByText(/3 stages \(1 gated\)/)).toBeInTheDocument();
  });

  it("renders empty state when vertical has no workflows", async () => {
    // Build a scenario where useVertical returns a vertical with zero workflows.
    // Click through step 1 by picking this vertical via mock swap at step 1.
    mockVerticals.mockReturnValue(
      stableQueryResult([
        {
          id: "v-empty-wf",
          name: "Empty Vertical",
          defining_constraint: "none",
          tool_count: 0,
          workflow_count: 0,
          role_count: 1,
        },
      ]),
    );
    renderWizard();
    fireEvent.click(screen.getByText("Empty Vertical"));
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(
        screen.getByText(/No workflows available for this vertical/i),
      ).toBeInTheDocument();
    });
  });

  it("Next is disabled until a workflow is selected", async () => {
    await advanceToWorkflowStep();
    const nextBtn = screen.getByRole("button", { name: /Next/i });
    expect(nextBtn).toBeDisabled();
  });

  it("clicking a workflow card enables Next", async () => {
    await advanceToWorkflowStep();
    fireEvent.click(screen.getByText("Golden Path"));
    const nextBtn = screen.getByRole("button", { name: /Next/i });
    expect(nextBtn).not.toBeDisabled();
  });

  it("Next click patches session with the workflow and advances to step 3", async () => {
    await advanceToWorkflowStep();
    fireEvent.click(screen.getByText("Golden Path"));
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(apiPatchMock).toHaveBeenCalledWith("/api/sessions/s1", {
        workflow: "wf1",
      });
    });
    expect(screen.getByText("Assign Profiles to Roles")).toBeInTheDocument();
  });
});

// ---- Step 3: Assign Profiles ----

describe("Wizard step 3 — Assign Profiles", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupHappyMocks();
  });

  it("renders a role slot per role defined on the vertical", async () => {
    await advanceToAssignStep();
    expect(screen.getByText("Analyst")).toBeInTheDocument();
    expect(screen.getByText("Operator")).toBeInTheDocument();
  });

  it("auto-selects first matching profile for each role (enables Next)", async () => {
    await advanceToAssignStep();
    await waitFor(() => {
      const nextBtn = screen.getByRole("button", { name: /Next/i });
      expect(nextBtn).not.toBeDisabled();
    });
  });

  it("shows 'no matching profiles' message when a role has none", async () => {
    // Return empty list for the operator role specifically.
    mockProfiles.mockImplementation((params?: { role_ref?: string }) => {
      if (!params || !params.role_ref) return PROFILES_ALL_QUERY;
      if (params.role_ref === "analyst") return PROFILES_ANALYST_QUERY;
      return PROFILES_NONE_QUERY;
    });
    await advanceToAssignStep();
    expect(screen.getByText(/No matching profiles/i)).toBeInTheDocument();
  });

  it("Next is disabled when any role slot has no matching profile", async () => {
    mockProfiles.mockImplementation((params?: { role_ref?: string }) => {
      if (!params || !params.role_ref) return PROFILES_ALL_QUERY;
      if (params.role_ref === "analyst") return PROFILES_ANALYST_QUERY;
      return PROFILES_NONE_QUERY;
    });
    await advanceToAssignStep();
    await waitFor(() => {
      expect(screen.getByText(/No matching profiles/i)).toBeInTheDocument();
    });
    const nextBtn = screen.getByRole("button", { name: /Next/i });
    expect(nextBtn).toBeDisabled();
  });

  it("Next click PUTs assignments and advances to step 4", async () => {
    await advanceToAssignStep();
    await waitFor(() => {
      const nextBtn = screen.getByRole("button", { name: /Next/i });
      expect(nextBtn).not.toBeDisabled();
    });
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(apiPutMock).toHaveBeenCalledWith(
        "/api/sessions/s1/assignments",
        expect.arrayContaining([
          { role_ref: "analyst", profile_id: "prof-analyst-1" },
          { role_ref: "operator", profile_id: "prof-operator-1" },
        ]),
      );
    });
    expect(screen.getByText("Session Context")).toBeInTheDocument();
  });
});

// ---- Step 4: Context ----

describe("Wizard step 4 — Context", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupHappyMocks();
  });

  it("auto-skips to Budget Overrides when vertical has empty context_schema", async () => {
    mockVerticals.mockReturnValue(
      stableQueryResult([
        {
          id: "v-no-ctx",
          name: "Minimal Vertical",
          defining_constraint: "none",
          tool_count: 1,
          workflow_count: 1,
          role_count: 1,
        },
      ]),
    );
    renderWizard();
    fireEvent.click(screen.getByText("Minimal Vertical"));
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(screen.getByText("Minimal Flow")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Minimal Flow"));
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(screen.getByText("Assign Profiles to Roles")).toBeInTheDocument();
    });
    await waitFor(() => {
      const nextBtn = screen.getByRole("button", { name: /Next/i });
      expect(nextBtn).not.toBeDisabled();
    });
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(screen.getByText(/Budget Overrides/i)).toBeInTheDocument();
    });
    expect(screen.queryByText("Session Context")).not.toBeInTheDocument();
  });

  it("renders a string field for goal", async () => {
    await advanceToContextStep();
    expect(screen.getByLabelText(/^goal/i)).toBeInTheDocument();
  });

  it("renders a number field for max_turns with its default value", async () => {
    await advanceToContextStep();
    const input = screen.getByLabelText(/^max_turns/i) as HTMLInputElement;
    expect(input.type).toBe("number");
    expect(input.value).toBe("10");
  });

  it("renders a checkbox for boolean field", async () => {
    await advanceToContextStep();
    const checkbox = screen.getByLabelText(/^dry_run/i) as HTMLInputElement;
    expect(checkbox.type).toBe("checkbox");
  });

  it("renders a select for enum field", async () => {
    await advanceToContextStep();
    const select = screen.getByLabelText(/^priority/i) as HTMLSelectElement;
    expect(select.tagName).toBe("SELECT");
    const opts = Array.from(select.options).map((o) => o.value);
    expect(opts).toEqual(expect.arrayContaining(["low", "normal", "high"]));
  });

  it("typing into a string field updates state (flows to summary)", async () => {
    await advanceToContextStep();
    const input = screen.getByLabelText(/^goal/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "ship the migration" } });
    // Advance to Review; the value should appear in the summary.
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(screen.getByText(/Budget Overrides/i)).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(screen.getByText("Review & Launch")).toBeInTheDocument();
    });
    expect(screen.getByText(/goal: ship the migration/)).toBeInTheDocument();
  });
});

// ---- Step 5: Budget Overrides ----

describe("Wizard step 5 — Budget Overrides", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupHappyMocks();
  });

  it("renders three budget inputs (cost, tokens, wall time)", async () => {
    await advanceToBudgetStep();
    expect(screen.getByLabelText(/Max Cost \(micros\)/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Max Tokens/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Max Wall Time \(ms\)/i)).toBeInTheDocument();
  });

  it("typing max cost flows to summary on review step", async () => {
    await advanceToBudgetStep();
    const input = screen.getByLabelText(/Max Cost \(micros\)/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "5000000" } });
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(screen.getByText("Review & Launch")).toBeInTheDocument();
    });
    expect(screen.getByText(/Max Cost: 5000000 micros/)).toBeInTheDocument();
  });

  it("allows proceeding to review with no budget set", async () => {
    await advanceToBudgetStep();
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(screen.getByText("Review & Launch")).toBeInTheDocument();
    });
    // Summary's "Budget Overrides" label (exact) should not appear; the step
    // indicator "5. Budget Overrides" is a different text and unaffected.
    expect(
      screen.queryByText("Budget Overrides", { exact: true }),
    ).not.toBeInTheDocument();
  });

  it("omits the budget patch on launch when no budget fields are set", async () => {
    await advanceToBudgetStep();
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(screen.getByText("Review & Launch")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: /Launch Session/i }));
    await waitFor(() => {
      expect(apiPostMock).toHaveBeenCalledWith("/api/sessions/s1/launch");
    });
    const budgetPatchCall = apiPatchMock.mock.calls.find((c) => {
      const body = c[1] as Record<string, unknown> | undefined;
      if (!body) return false;
      return (
        "max_tokens" in body ||
        "max_cost_micros" in body ||
        "max_wall_time_ms" in body
      );
    });
    expect(budgetPatchCall).toBeUndefined();
  });

  it("sends typed budget values in the patch on launch", async () => {
    await advanceToBudgetStep();
    const costInput = screen.getByLabelText(/Max Cost \(micros\)/i) as HTMLInputElement;
    fireEvent.change(costInput, { target: { value: "5000000" } });
    await waitFor(() => expect(costInput.value).toBe("5000000"));
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(screen.getByText("Review & Launch")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: /Launch Session/i }));
    await waitFor(() => {
      expect(apiPostMock).toHaveBeenCalledWith("/api/sessions/s1/launch");
    });
    const budgetPatchCall = apiPatchMock.mock.calls.find((c) => {
      const body = c[1] as Record<string, unknown> | undefined;
      return body && "max_cost_micros" in body;
    });
    expect(budgetPatchCall).toBeDefined();
    expect((budgetPatchCall?.[1] as Record<string, unknown>).max_cost_micros).toBe(5000000);
  });
});

// ---- Step 6: Review & Launch ----

describe("Wizard step 6 — Review & Launch", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupHappyMocks();
  });

  it("summary shows the selected vertical name", async () => {
    await advanceToReviewStep();
    const row = screen.getByText("Vertical", { exact: true }).parentElement;
    expect(row).not.toBeNull();
    expect(row?.textContent).toMatch(/SWE Vertical/);
  });

  it("summary shows the selected workflow name", async () => {
    await advanceToReviewStep();
    const row = screen.getByText("Workflow", { exact: true }).parentElement;
    expect(row?.textContent).toMatch(/Golden Path/);
  });

  it("summary shows each role → profile assignment", async () => {
    await advanceToReviewStep();
    expect(screen.getByText(/Analyst →/)).toBeInTheDocument();
    expect(screen.getByText(/Operator →/)).toBeInTheDocument();
    expect(screen.getByText(/Alpha Analyst/)).toBeInTheDocument();
    expect(screen.getByText(/Beta Operator/)).toBeInTheDocument();
  });

  it("Launch button calls /launch and onClose on success", async () => {
    const onClose = vi.fn();
    render(<Wizard onClose={onClose} />, { wrapper: wrapper() });
    fireEvent.click(screen.getByText("SWE Vertical"));
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => screen.getByText("Select a Workflow"));
    fireEvent.click(screen.getByText("Golden Path"));
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => screen.getByText("Assign Profiles to Roles"));
    await waitFor(() => {
      const nextBtn = screen.getByRole("button", { name: /Next/i });
      expect(nextBtn).not.toBeDisabled();
    });
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => screen.getByText("Session Context"));
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => screen.getByText(/Budget Overrides/i));
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => screen.getByText("Review & Launch"));

    fireEvent.click(screen.getByRole("button", { name: /Launch Session/i }));
    await waitFor(() => {
      expect(apiPostMock).toHaveBeenCalledWith("/api/sessions/s1/launch");
    });
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("validation failure surfaces the violation list", async () => {
    apiPostMock.mockImplementation(async (path: string) => {
      if (path === "/api/sessions") return { id: "s1" };
      if (path === "/api/sessions/s1/launch") {
        throw {
          message: "Validation failed",
          detail: { violations: ["context.goal is required", "budget.max_tokens missing"] },
        };
      }
      return {};
    });
    await advanceToReviewStep();
    fireEvent.click(screen.getByRole("button", { name: /Launch Session/i }));
    await waitFor(() => {
      expect(screen.getByText(/context\.goal is required/)).toBeInTheDocument();
    });
    expect(screen.getByText(/budget\.max_tokens missing/)).toBeInTheDocument();
  });

  it("runtime-unreachable surfaces the generic error message", async () => {
    apiPostMock.mockImplementation(async (path: string) => {
      if (path === "/api/sessions") return { id: "s1" };
      if (path === "/api/sessions/s1/launch") {
        throw { message: "runtime unreachable" };
      }
      return {};
    });
    await advanceToReviewStep();
    fireEvent.click(screen.getByRole("button", { name: /Launch Session/i }));
    await waitFor(() => {
      expect(screen.getByText(/runtime unreachable/)).toBeInTheDocument();
    });
  });

  it("shows 'Launching...' while the launch is in-flight", async () => {
    let resolveLaunch!: () => void;
    const launchPromise = new Promise<Record<string, unknown>>((r) => {
      resolveLaunch = () => r({});
    });
    apiPostMock.mockImplementation(async (path: string) => {
      if (path === "/api/sessions") return { id: "s1" };
      if (path === "/api/sessions/s1/launch") return launchPromise;
      return {};
    });
    await advanceToReviewStep();
    fireEvent.click(screen.getByRole("button", { name: /Launch Session/i }));
    await waitFor(() => {
      expect(screen.getByText(/Launching\.\.\./)).toBeInTheDocument();
    });
    resolveLaunch();
  });
});

// ---- Cross-cutting ----

describe("Wizard cross-cutting", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupHappyMocks();
  });

  it("Back from step 2 preserves vertical selection on step 1", async () => {
    await advanceToWorkflowStep();
    fireEvent.click(screen.getByRole("button", { name: /Back/i }));
    await waitFor(() => {
      expect(screen.getByText("Select a Vertical")).toBeInTheDocument();
    });
    // Next should still be enabled — selection is preserved.
    const nextBtn = screen.getByRole("button", { name: /Next/i });
    expect(nextBtn).not.toBeDisabled();
  });

  it("Discard calls cancel API when a session exists and then onClose", async () => {
    const { onClose } = await advanceToWorkflowStep();
    fireEvent.click(screen.getByRole("button", { name: /Discard/i }));
    await waitFor(() => {
      expect(apiPostMock).toHaveBeenCalledWith("/api/sessions/s1/cancel");
    });
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("Discard at step 1 calls onClose without hitting cancel API", async () => {
    const onClose = vi.fn();
    render(<Wizard onClose={onClose} />, { wrapper: wrapper() });
    fireEvent.click(screen.getByRole("button", { name: /Discard/i }));
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    // No session was ever created, so the cancel endpoint must not be called.
    const cancelCalled = apiPostMock.mock.calls.some(
      (c) => typeof c[0] === "string" && (c[0] as string).endsWith("/cancel"),
    );
    expect(cancelCalled).toBe(false);
  });

  it("step indicator shows labels for all six steps", () => {
    renderWizard();
    expect(screen.getByText(/1\. Select Vertical/)).toBeInTheDocument();
    expect(screen.getByText(/2\. Select Workflow/)).toBeInTheDocument();
    expect(screen.getByText(/3\. Assign Profiles/)).toBeInTheDocument();
    expect(screen.getByText(/4\. Context/)).toBeInTheDocument();
    expect(screen.getByText(/5\. Budget Overrides/)).toBeInTheDocument();
    expect(screen.getByText(/6\. Review & Launch/)).toBeInTheDocument();
  });

  it("step error from the API is surfaced under the step body", async () => {
    apiPostMock.mockImplementation(async (path: string) => {
      if (path === "/api/sessions") {
        throw { message: "database is down" };
      }
      return {};
    });
    renderWizard();
    fireEvent.click(screen.getByText("SWE Vertical"));
    fireEvent.click(screen.getByRole("button", { name: /Next/i }));
    await waitFor(() => {
      expect(screen.getByText(/database is down/)).toBeInTheDocument();
    });
    // Should still be on step 1.
    expect(screen.getByText("Select a Vertical")).toBeInTheDocument();
  });
});
