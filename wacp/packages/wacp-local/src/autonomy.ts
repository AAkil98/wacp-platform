/**
 * Operation type for autonomy gating.
 *
 * The nine string literals are the canonical core ops (`file_*`, `shell_*`,
 * `git_*`, `web_fetch`, `llm_call`) used by the built-in tooling. Verticals
 * are free to introduce vertical-specific operations (e.g. finance's
 * `data_read`/`trade_exec`, mlops' `compute_exec`); the trailing
 * `string & {}` keeps the union open to those while preserving literal
 * autocomplete for the canonical set.
 */
export type OperationType =
  | "file_read"
  | "file_write"
  | "file_delete"
  | "shell_exec"
  | "shell_exec_destructive"
  | "git_read"
  | "git_write"
  | "web_fetch"
  | "llm_call"
  | (string & {});

export type AutonomyPreset = "supervised" | "assisted" | "autonomous";

const PRESET_DEFAULTS: Record<AutonomyPreset, Set<OperationType>> = {
  supervised: new Set(["llm_call"]),
  assisted: new Set(["file_read", "git_read", "web_fetch", "llm_call"]),
  autonomous: new Set([
    "file_read",
    "file_write",
    "shell_exec",
    "git_read",
    "git_write",
    "web_fetch",
    "llm_call",
  ]),
};

export class AutonomyManager {
  private trusted: Set<OperationType>;
  private alwaysTrusted: Set<OperationType> = new Set();

  constructor(preset: AutonomyPreset) {
    this.trusted = new Set(PRESET_DEFAULTS[preset]);
  }

  /** Check if an operation is currently trusted. */
  check(op: OperationType): boolean {
    return this.trusted.has(op);
  }

  /** Grant trust for an operation (session-scoped). */
  grant(op: OperationType): void {
    this.trusted.add(op);
  }

  /** Grant trust permanently ("always allow"). */
  grantAlways(op: OperationType): void {
    this.trusted.add(op);
    this.alwaysTrusted.add(op);
  }

  /** Revoke trust for an operation. */
  revoke(op: OperationType): void {
    if (!this.alwaysTrusted.has(op)) {
      this.trusted.delete(op);
    }
  }

  /** Current trust surface. */
  trustSurface(): Set<OperationType> {
    return new Set(this.trusted);
  }

  /** Reset to a preset. Preserves "always" grants. */
  resetToPreset(preset: AutonomyPreset): void {
    this.trusted = new Set([...PRESET_DEFAULTS[preset], ...this.alwaysTrusted]);
  }
}
