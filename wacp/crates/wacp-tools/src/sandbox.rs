use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::handler::ToolError;

/// Isolation level for tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxLevel {
    /// In-process: same address space, no isolation.
    None,
    /// Child process: separate process, stdio IPC.
    Process,
    /// Docker container: full isolation (filesystem, network, user namespace).
    Container,
}

/// Sandbox configuration for a tool.
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    /// The isolation level.
    pub level: SandboxLevel,
    /// For container sandbox: image name, resource limits.
    pub container: Option<ContainerConfig>,
}

/// Container-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Docker image to use.
    pub image: String,
    /// Memory limit in bytes. Default: 256 MB.
    #[serde(default = "default_memory_limit")]
    pub memory_limit: u64,
    /// CPU limit in millicores. Default: 1000 (1 CPU).
    #[serde(default = "default_cpu_limit")]
    pub cpu_limit: u32,
    /// Whether the container has network access.
    #[serde(default)]
    pub network: bool,
}

fn default_memory_limit() -> u64 {
    256 * 1024 * 1024
}

fn default_cpu_limit() -> u32 {
    1000
}

/// Select the sandbox level based on tool declaration and deployer override.
pub fn select_policy(side_effects: bool, deployer_override: Option<SandboxLevel>) -> SandboxLevel {
    match deployer_override {
        Some(level) => level,
        None => {
            if side_effects {
                SandboxLevel::Process
            } else {
                SandboxLevel::None
            }
        }
    }
}

