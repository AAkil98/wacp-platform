import { useState } from "react";
import { useRoles, useRole, useVerticals } from "../../api/hooks";
import { EmptyState } from "../../components/EmptyState";

interface RoleListItem {
  name: string;
  base_role: string;
  extends?: string;
  capabilities_added: string[];
  capabilities_removed: string[];
  tools: string[];
  vertical?: string;
}

interface RoleDetail {
  name: string;
  base_role: string;
  extends?: string;
  capabilities_added: string[];
  capabilities_removed: string[];
  vertical?: string;
  tools: Array<{ name: string; description: string; vertical?: string; policy?: unknown }>;
  envelope_types: Array<{ name: string; description: string; sender_roles: string[]; receiver_roles: string[] }>;
  checkpoint_types: Array<{ name: string; description: string; allowed_roles: string[] }>;
}

const BASE_ROLES = ["coordinator", "worker", "observer"];

export function RolesTab() {
  const [baseRoleFilter, setBaseRoleFilter] = useState<string>("");
  const [verticalFilter, setVerticalFilter] = useState<string>("");
  const [selectedRole, setSelectedRole] = useState<string>("");
  const [collapsedVerticals, setCollapsedVerticals] = useState<Record<string, boolean>>({});

  const { data: rolesData, isLoading } = useRoles({
    base_role: baseRoleFilter || undefined,
    vertical: verticalFilter || undefined,
  });
  const { data: verticalsData } = useVerticals();
  const { data: roleDetail, isLoading: detailLoading } = useRole(selectedRole);

  const roles: RoleListItem[] = (rolesData as { items?: RoleListItem[] })?.items ?? [];
  const verticals: Array<{ id: string; name: string }> = (verticalsData as { items?: Array<{ id: string; name: string }> })?.items ?? [];

  // Split into base roles and derived roles grouped by vertical
  const baseRoles = roles.filter((r) => BASE_ROLES.includes(r.name) && !r.vertical);
  const derivedRoles = roles.filter((r) => !BASE_ROLES.includes(r.name) || r.vertical);

  // Group derived by vertical
  const byVertical: Record<string, RoleListItem[]> = {};
  for (const role of derivedRoles) {
    const key = role.vertical ?? "protocol";
    if (!byVertical[key]) byVertical[key] = [];
    byVertical[key].push(role);
  }
  const verticalKeys = Object.keys(byVertical).sort();

  const toggleVertical = (key: string) => {
    setCollapsedVerticals((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  const detail = roleDetail as RoleDetail | undefined;

  return (
    <div style={{ display: "flex", gap: 24 }}>
      {/* List */}
      <div style={{ flex: 1, minWidth: 0 }}>
        {/* Filters */}
        <div style={{ display: "flex", gap: 12, marginBottom: 16 }}>
          <select
            value={baseRoleFilter}
            onChange={(e) => setBaseRoleFilter(e.target.value)}
            aria-label="Filter by base role"
            style={{
              padding: "6px 10px",
              fontSize: 13,
              border: "1px solid var(--color-border)",
              borderRadius: 4,
              background: "var(--color-bg)",
              color: "var(--color-text)",
            }}
          >
            <option value="">All base roles</option>
            {BASE_ROLES.map((br) => (
              <option key={br} value={br}>{br}</option>
            ))}
          </select>
          <select
            value={verticalFilter}
            onChange={(e) => setVerticalFilter(e.target.value)}
            aria-label="Filter by vertical"
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

        {isLoading && <p style={{ color: "var(--color-text-muted)" }}>Loading roles...</p>}

        {/* Base Roles */}
        {baseRoles.length > 0 && (
          <div style={{ marginBottom: 16 }}>
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
              Base Roles
            </h3>
            {baseRoles.map((role) => (
              <RoleRow
                key={role.name}
                role={role}
                isSelected={selectedRole === role.name}
                onSelect={() => setSelectedRole(role.name)}
              />
            ))}
          </div>
        )}

        {/* Derived Roles by Vertical */}
        {verticalKeys.map((vKey) => (
          <div key={vKey} style={{ marginBottom: 16 }}>
            <button
              onClick={() => toggleVertical(vKey)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                fontSize: 12,
                fontWeight: 600,
                textTransform: "uppercase",
                letterSpacing: "0.05em",
                color: "var(--color-text-muted)",
                background: "transparent",
                border: "none",
                cursor: "pointer",
                marginBottom: 8,
                padding: 0,
              }}
            >
              <span>{collapsedVerticals[vKey] ? "+" : "-"}</span>
              {vKey} ({(byVertical[vKey] ?? []).length})
            </button>
            {!collapsedVerticals[vKey] &&
              (byVertical[vKey] ?? []).map((role) => (
                <RoleRow
                  key={role.name}
                  role={role}
                  isSelected={selectedRole === role.name}
                  onSelect={() => setSelectedRole(role.name)}
                />
              ))}
          </div>
        ))}

        {!isLoading && roles.length === 0 && (
          <EmptyState title="No roles found." size="compact" />
        )}
      </div>

      {/* Detail Panel */}
      {selectedRole && (
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
                  onClick={() => setSelectedRole("")}
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

              <DetailField label="Base role" value={detail.base_role} />
              {detail.extends && <DetailField label="Extends" value={detail.extends} />}
              {detail.vertical && <DetailField label="Vertical" value={detail.vertical} />}

              {detail.capabilities_added.length > 0 && (
                <div style={{ marginBottom: 12 }}>
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--color-text-secondary)" }}>
                    Capabilities added
                  </span>
                  <div style={{ display: "flex", flexWrap: "wrap", gap: 4, marginTop: 4 }}>
                    {detail.capabilities_added.map((cap) => (
                      <span
                        key={cap}
                        style={{
                          fontSize: 11,
                          padding: "2px 8px",
                          borderRadius: 3,
                          background: "var(--color-border)",
                          color: "var(--color-text-secondary)",
                        }}
                      >
                        {cap}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              {detail.capabilities_removed.length > 0 && (
                <div style={{ marginBottom: 12 }}>
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--color-text-secondary)" }}>
                    Capabilities removed
                  </span>
                  <div style={{ display: "flex", flexWrap: "wrap", gap: 4, marginTop: 4 }}>
                    {detail.capabilities_removed.map((cap) => (
                      <span
                        key={cap}
                        style={{
                          fontSize: 11,
                          padding: "2px 8px",
                          borderRadius: 3,
                          background: "var(--color-border)",
                          color: "var(--color-text-secondary)",
                        }}
                      >
                        {cap}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              {detail.tools.length > 0 && (
                <div style={{ marginBottom: 12 }}>
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--color-text-secondary)" }}>
                    Tools ({detail.tools.length})
                  </span>
                  <div style={{ marginTop: 4 }}>
                    {detail.tools.map((tool) => (
                      <div
                        key={tool.name}
                        style={{
                          fontSize: 12,
                          padding: "4px 0",
                          borderBottom: "1px solid var(--color-border)",
                        }}
                      >
                        <span style={{ fontWeight: 500 }}>{tool.name}</span>
                        {tool.policy ? (
                          <span style={{ color: "var(--color-accent)", marginLeft: 4, fontSize: 11 }}>[P]</span>
                        ) : null}
                        <div style={{ color: "var(--color-text-muted)", fontSize: 11 }}>
                          {tool.description}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {detail.envelope_types.length > 0 && (
                <div style={{ marginBottom: 12 }}>
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--color-text-secondary)" }}>
                    Envelope Types ({detail.envelope_types.length})
                  </span>
                  <div style={{ marginTop: 4 }}>
                    {detail.envelope_types.map((et) => (
                      <div key={et.name} style={{ fontSize: 12, padding: "4px 0" }}>
                        {et.name}
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {detail.checkpoint_types.length > 0 && (
                <div style={{ marginBottom: 12 }}>
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--color-text-secondary)" }}>
                    Checkpoint Types ({detail.checkpoint_types.length})
                  </span>
                  <div style={{ marginTop: 4 }}>
                    {detail.checkpoint_types.map((ct) => (
                      <div key={ct.name} style={{ fontSize: 12, padding: "4px 0" }}>
                        {ct.name}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

function RoleRow({
  role,
  isSelected,
  onSelect,
}: {
  role: RoleListItem;
  isSelected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      onClick={onSelect}
      style={{
        display: "block",
        width: "100%",
        textAlign: "left",
        padding: "8px 12px",
        marginBottom: 2,
        borderRadius: 4,
        border: isSelected ? "1px solid var(--color-accent)" : "1px solid transparent",
        background: isSelected ? "var(--color-bg-secondary)" : "transparent",
        cursor: "pointer",
        color: "var(--color-text)",
      }}
    >
      <div style={{ fontSize: 13, fontWeight: 500 }}>{role.name}</div>
      <div style={{ fontSize: 11, color: "var(--color-text-muted)" }}>
        extends {role.extends ?? role.base_role}
        {role.capabilities_added.length > 0 && ` | +${role.capabilities_added.length} caps`}
      </div>
    </button>
  );
}

function DetailField({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ marginBottom: 8 }}>
      <span style={{ fontSize: 12, fontWeight: 600, color: "var(--color-text-secondary)" }}>
        {label}
      </span>
      <div style={{ fontSize: 13 }}>{value}</div>
    </div>
  );
}
