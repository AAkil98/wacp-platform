import type { ProfileSummary } from "./ProfilesPage.types";
import {
  sidebar,
  sidebarHeader,
  listContainer,
  inputStyle,
  btnPrimary,
  btnSecondary,
  badge,
  LIST_ITEM_STYLE,
} from "./ProfilesPage.styles";

interface ProfilesSidebarProps {
  profiles: ProfileSummary[];
  isLoading: boolean;
  search: string;
  setSearch: (v: string) => void;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onImportClick: () => void;
}

export function ProfilesSidebar({
  profiles,
  isLoading,
  search,
  setSearch,
  selectedId,
  onSelect,
  onNew,
  onImportClick,
}: ProfilesSidebarProps) {
  return (
    <div style={sidebar}>
      <div style={sidebarHeader}>
        <h2 style={{ margin: 0, fontSize: 18, fontWeight: 700 }}>Profiles</h2>
        <input
          style={inputStyle}
          placeholder="Search profiles..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <div style={{ display: "flex", gap: 8 }}>
          <button style={btnPrimary} onClick={onNew}>
            Create New
          </button>
          <button style={btnSecondary} onClick={onImportClick}>
            Import YAML
          </button>
        </div>
      </div>

      <div style={listContainer}>
        {isLoading && (
          <div style={{ padding: 16, color: "var(--color-text-secondary)" }}>Loading...</div>
        )}
        {profiles.map((p) => {
          const selected = selectedId === p.id;
          return (
            <div
              key={p.id}
              style={LIST_ITEM_STYLE[selected ? "selected" : "unselected"]}
              onClick={() => onSelect(p.id)}
            >
              <div style={{ fontWeight: 600, fontSize: 14 }}>{p.name}</div>
              <div
                style={{
                  fontSize: 12,
                  color: selected
                    ? "rgba(255,255,255,0.8)"
                    : "var(--color-text-secondary)",
                  marginTop: 2,
                }}
              >
                {p.role_ref}
                <span style={badge}>{p.autonomy}</span>
                <span
                  style={{
                    ...badge,
                    background:
                      p.visibility === "shared"
                        ? "var(--color-accent)"
                        : "var(--color-border)",
                    color: p.visibility === "shared" ? "#fff" : "var(--color-text-secondary)",
                  }}
                >
                  {p.visibility}
                </span>
              </div>
            </div>
          );
        })}
        {!isLoading && profiles.length === 0 && (
          <div style={{ padding: 16, color: "var(--color-text-secondary)" }}>
            No profiles found.
          </div>
        )}
      </div>
    </div>
  );
}
