import { AutonomyManager, type AutonomyPreset } from "./autonomy.js";
import { SessionContext } from "./context.js";
import { SessionError } from "./errors.js";
import { LocalResources } from "./resources.js";

export type SessionState = "open" | "active" | "suspended" | "closed";

export interface SessionConfig {
  /** Working directory for local resources. */
  workingDir: string;
  /** Autonomy preset: supervised, assisted, autonomous. */
  autonomyPreset: AutonomyPreset;
  /** LLM provider configuration (optional — for single-agent mode). */
  llm?: LlmConfig;
  /** Runtime URL (optional — for multi-agent mode). */
  runtimeUrl?: string;
}

export interface LlmConfig {
  provider: "anthropic" | "openai" | "generic";
  apiKey: string;
  model?: string;
  baseUrl?: string;
}

/** Valid transitions: [from, to] */
const VALID_TRANSITIONS: [SessionState, SessionState][] = [
  ["open", "active"],
  ["active", "suspended"],
  ["suspended", "active"],
  ["active", "closed"],
  ["suspended", "closed"],
  ["open", "closed"],
];

function isValidTransition(from: SessionState, to: SessionState): boolean {
  return VALID_TRANSITIONS.some(([f, t]) => f === from && t === to);
}

let sessionCounter = 0;

export class LocalSession {
  readonly id: string;
  private _state: SessionState;
  readonly config: SessionConfig;
  readonly autonomy: AutonomyManager;
  readonly resources: LocalResources;
  readonly context: SessionContext;

  private constructor(config: SessionConfig) {
    this.id = `session-${++sessionCounter}`;
    this._state = "open";
    this.config = config;
    this.autonomy = new AutonomyManager(config.autonomyPreset);
    this.resources = new LocalResources(config.workingDir, this.autonomy);
    this.context = new SessionContext();
  }

  /** Current session state. */
  get state(): SessionState {
    return this._state;
  }

  /** Create a new session. */
  static async create(config: SessionConfig): Promise<LocalSession> {
    if (!config.workingDir) {
      throw new SessionError("config_invalid", "workingDir is required");
    }
    return new LocalSession(config);
  }

  /** Transition to active state. */
  async activate(): Promise<void> {
    this.transition("active");
  }

  /** Transition to suspended state. */
  async suspend(): Promise<void> {
    this.transition("suspended");
  }

  /** Close the session. */
  async close(): Promise<void> {
    this.transition("closed");
  }

  private transition(to: SessionState): void {
    if (this._state === "closed") {
      throw new SessionError("already_closed", "Session is already closed");
    }
    if (!isValidTransition(this._state, to)) {
      throw new SessionError(
        "invalid_transition",
        `Cannot transition from '${this._state}' to '${to}'`,
      );
    }
    this._state = to;
  }
}
