/** Error thrown when an operation is blocked by the autonomy manager. */
export class AutonomyError extends Error {
  constructor(
    public readonly operation: string,
    message?: string,
  ) {
    super(message ?? `Operation '${operation}' requires human approval`);
    this.name = "AutonomyError";
  }
}

/** Error thrown for session lifecycle violations. */
export class SessionError extends Error {
  constructor(
    public readonly code: SessionErrorCode,
    message: string,
  ) {
    super(message);
    this.name = "SessionError";
  }
}

export type SessionErrorCode =
  | "invalid_transition"
  | "already_closed"
  | "not_active"
  | "config_invalid";
