use orbitflare_sdk::{WsClientBuilder, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = WsClientBuilder::new()
        .url("ws://ny.rpc.orbitflare.com")
        .build()
        .await?;

    let mut sub = client.slot_subscribe().await?;

    while let Some(slot) = sub.next().await {
        println!("slot: {slot}");
    }

    sub.unsubscribe().await;

    Ok(())
}