/// Execute a tool handler in a child process via stdin/stdout JSON IPC.
///
/// Protocol:
/// - Parent writes JSON `{"tool": name, "capability": cap, "args": args, "config": config}` to stdin
/// - Child writes JSON result to stdout and exits with code 0
/// - On error, child writes error message to stdout and exits with non-zero code
pub async fn execute_in_process(
    program: &str,
    tool_name: &str,
    capability_name: &str,
    args: &serde_json::Value,
    config: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, ToolError> {
    let input = serde_json::json!({
        "tool": tool_name,
        "capability": capability_name,
        "args": args,
        "config": config,
    });

    let input_bytes = serde_json::to_vec(&input)
        .map_err(|e| ToolError::internal(format!("failed to serialize input: {e}")))?;

    let mut child = tokio::process::Command::new(program)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| ToolError::internal(format!("failed to spawn process: {e}")))?;

    // Write input to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&input_bytes)
            .await
            .map_err(|e| ToolError::internal(format!("failed to write to stdin: {e}")))?;
        // Drop stdin to signal EOF
    }

    // Wait with timeout
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| {
            ToolError::timeout(format!(
                "process exceeded timeout of {}ms",
                timeout.as_millis()
            ))
        })?
        .map_err(|e| ToolError::internal(format!("failed to wait for process: {e}")))?;

    if output.status.success() {
        // Parse stdout as JSON result
        serde_json::from_slice(&output.stdout)
            .map_err(|e| ToolError::internal(format!("process output is not valid JSON: {e}")))
    } else {
        // Non-zero exit → error
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if !stderr.is_empty() {
            stderr.to_string()
        } else if !stdout.is_empty() {
            stdout.to_string()
        } else {
            format!("process exited with status: {}", output.status)
        };

        // Truncate to 4096 bytes
        let truncated = if message.len() > 4096 {
            format!("{}...", &message[..4093])
        } else {
            message
        };

        Err(ToolError::internal(truncated))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Policy selection ---

    #[test]
    fn no_side_effects_no_override_is_none() {
        assert_eq!(select_policy(false, None), SandboxLevel::None);
    }

    #[test]
    fn side_effects_no_override_is_process() {
        assert_eq!(select_policy(true, None), SandboxLevel::Process);
    }

    #[test]
    fn override_none_wins() {
        assert_eq!(
            select_policy(true, Some(SandboxLevel::None)),
            SandboxLevel::None
        );
    }

    #[test]
    fn override_process_wins() {
        assert_eq!(
            select_policy(false, Some(SandboxLevel::Process)),
            SandboxLevel::Process
        );
    }

    #[test]
    fn override_container_wins() {
        assert_eq!(
            select_policy(false, Some(SandboxLevel::Container)),
            SandboxLevel::Container
        );
    }

    // --- Sandbox level serde ---

    #[test]
    fn sandbox_level_serde_roundtrip() {
        let levels = vec![
            SandboxLevel::None,
            SandboxLevel::Process,
            SandboxLevel::Container,
        ];
        for level in levels {
            let json = serde_json::to_string(&level).unwrap();
            let parsed: SandboxLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, level);
        }
    }

    #[test]
    fn sandbox_level_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&SandboxLevel::None).unwrap(),
            r#""none""#
        );
        assert_eq!(
            serde_json::to_string(&SandboxLevel::Process).unwrap(),
            r#""process""#
        );
        assert_eq!(
            serde_json::to_string(&SandboxLevel::Container).unwrap(),
            r#""container""#
        );
    }

    // --- Process execution ---

    /// Helper: write a temp script that implements the process sandbox IPC protocol.
    fn write_tool_script(dir: &std::path::Path, name: &str, body: &str) -> String {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/bash\n{body}")).unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        path.to_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn process_reads_stdin_writes_json_stdout() {
        // Script: reads JSON from stdin, wraps it in {"echo": <input>}
        let dir = tempfile::tempdir().unwrap();
        let script = write_tool_script(
            dir.path(),
            "echo_tool.sh",
            "INPUT=$(cat)\nprintf '{\"echo\": %s}' \"$INPUT\"",
        );

        let result = execute_in_process(
            &script,
            "test_tool",
            "run",
            &serde_json::json!({"x": 1}),
            &serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        // The script wraps the input in {"echo": ...}
        assert!(result.get("echo").is_some());
    }

    #[tokio::test]
    async fn process_success_returns_valid_json() {
        // Script: ignores stdin, writes fixed JSON to stdout
        let dir = tempfile::tempdir().unwrap();
        let script = write_tool_script(
            dir.path(),
            "fixed_tool.sh",
            r#"cat > /dev/null; echo '{"result": "ok", "value": 42}'"#,
        );

        let result = execute_in_process(
            &script,
            "test_tool",
            "run",
            &serde_json::json!({}),
            &serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        assert_eq!(result["result"], "ok");
        assert_eq!(result["value"], 42);
    }

    #[tokio::test]
    async fn process_nonexistent_program() {
        let result = execute_in_process(
            "/nonexistent/program",
            "test_tool",
            "run",
            &serde_json::json!({}),
            &serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, crate::handler::ToolErrorCode::InternalError);
        assert!(err.message.contains("spawn"));
    }

    #[tokio::test]
    async fn process_timeout_kills() {
        // Script: sleeps forever (ignores stdin)
        let dir = tempfile::tempdir().unwrap();
        let script = write_tool_script(dir.path(), "slow_tool.sh", "sleep 60");

        let result = execute_in_process(
            &script,
            "test_tool",
            "run",
            &serde_json::json!({}),
            &serde_json::json!({}),
            Duration::from_millis(100),
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, crate::handler::ToolErrorCode::Timeout);
    }

    #[tokio::test]
    async fn process_nonzero_exit_returns_stderr() {
        // Script: writes error to stderr, exits 1
        let dir = tempfile::tempdir().unwrap();
        let script = write_tool_script(
            dir.path(),
            "fail_tool.sh",
            r#"cat > /dev/null; echo "something went wrong" >&2; exit 1"#,
        );

        let result = execute_in_process(
            &script,
            "test_tool",
            "run",
            &serde_json::json!({}),
            &serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, crate::handler::ToolErrorCode::InternalError);
        assert!(err.message.contains("something went wrong"));
    }

    #[tokio::test]
    async fn process_invalid_json_output() {
        // Script: writes non-JSON to stdout, exits 0
        let dir = tempfile::tempdir().unwrap();
        let script = write_tool_script(
            dir.path(),
            "bad_json_tool.sh",
            r#"cat > /dev/null; echo "not json at all""#,
        );

        let result = execute_in_process(
            &script,
            "test_tool",
            "run",
            &serde_json::json!({}),
            &serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, crate::handler::ToolErrorCode::InternalError);
        assert!(err.message.contains("not valid JSON"));
    }

    // --- Container config serde ---

    #[test]
    fn container_config_defaults() {
        let json = r#"{"image": "python:3.12"}"#;
        let config: ContainerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.image, "python:3.12");
        assert_eq!(config.memory_limit, 256 * 1024 * 1024);
        assert_eq!(config.cpu_limit, 1000);
        assert!(!config.network);
    }
}
