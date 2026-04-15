/**
 * generate-manifests.ts — 27S.3
 *
 * Generates a `vertical.yaml` manifest for each loaded vertical by serialising
 * all non-function fields from the `*_VERTICAL` descriptor. Run via:
 *
 *   npx tsx scripts/generate-manifests.ts
 *
 * Output: ecosystem/{id}/vertical.yaml for all 7 verticals.
 * Deterministic — running twice produces byte-for-byte identical YAML.
 */
import { writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { stringify } from "yaml";
import { loadEcosystem, type LoadedVertical } from "../src/ecosystem.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(__dirname, "../../..");

const ecosystem = loadEcosystem();

for (const vertical of ecosystem.verticals) {
  const manifest = toManifest(vertical);
  const yaml = stringify(manifest, { lineWidth: 120, sortMapEntries: false });
  const outPath = resolve(REPO_ROOT, "ecosystem", vertical.id, "vertical.yaml");
  writeFileSync(outPath, yaml, "utf-8");
  console.log(`  wrote  ecosystem/${vertical.id}/vertical.yaml`);
}

console.log(`\nGenerated ${ecosystem.verticals.length} manifest(s).`);

// ---------------------------------------------------------------------------

function toManifest(v: LoadedVertical) {
  return {
    id: v.id,
    name: v.name,
    defining_constraint: v.defining_constraint,
    context_schema: v.context_schema,
    tool_policies: v.tool_policies,
    checkpoint_types: v.checkpoint_types,
    quality_criteria: v.quality_criteria,
    task_types: v.task_types,
    workflows: v.workflows.map((w) => ({
      id: w.id,
      name: w.name,
      description: w.description,
      stage_count: w.stages.length,
      gated_stage_count: w.stages.filter((s) => s.gated).length,
    })),
    profiles: v.profiles.map((p) => ({
      role_id: p.roleId,
      autonomy: p.autonomy,
    })),
    tools: v.toolDefinitions.map((t) => ({
      name: t.name,
      description: t.description,
    })),
  };
}
