use orbitflare_sdk::proto::geyser::subscribe_update::UpdateOneof;
use orbitflare_sdk::{GeyserClientBuilder, Result};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let client = GeyserClientBuilder::new()
        .url("http://ny.rpc.orbitflare.com:10000")
        .build()?;

    let config_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/config/grpc.yml");
    let mut stream = client.subscribe_yaml(config_path.to_str().unwrap())?;

    while let Some(update) = stream.next().await {
        let update = update?;
        match update.update_oneof {
            Some(UpdateOneof::Transaction(tx)) => {
                if let Some(info) = &tx.transaction {
                    let sig = bs58::encode(&info.signature).into_string();
                    println!("tx slot={} sig={}", tx.slot, &sig[..12]);
                }
            }
            Some(UpdateOneof::Ping(_)) => {}
            Some(UpdateOneof::Pong(p)) => {
                println!("pong id={}", p.id);
            }
            _ => {}
        }
    }

    Ok(())
}
