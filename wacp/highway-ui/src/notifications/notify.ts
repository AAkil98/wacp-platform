import type { PendingGate, ActiveEscalation } from "../store/index.js";

// ── Audio notifications via AudioContext ──

let audioCtx: AudioContext | null = null;

function getAudioContext(): AudioContext | null {
  if (typeof AudioContext === "undefined") return null;
  if (!audioCtx) {
    audioCtx = new AudioContext();
  }
  return audioCtx;
}

function playTone(frequency: number, durationMs: number): void {
  const ctx = getAudioContext();
  if (!ctx) return;

  const oscillator = ctx.createOscillator();
  const gain = ctx.createGain();
  oscillator.connect(gain);
  gain.connect(ctx.destination);

  oscillator.type = "sine";
  oscillator.frequency.value = frequency;
  gain.gain.value = 0.15;

  // Fade out to avoid click
  gain.gain.setValueAtTime(0.15, ctx.currentTime);
  gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + durationMs / 1000);

  oscillator.start(ctx.currentTime);
  oscillator.stop(ctx.currentTime + durationMs / 1000);
}

// ── Browser notifications ──

function requestBrowserNotification(title: string, body: string): void {
  if (typeof Notification === "undefined") return;
  if (document.hasFocus()) return;

  if (Notification.permission === "granted") {
    new Notification(title, { body, icon: undefined });
  } else if (Notification.permission !== "denied") {
    Notification.requestPermission().then((permission) => {
      if (permission === "granted") {
        new Notification(title, { body, icon: undefined });
      }
    });
  }
}

// ── Public API ──

const GATE_TONE_HZ = 880;      // A5 — brighter, attention-grabbing
const ESCALATION_TONE_HZ = 523; // C5 — lower, more urgent feel
const TONE_DURATION_MS = 200;

/**
 * Notify the human that a new gate has arrived.
 * Plays a short audio tone and requests a browser notification if the tab is not focused.
 */
export function notifyGate(gate: PendingGate): void {
  playTone(GATE_TONE_HZ, TONE_DURATION_MS);
  requestBrowserNotification(
    "Gate awaiting response",
    `${gate.gateType} — ${gate.workspaceId || "no workspace"}`,
  );
}

/**
 * Notify the human that an escalation has arrived.
 * Plays a distinct audio tone and requests a browser notification.
 * Escalation notifications are more urgent than gate notifications.
 */
export function notifyEscalation(esc: ActiveEscalation): void {
  // Double tone for escalations — more urgent
  playTone(ESCALATION_TONE_HZ, TONE_DURATION_MS);
  setTimeout(() => playTone(ESCALATION_TONE_HZ, TONE_DURATION_MS), 250);

  requestBrowserNotification(
    "Agent needs help",
    `Workspace ${esc.workspaceId} escalated: ${esc.context.slice(0, 100)}`,
  );
}
