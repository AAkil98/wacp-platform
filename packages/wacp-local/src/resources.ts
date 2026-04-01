import * as fs from "node:fs/promises";
import * as path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

import { AutonomyError } from "./errors.js";
import type { AutonomyManager, OperationType } from "./autonomy.js";

const execFileAsync = promisify(execFile);

export interface ExecOptions {
  cwd?: string;
  timeout?: number;
  stdin?: string;
}

export interface ExecResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

export class LocalResources {
  constructor(
    private readonly workingDir: string,
    private readonly autonomy: AutonomyManager,
  ) {}

  // ── Filesystem ─────────────────────────────────────────

  async readFile(filePath: string): Promise<string> {
    this.requireTrust("file_read");
    const resolved = this.resolvePath(filePath);
    return fs.readFile(resolved, "utf-8");
  }

  async writeFile(filePath: string, content: string): Promise<void> {
    this.requireTrust("file_write");
    const resolved = this.resolvePath(filePath);
    await fs.mkdir(path.dirname(resolved), { recursive: true });
    await fs.writeFile(resolved, content, "utf-8");
  }

  async deleteFile(filePath: string): Promise<void> {
    this.requireTrust("file_delete");
    const resolved = this.resolvePath(filePath);
    await fs.unlink(resolved);
  }

  async glob(pattern: string): Promise<string[]> {
    this.requireTrust("file_read");
    // Simple glob via readdir + filter (for production, use a glob library)
    const entries = await fs.readdir(this.workingDir, { recursive: true });
    return entries.filter((e) => e.includes(pattern.replace("*", "")));
  }

  async readDir(dirPath: string): Promise<string[]> {
    this.requireTrust("file_read");
    const resolved = this.resolvePath(dirPath);
    return fs.readdir(resolved);
  }

  async fileExists(filePath: string): Promise<boolean> {
    this.requireTrust("file_read");
    const resolved = this.resolvePath(filePath);
    try {
      await fs.access(resolved);
      return true;
    } catch {
      return false;
    }
  }

  // ── Shell ──────────────────────────────────────────────

  async exec(command: string, opts?: ExecOptions): Promise<ExecResult> {
    this.requireTrust("shell_exec");
    const cwd = opts?.cwd
      ? this.resolvePath(opts.cwd)
      : this.workingDir;
    const timeout = opts?.timeout ?? 30_000;

    try {
      const { stdout, stderr } = await execFileAsync(
        "/bin/sh",
        ["-c", command],
        { cwd, timeout, maxBuffer: 10 * 1024 * 1024 },
      );
      return { stdout, stderr, exitCode: 0 };
    } catch (err: unknown) {
      const e = err as { stdout?: string; stderr?: string; code?: number };
      return {
        stdout: e.stdout ?? "",
        stderr: e.stderr ?? "",
        exitCode: e.code ?? 1,
      };
    }
  }

  // ── Git ────────────────────────────────────────────────

  async gitStatus(): Promise<string> {
    this.requireTrust("git_read");
    const result = await this.exec("git status --porcelain");
    return result.stdout;
  }

  async gitDiff(ref?: string): Promise<string> {
    this.requireTrust("git_read");
    const cmd = ref ? `git diff ${ref}` : "git diff";
    const result = await this.exec(cmd);
    return result.stdout;
  }

  async gitLog(limit = 10): Promise<string> {
    this.requireTrust("git_read");
    const result = await this.exec(`git log --oneline -${limit}`);
    return result.stdout;
  }

  async gitStage(paths: string[]): Promise<void> {
    this.requireTrust("git_write");
    await this.exec(`git add ${paths.map((p) => `"${p}"`).join(" ")}`);
  }

  async gitCommit(message: string): Promise<string> {
    this.requireTrust("git_write");
    const result = await this.exec(
      `git commit -m "${message.replace(/"/g, '\\"')}"`,
    );
    return result.stdout;
  }

  // ── Internal ───────────────────────────────────────────

  private requireTrust(op: OperationType): void {
    if (!this.autonomy.check(op)) {
      throw new AutonomyError(op);
    }
  }

  /** Resolve a path relative to workingDir. Rejects traversal. */
  resolvePath(inputPath: string): string {
    const resolved = path.resolve(this.workingDir, inputPath);
    if (!resolved.startsWith(this.workingDir)) {
      throw new AutonomyError(
        "file_read",
        `Path '${inputPath}' escapes working directory`,
      );
    }
    return resolved;
  }
}
