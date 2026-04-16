import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ProfilesPage } from "./ProfilesPage";
import {
  SAMPLE_PROFILES,
  SAMPLE_PROFILE_DETAIL,
  SAMPLE_ROLES,
  SAMPLE_VERSIONS,
  queryWrapper,
  makeMutationMock,
  defaultQueryResult,
} from "./ProfilesPage.test-helpers";

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

describe("ProfilesPage — import YAML flow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
  });

  it("shows import dialog when Import YAML is clicked", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Import YAML"));
    expect(screen.getByText("Import Profile from YAML")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Paste YAML here...")).toBeInTheDocument();
  });

  it("calls import mutation with YAML content", () => {
    const importMut = makeMutationMock();
    mockImportProfile.mockReturnValue(importMut);

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Import YAML"));

    const textarea = screen.getByPlaceholderText("Paste YAML here...");
    fireEvent.change(textarea, {
      target: { value: "name: Test Profile\nrole_ref: analyst" },
    });

    fireEvent.click(screen.getByText("Import"));
    expect(importMut.mutate).toHaveBeenCalledWith(
      "name: Test Profile\nrole_ref: analyst",
      expect.anything(),
    );
  });

  it("does not call import mutation when textarea is empty", () => {
    const importMut = makeMutationMock();
    mockImportProfile.mockReturnValue(importMut);

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Import YAML"));
    fireEvent.click(screen.getByText("Import"));

    expect(importMut.mutate).not.toHaveBeenCalled();
  });

  it("hides import dialog when Cancel is clicked", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Import YAML"));
    expect(screen.getByText("Import Profile from YAML")).toBeInTheDocument();

    const dialogContainer = screen.getByText("Import Profile from YAML").closest("div")!;
    fireEvent.click(within(dialogContainer as HTMLElement).getByText("Cancel"));

    expect(screen.queryByText("Import Profile from YAML")).not.toBeInTheDocument();
  });

  it("clears textarea and hides dialog on successful import", () => {
    const importMut = makeMutationMock();
    mockImportProfile.mockReturnValue(importMut);

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Import YAML"));

    const textarea = screen.getByPlaceholderText("Paste YAML here...");
    fireEvent.change(textarea, { target: { value: "name: Test" } });
    fireEvent.click(screen.getByText("Import"));

    expect(screen.queryByText("Import Profile from YAML")).not.toBeInTheDocument();
  });

  it("shows Importing... while import is pending", () => {
    const importMut = makeMutationMock(undefined, { isPending: true });
    mockImportProfile.mockReturnValue(importMut);

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Import YAML"));

    expect(screen.getByText("Importing...")).toBeInTheDocument();
  });
});

describe("ProfilesPage — version history", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));
    mockProfileVersions.mockReturnValue(defaultQueryResult(SAMPLE_VERSIONS));
  });

  it("shows version history panel when button is clicked", async () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Version History")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Version History"));

    expect(screen.getByText(/Version History/)).toBeInTheDocument();
    expect(screen.getByText("v1")).toBeInTheDocument();
    expect(screen.getByText("v2")).toBeInTheDocument();
    expect(screen.getByText("Initial version")).toBeInTheDocument();
    expect(screen.getByText("Updated model")).toBeInTheDocument();
  });

  it("toggles to Hide Versions after opening", async () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Version History")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Version History"));
    expect(screen.getByText("Hide Versions")).toBeInTheDocument();
  });

  it("hides version panel when Hide Versions is clicked", async () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Version History")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Version History"));
    expect(screen.getByText("v1")).toBeInTheDocument();

    fireEvent.click(screen.getByText("Hide Versions"));
    expect(screen.queryByText("v1")).not.toBeInTheDocument();
  });

  it("shows empty state for versions when none available", async () => {
    mockProfileVersions.mockReturnValue(defaultQueryResult([]));

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Version History")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Version History"));
    expect(screen.getByText("No version history available.")).toBeInTheDocument();
  });

  it("shows loading state for versions", async () => {
    mockProfileVersions.mockReturnValue(defaultQueryResult([], { isLoading: true }));

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Version History")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Version History"));
    expect(screen.getByText("Loading versions...")).toBeInTheDocument();
  });
});
