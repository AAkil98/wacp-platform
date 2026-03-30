import { useNavigate } from "react-router";
import { useStore } from "@/store";

export function ConnectionBanner() {
  const sessionState = useStore((s) => s.session.state);
  const escalationBanner = useStore((s) => s.notifications.escalationBanner);
  const dismissEscalationBanner = useStore((s) => s.dismissEscalationBanner);
  const navigate = useNavigate();

  return (
    <>
      {/* Connection status banner */}
      {sessionState !== "connected" && (
        <div
          className={`${
            sessionState === "reconnecting"
              ? "bg-amber-600"
              : sessionState === "disconnected"
                ? "bg-red-600"
                : "bg-blue-600"
          } text-white text-sm px-4 py-2 text-center`}
        >
          {sessionState === "reconnecting"
            ? "Connection lost \u2014 reconnecting\u2026"
            : sessionState === "disconnected"
              ? "Disconnected from runtime."
              : "Authenticating\u2026"}
        </div>
      )}

      {/* Escalation alert banner */}
      {escalationBanner && (
        <div className="bg-red-700 text-white text-sm px-4 py-2 flex items-center">
          <span className="flex-1">
            Agent needs help &mdash;{" "}
            <button
              onClick={() => {
                navigate(`/workspaces/${escalationBanner.workspaceId}`);
                dismissEscalationBanner();
              }}
              className="underline hover:text-red-200"
            >
              {escalationBanner.workspaceId}
            </button>{" "}
            escalated.
          </span>
          <button
            onClick={() => navigate("/escalations")}
            className="text-xs px-2 py-0.5 rounded bg-red-800 hover:bg-red-600 mr-2"
          >
            View
          </button>
          <button
            onClick={dismissEscalationBanner}
            className="text-red-300 hover:text-white text-lg leading-none"
          >
            &times;
          </button>
        </div>
      )}
    </>
  );
}
