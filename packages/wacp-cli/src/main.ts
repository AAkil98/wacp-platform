#!/usr/bin/env node

import { LocalSession } from "@wacp/local";
import { loadConfigFile, mergeConfig, validateConfig, type CliFlags } from "./config.js";
import { formatBanner } from "./display.js";
import { repl } from "./repl.js";

async function main(): Promise<void> {
  // Parse CLI flags
  const flags = parseFlags(process.argv.slice(2));

  // Load and merge config
  const fileConfig = loadConfigFile(flags.config);
  const config = mergeConfig(flags, fileConfig);

  // Validate
  const errors = validateConfig(config);
  if (errors.length > 0) {
    for (const err of errors) {
      process.stderr.write(`Config error: ${err}\n`);
    }
    process.exit(1);
  }

  // Print banner
  process.stdout.write(formatBanner(config.workingDir, config.provider, config.model, config.autonomy));

  // Create session
  const session = await LocalSession.create({
    workingDir: config.workingDir,
    autonomyPreset: config.autonomy,
  });

  // Enter REPL
  await repl(session, config);

  // Ensure clean exit
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
