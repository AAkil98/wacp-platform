import { useAuthStore } from "../store/auth";

export function AdminGuard({ children }: { children: React.ReactNode }) {
  const { user } = useAuthStore();

  if (user?.console_role !== "admin") {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center p-8">
          <h2 className="text-2xl font-semibold mb-2" style={{ color: "var(--color-danger)" }}>
            403 Forbidden
          </h2>
          <p style={{ color: "var(--color-text-secondary)" }}>
            You do not have permission to access this page.
          </p>
        </div>
      </div>
    );
  }

  return <>{children}</>;
}
