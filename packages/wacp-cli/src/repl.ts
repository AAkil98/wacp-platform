import * as readline from "node:readline/promises";
import { stdin, stdout } from "node:process";

import type { LocalSession } from "@wacp/local";
import { InteractionStream } from "@wacp/local";

import type { CliConfig } from "./config.js";
import { handleCommand } from "./commands.js";
import { agentLoop } from "./agent.js";
import { detectTaskType, executeGoalWithWorkflow, type Workflow, type AgentProfile, type ProtocolClients } from "./workflow.js";

/** Loaded vertical — workflows + profiles available for execution. */
export interface LoadedVertical {
  workflows: Workflow[];
  profiles: AgentProfile[];
}

/**
 * Run the interactive REPL loop.
 *
 * Goals are routed through workflow detection:
 * 1. Detect task type from goal text
 * 2. Find matching workflow
 * 3. Execute via WorkflowExecutor with profile switching
 * 4. Fall back to raw agent loop if no workflow matches
 */
export async function repl(
  session: LocalSession,
  config: CliConfig,
  vertical?: LoadedVertical,
  clients?: ProtocolClients,
): Promise<void> {
  const rl = readline.createInterface({ input: stdin, output: stdout });
  const stream = new InteractionStream();

  let abortController: AbortController | null = null;
  let ctrlCCount = 0;

  rl.on("SIGINT", () => {
    if (abortController) {
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
      setTimeout(() => { ctrlCCount = 0; }, 1000);
    }
  });

  await session.activate();

  while (session.state !== "closed") {
    let input: string;
    try {
      input = await rl.question("wacp> ");
    } catch {
      break;
    }

    ctrlCCount = 0;
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
      case "amendment": {
        abortController = new AbortController();
        const callbacks = {
          promptGate: (prompt: string) => gatePrompt(rl, prompt),
          print: (text: string) => stdout.write(text + "\n"),
          write: (text: string) => stdout.write(text),
        };

        try {
          if (vertical) {
            // Protocol path: detect task type → select workflow → execute with profiles
            const detected = detectTaskType(classified.content);
            const workflow = vertical.workflows.find((w) => w.id === detected.workflowId);

            if (workflow) {
              stdout.write(`Task type: ${detected.id}\n`);
              await executeGoalWithWorkflow(
                session, config, classified.content,
                workflow, vertical.profiles,
                callbacks,
                clients ?? { coordinator: null, agent: null },
                abortController.signal,
              );
            } else {
              // Workflow not found (e.g., review-only, document-only not defined)
              // Fall back to direct execution
              stdout.write(`[no workflow for ${detected.workflowId}, using direct mode]\n`);
              await agentLoop(session, config, classified.content, callbacks, abortController.signal);
            }
          } else {
            // No vertical loaded — direct execution
            await agentLoop(session, config, classified.content, callbacks, abortController.signal);
          }
        } catch (err) {
          if ((err as Error).name !== "AbortError") {
            stdout.write(`\nError: ${(err as Error).message}\n`);
          }
        }
        abortController = null;
        break;
      }
      case "query": {
        // Queries go through direct agent loop (no workflow)
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

async function gatePrompt(rl: readline.Interface, prompt: string): Promise<string> {
  stdout.write(prompt);
  try {
    const answer = await rl.question("");
    return answer.trim() || "n";
  } catch {
    return "n";
  }
}
