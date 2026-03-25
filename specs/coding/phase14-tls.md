# Task 14.4: TLS

## Scope

Add rustls-based TLS for both gRPC endpoints via tonic's `ServerTlsConfig`. Both endpoints share a single certificate. mTLS support via `client_ca_file`. Min TLS version enforcement (1.2 or 1.3). Plaintext mode when TLS is disabled.

Changes span `wacp-transport` (server accepts optional TLS config) and `wacp-runtime` (builds TLS config from `TlsConfig`, wires it into gRPC server startup).

## Types

### `TlsError`

```rust
pub enum TlsError {
    CertRead { path: String, source: std::io::Error },
    KeyRead { path: String, source: std::io::Error },
    CaRead { path: String, source: std::io::Error },
}
```

## Functions

### `build_tls_config`

```rust
pub fn build_tls_config(tls: &TlsConfig) -> Result<ServerTlsConfig, TlsError>
```

Reads PEM cert+key, builds `Identity`, creates `ServerTlsConfig`. If `client_ca_file` is non-empty, adds client CA for mTLS.

### `GrpcServerConfig` update

```rust
pub struct GrpcServerConfig {
    pub agent_addr: SocketAddr,
    pub highway_addr: SocketAddr,
    pub tls: Option<ServerTlsConfig>,  // NEW
}
```

## Tests

| Test | Verifies |
|------|----------|
| `plaintext_no_tls` | Server starts without TLS when config.tls is None |
| `tls_config_builds` | `build_tls_config` succeeds with valid self-signed PEM cert+key |
| `tls_missing_cert` | Missing cert file returns TlsError::CertRead |
| `tls_missing_key` | Missing key file returns TlsError::KeyRead |

## Acceptance Criteria

- Both gRPC endpoints serve TLS when configured.
- Plaintext mode works when TLS disabled.
- mTLS enabled when `client_ca_file` set.
- Existing tests still pass (plaintext mode).
- `cargo clippy` clean.
