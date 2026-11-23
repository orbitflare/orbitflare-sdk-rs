use orbitflare_sdk::grpc::{BlockFooterFilter, Commitment, SubscribeRequestBuilder};
use orbitflare_sdk::proto::geyser::subscribe_update::UpdateOneof;
use orbitflare_sdk::{GeyserClientBuilder, Result};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let url = std::env::var("ORBITFLARE_GRPC_URL")
        .unwrap_or_else(|_| "http://ny.rpc.orbitflare.com:10000".to_string());
    let client = GeyserClientBuilder::new().url(&url).build()?;

    let request = SubscribeRequestBuilder::new()
        .block_footer(
            "footers",
            BlockFooterFilter::new().include_certificates(true),
        )
        .commitment(Commitment::Processed)
        .build();
    let mut stream = client.subscribe(request);

    while let Some(update) = stream.next().await {
        let Some(UpdateOneof::BlockFooter(f)) = update?.update_oneof else {
            continue;
        };
        println!(
            "slot={} bank_id={} bank_hash={} user_agent={:?} final={} skip={} notar={}",
            f.slot,
            f.bank_id,
            hex(&f.bank_hash),
            String::from_utf8_lossy(&f.block_user_agent),
            f.block_final_cert.is_some(),
            f.skip_reward_cert.is_some(),
            f.notar_reward_cert.is_some(),
        );
    }

    Ok(())
}
