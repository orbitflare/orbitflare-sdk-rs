use orbitflare_sdk::das::{
    DisplayOptions, GetAssetsByGroup, GetAssetsByOwner, SearchAssets, TokenType,
};
use orbitflare_sdk::{Result, RpcClientBuilder};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let client = RpcClientBuilder::new()
        .url("https://fra.rpc.orbitflare.com")
        .build()?;

    let asset = client
        .get_asset("GJfYbfW7gufzFYfm8gFatrUE71wYsjay6FwzDkTa3vaD")
        .await?;
    println!(
        "{} ({:?}), owner {}",
        asset.name().unwrap_or("?"),
        asset.interface,
        asset.owner().unwrap_or("?"),
    );

    let portfolio = client
        .get_assets_by_owner(
            &GetAssetsByOwner::new("86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY")
                .page(1)
                .limit(100)
                .options(
                    DisplayOptions::new()
                        .show_fungible(true)
                        .show_native_balance(true),
                ),
        )
        .await?;
    if let Some(sol) = &portfolio.native_balance {
        println!(
            "SOL: {} lamports (${:.2})",
            sol.lamports,
            sol.total_price.unwrap_or(0.0)
        );
    }
    println!("{} assets on page 1", portfolio.items.len());

    let tokens = client
        .search_assets(
            &SearchAssets::new()
                .owner("86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY")
                .token_type(TokenType::Fungible),
        )
        .await?;
    for t in &tokens.items {
        let info = t.token_info.as_ref();
        println!(
            "{:>10} {}",
            info.and_then(|i| i.symbol.as_deref()).unwrap_or("?"),
            info.and_then(|i| i.balance).unwrap_or(0),
        );
    }

    let proof = client
        .get_asset_with_proof("AssVbUqhyK9JWTRKTmd7verNPPCgFqupUMbhDDsc4TTU")
        .await?;
    println!(
        "cNFT leaf {} in tree {}, {} proof nodes",
        proof.asset.compression.as_ref().map_or(0, |c| c.leaf_id),
        proof.proof.tree_id,
        proof.proof.proof.len(),
    );

    // Walk a whole collection, page by page.
    let mut holders = std::collections::HashMap::<String, usize>::new();
    let mut pages = client.pages(GetAssetsByGroup::collection(
        "CDDyz6MHYY8972DPCXiEeReCxGqJSCbxHnimrvoyWrFF",
    ));
    while let Some(page) = pages.next_page().await {
        for asset in page?.items {
            if let Some(owner) = asset.owner() {
                *holders.entry(owner.to_string()).or_default() += 1;
            }
        }
    }
    println!("{} holders", holders.len());

    Ok(())
}
