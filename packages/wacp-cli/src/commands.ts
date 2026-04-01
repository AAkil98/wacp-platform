import type { AutonomyManager, OperationType, AutonomyPreset } from "@wacp/local";
import { formatHelp, formatTrustSurface } from "./display.js";

const VALID_OPERATIONS: OperationType[] = [
  "file_read", "file_write", "file_delete",
  "shell_exec", "shell_exec_destructive",
  "git_read", "git_write", "web_fetch", "llm_call",
];

const VALID_PRESETS: AutonomyPreset[] = ["supervised", "assisted", "autonomous"];

export interface CommandResult {
  output: string;
  exit: boolean;
}

/** Handle a slash command. Returns output text and whether to exit. */
export function handleCommand(
  input: string,
  autonomy: AutonomyManager,
): CommandResult {
  const parts = input.trim().split(/\s+/);
  const cmd = parts[0].toLowerCase();
  const arg = parts[1];

  switch (cmd) {
    case "/help":
      return { output: formatHelp(), exit: false };

    case "/exit":
    case "/quit":
      return { output: "Goodbye.", exit: true };

    case "/clear":
      return { output: "\x1B[2J\x1B[H", exit: false };

    case "/trust": {
      if (!arg) return { output: "Usage: /trust <operation>", exit: false };
      if (!VALID_OPERATIONS.includes(arg as OperationType)) {
        return { output: `Unknown operation: '${arg}'. Valid: ${VALID_OPERATIONS.join(", ")}`, exit: false };
      }
      autonomy.grant(arg as OperationType);
      return { output: `Granted: ${arg}`, exit: false };
    }

    case "/revoke": {
      if (!arg) return { output: "Usage: /revoke <operation>", exit: false };
      if (!VALID_OPERATIONS.includes(arg as OperationType)) {
        return { output: `Unknown operation: '${arg}'.`, exit: false };
      }
      autonomy.revoke(arg as OperationType);
      return { output: `Revoked: ${arg}`, exit: false };
    }

    case "/preset": {
      if (!arg) return { output: "Usage: /preset <supervised|assisted|autonomous>", exit: false };
      if (!VALID_PRESETS.includes(arg as AutonomyPreset)) {
        return { output: `Unknown preset: '${arg}'. Valid: ${VALID_PRESETS.join(", ")}`, exit: false };
      }
      autonomy.resetToPreset(arg as AutonomyPreset);
      return { output: `Preset: ${arg}`, exit: false };
    }

    case "/surface":
      return { output: formatTrustSurface(autonomy.trustSurface()), exit: false };

    default:
      return { output: `Unknown command: '${cmd}'. Type /help for available commands.`, exit: false };
  }
}
