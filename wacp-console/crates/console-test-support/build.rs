fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = "../../../wacp/proto";

    tonic_build::configure()
        .build_client(false)
        .build_server(true)
        .compile_protos(
            &[
                format!("{proto_dir}/agent.proto"),
                format!("{proto_dir}/coordinator.proto"),
                format!("{proto_dir}/highway.proto"),
                format!("{proto_dir}/primitives.proto"),
                format!("{proto_dir}/taxonomy.proto"),
            ],
            &[proto_dir],
        )?;

    Ok(())
}
