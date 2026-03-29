import { describe, it, expect } from "vitest";
import { ConnectError, Code } from "@connectrpc/connect";
import { classifyError, errorMessage } from "./errors.js";

describe("classifyError", () => {
  it("classifies UNAUTHENTICATED as authentication", () => {
    const err = new ConnectError("expired", Code.Unauthenticated);
    expect(classifyError(err)).toBe("authentication");
  });

  it("classifies PERMISSION_DENIED as permission", () => {
    const err = new ConnectError("denied", Code.PermissionDenied);
    expect(classifyError(err)).toBe("permission");
  });

  it("classifies INVALID_ARGUMENT as validation", () => {
    const err = new ConnectError("bad input", Code.InvalidArgument);
    expect(classifyError(err)).toBe("validation");
  });

  it("classifies NOT_FOUND as validation", () => {
    const err = new ConnectError("not found", Code.NotFound);
    expect(classifyError(err)).toBe("validation");
  });

  it("classifies FAILED_PRECONDITION as validation", () => {
    const err = new ConnectError("precondition", Code.FailedPrecondition);
    expect(classifyError(err)).toBe("validation");
  });

  it("classifies UNAVAILABLE as transient", () => {
    const err = new ConnectError("down", Code.Unavailable);
    expect(classifyError(err)).toBe("transient");
  });

  it("classifies INTERNAL as transient", () => {
    const err = new ConnectError("crash", Code.Internal);
    expect(classifyError(err)).toBe("transient");
  });

  it("classifies plain Error as transient", () => {
    expect(classifyError(new Error("oops"))).toBe("transient");
  });

  it("classifies unknown value as transient", () => {
    expect(classifyError("string error")).toBe("transient");
  });
});

describe("errorMessage", () => {
  it("extracts ConnectError message", () => {
    const err = new ConnectError("workspace not found", Code.NotFound);
    expect(errorMessage(err)).toContain("workspace not found");
  });

  it("extracts Error message", () => {
    expect(errorMessage(new Error("oops"))).toBe("oops");
  });

  it("handles unknown values", () => {
    expect(errorMessage(42)).toBe("An unknown error occurred");
  });
});
