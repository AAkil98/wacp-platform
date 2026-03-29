import { ConnectError, Code } from "@connectrpc/connect";

export type ErrorCategory =
  | "authentication"
  | "permission"
  | "validation"
  | "transient";

/** Classify a Connect/gRPC error into a UI error category. */
export function classifyError(err: unknown): ErrorCategory {
  if (err instanceof ConnectError) {
    switch (err.code) {
      case Code.Unauthenticated:
        return "authentication";
      case Code.PermissionDenied:
        return "permission";
      case Code.InvalidArgument:
      case Code.NotFound:
      case Code.FailedPrecondition:
        return "validation";
      default:
        return "transient";
    }
  }
  return "transient";
}

/** Extract a user-friendly message from a Connect error. */
export function errorMessage(err: unknown): string {
  if (err instanceof ConnectError) {
    return err.message || err.code.toString();
  }
  if (err instanceof Error) {
    return err.message;
  }
  return "An unknown error occurred";
}
