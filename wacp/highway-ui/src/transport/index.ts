export { initTransport, getClient, getTransport } from "./client.js";
export { classifyError, errorMessage, type ErrorCategory } from "./errors.js";
export { startTrailStream, startWorkspaceStream, fetchWorkspace, fetchTaskGraph } from "./streams.js";
export { SessionManager, sessionManager } from "./session.js";
export { injectEnvelope, respondToGate, respondToEscalation, GateDecision, type InjectRequest, type EscalationAction } from "./rpcs.js";
