use orbitflare_sdk::Result;
use orbitflare_sdk::jetstream::v2::{JetstreamClientBuilder, TransactionFilter};
use orbitflare_sdk::proto::jetstream::v2::subscribe_transactions_response::Payload;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let url = std::env::var("ORBITFLARE_JETSTREAM_URL")
        .unwrap_or_else(|_| "http://fra.jetstream.orbitflare.com".to_string());
    let client = JetstreamClientBuilder::new().url(&url).build()?;

    match client.get_version().await {
        Ok(v) => println!("version={v}"),
        Err(e) => println!("get_version error: {e}"),
    }
    match client.ping(7).await {
        Ok(n) => println!("ping={n}"),
        Err(e) => println!("ping error: {e}"),
    }

    let filter = TransactionFilter::new()
        .account_include(["So11111111111111111111111111111111111111112"])
        .include_enrichment(true)
        .with_id("wsol");

    let mut stream = client.subscribe_transactions(vec![filter]);

    while let Some(resp) = stream.next().await {
        let resp = resp?;
        match resp.payload {
            Some(Payload::Transaction(ft)) => {
                if let Some(tx) = &ft.transaction {
                    let sig = bs58::encode(&tx.signature).into_string();
                    println!(
                        "seq={} slot={} sig={} matched={:?}",
                        resp.sequence,
                        tx.slot,
                        &sig[..12.min(sig.len())],
                        ft.filter_ids
                    );
                }
            }
            Some(Payload::FilterValidation(r)) => {
                println!(
                    "filter {} accepted={} {}",
                    r.filter_id, r.accepted, r.rejection_reason
                );
            }
            Some(Payload::Heartbeat(hb)) => println!("heartbeat ts_ms={}", hb.server_ts_ms),
            Some(Payload::Pong(p)) => println!("pong id={}", p.ping_id),
            None => {}
        }
    }

    Ok(())
}
