import * as readline from "node:readline/promises";
import { stdin, stdout } from "node:process";

import type { LocalSession } from "@wacp/local";
import { InteractionStream } from "@wacp/local";

import type { CliConfig } from "./config.js";
import { handleCommand } from "./commands.js";
import { agentLoop } from "./agent.js";

/**
 * Run the interactive REPL loop.
 *
 * Reads input, classifies it, routes to the agent loop or command handlers.
 * Ctrl-C during agent work cancels the current operation.
 * Ctrl-C at the prompt re-prompts. Double Ctrl-C exits.
 */
export async function repl(session: LocalSession, config: CliConfig): Promise<void> {
  const rl = readline.createInterface({ input: stdin, output: stdout });
  const stream = new InteractionStream();

  let abortController: AbortController | null = null;
  let ctrlCCount = 0;

  // Handle Ctrl-C
  rl.on("SIGINT", () => {
    if (abortController) {
      // Cancel current agent work
      abortController.abort();
      abortController = null;
      stdout.write("\n");
    } else {
      ctrlCCount++;
      if (ctrlCCount >= 2) {
        stdout.write("\nExiting.\n");
        rl.close();
        return;
      }
      stdout.write("\n(Press Ctrl-C again to exit, or type /exit)\n");
      // Reset after 1 second
      setTimeout(() => { ctrlCCount = 0; }, 1000);
    }
  });

  await session.activate();

  while (session.state !== "closed") {
    let input: string;
    try {
      input = await rl.question("wacp> ");
    } catch {
      // EOF or readline error
      break;
    }

    ctrlCCount = 0; // reset on any input
    const trimmed = input.trim();
    if (!trimmed) continue;

    // Slash commands
    if (trimmed.startsWith("/")) {
      const result = handleCommand(trimmed, session.autonomy);
      stdout.write(result.output + "\n");
      if (result.exit) {
        await session.close();
        break;
      }
      continue;
    }

    // Classify input
    const classified = stream.classify(trimmed);

    switch (classified.type) {
      case "goal":
      case "amendment":
      case "query": {
        abortController = new AbortController();
        try {
          await agentLoop(session, config, classified.content, {
            promptGate: (prompt) => gatePrompt(rl, prompt),
            print: (text) => stdout.write(text + "\n"),
            write: (text) => stdout.write(text),
          }, abortController.signal);
        } catch (err) {
          if ((err as Error).name !== "AbortError") {
            stdout.write(`\nError: ${(err as Error).message}\n`);
          }
        }
        abortController = null;
        break;
      }
      case "approval":
        stdout.write("No pending gates to approve.\n");
        break;
      case "injection":
        stdout.write("Injection not supported in single-agent mode.\n");
        break;
    }
  }

  rl.close();
}

/** Prompt for gate approval. Returns the user's response. */
async function gatePrompt(rl: readline.Interface, prompt: string): Promise<string> {
  stdout.write(prompt);
  try {
    const answer = await rl.question("");
    return answer.trim() || "n";
  } catch {
    return "n"; // default to deny on error
  }
}
