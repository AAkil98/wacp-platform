import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { InjectionForm } from "./InjectionForm.js";
import { useStore } from "@/store";

function resetStore() {
  useStore.setState({
    session: { state: "connected", userId: "user-1", capabilities: ["inject"] },
    trail: {
      entries: [],
      paused: false,
      filters: { eventTypes: new Set(), workspaceId: "", actor: "" },
      expandedEntryIndex: null,
    },
    gates: { pending: new Map(), resolved: new Map(), inFlight: new Set() },
    escalations: { active: new Map(), resolved: new Map(), inFlight: new Set() },
    workspaces: { views: new Map(), changes: [], expandedNodes: new Set() },
    taskGraph: { tasks: [], lastFetched: null },
    notifications: { escalationBanner: null },
  });
}

function renderForm(initialRoute = "/inject") {
  return render(
    <MemoryRouter initialEntries={[initialRoute]}>
      <InjectionForm />
    </MemoryRouter>,
  );
}

describe("InjectionForm", () => {
  beforeEach(resetStore);

  it("renders all form fields", () => {
    renderForm();
    expect(screen.getByText("Target Workspace")).toBeDefined();
    expect(screen.getByText("Envelope Type")).toBeDefined();
    expect(screen.getByText("Priority")).toBeDefined();
    expect(screen.getByText("Payload")).toBeDefined();
    expect(screen.getByText("Send")).toBeDefined();
  });

  it("Send button disabled when workspace empty", () => {
    renderForm();
    const sendBtn = screen.getByText("Send");
    expect(sendBtn).toHaveProperty("disabled", true);
  });

  it("Send button enabled when workspace filled", () => {
    renderForm();
    const input = screen.getByPlaceholderText("ws-...");
    fireEvent.change(input, { target: { value: "ws-target" } });
    const sendBtn = screen.getByText("Send");
    expect(sendBtn).toHaveProperty("disabled", false);
  });

  it("renders priority radio buttons", () => {
    renderForm();
    expect(screen.getByText("Normal")).toBeDefined();
    expect(screen.getByText("Urgent")).toBeDefined();
    expect(screen.getByText("Blocking")).toBeDefined();
  });

  it("renders envelope type dropdown with three options", () => {
    renderForm();
    const select = screen.getByDisplayValue("directive");
    expect(select).toBeDefined();
    // The select should have 3 options
    const options = select.querySelectorAll("option");
    expect(options).toHaveLength(3);
  });

  it("pre-populates from query params", () => {
    renderForm("/inject?workspace=ws-prefilled&type=feedback");
    // The workspace input should be pre-filled
    const wsInput = screen.getByPlaceholderText("ws-...") as HTMLInputElement;
    expect(wsInput.value).toBe("ws-prefilled");
    // The type dropdown should be pre-selected
    const typeSelect = screen.getByDisplayValue("feedback");
    expect(typeSelect).toBeDefined();
  });

  it("shows workspace autocomplete from store", () => {
    useStore.getState().upsertWorkspace({
      id: "ws-known",
      state: "WORKSPACE_STATE_ACTIVE",
      role: "worker",
      parent: "",
      owner: "user-1",
      originator: "System",
      taskId: "",
      checkpointCount: 0,
    });
    renderForm();
    // Datalist should contain the workspace ID
    const datalist = document.getElementById("inject-workspace-ids");
    expect(datalist).not.toBeNull();
    const options = datalist!.querySelectorAll("option");
    expect(options).toHaveLength(1);
    expect(options[0]!.getAttribute("value")).toBe("ws-known");
  });
});
