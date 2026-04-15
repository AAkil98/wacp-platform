/**
 * WebSocket hook for real-time session events.
 * Connects to /api/sessions/:id/ws, parses JSON frames, dispatches to store.
 * Exponential backoff reconnection: 100ms → 5s cap.
 *
 * Spec: wcon-ui §11.2, wcon-sessions §8.3
 */

import { useEffect, useRef, useCallback } from "react";
import { useSessionStore } from "../store/session";

interface StreamOptions {
  sessionId: string;
  enabled?: boolean;
}

export function useSessionStream({ sessionId, enabled = true }: StreamOptions) {
  const wsRef = useRef<WebSocket | null>(null);
  const retryCountRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const { addTrailEntry, addGate, addEscalation, addRefusal, setWorkspaceState, setSessionEvent } =
    useSessionStore();

  const connect = useCallback(() => {
    if (!enabled || !sessionId) return;

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${protocol}//${window.location.host}/api/sessions/${sessionId}/ws`;

    const ws = new WebSocket(url);
    wsRef.current = ws;

    ws.onopen = () => {
      retryCountRef.current = 0;
    };

    ws.onmessage = (event) => {
      try {
        const frame = JSON.parse(event.data as string) as {
          channel: string;
          session_id: string;
          event: Record<string, unknown>;
        };

        switch (frame.channel) {
          case "trail":
            addTrailEntry(frame.event);
            break;
          case "gates":
            addGate(frame.event);
            break;
          case "escalations":
            addEscalation(frame.event);
            break;
          case "refusals":
            addRefusal(frame.event);
            break;
          case "workspaces":
            setWorkspaceState(frame.event);
            break;
          case "session":
            setSessionEvent(frame.event);
            break;
          case "notification":
            // Handled by notification system
            break;
        }
      } catch {
        // Ignore unparseable frames
      }
    };

    ws.onclose = () => {
      wsRef.current = null;
      if (enabled) {
        // Exponential backoff: 100ms, 200ms, 400ms, ... cap 5000ms
        const delay = Math.min(100 * Math.pow(2, retryCountRef.current), 5000);
        retryCountRef.current++;
        timerRef.current = setTimeout(connect, delay);
      }
    };

    ws.onerror = () => {
      ws.close();
    };
  }, [sessionId, enabled, addTrailEntry, addGate, addEscalation, addRefusal, setWorkspaceState, setSessionEvent]);

  useEffect(() => {
    connect();
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      if (wsRef.current) {
        wsRef.current.onclose = null; // prevent reconnect on intentional close
        wsRef.current.close();
      }
    };
  }, [connect]);
}
