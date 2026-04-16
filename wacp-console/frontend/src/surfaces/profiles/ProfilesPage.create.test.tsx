import { render, screen, fireEvent, within } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ProfilesPage } from "./ProfilesPage";
import {
  SAMPLE_PROFILES,
  SAMPLE_ROLES,
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

describe("ProfilesPage — create new profile", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
  });

  it("shows editor form in create mode when Create New is clicked", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Create New"));
    expect(screen.getByText("New Profile")).toBeInTheDocument();
    expect(screen.getByText("Create Profile")).toBeInTheDocument();
  });

  it("resets form fields for new profile", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Create New"));
    const nameInput = screen.getByLabelText("Name") as HTMLInputElement;
    expect(nameInput.value).toBe("");
  });

  it("calls createProfile mutation on save", () => {
    const createMut = makeMutationMock({ id: "p-new" });
    mockCreateProfile.mockReturnValue(createMut);
    render(<ProfilesPage />, { wrapper: queryWrapper() });

    fireEvent.click(screen.getByText("Create New"));

    const nameInput = screen.getByLabelText("Name");
    fireEvent.change(nameInput, { target: { value: "New Test Profile" } });

    fireEvent.click(screen.getByText("Create Profile"));
    expect(createMut.mutate).toHaveBeenCalled();
  });

  it("does not show Delete/Clone/Export buttons in create mode", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Create New"));
    expect(screen.queryByText("Clone")).not.toBeInTheDocument();
    expect(screen.queryByText("Export YAML")).not.toBeInTheDocument();
    expect(screen.queryByText("Delete")).not.toBeInTheDocument();
  });

  it("renders role dropdown with options from useRoles", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Create New"));
    const roleSelect = screen.getByLabelText("Role") as HTMLSelectElement;
    expect(within(roleSelect).getByText("-- Select role --")).toBeInTheDocument();
    expect(within(roleSelect).getByText("Analyst")).toBeInTheDocument();
    expect(within(roleSelect).getByText("Operator")).toBeInTheDocument();
    expect(within(roleSelect).getByText("Reviewer")).toBeInTheDocument();
  });

  it("renders autonomy radio buttons (autonomous, assisted, supervised)", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Create New"));
    const radios = screen.getAllByRole("radio");
    const autonomyRadios = radios.filter(
      (r) => (r as HTMLInputElement).name === "autonomy",
    );
    expect(autonomyRadios).toHaveLength(3);
  });

  it("renders visibility radio buttons (private, shared)", () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Create New"));
    const radios = screen.getAllByRole("radio");
    const visibilityRadios = radios.filter(
      (r) => (r as HTMLInputElement).name === "visibility",
    );
    expect(visibilityRadios).toHaveLength(2);
  });

  it("shows Saving... when create mutation is pending", () => {
    const createMut = makeMutationMock({ id: "p-new" }, { isPending: true });
    mockCreateProfile.mockReturnValue(createMut);
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Create New"));
    expect(screen.getByText("Saving...")).toBeInTheDocument();
  });
});
