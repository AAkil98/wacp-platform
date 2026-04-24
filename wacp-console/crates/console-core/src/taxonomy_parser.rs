//! Protocol taxonomy YAML parser — reads taxonomy files from the configured path.
//!
//! Spec: `wcon-discovery` §2.1, §3.2

use std::path::Path;

use console_runtime::Taxonomy;
use tracing::{info, warn};

/// The protocol version the Console expects.
const PROTOCOL_VERSION: &str = "0.1";

/// Attempt to load the protocol taxonomy from YAML files under `taxonomy_path`.
///
/// Reads all `.yaml`/`.yml` files in the directory, tries to parse each as a
/// `TaxonomyDefinition`, and returns the first successful parse as a validated
/// `Taxonomy`. Returns `None` if the path doesn't exist, is empty, or all
/// files fail to parse.
pub fn load_protocol_taxonomy(taxonomy_path: &str) -> Option<Taxonomy> {
    if taxonomy_path.is_empty() {
        return None;
    }

    let path = Path::new(taxonomy_path);
    if !path.exists() {
        info!(
            path = taxonomy_path,
            "taxonomy path does not exist — skipping protocol taxonomy"
        );
        return None;
    }

    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            warn!(path = taxonomy_path, error = %e, "failed to read taxonomy directory");
            return None;
        }
    };

    // Collect + sort by file_name for deterministic first-match-wins semantics
    // when multiple .yaml/.yml files contain a `protocol_version` key. Without
    // sorting, readdir order varies by filesystem (HEALTH-LOG §17 + v0.1.0-gate-
    // enforcement-plan §5.C-C.2) — same root-cause pattern as the wacp-trail
    // compaction bug. Choose a stable winner here so operators dropping a
    // duplicate file in the taxonomy dir get reproducible behavior.
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let file_path = entry.path();
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "yaml" && ext != "yml" {
            continue;
        }

        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(e) => {
                warn!(file = %file_path.display(), error = %e, "failed to read taxonomy file");
                continue;
            }
        };

        // Check if the file contains a taxonomy root key.
        if !content.contains("protocol_version") {
            continue;
        }

        match Taxonomy::load_yaml(&content, PROTOCOL_VERSION) {
            Ok(taxonomy) => {
                info!(
                    file = %file_path.display(),
                    roles = taxonomy.resolved_roles.len(),
                    envelope_types = taxonomy.envelope_types.len(),
                    checkpoint_types = taxonomy.checkpoint_types.len(),
                    "loaded protocol taxonomy"
                );
                return Some(taxonomy);
            }
            Err(e) => {
                warn!(
                    file = %file_path.display(),
                    error = %e,
                    "failed to parse taxonomy file"
                );
            }
        }
    }

    info!(path = taxonomy_path, "no valid taxonomy files found");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_path_returns_none() {
        assert!(load_protocol_taxonomy("").is_none());
    }

    #[test]
    fn nonexistent_path_returns_none() {
        assert!(load_protocol_taxonomy("/nonexistent/path").is_none());
    }

    #[test]
    fn loads_from_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"
id: test-taxonomy
version: "1.0"
protocol_version: "0.1"
roles:
  - name: reviewer
    extends: worker
    add:
      - "review: artifact"
envelope_types: []
checkpoint_types: []
"#;
        std::fs::write(dir.path().join("taxonomy.yaml"), yaml).unwrap();

        let result = load_protocol_taxonomy(dir.path().to_str().unwrap());
        assert!(result.is_some());
        let tax = result.unwrap();
        assert!(tax.resolved_roles.contains_key("reviewer"));
    }

    #[test]
    fn first_match_wins_is_alphabetically_deterministic() {
        // Regression guard for v0.1.0-gate-enforcement-plan §5.C-C.2: when the
        // taxonomy dir has multiple files containing `protocol_version`, the
        // sort-by-file_name added in P3.a guarantees alphabetical-first wins
        // regardless of filesystem readdir order. Without the sort, behavior
        // would depend on inode-allocation order (the §17 bug class).
        let dir = tempfile::tempdir().unwrap();

        let mk = |role: &str| {
            format!(
                "id: test-taxonomy
version: \"1.0\"
protocol_version: \"0.1\"
roles:
  - name: {role}
    extends: worker
    add:
      - \"review: artifact\"
envelope_types: []
checkpoint_types: []
"
            )
        };

        // Write in non-alphabetical order; sort should still pick "a-...".
        std::fs::write(dir.path().join("z-zebra.yaml"), mk("zebra")).unwrap();
        std::fs::write(dir.path().join("a-alpha.yaml"), mk("alpha")).unwrap();
        std::fs::write(dir.path().join("m-middle.yaml"), mk("middle")).unwrap();

        let tax = load_protocol_taxonomy(dir.path().to_str().unwrap()).unwrap();
        // First file alphabetically is `a-alpha.yaml` → role `alpha`.
        assert!(
            tax.resolved_roles.contains_key("alpha"),
            "expected 'alpha' (from a-alpha.yaml, alphabetically first); got roles: {:?}",
            tax.resolved_roles.keys().collect::<Vec<_>>()
        );
        assert!(!tax.resolved_roles.contains_key("zebra"));
    }
}
