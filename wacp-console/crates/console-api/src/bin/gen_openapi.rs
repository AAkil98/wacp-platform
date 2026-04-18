//! Generates the OpenAPI specification YAML and prints it to stdout.
//!
//! Run: `cargo run -p console-api --bin gen-openapi > wacp-console/openapi.yaml`
//! CI:  `cargo run -p console-api --bin gen-openapi > openapi.yaml.gen && diff openapi.yaml openapi.yaml.gen`

use std::io::Write;

fn main() {
    let yaml = console_api::openapi::generate_openapi_yaml();
    std::io::stdout()
        .write_all(yaml.as_bytes())
        .expect("failed to write openapi yaml to stdout");
}
