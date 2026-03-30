import { getClient } from "./client.js";
import { classifyError } from "./errors.js";
import { startTrailStream, startWorkspaceStream, fetchTaskGraph, fetchWorkspace } from "./streams.js";
import { useStore, type SessionState } from "../store/index.js";
import { notifyGate, notifyEscalation } from "../notifications/notify.js";

const MAX_RETRIES = 5;
const BACKOFF_BASE_MS = 1000;
const BACKOFF_CAP_MS = 30_000;

/**
 * Manages the highway session lifecycle: authentication, stream supervision,
 * reconnection with exponential backoff, and graceful disconnect.
 *
 * Spec reference: highway-ui.md §5 (Connection Lifecycle).
 */
export class SessionManager {
  private streamController: AbortController | null = null;
  private token: string | null = null;
  private consecutiveFailures = 0;
  private reconnecting = false;

  /** Authenticate and open all streams. */
  async connect(token: string): Promise<void> {
    this.token = token;
    this.setSessionState("authenticating");

    try {
      const client = getClient();
      const response = await client.authenticate({ authToken: token });
      this.setSessionState(
        "connected",
        response.userId,
        response.capabilities,
      );
      this.consecutiveFailures = 0;
      this.openStreams(true);
    } catch (err: unknown) {
      this.setSessionState("disconnected");
      const category = classifyError(err);
      throw new Error(
        category === "authentication"
          ? "Authentication failed — invalid token"
          : `Connection failed: ${(err as Error).message}`,
      );
    }
  }

  /** Cancel all streams and reset session. */
  disconnect(): void {
    this.abortStreams();
    this.token = null;
    this.consecutiveFailures = 0;
    this.reconnecting = false;
    this.setSessionState("disconnected");
  }

  /** Current session state. */
  getState(): SessionState {
    return useStore.getState().session.state;
  }

  // ── Internals ──

  private openStreams(fromBeginning: boolean): void {
    this.abortStreams();
    this.streamController = new AbortController();
    const { signal } = this.streamController;

    // Launch all 4 streams concurrently. Each runs until aborted or error.
    const streams = [
      startTrailStream({ signal, fromBeginning }),
      startWorkspaceStream({ signal }),
      this.runGateStream(signal),
      this.runEscalationStream(signal),
    ];

    // Supervise: if any stream fails, trigger reconnection
    for (const stream of streams) {
      stream.catch((err: unknown) => {
        if (signal.aborted) return;
        this.handleStreamFailure(err);
      });
    }
  }

  private async runGateStream(signal: AbortSignal): Promise<void> {
    const client = getClient();

    try {
      for await (const event of client.streamGates({}, { signal })) {
        const gate = {
          gateId: event.gateId,
          gateType: event.type.toString(),
          subject: event.subject,
          workspaceId: event.workspaceId,
          taskId: event.taskId,
          timeoutMs: event.timeoutMs,
          fallbackAction: event.fallbackAction,
          createdAt: event.createdAt?.physicalUs ?? 0n,
        };
        useStore.getState().addPendingGate(gate);
        notifyGate(gate);
      }
    } catch (err: unknown) {
      if (signal.aborted) return;
      throw err;
    }
  }

  private async runEscalationStream(signal: AbortSignal): Promise<void> {
    const client = getClient();

    try {
      for await (const event of client.streamEscalations({}, { signal })) {
        const esc = {
          escalationId: event.escalationId,
          workspaceId: event.workspaceId,
          owner: event.owner,
          context: new TextDecoder().decode(event.context),
          createdAt: event.createdAt?.physicalUs ?? 0n,
        };
        useStore.getState().addEscalation(esc);
        useStore.getState().setEscalationBanner({
          workspaceId: esc.workspaceId,
          escalationId: esc.escalationId,
        });
        notifyEscalation(esc);
      }
    } catch (err: unknown) {
      if (signal.aborted) return;
      throw err;
    }
  }

  private handleStreamFailure(_err: unknown): void {
    if (this.reconnecting) return;
    this.reconnecting = true;
    this.attemptReconnect();
  }

  private async attemptReconnect(): Promise<void> {
    this.abortStreams();
    this.setSessionState("reconnecting");

    while (this.consecutiveFailures < MAX_RETRIES) {
      const delayMs = Math.min(
        BACKOFF_BASE_MS * Math.pow(2, this.consecutiveFailures),
        BACKOFF_CAP_MS,
      );
      await sleep(delayMs);

      try {
        // Re-authenticate — token may have expired
        const client = getClient();
        const response = await client.authenticate({ authToken: this.token! });
        this.setSessionState(
          "connected",
          response.userId,
          response.capabilities,
        );

        // Re-open streams. fromBeginning: false — store already has history.
        this.consecutiveFailures = 0;
        this.reconnecting = false;
        this.openStreams(false);

        // Refresh entity state — streams may have missed events during the gap
        await this.refreshEntityState();
        return;
      } catch {
        this.consecutiveFailures++;
      }
    }

    // Max retries exceeded — give up
    this.reconnecting = false;
    this.setSessionState("disconnected");
  }

  private async refreshEntityState(): Promise<void> {
    try {
      const views = useStore.getState().workspaces.views;
      const refreshPromises: Promise<void>[] = [fetchTaskGraph()];
      for (const id of views.keys()) {
        refreshPromises.push(fetchWorkspace(id));
      }
      await Promise.allSettled(refreshPromises);
    } catch {
      // Non-fatal — streams will catch up
    }
  }

  private abortStreams(): void {
    if (this.streamController) {
      this.streamController.abort();
      this.streamController = null;
    }
  }

  private setSessionState(
    state: SessionState,
    userId?: string,
    capabilities?: string[],
  ): void {
    useStore.getState().setSession(state, userId, capabilities);
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Singleton session manager instance. */
export const sessionManager = new SessionManager();
