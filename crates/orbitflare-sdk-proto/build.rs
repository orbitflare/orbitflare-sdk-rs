fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(false)
        .compile_protos(&["protos/jetstream.proto"], &["protos"])?;

    tonic_prost_build::configure()
        .build_server(false)
        .compile_protos(&["protos/solana-storage.proto"], &["protos"])?;

    tonic_prost_build::configure()
        .build_server(false)
        .extern_path(
            ".solana.storage.ConfirmedBlock",
            "crate::solana_storage",
        )
        .compile_protos(&["protos/geyser.proto"], &["protos"])?;

    Ok(())
}
