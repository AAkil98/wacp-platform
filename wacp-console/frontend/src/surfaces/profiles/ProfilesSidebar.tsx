import { Virtuoso } from "react-virtuoso";
import { ClickCard } from "../../components/ClickCard";
import { EmptyState } from "../../components/EmptyState";
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

// Virtualize only when the list exceeds this threshold. Below it, react-virtuoso's
// setup overhead (mounting the inner scroller, measuring) exceeds the gain.
// HEALTH-LOG §4 item 7 — "currently not needed" (dozen-count lists) but wire
// it in so the list scales to tenant-sized data (hundreds) without a rewrite.
const VIRTUOSO_THRESHOLD = 50;

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

function ProfileRow({
  profile,
  selected,
  onSelect,
}: {
  profile: ProfileSummary;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <ClickCard
      aria-label={`Select profile ${profile.name}`}
      style={LIST_ITEM_STYLE[selected ? "selected" : "unselected"]}
      onClick={() => onSelect(profile.id)}
    >
      <div style={{ fontWeight: 600, fontSize: 14 }}>{profile.name}</div>
      <div
        style={{
          fontSize: 12,
          color: selected ? "rgba(255,255,255,0.8)" : "var(--color-text-secondary)",
          marginTop: 2,
        }}
      >
        {profile.role_ref}
        <span style={badge}>{profile.autonomy}</span>
        <span
          style={{
            ...badge,
            background:
              profile.visibility === "shared"
                ? "var(--color-accent)"
                : "var(--color-border)",
            color: profile.visibility === "shared" ? "#fff" : "var(--color-text-secondary)",
          }}
        >
          {profile.visibility}
        </span>
      </div>
    </ClickCard>
  );
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
        {!isLoading && profiles.length === 0 && (
          <EmptyState title="No profiles found." size="compact" />
        )}
        {profiles.length > 0 &&
          (profiles.length > VIRTUOSO_THRESHOLD ? (
            <Virtuoso
              style={{ height: "100%" }}
              data={profiles}
              itemContent={(_, p) => (
                <ProfileRow
                  profile={p}
                  selected={selectedId === p.id}
                  onSelect={onSelect}
                />
              )}
            />
          ) : (
            profiles.map((p) => (
              <ProfileRow
                key={p.id}
                profile={p}
                selected={selectedId === p.id}
                onSelect={onSelect}
              />
            ))
          ))}
      </div>
    </div>
  );
}
