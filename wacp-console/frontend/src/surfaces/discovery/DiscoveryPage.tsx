import { useState } from "react";
import { useSearch } from "../../api/hooks";
import { RolesTab } from "./RolesTab";
import { ToolsTab } from "./ToolsTab";
import { TypesTab } from "./TypesTab";
import { VerticalsTab } from "./VerticalsTab";
import { EmptyState } from "../../components/EmptyState";

const TABS = ["Roles", "Tools", "Types", "Verticals"] as const;
type Tab = (typeof TABS)[number];

const SEARCH_TYPE_LABELS: Record<string, string> = {
  roles: "Role",
  tools: "Tool",
  envelope_types: "Envelope Type",
  checkpoint_types: "Checkpoint Type",
  vertical_checkpoint_types: "Vertical Checkpoint",
  verticals: "Vertical",
  workflows: "Workflow",
  task_types: "Task Type",
  context_fields: "Context Field",
  tool_policies: "Tool Policy",
};

export function DiscoveryPage() {
  const [activeTab, setActiveTab] = useState<Tab>("Roles");
  const [searchQuery, setSearchQuery] = useState("");

  const { data: searchData, isLoading: searchLoading } = useSearch(searchQuery);

  const showSearch = searchQuery.length >= 2;

  // Collect all non-empty result groups
  const searchGroups: Array<{ type: string; hits: Array<{ id: string; name: string; description: string; match_field: string; vertical?: string; has_policy?: boolean }> }> = [];
  const resultsObj = (searchData as { results?: Record<string, unknown[]> })?.results;
  if (resultsObj) {
    for (const [key, hits] of Object.entries(resultsObj)) {
      const arr = hits as Array<{ id: string; name: string; description: string; match_field: string; vertical?: string; has_policy?: boolean }>;
      if (arr && arr.length > 0) {
        searchGroups.push({ type: key, hits: arr });
      }
    }
  }

  return (
    <div style={{ padding: 24, color: "var(--color-text)" }}>
      <h1 style={{ fontSize: 24, fontWeight: 700, marginBottom: 4 }}>Discovery Browser</h1>
      <p style={{ color: "var(--color-text-secondary)", marginBottom: 20 }}>
        Browse roles, tools, verticals, and types.
      </p>

      {/* Search */}
      <div style={{ marginBottom: 20 }}>
        <input
          type="text"
          placeholder="Search across all entities (min 2 characters)..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{
            width: "100%",
            maxWidth: 480,
            padding: "8px 12px",
            fontSize: 14,
            border: "1px solid var(--color-border)",
            borderRadius: 6,
            background: "var(--color-bg)",
            color: "var(--color-text)",
            outline: "none",
          }}
        />
      </div>

      {/* Search Results */}
      {showSearch && (
        <div
          style={{
            marginBottom: 24,
            padding: 16,
            background: "var(--color-bg-secondary)",
            borderRadius: 8,
            border: "1px solid var(--color-border)",
          }}
        >
          <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 12 }}>
            Search results for "{searchQuery}"
          </h3>
          {searchLoading && (
            <p style={{ color: "var(--color-text-muted)" }}>Searching...</p>
          )}
          {!searchLoading && searchGroups.length === 0 && (
            <EmptyState title="No results found." size="compact" />
          )}
          {searchGroups.map((group) => (
            <div key={group.type} style={{ marginBottom: 12 }}>
              <h4
                style={{
                  fontSize: 12,
                  fontWeight: 600,
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                  color: "var(--color-text-muted)",
                  marginBottom: 6,
                }}
              >
                {SEARCH_TYPE_LABELS[group.type] ?? group.type}
              </h4>
              {group.hits.map((hit) => (
                <div
                  key={hit.id}
                  style={{
                    padding: "6px 10px",
                    borderRadius: 4,
                    marginBottom: 2,
                    fontSize: 13,
                    display: "flex",
                    gap: 8,
                    alignItems: "baseline",
                  }}
                >
                  <span style={{ fontWeight: 600, color: "var(--color-accent)" }}>
                    {hit.name}
                  </span>
                  <span style={{ color: "var(--color-text-muted)", fontSize: 12 }}>
                    matched on {hit.match_field}
                  </span>
                  {hit.vertical && (
                    <span
                      style={{
                        fontSize: 11,
                        padding: "1px 6px",
                        borderRadius: 3,
                        background: "var(--color-border)",
                        color: "var(--color-text-secondary)",
                      }}
                    >
                      {hit.vertical}
                    </span>
                  )}
                </div>
              ))}
            </div>
          ))}
        </div>
      )}

      {/* Tabs */}
      <div
        style={{
          display: "flex",
          gap: 0,
          borderBottom: "2px solid var(--color-border)",
          marginBottom: 20,
        }}
      >
        {TABS.map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            style={{
              padding: "8px 20px",
              fontSize: 14,
              fontWeight: activeTab === tab ? 600 : 400,
              color: activeTab === tab ? "var(--color-accent)" : "var(--color-text-secondary)",
              background: "transparent",
              border: "none",
              borderBottom: activeTab === tab ? "2px solid var(--color-accent)" : "2px solid transparent",
              marginBottom: -2,
              cursor: "pointer",
            }}
          >
            {tab}
          </button>
        ))}
      </div>

      {/* Tab Content */}
      {activeTab === "Roles" && <RolesTab />}
      {activeTab === "Tools" && <ToolsTab />}
      {activeTab === "Types" && <TypesTab />}
      {activeTab === "Verticals" && <VerticalsTab />}
    </div>
  );
}
