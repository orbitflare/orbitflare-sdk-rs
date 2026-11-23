use orbitflare_sdk::Result;
use orbitflare_sdk::jetstream::v2::JetstreamClientBuilder;
use orbitflare_sdk::proto::jetstream::v2::SlotStatus;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let url = std::env::var("ORBITFLARE_JETSTREAM_URL")
        .unwrap_or_else(|_| "http://fra.jetstream.orbitflare.com".to_string());
    let client = JetstreamClientBuilder::new().url(&url).build()?;

    let mut slots = client.subscribe_slots_with_block_footers();

    while let Some(event) = slots.next().await {
        let event = event?;
        match event.status() {
            SlotStatus::BlockFooter => {
                let Some(footer) = &event.block_footer else {
                    continue;
                };
                println!(
                    "seq={} slot={} FOOTER bank_hash={} block_id={} user_agent={:?} final_cert={}",
                    event.sequence,
                    footer.slot,
                    hex(&footer.bank_hash),
                    hex(&footer.block_id),
                    String::from_utf8_lossy(&footer.block_user_agent),
                    footer.block_final_cert.is_some(),
                );
            }
            status => println!(
                "seq={} slot={} parent={} {:?}",
                event.sequence, event.slot, event.parent_slot, status
            ),
        }
    }

    Ok(())
}
