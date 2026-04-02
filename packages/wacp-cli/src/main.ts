#!/usr/bin/env node

import { LocalSession } from "@wacp/local";
import { loadConfigFile, mergeConfig, validateConfig, type CliFlags } from "./config.js";
import { formatBanner } from "./display.js";
import { repl, type LoadedVertical } from "./repl.js";
import { loadSweVertical } from "./vertical.js";
import { RuntimeManager } from "./runtime-manager.js";
import { CoordinatorClient, AgentClient } from "./protocol-client.js";
import type { ProtocolClients } from "./workflow.js";

async function main(): Promise<void> {
  const flags = parseFlags(process.argv.slice(2));
  const fileConfig = loadConfigFile(flags.config);
  const config = mergeConfig(flags, fileConfig);

  const errors = validateConfig(config);
  if (errors.length > 0) {
    for (const err of errors) process.stderr.write(`Config error: ${err}\n`);
    process.exit(1);
  }

  const vertical = loadSweVertical();

  process.stdout.write(formatBanner(config.workingDir, config.provider, config.model, config.autonomy));
  process.stdout.write(`  Vertical: SWE (${vertical.workflows.length} workflows, ${vertical.profiles.length} profiles)\n`);

  // Spawn WACP runtime
  const runtime = new RuntimeManager({
    workingDir: config.workingDir,
    dataDir: ".wacp/data",
  });

  let clients: ProtocolClients = { coordinator: null, agent: null };

  try {
    process.stdout.write("  Spawning WACP runtime...");
    await runtime.start(15_000);
    process.stdout.write(" ready.\n");

    clients = {
      coordinator: new CoordinatorClient(runtime.coordinatorUrl),
      agent: new AgentClient(runtime.agentUrl),
    };
    process.stdout.write(`  Connected: coordinator=${runtime.coordinatorUrl}, agent=${runtime.agentUrl}\n\n`);
  } catch (err) {
    process.stdout.write(` failed.\n`);
    process.stderr.write(`Warning: Runtime not available (${(err as Error).message}). Running in local-only mode.\n\n`);
  }

  const session = await LocalSession.create({
    workingDir: config.workingDir,
    autonomyPreset: config.autonomy,
  });

  const shutdown = async () => {
    clients.coordinator?.close();
    clients.agent?.close();
    await runtime.stop();
    if (session.state !== "closed") await session.close();
  };

  process.on("SIGTERM", shutdown);

  try {
    await repl(session, config, vertical, clients);
  } finally {
    await shutdown();
  }
}

function parseFlags(args: string[]): CliFlags {
  const flags: CliFlags = {};
  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--config": case "-c": flags.config = args[++i]; break;
      case "--provider": case "-p": flags.provider = args[++i]; break;
      case "--model": case "-m": flags.model = args[++i]; break;
      case "--working-dir": case "-d": flags.workingDir = args[++i]; break;
      case "--autonomy": case "-a": flags.autonomy = args[++i]; break;
      case "--help": case "-h": printUsage(); process.exit(0); break;
      default:
        if (args[i].startsWith("-")) { process.stderr.write(`Unknown flag: ${args[i]}\n`); process.exit(1); }
    }
  }
  return flags;
}

function printUsage(): void {
  process.stdout.write(`
Usage: wacp [options]

Options:
  -c, --config <path>       Config file path (default: ~/.wacp/config.yaml)
  -p, --provider <name>     LLM provider (anthropic, openai, generic)
  -m, --model <name>        Model name
  -d, --working-dir <path>  Working directory (default: current)
  -a, --autonomy <preset>   Autonomy preset (supervised, assisted, autonomous)
  -h, --help                Show this help

The CLI spawns a WACP runtime process and drives coordination via gRPC.
Requires 'wacp-runtime' to be built and available on PATH.

Environment variables:
  ANTHROPIC_API_KEY         API key for Anthropic
  OPENAI_API_KEY            API key for OpenAI
`);
}

main().catch((err) => {
  process.stderr.write(`Fatal: ${err.message}\n`);
  process.exit(1);
});
