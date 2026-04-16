import { render, screen, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { DiscoveryPage } from "./DiscoveryPage.tsx";

// Mock all four tab components so we can test DiscoveryPage in isolation
vi.mock("./RolesTab.tsx", () => ({
  RolesTab: () => <div data-testid="roles-tab">RolesTab content</div>,
}));
vi.mock("./ToolsTab.tsx", () => ({
  ToolsTab: () => <div data-testid="tools-tab">ToolsTab content</div>,
}));
vi.mock("./TypesTab.tsx", () => ({
  TypesTab: () => <div data-testid="types-tab">TypesTab content</div>,
}));
vi.mock("./VerticalsTab.tsx", () => ({
  VerticalsTab: () => <div data-testid="verticals-tab">VerticalsTab content</div>,
}));

// Mock useSearch hook
const mockUseSearch = vi.fn();
vi.mock("../../api/hooks", () => ({
  useSearch: (...args: unknown[]) => mockUseSearch(...args),
}));

function renderDiscovery() {
  return render(
    <MemoryRouter>
      <DiscoveryPage />
    </MemoryRouter>,
  );
}

describe("DiscoveryPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseSearch.mockReturnValue({ data: undefined, isLoading: false });
  });

  // --- rendering ---

  it("renders the page title and description", () => {
    renderDiscovery();
    expect(screen.getByText("Discovery Browser")).toBeInTheDocument();
    expect(screen.getByText("Browse roles, tools, verticals, and types.")).toBeInTheDocument();
  });

  it("renders the search input", () => {
    renderDiscovery();
    expect(
      screen.getByPlaceholderText("Search across all entities (min 2 characters)..."),
    ).toBeInTheDocument();
  });

  it("renders all four tab buttons", () => {
    renderDiscovery();
    expect(screen.getByRole("button", { name: "Roles" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tools" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Types" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Verticals" })).toBeInTheDocument();
  });

  // --- default tab ---

  it("shows Roles tab by default", () => {
    renderDiscovery();
    expect(screen.getByTestId("roles-tab")).toBeInTheDocument();
    expect(screen.queryByTestId("tools-tab")).toBeNull();
    expect(screen.queryByTestId("types-tab")).toBeNull();
    expect(screen.queryByTestId("verticals-tab")).toBeNull();
  });

  // --- tab switching ---

  it("switches to Tools tab when clicked", () => {
    renderDiscovery();
    fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    expect(screen.getByTestId("tools-tab")).toBeInTheDocument();
    expect(screen.queryByTestId("roles-tab")).toBeNull();
  });

  it("switches to Types tab when clicked", () => {
    renderDiscovery();
    fireEvent.click(screen.getByRole("button", { name: "Types" }));
    expect(screen.getByTestId("types-tab")).toBeInTheDocument();
    expect(screen.queryByTestId("roles-tab")).toBeNull();
  });

  it("switches to Verticals tab when clicked", () => {
    renderDiscovery();
    fireEvent.click(screen.getByRole("button", { name: "Verticals" }));
    expect(screen.getByTestId("verticals-tab")).toBeInTheDocument();
    expect(screen.queryByTestId("roles-tab")).toBeNull();
  });

  it("switches back to Roles tab from another tab", () => {
    renderDiscovery();
    fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    expect(screen.getByTestId("tools-tab")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Roles" }));
    expect(screen.getByTestId("roles-tab")).toBeInTheDocument();
    expect(screen.queryByTestId("tools-tab")).toBeNull();
  });

  // --- search: does not show results for query under 2 chars ---

  it("does not show search results section for 1-character query", () => {
    renderDiscovery();
    const input = screen.getByPlaceholderText("Search across all entities (min 2 characters)...");
    fireEvent.change(input, { target: { value: "a" } });
    expect(screen.queryByText(/Search results for/)).toBeNull();
  });

  // --- search: shows results section for query >= 2 chars ---

  it("shows search results section for query with 2+ characters", () => {
    mockUseSearch.mockReturnValue({ data: { results: {} }, isLoading: false });
    renderDiscovery();
    const input = screen.getByPlaceholderText("Search across all entities (min 2 characters)...");
    fireEvent.change(input, { target: { value: "co" } });
    expect(screen.getByText('Search results for "co"')).toBeInTheDocument();
  });

  // --- search: loading state ---

  it("shows 'Searching...' while search is loading", () => {
    mockUseSearch.mockReturnValue({ data: undefined, isLoading: true });
    renderDiscovery();
    const input = screen.getByPlaceholderText("Search across all entities (min 2 characters)...");
    fireEvent.change(input, { target: { value: "coord" } });
    expect(screen.getByText("Searching...")).toBeInTheDocument();
  });

  // --- search: empty results ---

  it("shows 'No results found.' when search returns empty results", () => {
    mockUseSearch.mockReturnValue({ data: { results: {} }, isLoading: false });
    renderDiscovery();
    const input = screen.getByPlaceholderText("Search across all entities (min 2 characters)...");
    fireEvent.change(input, { target: { value: "zzzzz" } });
    expect(screen.getByText("No results found.")).toBeInTheDocument();
  });

  // --- search: shows grouped results ---

  it("renders search results grouped by type", () => {
    mockUseSearch.mockReturnValue({
      data: {
        results: {
          roles: [
            { id: "r1", name: "coordinator", description: "Base coordinator", match_field: "name" },
          ],
          tools: [
            { id: "t1", name: "create_task", description: "Create a task", match_field: "description", vertical: "legal" },
          ],
        },
      },
      isLoading: false,
    });
    renderDiscovery();
    const input = screen.getByPlaceholderText("Search across all entities (min 2 characters)...");
    fireEvent.change(input, { target: { value: "coor" } });

    // Check group headings — uses SEARCH_TYPE_LABELS mapping
    expect(screen.getByText("Role")).toBeInTheDocument();
    expect(screen.getByText("Tool")).toBeInTheDocument();

    // Check individual hits
    expect(screen.getByText("coordinator")).toBeInTheDocument();
    expect(screen.getByText("matched on name")).toBeInTheDocument();

    expect(screen.getByText("create_task")).toBeInTheDocument();
    expect(screen.getByText("matched on description")).toBeInTheDocument();
    // Vertical badge
    expect(screen.getByText("legal")).toBeInTheDocument();
  });

  // --- search: skips empty result groups ---

  it("does not render group heading for empty result arrays", () => {
    mockUseSearch.mockReturnValue({
      data: {
        results: {
          roles: [
            { id: "r1", name: "coordinator", description: "Base", match_field: "name" },
          ],
          tools: [],
        },
      },
      isLoading: false,
    });
    renderDiscovery();
    const input = screen.getByPlaceholderText("Search across all entities (min 2 characters)...");
    fireEvent.change(input, { target: { value: "co" } });

    expect(screen.getByText("Role")).toBeInTheDocument();
    expect(screen.queryByText("Tool")).toBeNull();
  });

  // --- search passes query to useSearch ---

  it("calls useSearch with the query string", () => {
    renderDiscovery();
    const input = screen.getByPlaceholderText("Search across all entities (min 2 characters)...");
    fireEvent.change(input, { target: { value: "hello" } });
    expect(mockUseSearch).toHaveBeenCalledWith("hello");
  });
});
