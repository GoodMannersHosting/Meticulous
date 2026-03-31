fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile all proto files
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "../../proto/meticulous/common/v1/types.proto",
                "../../proto/meticulous/agent/v1/types.proto",
                "../../proto/meticulous/agent/v1/agent.proto",
                "../../proto/meticulous/controller/v1/controller.proto",
            ],
            &["../../proto"],
        )?;

    // Recompile if proto files change
    println!("cargo:rerun-if-changed=../../proto");

    Ok(())
}
