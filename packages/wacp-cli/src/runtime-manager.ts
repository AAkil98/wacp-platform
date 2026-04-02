import { spawn, type ChildProcess } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

/** Configuration for the runtime process. */
export interface RuntimeConfig {
  /** Path to the wacp-runtime binary. Default: "wacp-runtime" (on PATH). */
  binaryPath: string;
  /** Agent gRPC port. */
  agentPort: number;
  /** Highway gRPC port. */
  highwayPort: number;
  /** Coordinator gRPC port. */
  coordinatorPort: number;
  /** Data directory for trail/checkpoints. */
  dataDir: string;
  /** Working directory. */
  workingDir: string;
}

export const DEFAULT_RUNTIME_CONFIG: RuntimeConfig = {
  binaryPath: "wacp-runtime",
  agentPort: 9400,
  highwayPort: 9401,
  coordinatorPort: 9402,
  dataDir: ".wacp/data",
  workingDir: ".",
};

/** A managed Rust runtime child process. */
export class RuntimeManager {
  private process: ChildProcess | null = null;
  private _ready = false;
  readonly config: RuntimeConfig;

  constructor(config: Partial<RuntimeConfig> = {}) {
    this.config = { ...DEFAULT_RUNTIME_CONFIG, ...config };
  }

  /** Spawn the runtime and wait for it to become ready. */
  async start(timeoutMs = 10_000): Promise<void> {
    if (this.process) {
      throw new Error("Runtime already started");
    }

    const args = [
      "serve",
      "--config", "/dev/null", // use defaults, overridden by env
    ];

    const env = {
      ...process.env,
      WACP_SERVER__AGENT_LISTEN: `127.0.0.1:${this.config.agentPort}`,
      WACP_SERVER__HIGHWAY_LISTEN: `127.0.0.1:${this.config.highwayPort}`,
      WACP_SERVER__COORDINATOR_LISTEN: `127.0.0.1:${this.config.coordinatorPort}`,
      WACP_STORAGE__DATA_DIR: this.config.dataDir,
    };

    this.process = spawn(this.config.binaryPath, args, {
      env,
      cwd: this.config.workingDir,
      stdio: ["ignore", "pipe", "pipe"],
    });

    // Log stderr for debugging
    this.process.stderr?.on("data", (data: Buffer) => {
      const line = data.toString().trim();
      if (line) {
        process.stderr.write(`[runtime] ${line}\n`);
      }
    });

    this.process.on("error", (err) => {
      throw new Error(`Failed to spawn runtime: ${err.message}`);
    });

    this.process.on("exit", (code) => {
      this._ready = false;
      if (code !== null && code !== 0) {
        process.stderr.write(`[runtime] exited with code ${code}\n`);
      }
    });

    // Wait for runtime to become ready
    await this.waitForReady(timeoutMs);
    this._ready = true;
  }

  /** Stop the runtime process. */
  async stop(): Promise<void> {
    if (!this.process) return;

    this.process.kill("SIGTERM");

    // Wait for graceful shutdown (up to 5s)
    await new Promise<void>((resolve) => {
      const timeout = setTimeout(() => {
        this.process?.kill("SIGKILL");
        resolve();
      }, 5000);

      this.process?.on("exit", () => {
        clearTimeout(timeout);
        resolve();
      });
    });

    this.process = null;
    this._ready = false;
  }

  /** Whether the runtime is ready to accept connections. */
  get ready(): boolean {
    return this._ready;
  }

  /** gRPC endpoint URLs. */
  get agentUrl(): string {
    return `127.0.0.1:${this.config.agentPort}`;
  }

  get coordinatorUrl(): string {
    return `127.0.0.1:${this.config.coordinatorPort}`;
  }

  get highwayUrl(): string {
    return `127.0.0.1:${this.config.highwayPort}`;
  }

  /** Wait for the runtime to accept connections by polling the agent port. */
  private async waitForReady(timeoutMs: number): Promise<void> {
    const start = Date.now();
    const interval = 100;

    while (Date.now() - start < timeoutMs) {
      try {
        // Try connecting to the agent port
        const net = await import("node:net");
        const connected = await new Promise<boolean>((resolve) => {
          const socket = net.createConnection(
            { host: "127.0.0.1", port: this.config.agentPort },
            () => {
              socket.destroy();
              resolve(true);
            },
          );
          socket.on("error", () => resolve(false));
          socket.setTimeout(500, () => {
            socket.destroy();
            resolve(false);
          });
        });

        if (connected) return;
      } catch {
        // Connection failed — retry
      }

      await sleep(interval);
    }

    // Timeout — kill the process
    this.process?.kill("SIGKILL");
    this.process = null;
    throw new Error(
      `Runtime failed to start within ${timeoutMs}ms. ` +
      `Check that '${this.config.binaryPath}' is built and available.`,
    );
  }
}

