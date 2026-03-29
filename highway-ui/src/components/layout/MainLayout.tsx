import { Outlet } from "react-router";
import { Sidebar } from "./Sidebar.js";
import { ConnectionBanner } from "./ConnectionBanner.js";

export function MainLayout() {
  return (
    <div className="flex h-screen bg-zinc-950 text-zinc-100">
      <Sidebar />
      <div className="flex flex-col flex-1 min-w-0">
        <ConnectionBanner />
        <main className="flex-1 overflow-auto p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
