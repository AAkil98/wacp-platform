//! Generates the OpenAPI specification YAML file.
//!
//! Usage: `cargo run --bin gen-openapi`
//! CI gate: `cargo run --bin gen-openapi && git diff --exit-code openapi.yaml`

fn main() {
    let yaml = console_api::openapi::generate_openapi_yaml();
    let out_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("openapi.yaml");

    std::fs::write(&out_path, &yaml).expect("failed to write openapi.yaml");
    println!("wrote {} bytes to {}", yaml.len(), out_path.display());
}
