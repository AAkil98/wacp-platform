import { cleanup, render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, describe, it, expect, vi, beforeEach } from "vitest";
import type React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { ProfilesPage } from "./ProfilesPage";
import {
  SAMPLE_PROFILES,
  SAMPLE_PROFILE_DETAIL,
  SAMPLE_ROLES,
  makeMutationMock,
  defaultQueryResult,
} from "./ProfilesPage.test-helpers";

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

// ---- File-scoped mocks ----

const mockProfiles = vi.fn();
const mockProfile = vi.fn();
const mockProfileVersions = vi.fn();
const mockRoles = vi.fn();
const mockCreateProfile = vi.fn();
const mockUpdateProfile = vi.fn();
const mockDeleteProfile = vi.fn();
const mockCloneProfile = vi.fn();
const mockImportProfile = vi.fn();

vi.mock("../../api/hooks/index", () => ({
  useProfiles: (params?: unknown) => mockProfiles(params),
  useProfile: (id: string) => mockProfile(id),
  useProfileVersions: (id: string) => mockProfileVersions(id),
  useRoles: () => mockRoles(),
  useCreateProfile: () => mockCreateProfile(),
  useUpdateProfile: (id: string) => mockUpdateProfile(id),
  useDeleteProfile: (id: string) => mockDeleteProfile(id),
  useCloneProfile: (id: string) => mockCloneProfile(id),
  useImportProfile: () => mockImportProfile(),
}));

vi.mock("../../api/client", () => ({
  api: { get: vi.fn(), post: vi.fn(), put: vi.fn(), patch: vi.fn(), delete: vi.fn() },
}));

function setupDefaultMocks() {
  mockProfiles.mockReturnValue(defaultQueryResult(SAMPLE_PROFILES));
  mockProfile.mockReturnValue(defaultQueryResult(undefined));
  mockProfileVersions.mockReturnValue(defaultQueryResult([]));
  mockRoles.mockReturnValue(defaultQueryResult(SAMPLE_ROLES));
  mockCreateProfile.mockReturnValue(makeMutationMock({ id: "p-new" }));
  mockUpdateProfile.mockReturnValue(makeMutationMock());
  mockDeleteProfile.mockReturnValue(makeMutationMock());
  mockCloneProfile.mockReturnValue(makeMutationMock({ id: "p-clone" }));
  mockImportProfile.mockReturnValue(makeMutationMock());
}

// ---- Tests ----

describe("ProfilesPage — clone flow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
  });

  it("calls cloneProfile mutation when Clone is clicked", async () => {
    const cloneMut = makeMutationMock({ id: "p-clone" });
    mockCloneProfile.mockReturnValue(cloneMut);
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: wrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Clone")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Clone"));
    expect(cloneMut.mutate).toHaveBeenCalled();
  });

  it("selects the cloned profile after clone succeeds", async () => {
    const cloneMut = makeMutationMock({ id: "p-clone" });
    mockCloneProfile.mockReturnValue(cloneMut);
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: wrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Clone")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Clone"));

    await waitFor(() => {
      expect(mockProfile).toHaveBeenCalledWith("p-clone");
    });
  });

  it("shows Cloning... while clone is pending", async () => {
    const cloneMut = makeMutationMock({ id: "p-clone" }, { isPending: true });
    mockCloneProfile.mockReturnValue(cloneMut);
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: wrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Cloning...")).toBeInTheDocument();
    });
  });
});

describe("ProfilesPage — delete flow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));
  });

  it("shows confirmation dialog when Delete is clicked", async () => {
    render(<ProfilesPage />, { wrapper: wrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Delete")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Delete"));

    expect(screen.getByText(/Delete profile "Alpha Agent"\?/)).toBeInTheDocument();
    expect(screen.getByText("Confirm Delete")).toBeInTheDocument();
    expect(screen.getByText("Cancel")).toBeInTheDocument();
  });

  it("shows warning about sessions being affected", async () => {
    render(<ProfilesPage />, { wrapper: wrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Delete")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Delete"));

    expect(
      screen.getByText(/This action cannot be undone\. Any sessions using this profile may be affected\./),
    ).toBeInTheDocument();
  });

  it("calls deleteProfile mutation when Confirm Delete is clicked", async () => {
    const deleteMut = makeMutationMock();
    mockDeleteProfile.mockReturnValue(deleteMut);

    render(<ProfilesPage />, { wrapper: wrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Delete")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Delete"));
    fireEvent.click(screen.getByText("Confirm Delete"));

    expect(deleteMut.mutate).toHaveBeenCalled();
  });

  it("hides confirmation dialog when Cancel is clicked", async () => {
    render(<ProfilesPage />, { wrapper: wrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Delete")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Delete"));
    expect(screen.getByText("Confirm Delete")).toBeInTheDocument();

    fireEvent.click(screen.getByText("Cancel"));
    expect(screen.queryByText("Confirm Delete")).not.toBeInTheDocument();
  });

  it("shows Deleting... while delete mutation is pending", async () => {
    const deleteMut = makeMutationMock(undefined, { isPending: true });
    mockDeleteProfile.mockReturnValue(deleteMut);

    render(<ProfilesPage />, { wrapper: wrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Delete")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Delete"));
    expect(screen.getByText("Deleting...")).toBeInTheDocument();
  });
});

describe("ProfilesPage — export YAML", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
  });

  it("renders Export YAML button for existing profile", async () => {
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: wrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Export YAML")).toBeInTheDocument();
    });
  });
});

describe("ProfilesPage — selection switching", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
  });

  it("switches between profiles when different items are clicked", async () => {
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: wrapper() });

    fireEvent.click(screen.getByText("Alpha Agent"));
    await waitFor(() => {
      expect(screen.getByText("Edit: Alpha Agent")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Beta Bot"));
    await waitFor(() => {
      expect(mockProfile).toHaveBeenCalledWith("p2");
    });
  });

  it("clears delete dialog when switching profiles", async () => {
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: wrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Delete")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Delete"));
    expect(screen.getByText("Confirm Delete")).toBeInTheDocument();

    fireEvent.click(screen.getByText("Beta Bot"));

    await waitFor(() => {
      expect(screen.queryByText("Confirm Delete")).not.toBeInTheDocument();
    });
  });
});
