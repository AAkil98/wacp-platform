import { render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import { TaskGraphView } from "./TaskGraphView.js";

beforeEach(() => {
  useStore.setState({
    taskGraph: { tasks: [], lastFetched: null },
  });
});

describe("TaskGraphView", () => {
  it("shows empty state when no tasks", () => {
    render(<TaskGraphView />);
    expect(screen.getByText(/no tasks/i)).toBeInTheDocument();
  });

  it("renders task name and status", () => {
    useStore.setState({
      taskGraph: {
        tasks: [
          { id: "t-1", name: "Build feature", status: "TASK_STATUS_IN_PROGRESS", workspaceRef: "ws-1", dependsOn: [], parentTask: "" },
        ],
        lastFetched: null,
      },
    });
    render(<TaskGraphView />);
    expect(screen.getByText("Build feature")).toBeInTheDocument();
    expect(screen.getByText(/in_progress/i)).toBeInTheDocument();
  });

  it("applies status color class", () => {
    useStore.setState({
      taskGraph: {
        tasks: [
          { id: "t-1", name: "Failed task", status: "TASK_STATUS_FAILED", workspaceRef: "", dependsOn: [], parentTask: "" },
        ],
        lastFetched: null,
      },
    });
    render(<TaskGraphView />);
    const name = screen.getByText("Failed task");
    expect(name.className).toContain("red");
  });

  it("renders dependencies when present", () => {
    useStore.setState({
      taskGraph: {
        tasks: [
          { id: "t-2", name: "Dependent", status: "TASK_STATUS_PENDING", workspaceRef: "", dependsOn: ["t-1", "t-0"], parentTask: "" },
        ],
        lastFetched: null,
      },
    });
    render(<TaskGraphView />);
    expect(screen.getByText(/depends on/i)).toBeInTheDocument();
    expect(screen.getByText(/t-1/)).toBeInTheDocument();
  });

  it("renders multiple tasks", () => {
    useStore.setState({
      taskGraph: {
        tasks: [
          { id: "t-1", name: "Task A", status: "TASK_STATUS_DRAFT", workspaceRef: "", dependsOn: [], parentTask: "" },
          { id: "t-2", name: "Task B", status: "TASK_STATUS_COMPLETED", workspaceRef: "", dependsOn: [], parentTask: "" },
        ],
        lastFetched: null,
      },
    });
    render(<TaskGraphView />);
    expect(screen.getByText("Task A")).toBeInTheDocument();
    expect(screen.getByText("Task B")).toBeInTheDocument();
  });
});
