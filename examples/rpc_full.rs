use orbitflare_sdk::{Result, RpcClientBuilder};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let client = RpcClientBuilder::new()
        .url("http://fra.rpc.orbitflare.com")
        .fallback_url("http://ny.rpc.orbitflare.com")
        .commitment("confirmed")
        .build()?;

    let wallet = "CKs1E69a2e9TmH4mKKLrXFF8kD3ZnwKjoEuXa6sz9WqX";

    let slot = client.get_slot().await?;
    let (blockhash, _) = client.get_latest_blockhash().await?;
    println!("network slot: {slot}");
    println!("blockhash: {blockhash}");

    let balance = client.get_balance(wallet).await?;
    println!("\n{wallet}");
    println!("  SOL: {:.4}", balance as f64 / 1_000_000_000.0);

    let tokens = client
        .get_token_accounts_by_owner(wallet, None, None)
        .await?;
    for token in &tokens {
        let info = &token["account"]["data"]["parsed"]["info"];
        let mint = info["mint"].as_str().unwrap_or("unknown");
        let amount = info["tokenAmount"]["uiAmountString"]
            .as_str()
            .unwrap_or("0");
        if amount != "0" {
            println!("  {mint}: {amount}");
        }
    }

    let sigs = client.get_signatures_for_address(wallet, 5).await?;
    println!("\nrecent transactions:");
    for sig in &sigs {
        let signature = sig["signature"].as_str().unwrap_or("?");
        let slot = sig["slot"].as_u64().unwrap_or(0);
        let err = if sig["err"].is_null() { "ok" } else { "failed" };
        println!("  {slot} {err} {}", &signature[..20]);
    }

    let fees = client.get_recent_prioritization_fees(&[wallet]).await?;
    let avg: f64 = fees
        .iter()
        .filter_map(|f| f["prioritizationFee"].as_f64())
        .sum::<f64>()
        / fees.len().max(1) as f64;
    println!("\navg priority fee: {avg:.0} micro-lamports");

    Ok(())
}
