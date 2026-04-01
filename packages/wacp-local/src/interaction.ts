export type InputType = "goal" | "amendment" | "query" | "approval" | "injection";

export interface Gate {
  id: string;
  operation: string;
}

export interface ClassifiedInput {
  type: InputType;
  content: string;
  gateId?: string;
  targetWorkspace?: string;
}

export class InteractionStream {
  private buffer: string[] = [];

  /** Classify raw human input based on heuristics and context. */
  classify(input: string, pendingGates: Gate[] = []): ClassifiedInput {
    const trimmed = input.trim();

    // Approval/rejection of pending gates
    if (pendingGates.length > 0) {
      const lower = trimmed.toLowerCase();
      if (
        lower.startsWith("yes") ||
        lower.startsWith("approve") ||
        lower.startsWith("allow") ||
        lower === "y"
      ) {
        return {
          type: "approval",
          content: trimmed,
          gateId: pendingGates[0].id,
        };
      }
      if (
        lower.startsWith("no") ||
        lower.startsWith("reject") ||
        lower.startsWith("deny") ||
        lower === "n"
      ) {
        return {
          type: "approval",
          content: trimmed,
          gateId: pendingGates[0].id,
        };
      }
    }

    // Injection syntax: @ws-id: message
    const injectionMatch = trimmed.match(/^@([\w-]+):\s*(.+)$/s);
    if (injectionMatch) {
      return {
        type: "injection",
        content: injectionMatch[2],
        targetWorkspace: injectionMatch[1],
      };
    }

    // Query: starts with ?, what, how, why
    const lower = trimmed.toLowerCase();
    if (
      lower.startsWith("?") ||
      lower.startsWith("what ") ||
      lower.startsWith("how ") ||
      lower.startsWith("why ")
    ) {
      return { type: "query", content: trimmed };
    }

    // Amendment: starts with "actually", "instead", "change"
    if (
      lower.startsWith("actually") ||
      lower.startsWith("instead") ||
      lower.startsWith("change ")
    ) {
      return { type: "amendment", content: trimmed };
    }

    // Default: goal
    return { type: "goal", content: trimmed };
  }

  /** Buffer input during agent work. */
  bufferInput(input: string): void {
    this.buffer.push(input);
  }

  /** Flush all buffered input. */
  flush(): string[] {
    const items = [...this.buffer];
    this.buffer = [];
    return items;
  }

  /** Number of buffered items. */
  get bufferedCount(): number {
    return this.buffer.length;
  }
}
