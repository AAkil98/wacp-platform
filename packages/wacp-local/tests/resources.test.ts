import { describe, it, expect, beforeEach } from "vitest";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as os from "node:os";

import { LocalResources } from "../src/resources.js";
import { AutonomyManager } from "../src/autonomy.js";
import { AutonomyError } from "../src/errors.js";

describe("LocalResources", () => {
  let tmpDir: string;
  let autonomy: AutonomyManager;
  let resources: LocalResources;

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "wacp-test-"));
    autonomy = new AutonomyManager("autonomous");
    resources = new LocalResources(tmpDir, autonomy);
  });

  // --- Filesystem ---

  it("readFile succeeds when trusted", async () => {
    await fs.writeFile(path.join(tmpDir, "test.txt"), "hello");
    const content = await resources.readFile("test.txt");
    expect(content).toBe("hello");
  });

  it("readFile throws AutonomyError when not trusted", async () => {
    const restricted = new LocalResources(tmpDir, new AutonomyManager("supervised"));
    await expect(restricted.readFile("test.txt")).rejects.toThrow(AutonomyError);
  });

  it("writeFile creates file when trusted", async () => {
    await resources.writeFile("output.txt", "world");
    const content = await fs.readFile(path.join(tmpDir, "output.txt"), "utf-8");
    expect(content).toBe("world");
  });

  it("writeFile creates nested directories", async () => {
    await resources.writeFile("sub/dir/file.txt", "nested");
    const content = await fs.readFile(path.join(tmpDir, "sub/dir/file.txt"), "utf-8");
    expect(content).toBe("nested");
  });

  it("writeFile throws when not trusted", async () => {
    const assisted = new LocalResources(tmpDir, new AutonomyManager("assisted"));
    await expect(assisted.writeFile("x.txt", "data")).rejects.toThrow(AutonomyError);
  });

  it("fileExists returns true for existing file", async () => {
    await fs.writeFile(path.join(tmpDir, "exists.txt"), "");
    expect(await resources.fileExists("exists.txt")).toBe(true);
  });

  it("fileExists returns false for missing file", async () => {
    expect(await resources.fileExists("nope.txt")).toBe(false);
  });

  // --- Path scoping ---

  it("rejects path traversal", () => {
    expect(() => resources.resolvePath("../../etc/passwd")).toThrow(
      "escapes working directory",
    );
  });

  it("resolves relative paths within working dir", () => {
    const resolved = resources.resolvePath("src/main.ts");
    expect(resolved).toBe(path.join(tmpDir, "src/main.ts"));
  });

  // --- Shell ---

  it("exec runs command and returns output", async () => {
    const result = await resources.exec("echo hello");
    expect(result.stdout.trim()).toBe("hello");
    expect(result.exitCode).toBe(0);
  });

  it("exec returns non-zero exit code on failure", async () => {
    const result = await resources.exec("exit 42");
    expect(result.exitCode).toBe(42);
  });

  it("exec throws when not trusted", async () => {
    const restricted = new LocalResources(tmpDir, new AutonomyManager("supervised"));
    await expect(restricted.exec("echo hi")).rejects.toThrow(AutonomyError);
  });

  // --- Path edge cases ---

  it("resolves dot path within working dir", () => {
    const resolved = resources.resolvePath("./src/main.ts");
    expect(resolved).toBe(path.join(tmpDir, "src/main.ts"));
  });

  it("rejects double-dot traversal in subdirectory", () => {
    expect(() => resources.resolvePath("src/../../etc/passwd")).toThrow(
      "escapes working directory",
    );
  });

  // --- deleteFile ---

  it("deleteFile removes existing file when explicitly granted", async () => {
    autonomy.grant("file_delete"); // explicitly grant destructive op
    await fs.writeFile(path.join(tmpDir, "to-delete.txt"), "bye");
    await resources.deleteFile("to-delete.txt");
    await expect(fs.access(path.join(tmpDir, "to-delete.txt"))).rejects.toThrow();
  });

  it("deleteFile throws when not trusted", async () => {
    const assisted = new LocalResources(tmpDir, new AutonomyManager("assisted"));
    await expect(assisted.deleteFile("x.txt")).rejects.toThrow(AutonomyError);
  });

  // --- Git operations ---

  it("gitStatus throws when not trusted", async () => {
    const supervised = new LocalResources(tmpDir, new AutonomyManager("supervised"));
    await expect(supervised.gitStatus()).rejects.toThrow(AutonomyError);
  });

  it("gitStage throws when not trusted", async () => {
    const assisted = new LocalResources(tmpDir, new AutonomyManager("assisted"));
    await expect(assisted.gitStage(["file.txt"])).rejects.toThrow(AutonomyError);
  });
});
