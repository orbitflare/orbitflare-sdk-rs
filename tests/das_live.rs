//! End-to-end DAS tests against a live endpoint. They need an endpoint that serves DAS:
//!
//! ORBITFLARE_RPC_URL=https://fra.rpc.orbitflare.com ORBITFLARE_LICENSE_KEY=... \
//!     cargo test --test das_live -- --ignored --test-threads=1

use orbitflare_sdk::das::{
    AssetSortBy, DisplayOptions, GetAssetsByAuthority, GetAssetsByCreator, GetAssetsByGroup,
    GetAssetsByOwner, GetNftEditions, GetSignaturesForAsset, GetTokenAccounts, Interface,
    OwnershipModel, SearchAssets, SortBy, SortDirection, TokenType,
};
use orbitflare_sdk::error::Error;
use orbitflare_sdk::{RpcClientBuilder, rpc::RpcClient};

const ORBITFLARE_PASS: &str = "GJfYbfW7gufzFYfm8gFatrUE71wYsjay6FwzDkTa3vaD";
const ORBITFLARE_PASSES: &str = "CDDyz6MHYY8972DPCXiEeReCxGqJSCbxHnimrvoyWrFF";
const ORBITFLARE_PASS_AUTHORITY: &str = "9Ppfjr8tFAwK9xxvXsVN2xAaShRLZu5ZzDAwnCCLxewo";
// The OrbitFlare Pass creator is unverified, so the verified-creator check uses Mad Lads.
const MAD_LADS_CREATOR: &str = "5XvhfmRjwXkGp3jHGmaKpqeerNYjkuZZBYLVQYdeVcRv";
// Unicat #91.
const CNFT: &str = "AssVbUqhyK9JWTRKTmd7verNPPCgFqupUMbhDDsc4TTU";
const CNFT_COLLECTION: &str = "EuRteU98mnoRAer1eSc8do2BLeyxkY3aWAbTW27BSMUx";
const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const WALLET: &str = "86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY";

fn client() -> RpcClient {
    RpcClientBuilder::new()
        .build()
        .expect("set ORBITFLARE_RPC_URL")
}

#[tokio::test]
#[ignore]
async fn live_get_asset() {
    let asset = client().get_asset(ORBITFLARE_PASS).await.unwrap();
    assert_eq!(asset.id, ORBITFLARE_PASS);
    assert_eq!(asset.interface, Interface::ProgrammableNft);
    assert!(asset.name().unwrap().starts_with("OrbitFlare Pass"));
    assert_eq!(asset.collection(), Some(ORBITFLARE_PASSES));
    assert!(asset.owner().is_some());
    assert!(asset.last_indexed_slot.unwrap() > 0);
    println!(
        "{} owned by {}",
        asset.name().unwrap(),
        asset.owner().unwrap()
    );

    let with_meta = client()
        .get_asset_with_options(
            ORBITFLARE_PASS,
            &DisplayOptions::new().show_collection_metadata(true),
        )
        .await
        .unwrap();
    assert!(with_meta.grouping[0].collection_metadata.is_some());
}

#[tokio::test]
#[ignore]
async fn live_get_asset_fungible_and_compressed() {
    let usdc = client().get_asset(USDC).await.unwrap();
    assert_eq!(usdc.interface, Interface::FungibleToken);
    assert_eq!(usdc.token_info.as_ref().unwrap().decimals, Some(6));

    let cnft = client().get_asset(CNFT).await.unwrap();
    assert!(cnft.is_compressed());
    assert_eq!(cnft.collection(), Some(CNFT_COLLECTION));
}

#[tokio::test]
#[ignore]
async fn live_get_asset_not_found() {
    let err = client()
        .get_asset("11111111111111111111111111111111")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Rpc { code: -32000, .. }), "{err}");
}

#[tokio::test]
#[ignore]
async fn live_get_asset_batch() {
    let assets = client()
        .get_asset_batch(&[ORBITFLARE_PASS, "11111111111111111111111111111111", CNFT])
        .await
        .unwrap();
    assert_eq!(assets.len(), 3);
    assert_eq!(assets[0].as_ref().unwrap().id, ORBITFLARE_PASS);
    assert!(assets[1].is_none());
    assert_eq!(assets[2].as_ref().unwrap().id, CNFT);
}

#[tokio::test]
#[ignore]
async fn live_proofs() {
    let c = client();
    let proof = c.get_asset_proof(CNFT).await.unwrap();
    assert!(!proof.proof.is_empty());
    assert!(!proof.root.is_empty());

    let both = c.get_asset_with_proof(CNFT).await.unwrap();
    let compression = both.asset.compression.as_ref().unwrap();
    assert_eq!(both.proof.tree_id, compression.tree);
    assert_eq!(both.proof.leaf, compression.asset_hash);

    let batch = c
        .get_asset_proof_batch(&[CNFT, ORBITFLARE_PASS])
        .await
        .unwrap();
    assert!(batch[CNFT].is_some());
    assert!(batch.get(ORBITFLARE_PASS).is_none_or(Option::is_none));

    let err = c.get_asset_proof(ORBITFLARE_PASS).await.unwrap_err();
    assert!(matches!(err, Error::Rpc { code: -32000, .. }), "{err}");
}

