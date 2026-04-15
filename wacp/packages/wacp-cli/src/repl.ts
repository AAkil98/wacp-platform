import * as readline from "node:readline/promises";
import { stdin, stdout } from "node:process";

import type { LocalSession } from "@wacp/local";
import { InteractionStream } from "@wacp/local";

import type { CliConfig } from "./config.js";
import type { LoadedEcosystem } from "./ecosystem.js";
import { handleCommand } from "./commands.js";
import { agentLoop } from "./agent.js";
import { routeGoal, executeGoalWithWorkflow, type Workflow, type AgentProfile, type ProtocolClients } from "./workflow.js";

/**
 * Loaded vertical — kept for backward compatibility with pre-27R callers.
 * New code should use {@link LoadedEcosystem} from ./ecosystem.js.
 */
export interface LoadedVertical {
  workflows: Workflow[];
  profiles: AgentProfile[];
}

/**
 * Run the interactive REPL loop.
 *
 * Goals are routed through the ecosystem:
 * 1. Try each loaded vertical's detector in load order
 * 2. First non-null match wins (domain verticals before SWE catchall)
 * 3. Execute via WorkflowExecutor with profile switching, dispatching tools
 *    to the owning vertical's executor
 * 4. Fall back to raw agent loop if no workflow matches the routed workflow ID
 */
export async function repl(
  session: LocalSession,
  config: CliConfig,
  ecosystem?: LoadedEcosystem,
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
          if (ecosystem) {
            // Multi-vertical path: route goal across all loaded verticals → select workflow → execute
            const routed = routeGoal(classified.content, ecosystem);
            stdout.write(`Vertical: ${routed.verticalId} | Task type: ${routed.taskType}\n`);

            if (routed.workflow) {
              await executeGoalWithWorkflow(
                session, config, classified.content,
                routed.workflow, [...routed.profiles],
                callbacks,
                clients ?? { coordinator: null, agent: null },
                abortController.signal,
                ecosystem,
              );
            } else {
              // Workflow ID returned by detector has no actual workflow object
              // (e.g., *-only sentinels for direct-execution task types)
              // Fall back to direct execution with ecosystem-composed tools
              stdout.write(`[no workflow for ${routed.workflowId}, using direct mode]\n`);
              await agentLoop(session, config, classified.content, callbacks, abortController.signal, ecosystem);
            }
          } else {
            // No ecosystem loaded — direct execution with legacy tool list
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
          }, abortController.signal, ecosystem);
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
