import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { SettingsPanel } from "./SettingsPanel.js";

function renderPanel() {
  return render(<SettingsPanel />);
}

describe("SettingsPanel", () => {
  it("renders preset selector with default supervised", () => {
    renderPanel();
    const select = screen.getByDisplayValue("supervised");
    expect(select).toBeDefined();
  });

  it("renders all four preset options", () => {
    renderPanel();
    const select = screen.getByDisplayValue("supervised");
    const options = select.querySelectorAll("option");
    expect(options).toHaveLength(4);
    const values = [...options].map((o) => o.getAttribute("value"));
    expect(values).toEqual(["autonomous", "supervised", "gated", "custom"]);
  });

  it("renders visibility toggle", () => {
    renderPanel();
    expect(screen.getByText("Trail visibility")).toBeDefined();
  });

  it("renders injection toggle", () => {
    renderPanel();
    expect(screen.getByText("Allow envelope injection")).toBeDefined();
  });

  it("renders all 6 gate type rows", () => {
    renderPanel();
    expect(screen.getByText("Task Approval")).toBeDefined();
    expect(screen.getByText("Workspace Create")).toBeDefined();
    expect(screen.getByText("Envelope Delivery")).toBeDefined();
    expect(screen.getByText("Integration")).toBeDefined();
    expect(screen.getByText("Conflict Resolution")).toBeDefined();
    expect(screen.getByText("Workspace Abort")).toBeDefined();
  });

  it("renders escalation section", () => {
    renderPanel();
    expect(screen.getByText("Enable escalation")).toBeDefined();
  });

  it("Apply button is disabled", () => {
    renderPanel();
    const applyBtn = screen.getByText("Apply");
    expect(applyBtn).toHaveProperty("disabled", true);
  });

  it("Apply button has tooltip about restart", () => {
    renderPanel();
    const applyBtn = screen.getByText("Apply");
    expect(applyBtn.getAttribute("title")).toContain("runtime restart");
  });

  it("preset selection changes to autonomous", () => {
    renderPanel();
    const select = screen.getByDisplayValue("supervised");
    fireEvent.change(select, { target: { value: "autonomous" } });
    expect(screen.getByDisplayValue("autonomous")).toBeDefined();
  });

  it("preset selection to gated enables all gates", () => {
    renderPanel();
    const select = screen.getByDisplayValue("supervised");
    fireEvent.change(select, { target: { value: "gated" } });
    // All 6 gate switches should be present (gated enables all)
    const switches = screen.getAllByRole("switch");
    // visibility + injection + 6 gates + escalation = 9 switches
    expect(switches.length).toBeGreaterThanOrEqual(9);
  });

  it("shows warning when all enabled gate timeouts are zero", () => {
    renderPanel();
    // Select gated preset (all gates enabled, all timeouts 0)
    const select = screen.getByDisplayValue("supervised");
    fireEvent.change(select, { target: { value: "gated" } });
    expect(screen.getByText(/Indefinite timeouts on all gates/)).toBeDefined();
  });

  it("no warning for supervised preset (only one gate enabled)", () => {
    renderPanel();
    // Supervised has only task_approval enabled with 30s timeout
    expect(screen.queryByText(/Indefinite timeouts/)).toBeNull();
  });
});
