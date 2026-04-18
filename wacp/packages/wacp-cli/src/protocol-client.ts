import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import * as path from "node:path";

// ---------------------------------------------------------------------------
// Proto loading — loads .proto files at runtime (no codegen needed)
// ---------------------------------------------------------------------------

const PROTO_DIR = path.resolve(import.meta.dirname ?? ".", "../../..", "proto");

const PROTO_LOADER_OPTIONS: protoLoader.Options = {
  keepCase: false, // convert snake_case to camelCase
  longs: Number,
  enums: Number,
  defaults: true,
  oneofs: true,
};

function loadProtoPackage(): any {
  const packageDef = protoLoader.loadSync(
    [
      path.join(PROTO_DIR, "primitives.proto"),
      path.join(PROTO_DIR, "agent.proto"),
      path.join(PROTO_DIR, "coordinator.proto"),
      path.join(PROTO_DIR, "highway.proto"),
    ],
    PROTO_LOADER_OPTIONS,
  );
  return grpc.loadPackageDefinition(packageDef);
}

// ---------------------------------------------------------------------------
// Coordinator Client
// ---------------------------------------------------------------------------

export interface GoalResult {
  goalId: string;
  rootWorkspaceId: string;
}

export interface DispatchResult {
  workspaceId: string;
  taskId: string;
}

export interface TaskView {
  taskId: string;
  name: string;
  status: number;
}

export interface IntegrationResult {
  result: string;
  detail: string;
}

export class CoordinatorClient {
  private client: any;

  constructor(url: string) {
    const pkg = loadProtoPackage();
    const ServiceClass = pkg.wacp.v1.CoordinatorService;
    this.client = new ServiceClass(url, grpc.credentials.createInsecure());
  }

  async submitGoal(description: string, context: Buffer = Buffer.alloc(0)): Promise<GoalResult> {
    return this.unaryCall("submitGoal", {
      description,
      context,
      clientRequestId: "",
    });
  }

  async decompose(tasks: ReadonlyArray<{
    name: string;
    description: string;
    dependsOn: readonly string[];
    role: string;
    directivePayload: Buffer;
    tools: readonly string[];
  }>): Promise<string[]> {
    const resp = await this.unaryCall("decompose", { tasks, clientRequestId: "" });
    return resp.taskIds;
  }

  async getReadyTasks(): Promise<TaskView[]> {
    const resp = await this.unaryCall("getReadyTasks", { clientRequestId: "" });
    return resp.tasks ?? [];
  }

  async dispatch(
    taskId: string,
    role: string,
    directivePayload: Buffer = Buffer.alloc(0),
    tools: readonly string[] = [],
  ): Promise<DispatchResult> {
    return this.unaryCall("dispatch", {
      taskId,
      role,
      directivePayload,
      tools,
      budget: null,
      clientRequestId: "",
    });
  }

  async abortWorkspace(workspaceId: string, reason: string = ""): Promise<void> {
    await this.unaryCall("abortWorkspace", { workspaceId, reason, clientRequestId: "" });
  }

  async triggerIntegration(workspaceId: string): Promise<IntegrationResult> {
    return this.unaryCall("triggerIntegration", { workspaceId, clientRequestId: "" });
  }

  async sendDirective(workspaceId: string, payload: Buffer): Promise<string> {
    const resp = await this.unaryCall("sendDirective", { workspaceId, payload, clientRequestId: "" });
    return resp.envelopeId;
  }

  close(): void {
    this.client.close();
  }

  private unaryCall(method: string, request: any): Promise<any> {
    return new Promise((resolve, reject) => {
      this.client[method](request, (err: grpc.ServiceError | null, response: any) => {
        if (err) reject(err);
        else resolve(response);
      });
    });
  }
}

// ---------------------------------------------------------------------------
// Agent Client
// ---------------------------------------------------------------------------

export interface BindResult {
  workspaceId: string;
  role: string;
  directive: any;
  context: Buffer;
  visibility: string[];
  authority: string[];
}

export interface CheckpointResult {
  checkpointId: string;
  contentHash: string;
}

export class AgentClient {
  private client: any;

  constructor(url: string) {
    const pkg = loadProtoPackage();
    const ServiceClass = pkg.wacp.v1.AgentService;
    this.client = new ServiceClass(url, grpc.credentials.createInsecure());
  }

  async bind(workspaceId: string, authToken: string): Promise<BindResult> {
    return this.unaryCall("bind", { workspaceId, authToken, clientRequestId: "" });
  }

  async emitSignal(type: number, reason: string = "", context: Buffer = Buffer.alloc(0)): Promise<void> {
    await this.unaryCall("emitSignal", { type, reason, context, clientRequestId: "" });
  }

  async createCheckpoint(
    checkpointType: string,
    payload: Buffer,
    intent: string,
    status: number = 1, // provisional
    confidence: number = 1, // high
  ): Promise<CheckpointResult> {
    return this.unaryCall("createCheckpoint", {
      type: checkpointType,
      payload,
      intent,
      status,
      confidence,
      resourceUsage: null,
      clientRequestId: "",
    });
  }

  async sendEnvelope(
    toWorkspace: string,
    type: string,
    payload: Buffer,
    priority: number = 1,
  ): Promise<string> {
    const resp = await this.unaryCall("sendEnvelope", {
      toWorkspace,
      type,
      payload,
      inReplyTo: "",
      priority,
      clientRequestId: "",
    });
    return resp.envelopeId;
  }

  close(): void {
    this.client.close();
  }

  private unaryCall(method: string, request: any): Promise<any> {
    return new Promise((resolve, reject) => {
      this.client[method](request, (err: grpc.ServiceError | null, response: any) => {
        if (err) reject(err);
        else resolve(response);
      });
    });
  }
}

// ---------------------------------------------------------------------------
// Signal type constants (matching proto enum values)
// ---------------------------------------------------------------------------

export const SignalType = {
  READY: 1,
  STARTED: 2,
  BLOCKED: 3,
  CHECKPOINT: 4,
  COMPLETE: 5,
  FAILED: 6,
  INTEGRATE: 7,
  ACKNOWLEDGED: 8,
  ESCALATION: 9,
} as const;

export const CheckpointStatus = {
  PROVISIONAL: 1,
  FINAL: 2,
} as const;
