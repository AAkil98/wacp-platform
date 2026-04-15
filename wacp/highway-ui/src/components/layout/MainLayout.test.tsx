import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { useStore } from "@/store";
import { MainLayout } from "./MainLayout.js";

beforeEach(() => {
  useStore.setState({
    session: { state: "connected", userId: "user-1", capabilities: [] },
    gates: { pending: new Map(), resolved: new Map(), inFlight: new Set() },
    escalations: { active: new Map(), resolved: new Map(), inFlight: new Set() },
    notifications: { escalationBanner: null },
  });
});

describe("MainLayout", () => {
  it("renders sidebar with WACP Highway header", () => {
    render(
      <MemoryRouter initialEntries={["/trail"]}>
        <Routes>
          <Route element={<MainLayout />}>
            <Route path="/trail" element={<div>Trail Content</div>} />
          </Route>
        </Routes>
      </MemoryRouter>,
    );
    expect(screen.getByText("WACP Highway")).toBeInTheDocument();
  });

  it("renders outlet content", () => {
    render(
      <MemoryRouter initialEntries={["/trail"]}>
        <Routes>
          <Route element={<MainLayout />}>
            <Route path="/trail" element={<div>Trail Content</div>} />
          </Route>
        </Routes>
      </MemoryRouter>,
    );
    expect(screen.getByText("Trail Content")).toBeInTheDocument();
  });
});
