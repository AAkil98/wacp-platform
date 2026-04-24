use std::path::Path;

use crate::descriptor::ToolDescriptor;

/// Errors during tool discovery.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("failed to read directory '{path}': {source}")]
    ReadDir {
        path: String,
        source: std::io::Error,
    },

    #[error("failed to read descriptor '{path}': {source}")]
    ReadFile {
        path: String,
        source: std::io::Error,
    },

    #[error("failed to parse descriptor '{path}': {reason}")]
    ParseFailed { path: String, reason: String },

    #[error("descriptor validation failed for '{path}': {reason}")]
    ValidationFailed { path: String, reason: String },
}

/// Result of discovering tools in a directory.
#[derive(Debug)]
pub struct DiscoveryResult {
    /// Successfully loaded descriptors.
    pub descriptors: Vec<ToolDescriptor>,
    /// Errors encountered (skipped tools).
    pub errors: Vec<DiscoveryError>,
}

/// Scan a directory for tool descriptors.
///
/// Each file matching `*.json` or `*.yaml`/`*.yml` is attempted as a `ToolDescriptor`.
/// Invalid files are collected in `errors` and skipped — they do not prevent other tools
/// from loading.
pub fn discover_descriptors(dir: &Path) -> DiscoveryResult {
    let mut result = DiscoveryResult {
        descriptors: Vec::new(),
        errors: Vec::new(),
    };

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            result.errors.push(DiscoveryError::ReadDir {
                path: dir.display().to_string(),
                source: e,
            });
            return result;
        }
    };

    // Collect + sort by file_name so descriptors land in the result vec in a
    // deterministic order. Order doesn't affect downstream correctness (callers
    // index by descriptor name into HashMap-keyed structures), but stable
    // ordering matters for log output and for tests that assert on
    // `result.descriptors[0]`. Same defensive pattern as taxonomy_parser.rs
    // and wacp-trail/src/{compaction,fs_trail,tiered}.rs (§17 + plan §5.C).
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "json" => match load_json_descriptor(&path) {
                Ok(desc) => result.descriptors.push(desc),
                Err(e) => result.errors.push(e),
            },
            "yaml" | "yml" => match load_yaml_descriptor(&path) {
                Ok(desc) => result.descriptors.push(desc),
                Err(e) => result.errors.push(e),
            },
            _ => {} // skip non-descriptor files silently
        }
    }

    result
}

