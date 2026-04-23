import { useState } from "react";
import { useEnvelopeTypes, useCheckpointTypes, useVerticals, useVertical } from "../../api/hooks";
import { EmptyState } from "../../components/EmptyState";

interface EnvelopeType {
  name: string;
  description: string;
  sender_roles: string[];
  receiver_roles: string[];
}

interface CheckpointType {
  name: string;
  description: string;
  allowed_roles: string[];
}

interface VerticalSummary {
  id: string;
  name: string;
}

interface CheckpointField {
  name: string;
  type: string;
  description: string;
  enum_values?: string[];
}

interface VerticalCheckpointType {
  description: string;
  fields: CheckpointField[];
  required_by: string[];
}

export function TypesTab() {
  const { data: envelopeData, isLoading: envelopeLoading } = useEnvelopeTypes();
  const { data: checkpointData, isLoading: checkpointLoading } = useCheckpointTypes();
  const { data: verticalsData } = useVerticals();

  const envelopeTypes: EnvelopeType[] = (envelopeData as { items?: EnvelopeType[] })?.items ?? [];
  const checkpointTypes: CheckpointType[] = (checkpointData as { items?: CheckpointType[] })?.items ?? [];
  const verticals: VerticalSummary[] = (verticalsData as { items?: VerticalSummary[] })?.items ?? [];

  return (
    <div>
      {/* Envelope Types */}
      <section style={{ marginBottom: 32 }}>
        <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 12 }}>Envelope Types</h2>
        {envelopeLoading && <p style={{ color: "var(--color-text-muted)" }}>Loading...</p>}
        <div style={{ display: "grid", gap: 8 }}>
          {envelopeTypes.map((et) => (
            <div
              key={et.name}
              style={{
                padding: 12,
                background: "var(--color-bg-secondary)",
                borderRadius: 6,
                border: "1px solid var(--color-border)",
              }}
            >
              <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 4 }}>{et.name}</div>
              <div style={{ fontSize: 12, color: "var(--color-text-secondary)", marginBottom: 8 }}>
                {et.description}
              </div>
              <div style={{ display: "flex", gap: 24, fontSize: 12 }}>
                <div>
                  <span style={{ color: "var(--color-text-muted)" }}>Senders: </span>
                  {et.sender_roles.length > 0 ? et.sender_roles.join(", ") : "none"}
                </div>
                <div>
                  <span style={{ color: "var(--color-text-muted)" }}>Receivers: </span>
                  {et.receiver_roles.length > 0 ? et.receiver_roles.join(", ") : "none"}
                </div>
              </div>
            </div>
          ))}
          {!envelopeLoading && envelopeTypes.length === 0 && (
            <EmptyState title="No envelope types found." size="compact" />
          )}
        </div>
      </section>

      {/* Protocol Checkpoint Types */}
      <section style={{ marginBottom: 32 }}>
        <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 12 }}>Protocol Checkpoint Types</h2>
        {checkpointLoading && <p style={{ color: "var(--color-text-muted)" }}>Loading...</p>}
        <div style={{ display: "grid", gap: 8 }}>
          {checkpointTypes.map((ct) => (
            <div
              key={ct.name}
              style={{
                padding: 12,
                background: "var(--color-bg-secondary)",
                borderRadius: 6,
                border: "1px solid var(--color-border)",
              }}
            >
              <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 4 }}>{ct.name}</div>
              <div style={{ fontSize: 12, color: "var(--color-text-secondary)", marginBottom: 6 }}>
                {ct.description}
              </div>
              <div style={{ fontSize: 12 }}>
                <span style={{ color: "var(--color-text-muted)" }}>Allowed roles: </span>
                {ct.allowed_roles.length > 0 ? ct.allowed_roles.join(", ") : "none"}
              </div>
            </div>
          ))}
          {!checkpointLoading && checkpointTypes.length === 0 && (
            <EmptyState title="No protocol checkpoint types found." size="compact" />
          )}
        </div>
      </section>

      {/* Vertical Checkpoint Types */}
      <section>
        <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 12 }}>Vertical Checkpoint Types</h2>
        {verticals.length === 0 && (
          <EmptyState title="No verticals loaded." size="compact" />
        )}
        {verticals.map((v) => (
          <VerticalCheckpoints key={v.id} verticalId={v.id} verticalName={v.name} />
        ))}
      </section>
    </div>
  );
}

function VerticalCheckpoints({ verticalId, verticalName }: { verticalId: string; verticalName: string }) {
  const [expanded, setExpanded] = useState(false);
  const { data: verticalDetail } = useVertical(expanded ? verticalId : "");

  const checkpointTypes: Record<string, VerticalCheckpointType> =
    (verticalDetail as { checkpoint_types?: Record<string, VerticalCheckpointType> })?.checkpoint_types ?? {};
  const entries = Object.entries(checkpointTypes);

  return (
    <div style={{ marginBottom: 12 }}>
      <button
        onClick={() => setExpanded(!expanded)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          fontSize: 13,
          fontWeight: 600,
          color: "var(--color-text-secondary)",
          background: "transparent",
          border: "none",
          cursor: "pointer",
          padding: "4px 0",
        }}
      >
        <span>{expanded ? "-" : "+"}</span>
        {verticalName}
      </button>

      {expanded && (
        <div style={{ marginLeft: 16, marginTop: 8 }}>
          {entries.length === 0 && (
            <EmptyState title="No checkpoint types defined." size="compact" />
          )}
          {entries.map(([name, cp]) => (
            <div
              key={name}
              style={{
                padding: 10,
                marginBottom: 8,
                background: "var(--color-bg-secondary)",
                borderRadius: 6,
                border: "1px solid var(--color-border)",
              }}
            >
              <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 4 }}>{name}</div>
              <div style={{ fontSize: 12, color: "var(--color-text-secondary)", marginBottom: 6 }}>
                {cp.description}
              </div>
              {cp.fields.length > 0 && (
                <div>
                  <span
                    style={{
                      fontSize: 11,
                      fontWeight: 600,
                      color: "var(--color-text-muted)",
                      display: "block",
                      marginBottom: 4,
                    }}
                  >
                    Fields
                  </span>
                  <table
                    style={{
                      width: "100%",
                      fontSize: 11,
                      borderCollapse: "collapse",
                    }}
                  >
                    <thead>
                      <tr style={{ color: "var(--color-text-muted)", textAlign: "left" }}>
                        <th style={{ padding: "2px 8px 2px 0", fontWeight: 600 }}>Name</th>
                        <th style={{ padding: "2px 8px 2px 0", fontWeight: 600 }}>Type</th>
                        <th style={{ padding: "2px 8px 2px 0", fontWeight: 600 }}>Description</th>
                      </tr>
                    </thead>
                    <tbody>
                      {cp.fields.map((field) => (
                        <tr key={field.name}>
                          <td style={{ padding: "2px 8px 2px 0", fontWeight: 500 }}>{field.name}</td>
                          <td style={{ padding: "2px 8px 2px 0", color: "var(--color-text-muted)" }}>
                            {field.type}
                            {field.enum_values && ` (${field.enum_values.join(", ")})`}
                          </td>
                          <td style={{ padding: "2px 8px 2px 0", color: "var(--color-text-secondary)" }}>
                            {field.description}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
