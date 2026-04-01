export interface TrustDecision {
  operation: string;
  granted: boolean;
  timestamp: number;
}

export interface SessionSnapshot {
  historyLength: number;
  trustLog: TrustDecision[];
  metadata: Record<string, unknown>;
}

export class SessionContext {
  history: string[] = [];
  trustLog: TrustDecision[] = [];
  metadata: Record<string, unknown> = {};

  /** Record a trust decision. */
  recordTrust(operation: string, granted: boolean): void {
    this.trustLog.push({ operation, granted, timestamp: Date.now() });
  }

  /** Add to conversation history. */
  addHistory(entry: string): void {
    this.history.push(entry);
  }

  /** Take a snapshot of current state. */
  snapshot(): SessionSnapshot {
    return {
      historyLength: this.history.length,
      trustLog: [...this.trustLog],
      metadata: { ...this.metadata },
    };
  }

  /** Restore from a snapshot (trust log and metadata only). */
  restore(snapshot: SessionSnapshot): void {
    this.trustLog = [...snapshot.trustLog];
    this.metadata = { ...snapshot.metadata };
  }

  /** Prune old history entries to stay within a budget. */
  prune(maxEntries: number): number {
    if (this.history.length <= maxEntries) return 0;
    const removed = this.history.length - maxEntries;
    this.history = this.history.slice(-maxEntries);
    return removed;
  }
}
