import { render, screen, fireEvent } from "@testing-library/react";
import { LoginScreen } from "./LoginScreen.js";

describe("LoginScreen", () => {
  const onLogin = vi.fn();

  beforeEach(() => {
    onLogin.mockClear();
  });

  it("renders title and subtitle", () => {
    render(<LoginScreen onLogin={onLogin} error={null} loading={false} />);
    expect(screen.getByText("WACP Highway")).toBeInTheDocument();
    expect(screen.getByText(/authentication token/i)).toBeInTheDocument();
  });

  it("renders password input and connect button", () => {
    render(<LoginScreen onLogin={onLogin} error={null} loading={false} />);
    expect(screen.getByPlaceholderText(/paste token/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /connect/i })).toBeInTheDocument();
  });

  it("calls onLogin with trimmed token on submit", () => {
    render(<LoginScreen onLogin={onLogin} error={null} loading={false} />);
    const input = screen.getByPlaceholderText(/paste token/i);
    fireEvent.change(input, { target: { value: "  my-token  " } });
    fireEvent.submit(input.closest("form")!);
    expect(onLogin).toHaveBeenCalledWith("my-token");
  });

  it("shows error message when error prop is set", () => {
    render(<LoginScreen onLogin={onLogin} error="Bad token" loading={false} />);
    expect(screen.getByText("Bad token")).toBeInTheDocument();
  });

  it("shows loading state", () => {
    render(<LoginScreen onLogin={onLogin} error={null} loading={true} />);
    expect(screen.getByRole("button")).toHaveTextContent(/connecting/i);
    expect(screen.getByPlaceholderText(/paste token/i)).toBeDisabled();
  });

  it("disables button when input is empty", () => {
    render(<LoginScreen onLogin={onLogin} error={null} loading={false} />);
    expect(screen.getByRole("button", { name: /connect/i })).toBeDisabled();
  });
});
