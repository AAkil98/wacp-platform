import { useState } from "react";
import { useVerticals, useVertical } from "../../api/hooks";

interface VerticalSummary {
  id: string;
  name: string;
  defining_constraint: string;
  load_error?: string;
  task_type_count: number;
  workflow_count: number;
  tool_count: number;
  role_count: number;
}

interface ContextFieldEntry {
  type: string;
  required: boolean;
  description: string;
  enum_values?: string[];
  default?: unknown;
}

interface ToolPolicyEntry {
  kind: string;
  description: string;
  checkpoint_type?: string;
  matching_field?: string;
  expires_after_ms?: number;
  gate_condition?: string;
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

interface QualityCriterion {
  id: string;
  name: string;
  description: string;
  weight: number;
}

interface TaskType {
  id: string;
  name: string;
  description: string;
  workflow_id: string;
  keywords: string[];
}

interface WorkflowEntry {
  id: string;
  name: string;
  description: string;
  stage_count: number;
  gated_stage_count: number;
}

interface ProfileEntry {
  role_id: string;
  autonomy: string;
}

interface ToolSummaryEntry {
  name: string;
  description: string;
}

interface VerticalDetail {
  id: string;
  name: string;
  defining_constraint: string;
  load_error?: string;
  roles: string[];
  context_schema: Record<string, ContextFieldEntry>;
  tool_policies: Record<string, ToolPolicyEntry>;
  checkpoint_types: Record<string, VerticalCheckpointType>;
  quality_criteria: QualityCriterion[];
  task_types: TaskType[];
  workflows: WorkflowEntry[];
  default_profiles: ProfileEntry[];
  tools: ToolSummaryEntry[];
}

export function VerticalsTab() {
  const { data: verticalsData, isLoading } = useVerticals();
  const [expandedId, setExpandedId] = useState<string>("");

  const verticals: VerticalSummary[] = (verticalsData as { items?: VerticalSummary[] })?.items ?? [];

  return (
    <div>
      {isLoading && <p style={{ color: "var(--color-text-muted)" }}>Loading verticals...</p>}

      {verticals.map((v) => (
        <div
          key={v.id}
          style={{
            marginBottom: 8,
            borderRadius: 6,
            border: "1px solid var(--color-border)",
            overflow: "hidden",
          }}
        >
          {/* Summary row */}
          <button
            onClick={() => setExpandedId(expandedId === v.id ? "" : v.id)}
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              width: "100%",
              padding: "12px 16px",
              background: expandedId === v.id ? "var(--color-bg-secondary)" : "transparent",
              border: "none",
              cursor: "pointer",
              color: "var(--color-text)",
              textAlign: "left",
            }}
          >
            <div>
              <div style={{ fontSize: 14, fontWeight: 600 }}>{v.name}</div>
              <div style={{ fontSize: 12, color: "var(--color-text-secondary)", marginTop: 2 }}>
                {v.defining_constraint}
              </div>
            </div>
            <div style={{ fontSize: 11, color: "var(--color-text-muted)", textAlign: "right", flexShrink: 0, marginLeft: 16 }}>
              <div>{v.role_count} roles | {v.tool_count} tools</div>
              <div>{v.workflow_count} workflows | {v.task_type_count} task types</div>
            </div>
          </button>

          {/* Expanded detail */}
          {expandedId === v.id && (
            <VerticalDetailPanel verticalId={v.id} />
          )}
        </div>
      ))}

      {!isLoading && verticals.length === 0 && (
        <p style={{ color: "var(--color-text-muted)" }}>No verticals found.</p>
      )}
    </div>
  );
}

