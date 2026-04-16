import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ProfilesPage } from "./ProfilesPage";
import {
  SAMPLE_PROFILES,
  SAMPLE_PROFILE_DETAIL,
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

describe("ProfilesPage — edit existing profile", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
  });

  it("shows editor with profile data when a profile is selected", async () => {
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Edit: Alpha Agent")).toBeInTheDocument();
    });
  });

  it("populates form fields from loaded profile", async () => {
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      const nameInput = screen.getByLabelText("Name") as HTMLInputElement;
      expect(nameInput.value).toBe("Alpha Agent");
    });
  });

  it("calls updateProfile mutation on save", async () => {
    const updateMut = makeMutationMock();
    mockUpdateProfile.mockReturnValue(updateMut);
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Save Changes")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Save Changes"));
    expect(updateMut.mutate).toHaveBeenCalled();
  });

  it("shows action buttons for existing profile", async () => {
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByText("Clone")).toBeInTheDocument();
      expect(screen.getByText("Export YAML")).toBeInTheDocument();
      expect(screen.getByText("Delete")).toBeInTheDocument();
      expect(screen.getByText("Version History")).toBeInTheDocument();
    });
  });

  it("updates form field when user types into Name", async () => {
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByLabelText("Name")).toBeInTheDocument();
    });

    const nameInput = screen.getByLabelText("Name") as HTMLInputElement;
    fireEvent.change(nameInput, { target: { value: "Updated Name" } });
    expect(nameInput.value).toBe("Updated Name");
  });

  it("updates temperature via number input", async () => {
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));

    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByLabelText("Temperature")).toBeInTheDocument();
    });

    const tempInput = screen.getByLabelText("Temperature") as HTMLInputElement;
    fireEvent.change(tempInput, { target: { value: "0.9" } });
    expect(tempInput.value).toBe("0.9");
  });
});

describe("ProfilesPage — form fields", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
    mockProfile.mockReturnValue(defaultQueryResult(SAMPLE_PROFILE_DETAIL));
  });

  it("renders all form labels", async () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByLabelText("Name")).toBeInTheDocument();
    });

    expect(screen.getByLabelText("Description")).toBeInTheDocument();
    expect(screen.getByLabelText("Role")).toBeInTheDocument();
    expect(screen.getByLabelText("LLM Provider")).toBeInTheDocument();
    expect(screen.getByLabelText("LLM Model")).toBeInTheDocument();
    expect(screen.getByLabelText("Temperature")).toBeInTheDocument();
    expect(screen.getByLabelText("Max Tokens")).toBeInTheDocument();
    expect(screen.getByLabelText("Autonomy")).toBeInTheDocument();
    expect(screen.getByLabelText("Visibility")).toBeInTheDocument();
    expect(screen.getByLabelText("Budget Limit")).toBeInTheDocument();
    expect(screen.getByLabelText("Budget Window (seconds)")).toBeInTheDocument();
  });

  it("updates description textarea", async () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByLabelText("Description")).toBeInTheDocument();
    });

    const descInput = screen.getByLabelText("Description") as HTMLTextAreaElement;
    fireEvent.change(descInput, { target: { value: "New description" } });
    expect(descInput.value).toBe("New description");
  });

  it("updates LLM provider input", async () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByLabelText("LLM Provider")).toBeInTheDocument();
    });

    const providerInput = screen.getByLabelText("LLM Provider") as HTMLInputElement;
    fireEvent.change(providerInput, { target: { value: "openai" } });
    expect(providerInput.value).toBe("openai");
  });

  it("updates max tokens input", async () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByLabelText("Max Tokens")).toBeInTheDocument();
    });

    const tokensInput = screen.getByLabelText("Max Tokens") as HTMLInputElement;
    fireEvent.change(tokensInput, { target: { value: "8192" } });
    expect(tokensInput.value).toBe("8192");
  });

  it("toggles autonomy radio", async () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByLabelText("Autonomy")).toBeInTheDocument();
    });

    const autoRadio = screen.getByDisplayValue("autonomous") as HTMLInputElement;
    fireEvent.click(autoRadio);
    expect(autoRadio.checked).toBe(true);
  });

  it("toggles visibility radio", async () => {
    render(<ProfilesPage />, { wrapper: queryWrapper() });
    fireEvent.click(screen.getByText("Alpha Agent"));

    await waitFor(() => {
      expect(screen.getByLabelText("Visibility")).toBeInTheDocument();
    });

    const sharedRadio = screen.getByDisplayValue("shared") as HTMLInputElement;
    fireEvent.click(sharedRadio);
    expect(sharedRadio.checked).toBe(true);
  });
});
