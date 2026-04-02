#!/usr/bin/env node

import { LocalSession } from "@wacp/local";
import { loadConfigFile, mergeConfig, validateConfig, type CliFlags } from "./config.js";
import { formatBanner } from "./display.js";
import { repl, type LoadedVertical } from "./repl.js";
import { loadSweVertical } from "./vertical.js";

async function main(): Promise<void> {
  const flags = parseFlags(process.argv.slice(2));

  const fileConfig = loadConfigFile(flags.config);
  const config = mergeConfig(flags, fileConfig);

  const errors = validateConfig(config);
  if (errors.length > 0) {
    for (const err of errors) {
      process.stderr.write(`Config error: ${err}\n`);
    }
    process.exit(1);
  }

  // Load SWE vertical (workflows + profiles)
  const vertical = loadSweVertical();

  process.stdout.write(formatBanner(config.workingDir, config.provider, config.model, config.autonomy));
  process.stdout.write(`  Vertical: SWE (${vertical.workflows.length} workflows, ${vertical.profiles.length} profiles)\n\n`);

  const session = await LocalSession.create({
    workingDir: config.workingDir,
    autonomyPreset: config.autonomy,
  });

  await repl(session, config, vertical);

  if (session.state !== "closed") {
    await session.close();
  }
}

function parseFlags(args: string[]): CliFlags {
  const flags: CliFlags = {};
  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--config":
      case "-c":
        flags.config = args[++i];
        break;
      case "--provider":
      case "-p":
        flags.provider = args[++i];
        break;
      case "--model":
      case "-m":
        flags.model = args[++i];
        break;
      case "--working-dir":
      case "-d":
        flags.workingDir = args[++i];
        break;
      case "--autonomy":
      case "-a":
        flags.autonomy = args[++i];
        break;
      case "--help":
      case "-h":
        printUsage();
        process.exit(0);
        break;
      default:
        if (args[i].startsWith("-")) {
          process.stderr.write(`Unknown flag: ${args[i]}\n`);
          process.exit(1);
        }
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

Environment variables:
  ANTHROPIC_API_KEY         API key for Anthropic
  OPENAI_API_KEY            API key for OpenAI
`);
}

main().catch((err) => {
  process.stderr.write(`Fatal: ${err.message}\n`);
  process.exit(1);
});
