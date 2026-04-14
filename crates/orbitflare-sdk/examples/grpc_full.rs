use orbitflare_sdk::{GeyserClientBuilder, Result};
use orbitflare_sdk::proto::geyser::subscribe_update::UpdateOneof;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let client = GeyserClientBuilder::new()
        .url("http://ny.rpc.orbitflare.com:10000")
        .fallback_url("http://fra.rpc.orbitflare.com:10000")
        .build()?;

    let config_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/config/grpc.yml");
    let mut stream = client.subscribe_yaml(config_path.to_str().unwrap())?;
    let mut tx_count: u64 = 0;

    println!("streaming...");

    while let Some(update) = stream.next().await {
        let update = update?;

        match update.update_oneof {
            Some(UpdateOneof::Transaction(tx)) => {
                tx_count += 1;
                if let Some(info) = &tx.transaction {
                    let sig = bs58::encode(&info.signature).into_string();
                    let fee = info.meta.as_ref().map(|m| m.fee).unwrap_or(0);
                    let ix_count = info.transaction
                        .as_ref()
                        .and_then(|t| t.message.as_ref())
                        .map(|m| m.instructions.len())
                        .unwrap_or(0);

                    println!(
                        "#{tx_count} slot={} sig={}... fee={fee} instructions={ix_count}",
                        tx.slot,
                        &sig[..16],
                    );
                }
            }
            Some(UpdateOneof::Slot(slot)) => {
                println!("slot {} ({:?})", slot.slot, slot.status);
            }
            _ => {}
        }
    }

    Ok(())
}
