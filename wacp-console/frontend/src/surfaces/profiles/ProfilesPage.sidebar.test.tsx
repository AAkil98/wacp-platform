import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ProfilesPage } from "./ProfilesPage";
import {
  SAMPLE_PROFILES,
  SAMPLE_ROLES,
  queryWrapper,
  makeMutationMock,
  defaultQueryResult,
} from "./ProfilesPage.test-helpers";

// ---- File-scoped mocks (vi.mock is hoisted per-file) ----

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

describe("ProfilesPage — library sidebar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
  });

  it("renders the profile list", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    expect(screen.getByText("Alpha Agent")).toBeInTheDocument();
    expect(screen.getByText("Beta Bot")).toBeInTheDocument();
    expect(screen.getByText("Gamma Guard")).toBeInTheDocument();
  });

  it("shows loading state", () => {
    mockProfiles.mockReturnValue(defaultQueryResult([], { isLoading: true }));
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    expect(screen.getByText("Loading...")).toBeInTheDocument();
  });

  it("shows empty state when no profiles found", () => {
    mockProfiles.mockReturnValue(defaultQueryResult([]));
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    expect(screen.getByText("No profiles found.")).toBeInTheDocument();
  });

  it("passes search value to useProfiles hook", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    const searchInput = screen.getByPlaceholderText("Search profiles...");
    fireEvent.change(searchInput, { target: { value: "alpha" } });
    expect(mockProfiles).toHaveBeenCalledWith({ search: "alpha" });
  });

  it("displays role_ref and autonomy badge for each profile", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    expect(screen.getByText("supervised")).toBeInTheDocument();
    expect(screen.getByText("autonomous")).toBeInTheDocument();
    expect(screen.getByText("assisted")).toBeInTheDocument();
  });

  it("displays visibility badges", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    const privateBadges = screen.getAllByText("private");
    expect(privateBadges.length).toBe(2);
    expect(screen.getByText("shared")).toBeInTheDocument();
  });
});

describe("ProfilesPage — no selection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
  });

  it("shows placeholder text when no profile is selected", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    expect(screen.getByText("Select a profile from the library or create a new one.")).toBeInTheDocument();
  });
});
