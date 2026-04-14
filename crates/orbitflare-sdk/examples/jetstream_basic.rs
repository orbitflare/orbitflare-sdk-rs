use orbitflare_sdk::{JetstreamClientBuilder, Result};
use orbitflare_sdk::proto::jetstream::subscribe_update::UpdateOneof;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let client = JetstreamClientBuilder::new()
        .url("http://ny.jetstream.orbitflare.com")
        .build()?;

    let config_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/config/jetstream.yml");
    let mut stream = client.subscribe_yaml(config_path.to_str().unwrap())?;

    while let Some(update) = stream.next().await {
        let update = update?;
        if let Some(UpdateOneof::Transaction(tx)) = update.update_oneof {
            if let Some(info) = &tx.transaction {
                let sig = bs58::encode(&info.signature).into_string();
                println!("jet tx slot={} sig={}", tx.slot, &sig[..12]);
            }
        }
    }

    Ok(())
}
