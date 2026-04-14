import { NavLink } from "react-router";
import { useAuthStore } from "../store/auth";
import { useUiStore } from "../store/ui";
import {
  Compass, FolderOpen, PlayCircle, Settings, Users, FileText,
  ChevronLeft, ChevronRight, LogOut, Moon, Sun, Monitor,
} from "lucide-react";

const navItems = [
  { to: "/discovery", label: "Discovery", icon: Compass },
  { to: "/profiles", label: "Profiles", icon: FolderOpen },
  { to: "/sessions", label: "Sessions", icon: PlayCircle },
  { to: "/settings", label: "Settings", icon: Settings },
];

const adminItems = [
  { to: "/admin/users", label: "Users", icon: Users },
  { to: "/admin/audit", label: "Audit Log", icon: FileText },
];

export function Sidebar() {
  const { user, logout } = useAuthStore();
  const { sidebarOpen, toggleSidebar, theme, setTheme } = useUiStore();
  const isAdmin = user?.console_role === "admin";

  return (
    <aside
      className={`flex flex-col border-r transition-all duration-200 ${
        sidebarOpen ? "w-56" : "w-14"
      }`}
      style={{
        backgroundColor: "var(--color-sidebar-bg)",
        borderColor: "var(--color-border)",
      }}
    >
      {/* Header */}
      <div className="flex items-center justify-between p-3 border-b" style={{ borderColor: "var(--color-border)" }}>
        {sidebarOpen && <span className="font-semibold text-sm">WACP Console</span>}
        <button onClick={toggleSidebar} className="p-1 rounded hover:opacity-70">
          {sidebarOpen ? <ChevronLeft size={16} /> : <ChevronRight size={16} />}
        </button>
      </div>

      {/* Navigation */}
      <nav className="flex-1 py-2">
        {navItems.map((item) => (
          <SidebarLink key={item.to} {...item} collapsed={!sidebarOpen} />
        ))}

        {isAdmin && (
          <>
            <div className="mx-3 my-2 border-t" style={{ borderColor: "var(--color-border)" }} />
            {sidebarOpen && (
              <span className="px-3 text-xs uppercase tracking-wider" style={{ color: "var(--color-text-muted)" }}>
                Admin
              </span>
            )}
            {adminItems.map((item) => (
              <SidebarLink key={item.to} {...item} collapsed={!sidebarOpen} />
            ))}
          </>
        )}
      </nav>

      {/* Footer */}
      <div className="border-t p-2 space-y-1" style={{ borderColor: "var(--color-border)" }}>
        {/* Theme toggle */}
        <div className="flex justify-center gap-1">
          {([["light", Sun], ["dark", Moon], ["system", Monitor]] as const).map(([t, Icon]) => (
            <button
              key={t}
              onClick={() => setTheme(t)}
              className={`p-1.5 rounded ${theme === t ? "opacity-100" : "opacity-40 hover:opacity-70"}`}
              title={t}
            >
              <Icon size={14} />
            </button>
          ))}
        </div>

        {/* User + logout */}
        {user && sidebarOpen && (
          <div className="flex items-center justify-between px-2 py-1 text-xs" style={{ color: "var(--color-text-secondary)" }}>
            <span className="truncate">{user.username}</span>
            <button onClick={() => void logout()} className="p-1 hover:opacity-70" title="Logout">
              <LogOut size={14} />
            </button>
          </div>
        )}
      </div>
    </aside>
  );
}

function SidebarLink({
  to,
  label,
  icon: Icon,
  collapsed,
}: {
  to: string;
  label: string;
  icon: React.ComponentType<{ size?: number }>;
  collapsed: boolean;
}) {
  return (
    <NavLink
      to={to}
      className={({ isActive }) =>
        `flex items-center gap-2 px-3 py-2 mx-1 rounded text-sm transition-colors ${
          isActive ? "font-medium" : "hover:opacity-80"
        }`
      }
      style={({ isActive }) => ({
        backgroundColor: isActive ? "var(--color-sidebar-active)" : undefined,
        color: isActive ? "var(--color-accent)" : "var(--color-text)",
      })}
      title={collapsed ? label : undefined}
    >
      <Icon size={18} />
      {!collapsed && label}
    </NavLink>
  );
}
