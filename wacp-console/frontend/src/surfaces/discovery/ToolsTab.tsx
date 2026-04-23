import { useState } from "react";
import { useTools, useTool, useVerticals } from "../../api/hooks";
import { EmptyState } from "../../components/EmptyState";

interface ToolListItem {
  name: string;
  description: string;
  vertical?: string;
  roles: string[];
  policy?: {
    kind: string;
    description: string;
    checkpoint_type?: string;
    matching_field?: string;
    expires_after_ms?: number;
    gate_condition?: string;
  };
}

interface ToolDetail {
  name: string;
  description: string;
  vertical?: string;
  roles: Array<{
    name: string;
    base_role: string;
    vertical?: string;
  }>;
  policy?: {
    kind: string;
    description: string;
    checkpoint_type?: string;
    matching_field?: string;
    expires_after_ms?: number;
    gate_condition?: string;
  };
}

export function ToolsTab() {
  const [verticalFilter, setVerticalFilter] = useState<string>("");
  const [selectedTool, setSelectedTool] = useState<string>("");

  const { data: toolsData, isLoading } = useTools({
    vertical: verticalFilter || undefined,
  });
  const { data: verticalsData } = useVerticals();
  const { data: toolDetail, isLoading: detailLoading } = useTool(selectedTool);

  const tools: ToolListItem[] = (toolsData as { items?: ToolListItem[] })?.items ?? [];
  const verticals: Array<{ id: string; name: string }> = (verticalsData as { items?: Array<{ id: string; name: string }> })?.items ?? [];

  // Group by vertical
  const byVertical: Record<string, ToolListItem[]> = {};
  for (const tool of tools) {
    const key = tool.vertical ?? "protocol";
    if (!byVertical[key]) byVertical[key] = [];
    byVertical[key].push(tool);
  }
  const verticalKeys = Object.keys(byVertical).sort();

  const detail = toolDetail as ToolDetail | undefined;

  return (
    <div style={{ display: "flex", gap: 24 }}>
      {/* List */}
      <div style={{ flex: 1, minWidth: 0 }}>
        {/* Filter */}
        <div style={{ marginBottom: 16 }}>
          <select
            value={verticalFilter}
            onChange={(e) => setVerticalFilter(e.target.value)}
            style={{
              padding: "6px 10px",
              fontSize: 13,
              border: "1px solid var(--color-border)",
              borderRadius: 4,
              background: "var(--color-bg)",
              color: "var(--color-text)",
            }}
          >
            <option value="">All verticals</option>
            {verticals.map((v) => (
              <option key={v.id} value={v.id}>{v.name}</option>
            ))}
          </select>
        </div>

        {isLoading && <p style={{ color: "var(--color-text-muted)" }}>Loading tools...</p>}

        {verticalKeys.map((vKey) => (
          <div key={vKey} style={{ marginBottom: 16 }}>
            <h3
              style={{
                fontSize: 12,
                fontWeight: 600,
                textTransform: "uppercase",
                letterSpacing: "0.05em",
                color: "var(--color-text-muted)",
                marginBottom: 8,
              }}
            >
              {vKey}
            </h3>
            {(byVertical[vKey] ?? []).map((tool) => (
              <button
                key={tool.name}
                onClick={() => setSelectedTool(tool.name)}
                style={{
                  display: "block",
                  width: "100%",
                  textAlign: "left",
                  padding: "8px 12px",
                  marginBottom: 2,
                  borderRadius: 4,
                  border: selectedTool === tool.name ? "1px solid var(--color-accent)" : "1px solid transparent",
                  background: selectedTool === tool.name ? "var(--color-bg-secondary)" : "transparent",
                  cursor: "pointer",
                  color: "var(--color-text)",
                }}
              >
                <div style={{ fontSize: 13, fontWeight: 500 }}>
                  {tool.name}
                  {tool.policy && (
                    <span style={{ color: "var(--color-accent)", marginLeft: 6, fontSize: 11 }}>
                      [P]
                    </span>
                  )}
                </div>
                <div style={{ fontSize: 11, color: "var(--color-text-muted)" }}>
                  {tool.description}
                </div>
              </button>
            ))}
          </div>
        ))}

        {!isLoading && tools.length === 0 && (
          <EmptyState title="No tools found." size="compact" />
        )}
      </div>

      {/* Detail Panel */}
      {selectedTool && (
        <div
          style={{
            width: 380,
            flexShrink: 0,
            padding: 16,
            background: "var(--color-bg-secondary)",
            borderRadius: 8,
            border: "1px solid var(--color-border)",
            alignSelf: "flex-start",
          }}
        >
          {detailLoading && <p style={{ color: "var(--color-text-muted)" }}>Loading...</p>}
          {detail && (
            <>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
                <h3 style={{ fontSize: 16, fontWeight: 600 }}>{detail.name}</h3>
                <button
                  onClick={() => setSelectedTool("")}
                  style={{
                    background: "transparent",
                    border: "none",
                    color: "var(--color-text-muted)",
                    cursor: "pointer",
                    fontSize: 16,
                  }}
                >
                  x
                </button>
              </div>

              <p style={{ fontSize: 13, color: "var(--color-text-secondary)", marginBottom: 12 }}>
                {detail.description}
              </p>

              {detail.vertical && (
                <div style={{ marginBottom: 8 }}>
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--color-text-secondary)" }}>
                    Vertical
                  </span>
                  <div style={{ fontSize: 13 }}>{detail.vertical}</div>
                </div>
              )}

              {detail.roles.length > 0 && (
                <div style={{ marginBottom: 12 }}>
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--color-text-secondary)" }}>
                    Roles ({detail.roles.length})
                  </span>
                  <div style={{ marginTop: 4 }}>
                    {detail.roles.map((role) => (
                      <div
                        key={role.name}
                        style={{
                          fontSize: 12,
                          padding: "4px 0",
                          borderBottom: "1px solid var(--color-border)",
                        }}
                      >
                        <span style={{ fontWeight: 500 }}>{role.name}</span>
                        <span style={{ color: "var(--color-text-muted)", marginLeft: 6 }}>
                          ({role.base_role})
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {detail.policy && (
                <div
                  style={{
                    marginTop: 12,
                    padding: 12,
                    background: "var(--color-bg)",
                    borderRadius: 6,
                    border: "1px solid var(--color-border)",
                  }}
                >
                  <div style={{ fontSize: 12, fontWeight: 600, color: "var(--color-accent)", marginBottom: 6 }}>
                    Policy: {detail.policy.kind}
                  </div>
                  <p style={{ fontSize: 12, color: "var(--color-text-secondary)", marginBottom: 6 }}>
                    {detail.policy.description}
                  </p>
                  {detail.policy.checkpoint_type && (
                    <PolicyField label="Checkpoint type" value={detail.policy.checkpoint_type} />
                  )}
                  {detail.policy.matching_field && (
                    <PolicyField label="Matching field" value={detail.policy.matching_field} />
                  )}
                  {detail.policy.expires_after_ms != null && (
                    <PolicyField label="Expires after" value={`${detail.policy.expires_after_ms}ms`} />
                  )}
                  {detail.policy.gate_condition && (
                    <PolicyField label="Gate condition" value={detail.policy.gate_condition} />
                  )}
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

function PolicyField({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ fontSize: 11, marginBottom: 4 }}>
      <span style={{ color: "var(--color-text-muted)" }}>{label}: </span>
      <span style={{ color: "var(--color-text)" }}>{value}</span>
    </div>
  );
}
