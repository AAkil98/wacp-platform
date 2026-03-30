use std::fs::OpenOptions;
use std::sync::Mutex;

use tracing_subscriber::EnvFilter;

use crate::config::LoggingConfig;

#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("invalid log level: {0}")]
    InvalidLevel(String),
    #[error("failed to open log file {path}: {source}")]
    FileOpen {
        path: String,
        source: std::io::Error,
    },
}

/// Initialize the global tracing subscriber. Must be called once, before any other subsystem.
pub fn init_logging(config: &LoggingConfig) -> Result<(), LoggingError> {
    let env_filter = EnvFilter::try_new(&config.level)
        .map_err(|e| LoggingError::InvalidLevel(e.to_string()))?;

    let builder = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false);

    match (config.format.as_str(), config.output.as_str()) {
        ("json", "stderr") => {
            builder.json().with_writer(std::io::stderr).init();
        }
        ("json", "file") => {
            let file = open_log_file(&config.file)?;
            builder
                .json()
                .with_writer(Mutex::new(file))
                .init();
        }
        ("pretty", "stderr") => {
            builder.pretty().with_writer(std::io::stderr).init();
        }
        ("pretty", "file") => {
            let file = open_log_file(&config.file)?;
            builder
                .pretty()
                .with_ansi(false)
                .with_writer(Mutex::new(file))
                .init();
        }
        _ => {
            // Config validation ensures this is unreachable, but handle gracefully.
            builder.with_writer(std::io::stderr).init();
        }
    }

    Ok(())
}

fn open_log_file(path: &str) -> Result<std::fs::File, LoggingError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| LoggingError::FileOpen {
            path: path.into(),
            source: e,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logging_invalid_directive_rejects() {
        let config = LoggingConfig {
            level: "[invalid".into(),
            format: "json".into(),
            output: "stderr".into(),
            file: String::new(),
        };
        let err = init_logging(&config).unwrap_err();
        assert!(matches!(err, LoggingError::InvalidLevel(_)));
    }

    #[test]
    fn logging_file_creates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.log");
        let file = open_log_file(path.to_str().unwrap()).unwrap();
        drop(file);
        assert!(path.exists());
    }

    #[test]
    fn logging_file_nonexistent_dir_fails() {
        let err = open_log_file("/nonexistent/dir/test.log").unwrap_err();
        assert!(matches!(err, LoggingError::FileOpen { .. }));
    }
}
