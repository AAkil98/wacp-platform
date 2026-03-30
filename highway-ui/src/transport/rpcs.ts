import { create } from "@bufbuild/protobuf";
import { getClient } from "./client.js";
import { useStore } from "../store/index.js";
import { GateDecision } from "../gen/primitives_pb.js";
import {
  EnvelopeSchema,
  EnvelopePriority,
  EnvelopeOrigin,
} from "../gen/primitives_pb.js";

// ── Gate response ──

export { GateDecision };

/**
 * Send a gate response (approve/reject/modify) and handle the acknowledgment.
 * Moves the gate to inFlight immediately, then to resolved on ack.
 */
export async function respondToGate(
  gateId: string,
  decision: GateDecision,
  modifications?: Uint8Array,
): Promise<{ applied: boolean }> {
  const client = getClient();
  const { markGateInFlight, resolveGate } = useStore.getState();

  markGateInFlight(gateId);

  const ack = await client.respondToGate({
    gateId,
    decision,
    modifications: modifications ?? new Uint8Array(),
    clientRequestId: crypto.randomUUID(),
  });

  resolveGate(gateId, ack.applied);
  return { applied: ack.applied };
}

// ── Escalation response ──

export type EscalationAction =
  | { type: "feedback"; envelope: { toWorkspace: string; envelopeType: string; priority: string; payload: string } }
  | { type: "abort" }
  | { type: "delegate" };

const PRIORITY_MAP: Record<string, EnvelopePriority> = {
  Normal: EnvelopePriority.NORMAL,
  Urgent: EnvelopePriority.URGENT,
  Blocking: EnvelopePriority.BLOCKING,
};

/**
 * Send an escalation response and handle the acknowledgment.
 * Moves the escalation to inFlight immediately, then to resolved on ack.
 */
export async function respondToEscalation(
  escalationId: string,
  action: EscalationAction,
): Promise<{ applied: boolean }> {
  const client = getClient();
  const { markEscalationInFlight, resolveEscalation } = useStore.getState();

  markEscalationInFlight(escalationId);

  let actionPayload: Parameters<typeof client.respondToEscalation>[0]["action"];

  switch (action.type) {
    case "feedback": {
      const envelope = create(EnvelopeSchema, {
        toWorkspace: action.envelope.toWorkspace,
        type: action.envelope.envelopeType,
        payload: new TextEncoder().encode(action.envelope.payload),
        priority: PRIORITY_MAP[action.envelope.priority] ?? EnvelopePriority.NORMAL,
        origin: EnvelopeOrigin.HUMAN,
      });
      actionPayload = { case: "feedback", value: envelope };
      break;
    }
    case "abort":
      actionPayload = { case: "abort", value: true };
      break;
    case "delegate":
      actionPayload = { case: "delegateToCoordinator", value: true };
      break;
  }

  const ack = await client.respondToEscalation({
    escalationId,
    action: actionPayload,
    clientRequestId: crypto.randomUUID(),
  });

  resolveEscalation(escalationId, ack.applied);
  return { applied: ack.applied };
}
