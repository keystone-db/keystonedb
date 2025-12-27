fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use vendored protoc to avoid requiring system installation
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path().unwrap());

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/keystone.proto"], &["proto"])?;
    Ok(())
}
