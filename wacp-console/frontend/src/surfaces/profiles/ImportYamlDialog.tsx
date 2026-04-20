import { inputStyle, btnPrimary, btnSecondary } from "./ProfilesPage.styles";

interface ImportYamlDialogProps {
  yaml: string;
  setYaml: (v: string) => void;
  onImport: () => void;
  onCancel: () => void;
  isPending: boolean;
}

export function ImportYamlDialog({
  yaml,
  setYaml,
  onImport,
  onCancel,
  isPending,
}: ImportYamlDialogProps) {
  return (
    <div
      style={{
        marginBottom: 24,
        padding: 16,
        border: "1px solid var(--color-border)",
        borderRadius: 6,
        background: "var(--color-bg-secondary)",
      }}
    >
      <h3 style={{ margin: "0 0 8px" }}>Import Profile from YAML</h3>
      <textarea
        style={{ ...inputStyle, minHeight: 120, fontFamily: "monospace" }}
        placeholder="Paste YAML here..."
        value={yaml}
        onChange={(e) => setYaml(e.target.value)}
      />
      <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
        <button style={btnPrimary} onClick={onImport} disabled={isPending}>
          {isPending ? "Importing..." : "Import"}
        </button>
        <button style={btnSecondary} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}
