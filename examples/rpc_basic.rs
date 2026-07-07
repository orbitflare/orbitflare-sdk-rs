use orbitflare_sdk::{Result, RpcClientBuilder};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let client = RpcClientBuilder::new()
        .url("http://ny.rpc.orbitflare.com")
        .commitment("confirmed")
        .build()?;

    let slot = client.get_slot().await?;
    println!("slot: {slot}");

    let balance = client
        .get_balance("CKs1E69a2e9TmH4mKKLrXFF8kD3ZnwKjoEuXa6sz9WqX")
        .await?;
    println!("balance: {balance} lamports");

    let (blockhash, last_valid) = client.get_latest_blockhash().await?;
    println!("blockhash: {blockhash} (valid until block {last_valid})");

    let inflation = client.request("getInflationRate", json!([])).await?;
    println!("inflation: {inflation}");

    let raw = client
        .request_raw(r#"{"jsonrpc":"2.0","id":1,"method":"getHealth","params":[]}"#)
        .await?;
    println!("health: {raw}");

    Ok(())
}
