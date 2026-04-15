use orbitflare_sdk::{Result, WsClientBuilder};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let client = WsClientBuilder::new()
        .url("ws://ny.rpc.orbitflare.com")
        .build()
        .await?;

    let mut slots = client.slot_subscribe().await?;
    let mut usdc = client
        .account_subscribe("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "confirmed")
        .await?;

    println!("watching slots and USDC mint account...");

    loop {
        tokio::select! {
            Some(slot) = slots.next() => {
                let s = slot["slot"].as_u64().unwrap_or(0);
                let parent = slot["parent"].as_u64().unwrap_or(0);
                println!("slot {s} (parent {parent})");
            }
            Some(acct) = usdc.next() => {
                let lamports = acct["lamports"].as_u64().unwrap_or(0);
                let data_len = acct["data"][0]
                    .as_str()
                    .map(|s| s.len())
                    .unwrap_or(0);
                println!("USDC mint updated: {lamports} lamports, {data_len} bytes");
            }
        }
    }
}
