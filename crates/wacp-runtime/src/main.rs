mod agent;
mod config;
mod init;

pub use agent::TestAgent;
pub use config::RuntimeConfig;
pub use init::{Runtime, RuntimeError};

#[tokio::main]
async fn main() {
    let config = RuntimeConfig::default();

    eprintln!("wacp-runtime v{}", env!("CARGO_PKG_VERSION"));
    eprintln!("protocol version: {}", config.protocol_version);
    eprintln!("data directory: {}", config.data_dir.display());

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
