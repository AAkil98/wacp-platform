mod agent;
pub mod config;
mod init;

pub use agent::TestAgent;
pub use config::{ConfigError, RuntimeConfig, PROTOCOL_VERSION};
pub use init::{Runtime, RuntimeError};

#[tokio::main]
async fn main() {
    let (config, source) = match RuntimeConfig::load(None) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("fatal: configuration error: {e}");
            std::process::exit(1);
        }
    };

    eprintln!("wacp-runtime v{}", env!("CARGO_PKG_VERSION"));
    eprintln!("protocol version: {}", PROTOCOL_VERSION);
    if let Some(path) = &source {
        eprintln!("configuration: {}", path.display());
    } else {
        eprintln!("configuration: defaults (no config file found)");
    }
    eprintln!("data directory: {}", config.storage.data_dir);

    let mut runtime = match Runtime::init(config).await {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("fatal: initialization failed: {e}");
            std::process::exit(1);
        }
    };

    runtime.run().await;
}

#[cfg(test)]
mod tests;
