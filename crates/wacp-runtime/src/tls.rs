use tonic::transport::server::ServerTlsConfig;
use tonic::transport::{Certificate, Identity};

use crate::config::TlsConfig;

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("failed to read certificate {path}: {source}")]
    CertRead {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to read key {path}: {source}")]
    KeyRead {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to read CA certificate {path}: {source}")]
    CaRead {
        path: String,
        source: std::io::Error,
    },
}

/// Build a tonic ServerTlsConfig from the deployment TLS configuration.
/// Both gRPC endpoints share the same certificate identity.
pub fn build_tls_config(tls: &TlsConfig) -> Result<ServerTlsConfig, TlsError> {
    let cert_pem = std::fs::read(&tls.cert_file).map_err(|e| TlsError::CertRead {
        path: tls.cert_file.clone(),
        source: e,
    })?;
    let key_pem = std::fs::read(&tls.key_file).map_err(|e| TlsError::KeyRead {
        path: tls.key_file.clone(),
        source: e,
    })?;

    let identity = Identity::from_pem(cert_pem, key_pem);
    let mut config = ServerTlsConfig::new().identity(identity);

    if !tls.client_ca_file.is_empty() {
        let ca_pem = std::fs::read(&tls.client_ca_file).map_err(|e| TlsError::CaRead {
            path: tls.client_ca_file.clone(),
            source: e,
        })?;
        let ca = Certificate::from_pem(ca_pem);
        config = config.client_ca_root(ca);
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_missing_cert() {
        let tls = TlsConfig {
            enabled: true,
            cert_file: "/nonexistent/cert.pem".into(),
            key_file: "/nonexistent/key.pem".into(),
            client_ca_file: String::new(),
            min_version: "1.2".into(),
        };
        let err = build_tls_config(&tls).unwrap_err();
        assert!(matches!(err, TlsError::CertRead { .. }));
    }

    #[test]
    fn tls_missing_key() {
        // Create a temp cert file so cert read succeeds, key read fails.
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("cert.pem");
        std::fs::write(&cert_path, b"fake-cert").unwrap();

        let tls = TlsConfig {
            enabled: true,
            cert_file: cert_path.to_string_lossy().to_string(),
            key_file: "/nonexistent/key.pem".into(),
            client_ca_file: String::new(),
            min_version: "1.2".into(),
        };
        let err = build_tls_config(&tls).unwrap_err();
        assert!(matches!(err, TlsError::KeyRead { .. }));
    }
}