fn load_json_descriptor(path: &Path) -> Result<ToolDescriptor, DiscoveryError> {
    let content = std::fs::read_to_string(path).map_err(|e| DiscoveryError::ReadFile {
        path: path.display().to_string(),
        source: e,
    })?;

    let desc: ToolDescriptor =
        serde_json::from_str(&content).map_err(|e| DiscoveryError::ParseFailed {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;

    desc.validate()
        .map_err(|e| DiscoveryError::ValidationFailed {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;

    Ok(desc)
}

fn load_yaml_descriptor(path: &Path) -> Result<ToolDescriptor, DiscoveryError> {
    let content = std::fs::read_to_string(path).map_err(|e| DiscoveryError::ReadFile {
        path: path.display().to_string(),
        source: e,
    })?;

    let desc: ToolDescriptor =
        serde_yaml_ng::from_str(&content).map_err(|e| DiscoveryError::ParseFailed {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;

    desc.validate()
        .map_err(|e| DiscoveryError::ValidationFailed {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;

    Ok(desc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_json_descriptor(dir: &Path, name: &str) {
        let content = serde_json::json!({
            "name": name,
            "version": "1.0.0",
            "description": format!("{name} tool"),
            "capabilities": [{
                "name": "run",
                "description": "Run it",
                "input_schema": {"type": "object"},
                "output_schema": {"type": "object"}
            }]
        });
        fs::write(
            dir.join(format!("{name}.json")),
            serde_json::to_string_pretty(&content).unwrap(),
        )
        .unwrap();
    }

    fn write_yaml_descriptor(dir: &Path, name: &str) {
        let content = format!(
            r#"name: {name}
version: "1.0.0"
description: "{name} tool"
capabilities:
  - name: run
    description: "Run it"
    input_schema:
      type: object
    output_schema:
      type: object
"#
        );
        fs::write(dir.join(format!("{name}.yaml")), content).unwrap();
    }

    #[test]
    fn discover_json_descriptors() {
        let dir = tempfile::tempdir().unwrap();
        write_json_descriptor(dir.path(), "tool_a");
        write_json_descriptor(dir.path(), "tool_b");

        let result = discover_descriptors(dir.path());
        assert_eq!(result.descriptors.len(), 2);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn discover_yaml_descriptors() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml_descriptor(dir.path(), "tool_c");

        let result = discover_descriptors(dir.path());
        assert_eq!(result.descriptors.len(), 1);
        assert_eq!(result.descriptors[0].name, "tool_c");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn discover_mixed_formats() {
        let dir = tempfile::tempdir().unwrap();
        write_json_descriptor(dir.path(), "json_tool");
        write_yaml_descriptor(dir.path(), "yaml_tool");

        let result = discover_descriptors(dir.path());
        assert_eq!(result.descriptors.len(), 2);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn discover_skips_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        write_json_descriptor(dir.path(), "good_tool");
        fs::write(dir.path().join("bad.json"), "not json at all").unwrap();

        let result = discover_descriptors(dir.path());
        assert_eq!(result.descriptors.len(), 1);
        assert_eq!(result.errors.len(), 1);
        assert!(matches!(
            &result.errors[0],
            DiscoveryError::ParseFailed { .. }
        ));
    }

    #[test]
    fn discover_skips_invalid_descriptor() {
        let dir = tempfile::tempdir().unwrap();
        write_json_descriptor(dir.path(), "good_tool");
        // Valid JSON but invalid descriptor (uppercase name)
        let bad = serde_json::json!({
            "name": "BAD_NAME",
            "version": "1.0.0",
            "description": "bad",
            "capabilities": [{"name": "run", "description": "r", "input_schema": {"type": "object"}, "output_schema": {"type": "object"}}]
        });
        fs::write(
            dir.path().join("bad_desc.json"),
            serde_json::to_string(&bad).unwrap(),
        )
        .unwrap();

        let result = discover_descriptors(dir.path());
        assert_eq!(result.descriptors.len(), 1);
        assert_eq!(result.errors.len(), 1);
        assert!(matches!(
            &result.errors[0],
            DiscoveryError::ValidationFailed { .. }
        ));
    }

    #[test]
    fn discover_ignores_non_descriptor_files() {
        let dir = tempfile::tempdir().unwrap();
        write_json_descriptor(dir.path(), "tool_a");
        fs::write(dir.path().join("readme.md"), "# readme").unwrap();
        fs::write(dir.path().join("notes.txt"), "notes").unwrap();

        let result = discover_descriptors(dir.path());
        assert_eq!(result.descriptors.len(), 1);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn discover_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let result = discover_descriptors(dir.path());
        assert!(result.descriptors.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn discover_nonexistent_directory() {
        let result = discover_descriptors(Path::new("/nonexistent/dir"));
        assert!(result.descriptors.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert!(matches!(&result.errors[0], DiscoveryError::ReadDir { .. }));
    }

    #[test]
    fn discover_skips_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        write_json_descriptor(dir.path(), "tool_a");
        fs::create_dir(dir.path().join("subdir")).unwrap();
        fs::write(dir.path().join("subdir/nested.json"), "{}").unwrap();

        let result = discover_descriptors(dir.path());
        assert_eq!(result.descriptors.len(), 1); // only top-level
        assert!(result.errors.is_empty());
    }

    #[test]
    fn discover_returns_descriptors_in_alphabetical_order() {
        // Regression guard for v0.1.0-gate-enforcement-plan §5.C-C.3: descriptors
        // should land in the result vec in deterministic order regardless of
        // filesystem readdir behavior. The sort-by-file_name added in P3.b
        // makes this stable for log output + test assertions.
        let dir = tempfile::tempdir().unwrap();

        // Write in non-alphabetical order; result should still be sorted.
        write_json_descriptor(dir.path(), "z_tool");
        write_json_descriptor(dir.path(), "a_tool");
        write_json_descriptor(dir.path(), "m_tool");

        let result = discover_descriptors(dir.path());
        assert_eq!(result.descriptors.len(), 3);
        let names: Vec<&str> = result.descriptors.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["a_tool", "m_tool", "z_tool"]);
    }
}
