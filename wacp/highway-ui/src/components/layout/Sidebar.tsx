import { NavLink } from "react-router";
import { useStore } from "@/store";

const navItems = [
  { to: "/trail", label: "Trail" },
  { to: "/gates", label: "Gates" },
  { to: "/escalations", label: "Escalations" },
  { to: "/workspaces", label: "Workspaces" },
  { to: "/tasks", label: "Task Graph" },
  { to: "/inject", label: "Inject" },
  { to: "/settings", label: "Settings" },
] as const;

export function Sidebar() {
  const pendingGates = useStore((s) => s.gates.pending.size);
  const activeEscalations = useStore((s) => s.escalations.active.size);

  return (
    <nav className="w-56 bg-zinc-900 text-zinc-300 flex flex-col py-4 shrink-0">
      <div className="px-4 pb-4 mb-4 border-b border-zinc-700">
        <h1 className="text-lg font-semibold text-white">WACP Highway</h1>
      </div>
      {navItems.map(({ to, label }) => (
        <NavLink
          key={to}
          to={to}
          className={({ isActive }) =>
            `px-4 py-2 text-sm flex items-center justify-between hover:bg-zinc-800 ${
              isActive ? "bg-zinc-800 text-white font-medium" : ""
            }`
          }
        >
          {label}
          {label === "Gates" && pendingGates > 0 && (
            <span className="bg-amber-500 text-black text-xs px-1.5 rounded-full font-bold">
              {pendingGates}
            </span>
          )}
          {label === "Escalations" && activeEscalations > 0 && (
            <span className="bg-red-500 text-white text-xs px-1.5 rounded-full font-bold">
              {activeEscalations}
            </span>
          )}
        </NavLink>
      ))}
    </nav>
  );
}