function VerticalDetailPanel({ verticalId }: { verticalId: string }) {
  const { data: rawDetail, isLoading } = useVertical(verticalId);
  const [openSections, setOpenSections] = useState<Record<string, boolean>>({});

  const detail = rawDetail as VerticalDetail | undefined;

  const toggleSection = (key: string) => {
    setOpenSections((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  if (isLoading) {
    return <div style={{ padding: 16, color: "var(--color-text-muted)" }}>Loading detail...</div>;
  }
  if (!detail) return null;

  const contextEntries = Object.entries(detail.context_schema);
  const policyEntries = Object.entries(detail.tool_policies);
  const checkpointEntries = Object.entries(detail.checkpoint_types);

  return (
    <div style={{ padding: "0 16px 16px", background: "var(--color-bg-secondary)" }}>
      {/* Context Schema */}
      <CollapsibleSection
        title={`Context Schema (${contextEntries.length})`}
        open={!!openSections["context"]}
        onToggle={() => toggleSection("context")}
      >
        {contextEntries.length === 0 && <Muted>No context fields defined.</Muted>}
        <table style={{ width: "100%", fontSize: 12, borderCollapse: "collapse" }}>
          <thead>
            <tr style={{ color: "var(--color-text-muted)", textAlign: "left" }}>
              <th style={{ padding: "4px 8px 4px 0", fontWeight: 600 }}>Field</th>
              <th style={{ padding: "4px 8px 4px 0", fontWeight: 600 }}>Type</th>
              <th style={{ padding: "4px 8px 4px 0", fontWeight: 600 }}>Required</th>
              <th style={{ padding: "4px 8px 4px 0", fontWeight: 600 }}>Description</th>
            </tr>
          </thead>
          <tbody>
            {contextEntries.map(([name, field]) => (
              <tr key={name}>
                <td style={{ padding: "4px 8px 4px 0", fontWeight: 500 }}>{name}</td>
                <td style={{ padding: "4px 8px 4px 0", color: "var(--color-text-muted)" }}>
                  {field.type}
                  {field.enum_values && ` (${field.enum_values.join(", ")})`}
                </td>
                <td style={{ padding: "4px 8px 4px 0" }}>{field.required ? "yes" : "no"}</td>
                <td style={{ padding: "4px 8px 4px 0", color: "var(--color-text-secondary)" }}>
                  {field.description}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </CollapsibleSection>

      {/* Tool Policies */}
      <CollapsibleSection
        title={`Tool Policies (${policyEntries.length})`}
        open={!!openSections["policies"]}
        onToggle={() => toggleSection("policies")}
      >
        {policyEntries.length === 0 && <Muted>No tool policies defined.</Muted>}
        {policyEntries.map(([toolName, policy]) => (
          <div
            key={toolName}
            style={{
              padding: 8,
              marginBottom: 6,
              background: "var(--color-bg)",
              borderRadius: 4,
              border: "1px solid var(--color-border)",
            }}
          >
            <div style={{ fontSize: 13, fontWeight: 600 }}>{toolName}</div>
            <div style={{ fontSize: 12, color: "var(--color-accent)", marginBottom: 2 }}>
              {policy.kind}
            </div>
            <div style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>
              {policy.description}
            </div>
            {policy.checkpoint_type && (
              <div style={{ fontSize: 11, color: "var(--color-text-muted)", marginTop: 2 }}>
                Checkpoint: {policy.checkpoint_type}
              </div>
            )}
            {policy.gate_condition && (
              <div style={{ fontSize: 11, color: "var(--color-text-muted)", marginTop: 2 }}>
                Gate: {policy.gate_condition}
              </div>
            )}
          </div>
        ))}
      </CollapsibleSection>

      {/* Checkpoint Types */}
      <CollapsibleSection
        title={`Checkpoint Types (${checkpointEntries.length})`}
        open={!!openSections["checkpoints"]}
        onToggle={() => toggleSection("checkpoints")}
      >
        {checkpointEntries.length === 0 && <Muted>No checkpoint types defined.</Muted>}
        {checkpointEntries.map(([name, cp]) => (
          <div
            key={name}
            style={{
              padding: 8,
              marginBottom: 6,
              background: "var(--color-bg)",
              borderRadius: 4,
              border: "1px solid var(--color-border)",
            }}
          >
            <div style={{ fontSize: 13, fontWeight: 600 }}>{name}</div>
            <div style={{ fontSize: 12, color: "var(--color-text-secondary)", marginBottom: 4 }}>
              {cp.description}
            </div>
            {cp.fields.length > 0 && (
              <table style={{ width: "100%", fontSize: 11, borderCollapse: "collapse" }}>
                <thead>
                  <tr style={{ color: "var(--color-text-muted)", textAlign: "left" }}>
                    <th style={{ padding: "2px 6px 2px 0", fontWeight: 600 }}>Field</th>
                    <th style={{ padding: "2px 6px 2px 0", fontWeight: 600 }}>Type</th>
                    <th style={{ padding: "2px 6px 2px 0", fontWeight: 600 }}>Description</th>
                  </tr>
                </thead>
                <tbody>
                  {cp.fields.map((field) => (
                    <tr key={field.name}>
                      <td style={{ padding: "2px 6px 2px 0", fontWeight: 500 }}>{field.name}</td>
                      <td style={{ padding: "2px 6px 2px 0", color: "var(--color-text-muted)" }}>
                        {field.type}
                        {field.enum_values && ` (${field.enum_values.join(", ")})`}
                      </td>
                      <td style={{ padding: "2px 6px 2px 0", color: "var(--color-text-secondary)" }}>
                        {field.description}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        ))}
      </CollapsibleSection>

      {/* Quality Criteria */}
      <CollapsibleSection
        title={`Quality Criteria (${detail.quality_criteria.length})`}
        open={!!openSections["quality"]}
        onToggle={() => toggleSection("quality")}
      >
        {detail.quality_criteria.length === 0 && <Muted>No quality criteria defined.</Muted>}
        {detail.quality_criteria.map((qc) => (
          <div
            key={qc.id}
            style={{
              fontSize: 12,
              padding: "4px 0",
              borderBottom: "1px solid var(--color-border)",
              display: "flex",
              gap: 12,
            }}
          >
            <span style={{ fontWeight: 600, minWidth: 100 }}>{qc.name}</span>
            <span style={{ color: "var(--color-text-secondary)", flex: 1 }}>{qc.description}</span>
            <span style={{ color: "var(--color-text-muted)", flexShrink: 0 }}>
              weight: {qc.weight}
            </span>
          </div>
        ))}
      </CollapsibleSection>

      {/* Task Types */}
      <CollapsibleSection
        title={`Task Types (${detail.task_types.length})`}
        open={!!openSections["tasks"]}
        onToggle={() => toggleSection("tasks")}
      >
        {detail.task_types.length === 0 && <Muted>No task types defined.</Muted>}
        {detail.task_types.map((tt) => (
          <div
            key={tt.id}
            style={{
              padding: 8,
              marginBottom: 6,
              background: "var(--color-bg)",
              borderRadius: 4,
              border: "1px solid var(--color-border)",
            }}
          >
            <div style={{ fontSize: 13, fontWeight: 600 }}>{tt.name}</div>
            <div style={{ fontSize: 12, color: "var(--color-text-secondary)", marginBottom: 2 }}>
              {tt.description}
            </div>
            <div style={{ fontSize: 11, color: "var(--color-text-muted)" }}>
              Workflow: {tt.workflow_id}
            </div>
            {tt.keywords.length > 0 && (
              <div style={{ display: "flex", flexWrap: "wrap", gap: 4, marginTop: 4 }}>
                {tt.keywords.map((kw) => (
                  <span
                    key={kw}
                    style={{
                      fontSize: 10,
                      padding: "1px 6px",
                      borderRadius: 3,
                      background: "var(--color-border)",
                      color: "var(--color-text-secondary)",
                    }}
                  >
                    {kw}
                  </span>
                ))}
              </div>
            )}
          </div>
        ))}
      </CollapsibleSection>

      {/* Workflows */}
      <CollapsibleSection
        title={`Workflows (${detail.workflows.length})`}
        open={!!openSections["workflows"]}
        onToggle={() => toggleSection("workflows")}
      >
        {detail.workflows.length === 0 && <Muted>No workflows defined.</Muted>}
        {detail.workflows.map((wf) => (
          <div
            key={wf.id}
            style={{
              padding: 8,
              marginBottom: 6,
              background: "var(--color-bg)",
              borderRadius: 4,
              border: "1px solid var(--color-border)",
            }}
          >
            <div style={{ fontSize: 13, fontWeight: 600 }}>{wf.name}</div>
            <div style={{ fontSize: 12, color: "var(--color-text-secondary)", marginBottom: 2 }}>
              {wf.description}
            </div>
            <div style={{ fontSize: 11, color: "var(--color-text-muted)" }}>
              {wf.stage_count} stages | {wf.gated_stage_count} gated
            </div>
          </div>
        ))}
      </CollapsibleSection>

      {/* Default Profiles */}
      <CollapsibleSection
        title={`Default Profiles (${detail.default_profiles.length})`}
        open={!!openSections["profiles"]}
        onToggle={() => toggleSection("profiles")}
      >
        {detail.default_profiles.length === 0 && <Muted>No default profiles defined.</Muted>}
        {detail.default_profiles.map((p) => (
          <div
            key={p.role_id}
            style={{
              fontSize: 12,
              padding: "4px 0",
              borderBottom: "1px solid var(--color-border)",
            }}
          >
            <span style={{ fontWeight: 500 }}>{p.role_id}</span>
            <span style={{ color: "var(--color-text-muted)", marginLeft: 8 }}>
              autonomy: {p.autonomy}
            </span>
          </div>
        ))}
      </CollapsibleSection>

      {/* Tools */}
      <CollapsibleSection
        title={`Tools (${detail.tools.length})`}
        open={!!openSections["tools"]}
        onToggle={() => toggleSection("tools")}
      >
        {detail.tools.length === 0 && <Muted>No tools defined.</Muted>}
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
            <span style={{ color: "var(--color-text-secondary)", marginLeft: 8 }}>
              {tool.description}
            </span>
          </div>
        ))}
      </CollapsibleSection>
    </div>
  );
}

function CollapsibleSection({
  title,
  open,
  onToggle,
  children,
}: {
  title: string;
  open: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  return (
    <div style={{ marginTop: 12 }}>
      <button
        onClick={onToggle}
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
          marginBottom: 6,
        }}
      >
        <span>{open ? "-" : "+"}</span>
        {title}
      </button>
      {open && <div style={{ marginLeft: 8 }}>{children}</div>}
    </div>
  );
}

function Muted({ children }: { children: React.ReactNode }) {
  return <p style={{ color: "var(--color-text-muted)", fontSize: 12 }}>{children}</p>;
}
