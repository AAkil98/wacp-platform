import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { useStore } from "@/store";
import { Sidebar } from "./Sidebar.js";

function renderSidebar() {
  return render(
    <MemoryRouter>
      <Sidebar />
    </MemoryRouter>,
  );
}

beforeEach(() => {
  useStore.setState({
    gates: { pending: new Map(), resolved: new Map(), inFlight: new Set() },
    escalations: { active: new Map(), resolved: new Map(), inFlight: new Set() },
  });
});

describe("Sidebar", () => {
  it("renders all 7 navigation items", () => {
    renderSidebar();
    expect(screen.getByText("Trail")).toBeInTheDocument();
    expect(screen.getByText("Gates")).toBeInTheDocument();
    expect(screen.getByText("Escalations")).toBeInTheDocument();
    expect(screen.getByText("Workspaces")).toBeInTheDocument();
    expect(screen.getByText("Task Graph")).toBeInTheDocument();
    expect(screen.getByText("Inject")).toBeInTheDocument();
    expect(screen.getByText("Settings")).toBeInTheDocument();
  });

  it("renders WACP Highway header", () => {
    renderSidebar();
    expect(screen.getByText("WACP Highway")).toBeInTheDocument();
  });

  it("shows pending gates badge when count > 0", () => {
    const pending = new Map([["g-1", {} as any], ["g-2", {} as any]]);
    useStore.setState({
      gates: { pending, resolved: new Map(), inFlight: new Set() },
    });
    renderSidebar();
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("shows active escalations badge when count > 0", () => {
    const active = new Map([["e-1", {} as any]]);
    useStore.setState({
      escalations: { active, resolved: new Map(), inFlight: new Set() },
    });
    renderSidebar();
    expect(screen.getByText("1")).toBeInTheDocument();
  });

  it("hides badges when counts are zero", () => {
    renderSidebar();
    // No badge elements should have numeric content
    const allText = screen.queryAllByText(/^\d+$/);
    expect(allText).toHaveLength(0);
  });
});
