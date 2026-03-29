import { useStore } from "@/store";

export function ConnectionBanner() {
  const sessionState = useStore((s) => s.session.state);

  if (sessionState === "connected") return null;

  const messages: Record<string, string> = {
    reconnecting: "Connection lost \u2014 reconnecting\u2026",
    disconnected: "Disconnected from runtime.",
    authenticating: "Authenticating\u2026",
  };

  const colors: Record<string, string> = {
    reconnecting: "bg-amber-600",
    disconnected: "bg-red-600",
    authenticating: "bg-blue-600",
  };

  return (
    <div
      className={`${colors[sessionState] ?? "bg-zinc-700"} text-white text-sm px-4 py-2 text-center`}
    >
      {messages[sessionState] ?? sessionState}
    </div>
  );
}
