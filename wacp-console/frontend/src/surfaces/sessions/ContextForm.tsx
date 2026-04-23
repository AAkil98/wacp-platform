import type React from "react";
import { EmptyState } from "../../components/EmptyState";

// --- Types ---

interface FieldSchema {
  field_type: string;
  required: boolean;
  description: string;
  enum_values?: string[];
  default?: unknown;
}

interface ContextFormProps {
  schema: Record<string, FieldSchema>;
  values: Record<string, unknown>;
  onChange: (values: Record<string, unknown>) => void;
}

// --- Styles ---

const fieldWrapper: React.CSSProperties = {
  marginBottom: 16,
};

const labelStyle: React.CSSProperties = {
  display: "block",
  marginBottom: 4,
  fontSize: 13,
  fontWeight: 600,
  color: "var(--color-text-secondary)",
};

const requiredMark: React.CSSProperties = {
  color: "var(--color-danger)",
  marginLeft: 2,
};

const descriptionStyle: React.CSSProperties = {
  display: "block",
  fontSize: 12,
  color: "var(--color-text-secondary)",
  marginBottom: 4,
};

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "8px 10px",
  border: "1px solid var(--color-border)",
  borderRadius: 4,
  background: "var(--color-bg)",
  color: "var(--color-text)",
  fontSize: 14,
};

const checkboxRow: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 8,
  paddingTop: 4,
};

// --- Component ---

export function ContextForm({ schema, values, onChange }: ContextFormProps) {
  const keys = Object.keys(schema);

  function handleChange(key: string, value: unknown) {
    onChange({ ...values, [key]: value });
  }

  if (keys.length === 0) {
    return <EmptyState title="No context fields required for this vertical." size="compact" />;
  }

  return (
    <div>
      {keys.map((key) => {
        const field = schema[key];
        if (!field) return null;

        const currentValue = values[key];

        return (
          <div key={key} style={fieldWrapper}>
            {field.field_type === "boolean" ? (
              <div style={checkboxRow}>
                <input
                  type="checkbox"
                  id={`ctx-${key}`}
                  checked={Boolean(currentValue)}
                  onChange={(e) => handleChange(key, e.target.checked)}
                />
                <label htmlFor={`ctx-${key}`} style={{ ...labelStyle, marginBottom: 0 }}>
                  {key}
                  {field.required && <span style={requiredMark}>*</span>}
                </label>
              </div>
            ) : (
              <label style={labelStyle} htmlFor={`ctx-${key}`}>
                {key}
                {field.required && <span style={requiredMark}>*</span>}
              </label>
            )}

            {field.description && (
              <span style={descriptionStyle}>{field.description}</span>
            )}

            {field.field_type === "string" && !field.enum_values && (
              <input
                id={`ctx-${key}`}
                type="text"
                style={inputStyle}
                value={typeof currentValue === "string" ? currentValue : ""}
                onChange={(e) => handleChange(key, e.target.value)}
              />
            )}

            {field.field_type === "string" && field.enum_values && (
              <select
                id={`ctx-${key}`}
                style={inputStyle}
                value={typeof currentValue === "string" ? currentValue : ""}
                onChange={(e) => handleChange(key, e.target.value)}
              >
                <option value="">-- Select --</option>
                {field.enum_values.map((v) => (
                  <option key={v} value={v}>{v}</option>
                ))}
              </select>
            )}

            {field.field_type === "number" && (
              <input
                id={`ctx-${key}`}
                type="number"
                style={inputStyle}
                value={typeof currentValue === "number" ? currentValue : ""}
                onChange={(e) => {
                  const parsed = parseFloat(e.target.value);
                  handleChange(key, Number.isNaN(parsed) ? "" : parsed);
                }}
              />
            )}

            {/* boolean is rendered above as checkbox */}
          </div>
        );
      })}
    </div>
  );
}
