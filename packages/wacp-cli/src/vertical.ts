import type { LoadedVertical } from "./repl.js";
import type { Workflow, AgentProfile } from "@wacp/local";
import { SWE_VERTICAL } from "@wacp/swe";

/**
 * Backward-compat shim: load just the SWE vertical and return it in the
 * legacy {@link LoadedVertical} shape (workflows + profiles only).
 *
 * Pre-27R callers used this for SWE-only loading. New code should use
 * {@link loadEcosystem} from ./ecosystem.js to load the full multi-vertical
 * ecosystem with tool dispatch and per-vertical detectors.
 *
 * The actual SWE workflows + profiles come from `@wacp/swe` directly — there
 * is no inlined duplication. The "circular dependency" reason cited by the
 * original implementation was incorrect (verticals depend on @wacp/local, not
 * vice versa) and has been removed by Phase 27R.
 */
export function loadSweVertical(): LoadedVertical {
  return {
    workflows: SWE_VERTICAL.workflows as Workflow[],
    profiles: SWE_VERTICAL.profiles as AgentProfile[],
  };
}
