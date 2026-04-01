import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import YAML from "yaml";

import type { AutonomyPreset } from "@wacp/local";

export interface CliConfig {
  provider: "anthropic" | "openai" | "generic";
  model: string;
  apiKey: string;
  workingDir: string;
  autonomy: AutonomyPreset;
  baseUrl?: string;
  maxTokens: number;
  temperature: number;
  systemPrompt: string;
}

export interface CliFlags {
  config?: string;
  provider?: string;
  model?: string;
  workingDir?: string;
  autonomy?: string;
}

const DEFAULTS: CliConfig = {
  provider: "anthropic",
  model: "claude-sonnet-4-20250514",
  apiKey: "",
  workingDir: process.cwd(),
  autonomy: "assisted",
  maxTokens: 16384,
  temperature: 0,
  systemPrompt: `You are a helpful coding assistant working in the user's project directory. You have access to filesystem, shell, and git tools. Use them to accomplish the user's goals. Be concise and direct.`,
};

/** Resolve a value that may contain ${ENV_VAR} references. */
export function resolveEnvVars(value: string): string {
  return value.replace(/\$\{([^}]+)\}/g, (_match, varName) => {
    const envValue = process.env[varName];
    if (envValue === undefined) {
      throw new Error(`Environment variable '${varName}' is not set`);
    }
    return envValue;
  });
}

/** Load config from a YAML string. */
export function parseConfig(yamlContent: string): Partial<CliConfig> {
  const raw = YAML.parse(yamlContent) as Record<string, unknown>;
  if (!raw || typeof raw !== "object") return {};

  const config: Partial<CliConfig> = {};

  if (typeof raw.provider === "string") config.provider = raw.provider as CliConfig["provider"];
  if (typeof raw.model === "string") config.model = raw.model;
  if (typeof raw.api_key === "string") config.apiKey = resolveEnvVars(raw.api_key);
  if (typeof raw.working_dir === "string") config.workingDir = raw.working_dir;
  if (typeof raw.autonomy === "string") config.autonomy = raw.autonomy as AutonomyPreset;
  if (typeof raw.base_url === "string") config.baseUrl = raw.base_url;
  if (typeof raw.max_tokens === "number") config.maxTokens = raw.max_tokens;
  if (typeof raw.temperature === "number") config.temperature = raw.temperature;
  if (typeof raw.system_prompt === "string") config.systemPrompt = raw.system_prompt;

  return config;
}

/** Load config file from disk. Returns empty config if file doesn't exist. */
export function loadConfigFile(configPath?: string): Partial<CliConfig> {
  const filePath = configPath ?? path.join(os.homedir(), ".wacp", "config.yaml");
  try {
    const content = fs.readFileSync(filePath, "utf-8");
    return parseConfig(content);
  } catch {
    return {}; // file not found or unreadable — use defaults
  }
}

/** Merge configs: flags > env > file > defaults. */
export function mergeConfig(flags: CliFlags, fileConfig: Partial<CliConfig>): CliConfig {
  const merged: CliConfig = { ...DEFAULTS };

  // File config
  Object.assign(merged, stripUndefined(fileConfig));

  // Env vars (direct, not from config file)
  if (process.env.WACP_PROVIDER) merged.provider = process.env.WACP_PROVIDER as CliConfig["provider"];
  if (process.env.WACP_MODEL) merged.model = process.env.WACP_MODEL;
  if (process.env.ANTHROPIC_API_KEY && !merged.apiKey) merged.apiKey = process.env.ANTHROPIC_API_KEY;
  if (process.env.OPENAI_API_KEY && !merged.apiKey) merged.apiKey = process.env.OPENAI_API_KEY;

  // CLI flags
  if (flags.provider) merged.provider = flags.provider as CliConfig["provider"];
  if (flags.model) merged.model = flags.model;
  if (flags.workingDir) merged.workingDir = flags.workingDir;
  if (flags.autonomy) merged.autonomy = flags.autonomy as AutonomyPreset;

  return merged;
}

/** Validate that the config has all required fields. */
export function validateConfig(config: CliConfig): string[] {
  const errors: string[] = [];

  if (!["anthropic", "openai", "generic"].includes(config.provider)) {
    errors.push(`Invalid provider: '${config.provider}'. Must be anthropic, openai, or generic.`);
  }
  if (!config.model) {
    errors.push("Model is required.");
  }
  if (!config.apiKey) {
    errors.push("API key is required. Set via config file, ANTHROPIC_API_KEY, or OPENAI_API_KEY.");
  }

  return errors;
}

function stripUndefined(obj: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj)) {
    if (value !== undefined) result[key] = value;
  }
  return result;
}
