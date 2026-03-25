mod agent;
pub mod config;
mod init;

pub use agent::TestAgent;
pub use config::{ConfigError, RuntimeConfig, PROTOCOL_VERSION};
pub use init::{Runtime, RuntimeError};

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "wacp-runtime",
    version = version_string(),
    about = "WACP Runtime"
)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the runtime (default)
    Serve,
    /// Parse and validate configuration, then exit
    Validate,
    /// Print the full default configuration to stdout
    Defaults,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => cmd_serve(cli.config.as_deref()),
        Command::Validate => cmd_validate(cli.config.as_deref()),
        Command::Defaults => cmd_defaults(),
    }
}

fn cmd_serve(config_path: Option<&std::path::Path>) -> ExitCode {
    let (config, source) = match RuntimeConfig::load(config_path) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("fatal: configuration error: {e}");
            return ExitCode::from(1);
        }
    };

    eprintln!(
        "wacp-runtime v{} (protocol wacp-v{}, commit {}, built {})",
        env!("CARGO_PKG_VERSION"),
        PROTOCOL_VERSION,
        env!("WACP_GIT_COMMIT"),
        env!("WACP_BUILD_TIMESTAMP"),
    );
    if let Some(path) = &source {
        eprintln!("configuration: {}", path.display());
    } else {
        eprintln!("configuration: defaults (no config file found)");
    }
    eprintln!("data directory: {}", config.storage.data_dir);

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let mut runtime = match Runtime::init(config).await {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("fatal: initialization failed: {e}");
                return exit_code_for_runtime_error(&e);
            }
        };
        runtime.run().await;
        ExitCode::SUCCESS
    })
}

fn cmd_validate(config_path: Option<&std::path::Path>) -> ExitCode {
    match RuntimeConfig::load(config_path) {
        Ok((config, _)) => {
            // Also validate taxonomy if configured.
            if !config.taxonomy.file.is_empty() {
                let path = std::path::Path::new(&config.taxonomy.file);
                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        let result = if path.extension().is_some_and(|e| e == "json") {
                            wacp_taxonomy::Taxonomy::load_json(&content, PROTOCOL_VERSION)
                        } else {
                            wacp_taxonomy::Taxonomy::load_yaml(&content, PROTOCOL_VERSION)
                        };
                        if let Err(e) = result {
                            eprintln!("taxonomy error: {e}");
                            return ExitCode::from(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("taxonomy error: failed to read {}: {e}", path.display());
                        return ExitCode::from(1);
                    }
                }
            }
            eprintln!("configuration valid");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("configuration error: {e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_defaults() -> ExitCode {
    print!("{}", RuntimeConfig::default_yaml());
    ExitCode::SUCCESS
}

fn exit_code_for_runtime_error(e: &RuntimeError) -> ExitCode {
    match e {
        RuntimeError::Taxonomy(_) => ExitCode::from(1),
        RuntimeError::Storage(_) => ExitCode::from(2),
        RuntimeError::Transport(_) => ExitCode::from(3),
        RuntimeError::Recovery(_) => ExitCode::from(2),
        RuntimeError::Io(_) => ExitCode::from(2),
    }
}

const fn version_string() -> &'static str {
    concat!(
        env!("CARGO_PKG_VERSION"),
        "\nprotocol: wacp-v0.1\nbuilt: ",
        env!("WACP_BUILD_TIMESTAMP"),
        "\ncommit: ",
        env!("WACP_GIT_COMMIT"),
    )
}

#[cfg(test)]
mod tests;
