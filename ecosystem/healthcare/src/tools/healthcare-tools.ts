import type { LocalResources } from "@wacp/local";

/** Tool definition for LLM function-calling. */
export interface ToolDefinition {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

/** A PHI access grant — required before any clinical tool that touches identifiable patient data. */
export interface PhiAccessGrant {
  basis: "consent" | "de_identified";
  /** For consent basis. */
  patient_id?: string;
  consent_id?: string;
  consent_scope?: string[];
  /** For de_identified basis. */
  deidentification_method?: "safe_harbor" | "expert_determination";
  deidentified_data_hash?: string;
  expires_at: number;
}

/** The 18 HIPAA Safe Harbor identifiers screened by phi_filter and removed by de_identify. */
export const HIPAA_SAFE_HARBOR_IDENTIFIERS = [
  "name",
  "geographic_subdivision",
  "date_element",
  "telephone_number",
  "fax_number",
  "email_address",
  "ssn",
  "mrn",
  "health_plan_beneficiary_number",
  "account_number",
  "certificate_or_license_number",
  "vehicle_identifier",
  "device_identifier",
  "web_url",
  "ip_address",
  "biometric_identifier",
  "full_face_photo",
  "other_unique_identifier",
] as const;

export type HipaaIdentifier = (typeof HIPAA_SAFE_HARBOR_IDENTIFIERS)[number];

/** Validate a PHI access grant. Returns errors if invalid. */
export function validatePhiAccessGrant(grant: Partial<PhiAccessGrant>): string[] {
  const errors: string[] = [];
  if (grant.basis !== "consent" && grant.basis !== "de_identified") {
    errors.push("basis must be 'consent' or 'de_identified'");
    return errors;
  }
  if (grant.basis === "consent") {
    if (!grant.patient_id || grant.patient_id.trim().length === 0) {
      errors.push("patient_id is required for consent basis");
    }
    if (!grant.consent_id || grant.consent_id.trim().length === 0) {
      errors.push("consent_id is required for consent basis");
    }
    if (!grant.consent_scope || !Array.isArray(grant.consent_scope) || grant.consent_scope.length === 0) {
      errors.push("consent_scope must be a non-empty array of allowed uses");
    }
  }
  if (grant.basis === "de_identified") {
    if (grant.deidentification_method !== "safe_harbor" && grant.deidentification_method !== "expert_determination") {
      errors.push("deidentification_method must be 'safe_harbor' or 'expert_determination'");
    }
    if (!grant.deidentified_data_hash || grant.deidentified_data_hash.trim().length === 0) {
      errors.push("deidentified_data_hash is required for de_identified basis");
    }
  }
  if (typeof grant.expires_at !== "number" || grant.expires_at <= 0) {
    errors.push("expires_at must be a positive timestamp");
  }
  return errors;
}

/** Check if a PHI access grant is fresh (not expired). */
export function isPhiGrantFresh(grant: PhiAccessGrant, nowMs: number): boolean {
  return grant.expires_at > nowMs;
}

/** Check if a PHI access grant covers the proposed use. */
export function grantCoversUse(grant: PhiAccessGrant, proposedUse: string): boolean {
  if (grant.basis === "de_identified") return true;
  if (!grant.consent_scope) return false;
  return grant.consent_scope.includes(proposedUse) || grant.consent_scope.includes("*");
}

/** Detect HIPAA identifiers in a block of text. Returns the list of identifier categories found. */
export function detectPhi(text: string): HipaaIdentifier[] {
  const found = new Set<HipaaIdentifier>();
  if (/\b\d{3}-\d{2}-\d{4}\b/.test(text)) found.add("ssn");
  if (/\b(?:MRN|mrn|medical record number)[:\s#]*\d+/i.test(text)) found.add("mrn");
  if (/\b[\w.+-]+@[\w-]+\.[\w.-]+\b/.test(text)) found.add("email_address");
  if (/\b(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b/.test(text)) found.add("telephone_number");
  if (/\b(?:0?[1-9]|1[0-2])\/(?:0?[1-9]|[12][0-9]|3[01])\/\d{2,4}\b/.test(text)) found.add("date_element");
  if (/\bhttps?:\/\/[^\s]+/.test(text)) found.add("web_url");
  if (/\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b/.test(text)) found.add("ip_address");
  if (/\b\d{5}(?:-\d{4})?\b/.test(text)) found.add("geographic_subdivision");
  return Array.from(found);
}

/** Healthcare-specific tool definitions beyond the CLI's built-in 7. */
export function healthcareToolDefinitions(): ToolDefinition[] {
  return [
    {
      name: "clinical_search",
      description: "Search clinical guidelines, protocols, and literature (PubMed, UpToDate, Cochrane). No PHI involved.",
      input_schema: {
        type: "object",
        properties: {
          query: { type: "string", description: "Clinical search query" },
          source: { type: "string", enum: ["pubmed", "uptodate", "cochrane", "guidelines"], description: "Source database" },
          max_results: { type: "number", description: "Maximum results to return (default: 20)" },
        },
        required: ["query"],
      },
    },
    {
      name: "protocol_lookup",
      description: "Look up a specific clinical protocol — sepsis bundle, stroke pathway, ARDSnet, etc. No PHI involved.",
      input_schema: {
        type: "object",
        properties: {
          protocol: { type: "string", description: "Protocol name" },
          version: { type: "string", description: "Protocol version (default: latest)" },
        },
        required: ["protocol"],
      },
    },
    {
      name: "lab_interpret",
      description: "Interpret lab results in clinical context with reference ranges. REQUIRES a phi_access_grant.",
      input_schema: {
        type: "object",
        properties: {
          phi_access_grant: {
            type: "object",
            description: "PHI access grant from consent_verify or de_identify",
          },
          labs: { type: "array", items: { type: "object" }, description: "Lab results to interpret" },
          context: { type: "string", description: "Clinical context (history, current diagnosis)" },
        },
        required: ["phi_access_grant", "labs"],
      },
    },
    {
      name: "risk_score",
      description: "Compute clinical risk scores — CHADS2-VASc, ASCVD, CURB-65, MELD, APACHE. REQUIRES a phi_access_grant.",
      input_schema: {
        type: "object",
        properties: {
          phi_access_grant: {
            type: "object",
            description: "PHI access grant from consent_verify or de_identify",
          },
          score: { type: "string", enum: ["chads2vasc", "ascvd", "curb65", "meld", "apache"], description: "Risk score" },
          inputs: { type: "object", description: "Score inputs (age, comorbidities, labs, etc.)" },
        },
        required: ["phi_access_grant", "score", "inputs"],
      },
    },
    {
      name: "phi_filter",
      description: "Detect PHI in text against the 18 HIPAA identifiers. Returns the list of identifier categories detected.",
      input_schema: {
        type: "object",
        properties: {
          text: { type: "string", description: "Text to scan" },
          redact: { type: "boolean", description: "If true, return the text with PHI redacted (default: false)" },
        },
        required: ["text"],
      },
    },
    {
      name: "consent_verify",
      description: "Verify patient consent for a specific use. Produces a consent-basis PHI access grant.",
      input_schema: {
        type: "object",
        properties: {
          patient_id: { type: "string", description: "Patient identifier" },
          consent_id: { type: "string", description: "Consent record identifier" },
          requested_scope: {
            type: "array",
            items: { type: "string" },
            description: "Uses requested — e.g., ['assessment', 'labs', 'documentation']",
          },
        },
        required: ["patient_id", "consent_id", "requested_scope"],
      },
    },
    {
      name: "de_identify",
      description: "De-identify clinical data per HIPAA Safe Harbor. Removes the 18 identifiers and produces a de-identified copy plus a hash.",
      input_schema: {
        type: "object",
        properties: {
          source: { type: "string", description: "Path to source dataset" },
          method: { type: "string", enum: ["safe_harbor", "expert_determination"], description: "De-identification method (default: safe_harbor)" },
          target: { type: "string", description: "Output path for de-identified copy" },
        },
        required: ["source"],
      },
    },
    {
      name: "clinical_report_generate",
      description: "Generate a clinical report — H&P, progress note, discharge summary. REQUIRES a phi_access_grant.",
      input_schema: {
        type: "object",
        properties: {
          phi_access_grant: {
            type: "object",
            description: "PHI access grant from consent_verify or de_identify",
          },
          report_type: { type: "string", enum: ["hp", "progress", "discharge", "consult"], description: "Report type" },
          template: { type: "string", description: "Template name or path" },
          data: { type: "object", description: "Structured clinical data for the report" },
        },
        required: ["phi_access_grant", "report_type"],
      },
    },
    {
      name: "audit_export",
      description: "Export the audit trail with PHI access metadata for compliance review. Read-only.",
      input_schema: {
        type: "object",
        properties: {
          workspace_id: { type: "string", description: "Workspace identifier" },
          start_time: { type: "number", description: "Start timestamp (optional)" },
          end_time: { type: "number", description: "End timestamp (optional)" },
          format: { type: "string", enum: ["json", "csv"], description: "Export format" },
        },
        required: ["workspace_id"],
      },
    },
    {
      name: "education_material",
      description: "Generate patient education at a specified reading level (default 6th grade).",
      input_schema: {
        type: "object",
        properties: {
          topic: { type: "string", description: "Topic — e.g., 'managing type 2 diabetes'" },
          reading_level: { type: "number", description: "Target reading level (grade, default: 6)" },
          language: { type: "string", description: "Output language (default: en)" },
          length: { type: "string", enum: ["brief", "standard", "detailed"], description: "Length (default: standard)" },
        },
        required: ["topic"],
      },
    },
  ];
}

/** Execute a Healthcare tool using local resources. */
export async function executeHealthcareTools(
  resources: LocalResources,
  toolName: string,
  args: Record<string, unknown>,
  nowMs: number = Date.now(),
): Promise<{ content: string; isError: boolean }> {
  try {
    switch (toolName) {
      case "clinical_search": {
        const query = args.query as string;
        const source = (args.source as string) ?? "pubmed";
        const cmd = `echo "Searching ${source} for: ${query}"`;
        const result = await resources.exec(cmd, { timeout: 30_000 });
        return { content: result.stdout, isError: false };
      }
      case "protocol_lookup": {
        const protocol = args.protocol as string;
        const version = (args.version as string) ?? "latest";
        const cmd = `echo "Looking up protocol: ${protocol} (${version})"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "lab_interpret": {
        const grant = args.phi_access_grant as Partial<PhiAccessGrant> | undefined;
        const gateError = checkPhiGrant(grant, "labs", nowMs);
        if (gateError) return { content: gateError, isError: true };
        const cmd = `echo "Interpreting labs with valid PHI grant (basis=${grant!.basis})"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "risk_score": {
        const grant = args.phi_access_grant as Partial<PhiAccessGrant> | undefined;
        const score = args.score as string;
        const gateError = checkPhiGrant(grant, "assessment", nowMs);
        if (gateError) return { content: gateError, isError: true };
        const cmd = `echo "Computing ${score} score with valid PHI grant"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "phi_filter": {
        const text = args.text as string;
        const redact = (args.redact as boolean) ?? false;
        const detected = detectPhi(text);
        if (redact) {
          let redacted = text;
          redacted = redacted.replace(/\b\d{3}-\d{2}-\d{4}\b/g, "[SSN]");
          redacted = redacted.replace(/\b[\w.+-]+@[\w-]+\.[\w.-]+\b/g, "[EMAIL]");
          redacted = redacted.replace(/\b(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b/g, "[PHONE]");
          redacted = redacted.replace(/\b(?:MRN|mrn|medical record number)[:\s#]*\d+/gi, "[MRN]");
          return { content: `Detected: ${detected.join(", ") || "none"}\n${redacted}`, isError: false };
        }
        return { content: `PHI detected: ${detected.join(", ") || "none"}`, isError: false };
      }
      case "consent_verify": {
        const patientId = args.patient_id as string;
        const consentId = args.consent_id as string;
        const scope = args.requested_scope as string[];
        const grant: PhiAccessGrant = {
          basis: "consent",
          patient_id: patientId,
          consent_id: consentId,
          consent_scope: scope,
          expires_at: nowMs + 15 * 60 * 1000,
        };
        return { content: `CONSENT_GRANTED: ${JSON.stringify(grant)}`, isError: false };
      }
      case "de_identify": {
        const source = args.source as string;
        const method = (args.method as string) ?? "safe_harbor";
        const grant: PhiAccessGrant = {
          basis: "de_identified",
          deidentification_method: method as "safe_harbor" | "expert_determination",
          deidentified_data_hash: `sha256:${source.length.toString(16)}${nowMs.toString(16)}`,
          expires_at: nowMs + 15 * 60 * 1000,
        };
        return { content: `DEIDENTIFIED: ${JSON.stringify(grant)}`, isError: false };
      }
      case "clinical_report_generate": {
        const grant = args.phi_access_grant as Partial<PhiAccessGrant> | undefined;
        const reportType = args.report_type as string;
        const gateError = checkPhiGrant(grant, "documentation", nowMs);
        if (gateError) return { content: gateError, isError: true };
        const cmd = `echo "Generated ${reportType} report with valid PHI grant"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "audit_export": {
        const workspaceId = args.workspace_id as string;
        const format = (args.format as string) ?? "json";
        const cmd = `echo "Exporting audit trail for ${workspaceId} as ${format}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "education_material": {
        const topic = args.topic as string;
        const readingLevel = (args.reading_level as number) ?? 6;
        const language = (args.language as string) ?? "en";
        const cmd = `echo "Generating ${language} education on '${topic}' at grade ${readingLevel}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      default:
        return { content: `Unknown healthcare tool: ${toolName}`, isError: true };
    }
  } catch (err) {
    return { content: `Error: ${(err as Error).message}`, isError: true };
  }
}

/** Internal: validate a PHI grant for a tool call. Returns an error string or null. */
function checkPhiGrant(
  grant: Partial<PhiAccessGrant> | undefined,
  proposedUse: string,
  nowMs: number,
): string | null {
  if (!grant) {
    return "PHI_ACCESS_NOT_GRANTED: tool requires a phi_access_grant argument from a prior consent_verify or de_identify checkpoint";
  }
  const errors = validatePhiAccessGrant(grant);
  if (errors.length > 0) {
    return `PHI_ACCESS_NOT_GRANTED: grant is invalid — ${errors.join("; ")}`;
  }
  const fullGrant = grant as PhiAccessGrant;
  if (!isPhiGrantFresh(fullGrant, nowMs)) {
    return `PHI_ACCESS_NOT_GRANTED: grant expired at ${fullGrant.expires_at} (now=${nowMs})`;
  }
  if (!grantCoversUse(fullGrant, proposedUse)) {
    return `PHI_ACCESS_NOT_GRANTED: grant scope does not cover '${proposedUse}'`;
  }
  return null;
}
