fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = "../../proto";

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                format!("{proto_dir}/primitives.proto"),
                format!("{proto_dir}/agent.proto"),
                format!("{proto_dir}/highway.proto"),
                format!("{proto_dir}/taxonomy.proto"),
                format!("{proto_dir}/coordinator.proto"),
            ],
            &[proto_dir],
        )?;

    Ok(())
}