#[tokio::test]
#[ignore]
async fn live_get_assets_by_owner_with_tokens() {
    let req = GetAssetsByOwner::new(WALLET).page(1).limit(50).options(
        DisplayOptions::new()
            .show_fungible(true)
            .show_native_balance(true),
    );
    let list = client().get_assets_by_owner(&req).await.unwrap();
    assert_eq!(list.page, Some(1));
    assert!(list.items.len() <= 50);
    assert!(list.native_balance.is_some());
    assert!(
        list.items
            .iter()
            .all(|a| a.owner().is_none_or(|o| o == WALLET))
    );
    let tokens = list
        .items
        .iter()
        .filter(|a| {
            a.ownership
                .as_ref()
                .is_some_and(|o| o.ownership_model == OwnershipModel::Token)
        })
        .count();
    println!("{} items, {tokens} fungible", list.items.len());
}

#[tokio::test]
#[ignore]
async fn live_list_methods() {
    let c = client();

    let group = c
        .get_assets_by_group(&GetAssetsByGroup::collection(ORBITFLARE_PASSES).limit(3))
        .await
        .unwrap();
    assert_eq!(group.items.len(), 3);
    assert!(
        group
            .items
            .iter()
            .all(|a| a.collection() == Some(ORBITFLARE_PASSES))
    );
    assert!(group.cursor.is_some(), "cursor paging returns a cursor");

    let creator = c
        .get_assets_by_creator(
            &GetAssetsByCreator::new(MAD_LADS_CREATOR)
                .only_verified(true)
                .limit(2),
        )
        .await
        .unwrap();
    assert!(!creator.items.is_empty());
    assert!(
        creator.items[0]
            .creators
            .iter()
            .any(|cr| cr.address == MAD_LADS_CREATOR && cr.verified)
    );

    let authority = c
        .get_assets_by_authority(
            &GetAssetsByAuthority::new(ORBITFLARE_PASS_AUTHORITY)
                .page(1)
                .limit(2),
        )
        .await
        .unwrap();
    assert!(!authority.items.is_empty());

    let sorted = c
        .get_assets_by_group(
            &GetAssetsByGroup::collection(ORBITFLARE_PASSES)
                .sort_by(SortBy::new(AssetSortBy::Created, SortDirection::Desc))
                .page(1)
                .limit(2),
        )
        .await
        .unwrap();
    assert_eq!(sorted.items.len(), 2);
}

#[tokio::test]
#[ignore]
async fn live_search_assets() {
    let c = client();
    let fungible = c
        .search_assets(
            &SearchAssets::new()
                .owner(WALLET)
                .token_type(TokenType::Fungible)
                .limit(5),
        )
        .await
        .unwrap();
    assert!(fungible.items.iter().all(|a| a.token_info.is_some()));

    let in_collection = c
        .search_assets(
            &SearchAssets::new()
                .collection(ORBITFLARE_PASSES)
                .burnt(false)
                .page(1)
                .limit(2),
        )
        .await
        .unwrap();
    assert_eq!(in_collection.items.len(), 2);
    assert!(in_collection.items.iter().all(|a| !a.burnt));
}

#[tokio::test]
#[ignore]
async fn live_get_token_accounts() {
    let list = client()
        .get_token_accounts(&GetTokenAccounts::by_mint(USDC).limit(5))
        .await
        .unwrap();
    assert_eq!(list.token_accounts.len(), 5);
    assert!(
        list.token_accounts
            .iter()
            .all(|t| t.mint == USDC && t.amount > 0)
    );
}

#[tokio::test]
#[ignore]
async fn live_get_nft_editions() {
    // OrbitFlare Passes aren't master editions, so this only checks the method is wired up.
    match client()
        .get_nft_editions(&GetNftEditions::new(ORBITFLARE_PASS).limit(5))
        .await
    {
        Ok(editions) => println!("{} editions", editions.supply),
        Err(Error::Rpc {
            code: -32000,
            message,
        }) => println!("not a master edition: {message}"),
        Err(e) => panic!("{e}"),
    }
}

#[tokio::test]
#[ignore]
async fn live_get_signatures_for_asset() {
    let sigs = client()
        .get_signatures_for_asset(&GetSignaturesForAsset::new(CNFT).limit(5))
        .await
        .unwrap();
    assert!(!sigs.items.is_empty());
    assert!(
        sigs.items
            .iter()
            .all(|s| !s.signature.is_empty() && !s.instruction.is_empty())
    );
    println!(
        "first: {} {}",
        sigs.items[0].instruction, sigs.items[0].signature
    );
}

#[tokio::test]
#[ignore]
async fn live_pages_by_cursor() {
    let c = client();
    let mut pages = c.pages(GetAssetsByGroup::collection(ORBITFLARE_PASSES).limit(5));
    let mut ids = Vec::new();
    for _ in 0..3 {
        let page = pages.next_page().await.unwrap().unwrap();
        assert_eq!(page.items.len(), 5);
        ids.extend(page.items.into_iter().map(|a| a.id));
    }
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 15, "pages must not overlap");
}
