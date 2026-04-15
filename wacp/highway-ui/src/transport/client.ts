import { createClient, type Transport } from "@connectrpc/connect";
import { createGrpcWebTransport } from "@connectrpc/connect-web";
import { HighwayService } from "../gen/highway_pb.js";

let transport: Transport | null = null;

/** Initialize the gRPC-Web transport. Call once at app startup. */
export function initTransport(baseUrl: string): void {
  transport = createGrpcWebTransport({ baseUrl });
}

/** Get the typed HighwayService client. Throws if transport not initialized. */
export function getClient() {
  if (!transport) {
    throw new Error("Transport not initialized — call initTransport() first");
  }
  return createClient(HighwayService, transport);
}

/** Get the raw transport for direct use (e.g., custom interceptors). */
export function getTransport(): Transport {
  if (!transport) {
    throw new Error("Transport not initialized — call initTransport() first");
  }
  return transport;
}
