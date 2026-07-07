use orbitflare_sdk::jetstream::{SubscribeRequestBuilder, TransactionFilter};
use orbitflare_sdk::proto::jetstream::subscribe_update::UpdateOneof;
use orbitflare_sdk::{JetstreamClientBuilder, Result};

const PUMP_FUN: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const WSOL: &str = "So11111111111111111111111111111111111111112";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let url = std::env::var("ORBITFLARE_JETSTREAM_URL")
        .unwrap_or_else(|_| "http://slc.jetstream.orbitflare.com".to_string());
    let client = JetstreamClientBuilder::new().url(&url).build()?;

    let request = SubscribeRequestBuilder::new()
        .transactions(
            "pumpfun",
            TransactionFilter::new()
                .account_include([PUMP_FUN])
                .account_required([WSOL]),
        )
        .build();

    let mut stream = client.subscribe(request);

    while let Some(update) = stream.next().await {
        let update = update?;
        if let Some(UpdateOneof::Transaction(tx)) = update.update_oneof {
            if let Some(info) = &tx.transaction {
                let sig = bs58::encode(&info.signature).into_string();
                println!(
                    "tx slot={} sig={} filters={:?}",
                    tx.slot,
                    &sig[..12.min(sig.len())],
                    update.filters
                );
            }
        }
    }

    Ok(())
}
