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
    let mut count: u64 = 0;

    println!("streaming raydium txs...");

    while let Some(update) = stream.next().await {
        let update = update?;

        if let Some(UpdateOneof::Transaction(tx)) = update.update_oneof {
            count += 1;
            if let Some(info) = &tx.transaction {
                let sig = bs58::encode(&info.signature).into_string();
                let num_ix = info.instructions.len();
                let num_accounts = info.account_keys.len();

                println!(
                    "#{count} slot={} sig={}... ix={num_ix} accounts={num_accounts}",
                    tx.slot,
                    &sig[..16],
                );
            }
        }
    }

    Ok(())
}
