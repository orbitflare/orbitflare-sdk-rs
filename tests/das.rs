use orbitflare_sdk::das::{
    AssetSortBy, ConditionType, DisplayOptions, GetAssetsByCreator, GetAssetsByGroup,
    GetAssetsByOwner, GetNftEditions, GetSignaturesForAsset, GetTokenAccounts, Interface,
    OwnershipModel, RoyaltyModel, Scope, SearchAssets, SortBy, SortDirection, TokenType,
};
use orbitflare_sdk::error::Error;
use orbitflare_sdk::{RetryPolicy, RpcClientBuilder, rpc::RpcClient};
use serde_json::{Value, json};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn client(server: &MockServer) -> RpcClient {
    RpcClientBuilder::new()
        .url(&server.uri())
        .retry(RetryPolicy {
            max_attempts: 1,
            ..Default::default()
        })
        .build()
        .unwrap()
}

async fn mock_result(result: Value) -> (MockServer, RpcClient) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"jsonrpc":"2.0","result":result,"id":1})),
        )
        .mount(&server)
        .await;
    let c = client(&server);
    (server, c)
}

async fn bodies(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

/// The compressed DRiP NFT from the getAsset docs.
fn drip_cnft() -> Value {
    json!({
        "last_indexed_slot": 457714576,
        "interface": "V1_NFT",
        "id": "JEKNNLZK6uQaQUEUz9GRoYZ7buHQ4rTN6k3qhX9NG8qU",
        "content": {
            "$schema": "https://schema.metaplex.com/nft1.0.json",
            "json_uri": "https://arweave.net/ymSDHwAqb3hnNKBid03WBUqY1Svj5Hzr0fdAYGvZBKA",
            "files": [{
                "uri": "https://arweave.net/2dfCrqZmpUTPEaPHuI9QUdZocNa-JYkG-3nBKycF-AM?ext=jpg",
                "mime": "image/jpeg"
            }],
            "metadata": {
                "attributes": [
                    { "value": "3", "trait_type": "Season" },
                    { "value": "Common", "trait_type": "Rarity" }
                ],
                "description": "Combination between Angkor Wat photography with microscopic creation as the sky.",
                "name": "Aurora borealis",
                "symbol": "",
                "token_standard": "NonFungible"
            },
            "links": {
                "external_url": "https://drip.haus/showcase",
                "image": "https://arweave.net/2dfCrqZmpUTPEaPHuI9QUdZocNa-JYkG-3nBKycF-AM?ext=jpg"
            },
            "category": "image"
        },
        "authorities": [{ "address": "5warkgXZEtubURnNHN1BcpgSBja31zeSvWudeXQEPpTr", "scopes": ["full"] }],
        "compression": {
            "eligible": false,
            "compressed": true,
            "data_hash": "6TuHoKagPg1Vqz92K5fiUNJtSC7wXHXSQfadLPnXsP8s",
            "creator_hash": "EuiqSYWYf6KnUcEmorrTrGQovPKF9a1wSVJAk7bfLgnR",
            "asset_hash": "CcMhrzXFkjyLwZdijjRibgSYw4tUzXYALoCxwms4aMFH",
            "tree": "E7fYCRWkNykc8HNZvEMRj5GuNYSwJ3WjA6HuLsh3YHvL",
            "seq": 627605,
            "leaf_id": 622994
        },
        "grouping": [{ "group_key": "collection", "group_value": "DRiP3vrQ7LuvQtmwL74RcejviWmQ1HuuKFcJdUpYG6Di" }],
        "royalty": {
            "royalty_model": "creators",
            "target": null,
            "percent": 0.015,
            "basis_points": 150,
            "primary_sale_happened": false,
            "locked": false
        },
        "creators": [
            { "address": "DRiPPP2LytGjNZ5fVpdZS7Xi1oANSY3Df1gSxvUKpzny", "share": 0, "verified": true },
            { "address": "WoMbXFtdfH8crq2Zi7bQhfGx2Gv8EN4saP13gcdUGog", "share": 100, "verified": false }
        ],
        "ownership": {
            "frozen": false,
            "delegated": false,
            "delegate": null,
            "ownership_model": "single",
            "owner": "5MRNQW3NxNes1EDnxUhqyXwffseQBLpdu6QJF8G4MdN9"
        },
        "supply": { "print_max_supply": 0, "print_current_supply": 0, "edition_nonce": null },
        "mutable": true,
        "burnt": false
    })
}

fn proof() -> Value {
    json!({
        "last_indexed_slot": 457714576,
        "root": "3wxyaNRvzuZqDd9jtqthCu87spdKuhcpaKVzP1msGyHW",
        "proof": [
            "8mjbWMPrBwYJeEmQbeYLR7mscS4umW4U3kxibuH8hcY1",
            "DV31TLc5BXCnCQMaQ9H6empbG9i69w4AWkjR5RPiyXr8",
            "G6f1pEKetujNm6JXQxD47eBudxLV1irR1U5atpAXBb4q"
        ],
        "node_index": 1671570,
        "leaf": "CcMhrzXFkjyLwZdijjRibgSYw4tUzXYALoCxwms4aMFH",
        "tree_id": "E7fYCRWkNykc8HNZvEMRj5GuNYSwJ3WjA6HuLsh3YHvL"
    })
}

#[tokio::test]
async fn get_asset_parses_compressed_nft() {
    let (server, client) = mock_result(drip_cnft()).await;
    let asset = client
        .get_asset("JEKNNLZK6uQaQUEUz9GRoYZ7buHQ4rTN6k3qhX9NG8qU")
        .await
        .unwrap();

    assert_eq!(asset.interface, Interface::V1Nft);
    assert_eq!(asset.last_indexed_slot, Some(457714576));
    assert_eq!(asset.name(), Some("Aurora borealis"));
    assert_eq!(asset.symbol(), None);
    assert_eq!(
        asset.owner(),
        Some("5MRNQW3NxNes1EDnxUhqyXwffseQBLpdu6QJF8G4MdN9")
    );
    assert_eq!(
        asset.collection(),
        Some("DRiP3vrQ7LuvQtmwL74RcejviWmQ1HuuKFcJdUpYG6Di")
    );
    assert!(asset.image().unwrap().starts_with("https://arweave.net/"));
    assert!(asset.is_compressed());

    let c = asset.compression.as_ref().unwrap();
    assert_eq!((c.seq, c.leaf_id), (627605, 622994));
    assert_eq!(c.tree, "E7fYCRWkNykc8HNZvEMRj5GuNYSwJ3WjA6HuLsh3YHvL");

    let content = asset.content.as_ref().unwrap();
    assert_eq!(content.schema, "https://schema.metaplex.com/nft1.0.json");
    assert_eq!(content.files[0].mime.as_deref(), Some("image/jpeg"));
    assert_eq!(
        content.metadata.attributes[1].trait_type.as_deref(),
        Some("Rarity")
    );
    assert_eq!(content.metadata.attributes[1].value, "Common");

    let royalty = asset.royalty.as_ref().unwrap();
    assert_eq!(royalty.royalty_model, RoyaltyModel::Creators);
    assert_eq!(royalty.basis_points, 150);
    assert!(royalty.target.is_none());

    assert_eq!(asset.creators[1].share, 100);
    assert!(!asset.creators[1].verified);
    assert_eq!(asset.authorities[0].scopes, vec![Scope::Full]);
    assert_eq!(asset.supply.as_ref().unwrap().edition_nonce, None);
    assert!(asset.mutable && !asset.burnt);

    let sent = &bodies(&server).await[0];
    assert_eq!(sent["method"], "getAsset");
    assert_eq!(
        sent["params"],
        json!({"id": "JEKNNLZK6uQaQUEUz9GRoYZ7buHQ4rTN6k3qhX9NG8qU"})
    );
}

#[tokio::test]
async fn get_asset_parses_full_asset_object() {
    // The Mad Lads programmable NFT from the asset object reference.
    let (_server, client) = mock_result(json!({
        "interface": "ProgrammableNFT",
        "id": "JEGruwYE13mhX2wi2MGrPmeLiVyZtbBptmVy9vG3pXRC",
        "content": {
            "$schema": "https://schema.metaplex.com/nft1.0.json",
            "json_uri": "https://madlads.s3.us-west-2.amazonaws.com/json/6867.json",
            "files": [{ "uri": "https://madlads.s3.us-west-2.amazonaws.com/images/6867.png", "mime": "image/png" }],
            "metadata": {
                "attributes": [{ "value": "Male", "trait_type": "Gender" }, { "value": 7, "trait_type": 1 }],
                "description": "Fock it.",
                "name": "Mad Lads #6867",
                "symbol": "MAD",
                "token_standard": "ProgrammableNonFungible"
            },
            "links": { "image": "https://madlads.s3.us-west-2.amazonaws.com/images/6867.png", "external_url": "https://madlads.com" },
            "category": "image"
        },
        "authorities": [{ "address": "2RtGg6fsFiiF1EQzHqbd66AhW7R5bWeQGpTbv2UMkCdW", "scopes": ["full"] }],
        "compression": {
            "eligible": false, "compressed": false, "data_hash": "", "creator_hash": "",
            "asset_hash": "", "tree": "", "seq": 0, "leaf_id": 0
        },
        "grouping": [{ "group_key": "collection", "group_value": "J1S9H3QjnRtBbbuD4HjPV6RpRhwuk4zKbxsnCHuTgh9w" }],
        "royalty": {
            "royalty_model": "creators", "target": null, "percent": 0.042, "basis_points": 420,
            "primary_sale_happened": true, "locked": false
        },
        "creators": [
            { "address": "5XvhfmRjwXkGp3jHGmaKpqeerNYjkuZZBYLVQYdeVcRv", "share": 0, "verified": true },
            { "address": "2RtGg6fsFiiF1EQzHqbd66AhW7R5bWeQGpTbv2UMkCdW", "share": 100, "verified": true }
        ],
        "ownership": {
            "frozen": true, "delegated": true,
            "delegate": "FARqKAafAbgT25QcgiX5d1g6xpadgG7xymu5N6gSmp4x",
            "ownership_model": "single",
            "owner": "CxBhuJwhVgNMc7yjnRx3XFAsTnnbYDc2bhAGSaLBjNqZ"
        },
        "supply": { "print_max_supply": 0, "print_current_supply": 0, "edition_nonce": 254 },
        "mutable": true,
        "burnt": false,
        "token_info": {
            "supply": 1,
            "decimals": 0,
            "token_program": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
            "associated_token_address": "J4z9rkXuQFVFYEcE4FnbZrT677Eh6WU3f125wyaDyQXg"
        },
        "mpl_core_info": { "num_minted": 3, "current_size": 3, "plugins_json_version": 1 },
        "some_future_field": { "x": 1 }
    }))
    .await;
    let asset = client
        .get_asset("JEGruwYE13mhX2wi2MGrPmeLiVyZtbBptmVy9vG3pXRC")
        .await
        .unwrap();

    assert_eq!(asset.interface, Interface::ProgrammableNft);
    assert_eq!(asset.symbol(), Some("MAD"));
    assert!(!asset.is_compressed());
    let ownership = asset.ownership.as_ref().unwrap();
    assert!(ownership.frozen && ownership.delegated);
    assert_eq!(ownership.ownership_model, OwnershipModel::Single);
    assert_eq!(
        ownership.delegate.as_deref(),
        Some("FARqKAafAbgT25QcgiX5d1g6xpadgG7xymu5N6gSmp4x")
    );
    assert_eq!(asset.supply.as_ref().unwrap().edition_nonce, Some(254));
    let token = asset.token_info.as_ref().unwrap();
    assert_eq!((token.supply, token.decimals), (Some(1), Some(0)));
    assert_eq!(asset.mpl_core_info.as_ref().unwrap().num_minted, Some(3));
    // Hand-written metadata can use numbers where strings are expected.
    let odd = &asset.content.as_ref().unwrap().metadata.attributes[1];
    assert_eq!(odd.trait_type.as_deref(), Some("1"));
    assert_eq!(odd.value, 7);
}

#[tokio::test]
async fn get_asset_tolerates_nulls_and_unknown_values() {
    let (_server, client) = mock_result(json!({
        "interface": "SomethingNew",
        "id": "x",
        "content": { "$schema": null, "json_uri": null, "files": null, "metadata": { "name": null, "symbol": null }, "links": null },
        "authorities": null,
        "grouping": null,
        "creators": null,
        "ownership": { "owner": null, "ownership_model": "weird", "frozen": null, "delegated": false, "delegate": null },
        "royalty": { "royalty_model": "single", "target": "abc", "percent": null, "basis_points": null },
        "supply": null,
        "mutable": null,
        "burnt": false
    }))
    .await;
    let asset = client.get_asset("x").await.unwrap();
    assert_eq!(asset.interface, Interface::Unknown);
    assert_eq!(asset.name(), None);
    assert_eq!(asset.owner(), None);
    assert!(asset.creators.is_empty() && asset.grouping.is_empty());
    assert_eq!(
        asset.ownership.unwrap().ownership_model,
        OwnershipModel::Unknown
    );
    assert_eq!(asset.royalty.unwrap().royalty_model, RoyaltyModel::Single);
    assert!(asset.supply.is_none());
}

#[tokio::test]
async fn get_asset_with_options_sends_camel_case_options() {
    let (server, client) = mock_result(drip_cnft()).await;
    let options = DisplayOptions::new()
        .show_collection_metadata(true)
        .show_unverified_collections(true);
    client.get_asset_with_options("id", &options).await.unwrap();
    assert_eq!(
        bodies(&server).await[0]["params"],
        json!({"id": "id", "options": {"showCollectionMetadata": true, "showUnverifiedCollections": true}})
    );
}

/// Answers a batch with one `null` per requested ID, so the order and chunking are visible.
struct EchoBatch;

impl Respond for EchoBatch {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        let ids = body["params"]["ids"].as_array().unwrap();
        let result: Vec<Value> = ids
            .iter()
            .map(|id| {
                if id == "missing" {
                    Value::Null
                } else {
                    json!({"interface": "V1_NFT", "id": id})
                }
            })
            .collect();
        ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","result":result,"id":1}))
    }
}

#[tokio::test]
async fn get_asset_batch_keeps_order_and_nulls() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(EchoBatch)
        .mount(&server)
        .await;
    let assets = client(&server)
        .get_asset_batch(&["a", "missing", "b"])
        .await
        .unwrap();
    assert_eq!(assets.len(), 3);
    assert_eq!(assets[0].as_ref().unwrap().id, "a");
    assert!(assets[1].is_none());
    assert_eq!(assets[2].as_ref().unwrap().id, "b");
    assert_eq!(bodies(&server).await[0]["method"], "getAssetBatch");
}

#[tokio::test]
async fn get_asset_batch_splits_into_chunks_of_1000() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(EchoBatch)
        .mount(&server)
        .await;
    let ids: Vec<String> = (0..2500).map(|i| format!("id{i}")).collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();

    let assets = client(&server).get_asset_batch(&refs).await.unwrap();
    assert_eq!(assets.len(), 2500);
    assert_eq!(assets[2499].as_ref().unwrap().id, "id2499");

    let sent = bodies(&server).await;
    let sizes: Vec<usize> = sent
        .iter()
        .map(|b| b["params"]["ids"].as_array().unwrap().len())
        .collect();
    assert_eq!(sizes, vec![1000, 1000, 500]);
}

#[tokio::test]
async fn get_asset_batch_with_no_ids_makes_no_request() {
    let server = MockServer::start().await;
    let assets = client(&server).get_asset_batch(&[]).await.unwrap();
    assert!(assets.is_empty());
    assert!(bodies(&server).await.is_empty());
}

#[tokio::test]
async fn get_asset_proof_and_proof_path() {
    let (server, client) = mock_result(proof()).await;
    let p = client.get_asset_proof("id").await.unwrap();
    assert_eq!(p.root, "3wxyaNRvzuZqDd9jtqthCu87spdKuhcpaKVzP1msGyHW");
    assert_eq!(p.node_index, 1671570);
    assert_eq!(p.tree_id, "E7fYCRWkNykc8HNZvEMRj5GuNYSwJ3WjA6HuLsh3YHvL");
    assert_eq!(p.proof_path(0).len(), 3);
    assert_eq!(p.proof_path(1), &p.proof[..2]);
    assert!(p.proof_path(10).is_empty());
    assert_eq!(bodies(&server).await[0]["method"], "getAssetProof");
}

#[tokio::test]
async fn get_asset_proof_batch_maps_ids() {
    let (server, client) = mock_result(json!({
        "JEKNNLZK6uQaQUEUz9GRoYZ7buHQ4rTN6k3qhX9NG8qU": proof(),
        "JEGruwYE13mhX2wi2MGrPmeLiVyZtbBptmVy9vG3pXRC": null
    }))
    .await;
    let proofs = client
        .get_asset_proof_batch(&[
            "JEKNNLZK6uQaQUEUz9GRoYZ7buHQ4rTN6k3qhX9NG8qU",
            "JEGruwYE13mhX2wi2MGrPmeLiVyZtbBptmVy9vG3pXRC",
        ])
        .await
        .unwrap();
    assert_eq!(
        proofs["JEKNNLZK6uQaQUEUz9GRoYZ7buHQ4rTN6k3qhX9NG8qU"]
            .as_ref()
            .unwrap()
            .node_index,
        1671570
    );
    assert!(proofs["JEGruwYE13mhX2wi2MGrPmeLiVyZtbBptmVy9vG3pXRC"].is_none());
    assert_eq!(bodies(&server).await[0]["method"], "getAssetProofBatch");
}

#[tokio::test]
async fn get_asset_with_proof_returns_both() {
    let (server, client) = mock_result(json!({ "asset": drip_cnft(), "proof": proof() })).await;
    let both = client.get_asset_with_proof("id").await.unwrap();
    assert_eq!(both.asset.compression.as_ref().unwrap().leaf_id, 622994);
    assert_eq!(
        both.proof.leaf,
        both.asset.compression.as_ref().unwrap().asset_hash
    );
    assert_eq!(bodies(&server).await[0]["method"], "getAssetWithProof");
}

#[tokio::test]
async fn get_assets_by_owner_parses_tokens_and_native_balance() {
    let (server, client) = mock_result(json!({
        "last_indexed_slot": 457714577,
        "total": 1,
        "limit": 100,
        "page": 1,
        "nativeBalance": { "lamports": 1328527472954u64, "price_per_sol": 101.5, "total_price": 134845.54 },
        "items": [{
            "interface": "FungibleToken",
            "id": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "ownership": { "owner": "86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY", "ownership_model": "token" },
            "token_info": {
                "symbol": "USDC",
                "balance": 2500000,
                "decimals": 6,
                "token_program": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
                "price_info": { "price_per_token": 1.0, "total_price": 2.5, "currency": "USDC" }
            }
        }]
    }))
    .await;

    let req = GetAssetsByOwner::new("86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY")
        .page(1)
        .limit(100)
        .options(
            DisplayOptions::new()
                .show_fungible(true)
                .show_native_balance(true),
        );
    let list = client.get_assets_by_owner(&req).await.unwrap();

    assert_eq!((list.total, list.limit, list.page), (1, 100, Some(1)));
    let native = list.native_balance.unwrap();
    assert_eq!(native.lamports, 1328527472954);
    assert_eq!(native.price_per_sol, Some(101.5));
    let usdc = &list.items[0];
    assert_eq!(usdc.interface, Interface::FungibleToken);
    assert_eq!(
        usdc.ownership.as_ref().unwrap().ownership_model,
        OwnershipModel::Token
    );
    let info = usdc.token_info.as_ref().unwrap();
    assert_eq!((info.balance, info.decimals), (Some(2500000), Some(6)));
    assert_eq!(info.price_info.as_ref().unwrap().total_price, Some(2.5));

    let sent = &bodies(&server).await[0];
    assert_eq!(sent["method"], "getAssetsByOwner");
    assert_eq!(
        sent["params"],
        json!({
            "ownerAddress": "86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY",
            "page": 1,
            "limit": 100,
            "options": { "showFungible": true, "showNativeBalance": true }
        })
    );
}

#[tokio::test]
async fn list_requests_send_documented_params() {
    let (server, client) = mock_result(json!({"total": 0, "limit": 1000, "items": []})).await;

    client
        .get_assets_by_group(
            &GetAssetsByGroup::collection("J1S9H3QjnRtBbbuD4HjPV6RpRhwuk4zKbxsnCHuTgh9w")
                .sort_by(SortBy::new(AssetSortBy::RecentAction, SortDirection::Desc))
                .page(2),
        )
        .await
        .unwrap();
    client
        .get_assets_by_creator(
            &GetAssetsByCreator::new("creator")
                .only_verified(true)
                .cursor("c1"),
        )
        .await
        .unwrap();
    client
        .get_assets_by_authority(
            &orbitflare_sdk::das::GetAssetsByAuthority::new("auth")
                .after("a")
                .limit(5),
        )
        .await
        .unwrap();

    let sent = bodies(&server).await;
    assert_eq!(sent[0]["method"], "getAssetsByGroup");
    assert_eq!(
        sent[0]["params"],
        json!({
            "groupKey": "collection",
            "groupValue": "J1S9H3QjnRtBbbuD4HjPV6RpRhwuk4zKbxsnCHuTgh9w",
            "sortBy": { "sortBy": "recent_action", "sortDirection": "desc" },
            "page": 2
        })
    );
    assert_eq!(sent[1]["method"], "getAssetsByCreator");
    assert_eq!(
        sent[1]["params"],
        json!({"creatorAddress": "creator", "onlyVerified": true, "cursor": "c1"})
    );
    assert_eq!(sent[2]["method"], "getAssetsByAuthority");
    assert_eq!(
        sent[2]["params"],
        json!({"authorityAddress": "auth", "after": "a", "limit": 5})
    );
}

#[tokio::test]
async fn search_assets_serializes_filters() {
    let (server, client) = mock_result(json!({"total": 0, "limit": 1000, "items": []})).await;
    let req = SearchAssets::new()
        .owner("86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY")
        .collection("DRiP3vrQ7LuvQtmwL74RcejviWmQ1HuuKFcJdUpYG6Di")
        .creator("creator", true)
        .interface(Interface::ProgrammableNft)
        .token_type(TokenType::CompressedNft)
        .owner_type(OwnershipModel::Single)
        .royalty_target_type(RoyaltyModel::Fanout)
        .royalty_amount(500)
        .compressed(true)
        .burnt(false)
        .condition_type(ConditionType::Any)
        .negate(true)
        .limit(10);
    client.search_assets(&req).await.unwrap();

    assert_eq!(
        bodies(&server).await[0]["params"],
        json!({
            "ownerAddress": "86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY",
            "grouping": ["collection", "DRiP3vrQ7LuvQtmwL74RcejviWmQ1HuuKFcJdUpYG6Di"],
            "creatorAddress": "creator",
            "creatorVerified": true,
            "interface": "ProgrammableNFT",
            "tokenType": "compressedNFT",
            "ownerType": "single",
            "royaltyTargetType": "fanout",
            "royaltyAmount": 500,
            "compressed": true,
            "burnt": false,
            "conditionType": "any",
            "negate": true,
            "limit": 10
        })
    );
}

#[tokio::test]
async fn get_token_accounts_parses_accounts() {
    let (server, client) = mock_result(json!({
        "total": 2,
        "limit": 2,
        "page": 1,
        "token_accounts": [
            {
                "address": "1113odRAP5wXa8n7NFVBG53XrqWeDrViZu5rSiSsVA3",
                "mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
                "owner": "HJNr4feTNiDn59RCL1pYoARXMFoBEQSmAY7R7Cm21z9T",
                "amount": 170812, "delegate": null, "delegated_amount": 0,
                "close_authority": null, "frozen": false
            },
            {
                "address": "11NCSQ2Bm6UZdtDqBeHFcos5eFXT1kJ3jG29qKJCWU",
                "mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
                "owner": "FRPPBdG4sYCH7EnTMeKFSVLsfzfEUo4PQ1wWCUkzzmuv",
                "amount": 1804811, "delegate": null, "delegated_amount": 0,
                "close_authority": null, "frozen": true,
                "token_extensions": { "immutable_owner": true }
            }
        ]
    }))
    .await;
    let req = GetTokenAccounts::by_mint("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
        .page(1)
        .limit(2)
        .show_zero_balance(true);
    let list = client.get_token_accounts(&req).await.unwrap();
    assert_eq!(list.token_accounts.len(), 2);
    assert_eq!(list.token_accounts[1].amount, 1804811);
    assert!(list.token_accounts[1].frozen);
    assert!(list.token_accounts[1].token_extensions.is_some());
    assert_eq!(
        bodies(&server).await[0]["params"],
        json!({
            "mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "page": 1,
            "limit": 2,
            "options": { "showZeroBalance": true }
        })
    );
}

#[tokio::test]
async fn get_token_accounts_needs_owner_or_mint() {
    let server = MockServer::start().await;
    let err = client(&server)
        .get_token_accounts(&GetTokenAccounts::default())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Config(_)), "{err}");
    assert!(bodies(&server).await.is_empty());
}

#[tokio::test]
async fn get_nft_editions_parses_unlimited_supply() {
    let (server, client) = mock_result(json!({
        "total": 2, "limit": 100, "page": 1,
        "master_edition_address": "MASTER", "supply": 2, "max_supply": null,
        "editions": [
            { "mint": "PRINT_MINT_1", "edition_address": "PRINT_EDITION_1", "edition": 1 },
            { "mint": "PRINT_MINT_2", "edition_address": "PRINT_EDITION_2", "edition": 2 }
        ]
    }))
    .await;
    let editions = client
        .get_nft_editions(&GetNftEditions::new("mint").page(1).limit(100))
        .await
        .unwrap();
    assert_eq!(editions.supply, 2);
    assert_eq!(editions.max_supply, None);
    assert_eq!(editions.editions[1].edition, 2);
    assert_eq!(
        bodies(&server).await[0]["params"],
        json!({"mint": "mint", "page": 1, "limit": 100})
    );
}

#[tokio::test]
async fn get_signatures_for_asset_parses_pairs() {
    let (server, client) = mock_result(json!({
        "last_indexed_slot": 457714577, "total": 1, "limit": 100, "page": 1,
        "items": [[
            "3eUm7YYq71vcZdgYT3X51xrJeNKaUg5fFfCFaBS2fwP3BPvip2hYxaAaqfNDs3Pj53JPDriQauBtWbz4u7cFN2dw",
            "MintToCollectionV1"
        ]]
    }))
    .await;
    let sigs = client
        .get_signatures_for_asset(
            &GetSignaturesForAsset::by_leaf("tree", 622994).sort_direction(SortDirection::Desc),
        )
        .await
        .unwrap();
    assert_eq!(sigs.items[0].instruction, "MintToCollectionV1");
    assert!(sigs.items[0].signature.starts_with("3eUm7"));
    assert_eq!(
        bodies(&server).await[0]["params"],
        json!({"tree": "tree", "leafIndex": 622994, "sortDirection": "desc"})
    );
}

#[tokio::test]
async fn get_signatures_for_asset_needs_an_asset() {
    let server = MockServer::start().await;
    let req = GetSignaturesForAsset {
        tree: Some("tree".into()),
        ..Default::default()
    };
    let err = client(&server)
        .get_signatures_for_asset(&req)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Config(_)), "{err}");
}

/// Serves `sizes.len()` pages of assets, by cursor or by page number.
struct Paged {
    sizes: Vec<usize>,
}

impl Respond for Paged {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        let p = &body["params"];
        let index = match (p["page"].as_u64(), p["cursor"].as_str()) {
            (Some(page), _) => page as usize - 1,
            (None, Some(c)) => c.trim_start_matches('c').parse::<usize>().unwrap(),
            (None, None) => 0,
        };
        let n = self.sizes.get(index).copied().unwrap_or(0);
        let items: Vec<Value> = (0..n)
            .map(|i| json!({"interface": "V1_NFT", "id": format!("{index}-{i}")}))
            .collect();
        let mut result =
            json!({"total": n, "limit": p["limit"].as_u64().unwrap_or(1000), "items": items});
        if p["page"].is_null() {
            result["cursor"] = json!(format!("c{}", index + 1));
        } else {
            result["page"] = p["page"].clone();
        }
        ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","result":result,"id":1}))
    }
}

#[tokio::test]
async fn pages_walk_by_cursor_until_a_short_page() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(Paged {
            sizes: vec![1000, 1000, 3],
        })
        .mount(&server)
        .await;
    let client = client(&server);

    let items = client
        .pages(GetAssetsByGroup::collection("col"))
        .collect_items()
        .await
        .unwrap();
    assert_eq!(items.len(), 2003);
    assert_eq!(items[2002].id, "2-2");

    let sent = bodies(&server).await;
    let cursors: Vec<Value> = sent.iter().map(|b| b["params"]["cursor"].clone()).collect();
    assert_eq!(cursors, vec![Value::Null, json!("c1"), json!("c2")]);
    assert!(sent.iter().all(|b| b["params"]["page"].is_null()));
}

#[tokio::test]
async fn pages_walk_by_number_when_not_sorted_by_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(Paged {
            sizes: vec![2, 2, 1],
        })
        .mount(&server)
        .await;
    let client = client(&server);

    let req = GetAssetsByOwner::new("owner")
        .limit(2)
        .sort_by(SortBy::new(AssetSortBy::Created, SortDirection::Desc));
    let mut pages = client.pages(req);
    let mut seen = Vec::new();
    while let Some(page) = pages.next_page().await {
        let page = page.unwrap();
        seen.push((page.page, page.items.len()));
    }
    assert_eq!(seen, vec![(Some(1), 2), (Some(2), 2), (Some(3), 1)]);
    assert!(
        bodies(&server)
            .await
            .iter()
            .all(|b| b["params"]["cursor"].is_null())
    );
}

#[tokio::test]
async fn pages_stop_on_an_empty_full_page_boundary() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(Paged { sizes: vec![2, 0] })
        .mount(&server)
        .await;
    let items = client(&server)
        .pages(SearchAssets::new().owner("o").limit(2))
        .collect_items()
        .await
        .unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(bodies(&server).await.len(), 2);
}

#[tokio::test]
async fn signature_pages_always_use_page_numbers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "result": {"total": 1, "limit": 1000, "page": 1, "items": [["sig", "Transfer"]]},
            "id": 1
        })))
        .mount(&server)
        .await;
    let items = client(&server)
        .pages(GetSignaturesForAsset::new("asset"))
        .collect_items()
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        bodies(&server).await[0]["params"],
        json!({"id": "asset", "page": 1})
    );
}

#[tokio::test]
async fn asset_not_found_is_an_rpc_error_and_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "error": {"code": -32000, "message": "Database Error: RecordNotFound Error: Asset Not Found"},
            "id": 1
        })))
        .mount(&server)
        .await;
    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .retry(RetryPolicy {
            max_attempts: 5,
            initial_delay: std::time::Duration::from_millis(1),
            ..Default::default()
        })
        .build()
        .unwrap();

    let err = client
        .get_asset("11111111111111111111111111111111")
        .await
        .unwrap_err();
    match err {
        Error::Rpc { code, message } => {
            assert_eq!(code, -32000);
            assert!(message.contains("Asset Not Found"), "{message}");
        }
        other => panic!("expected an RPC error, got {other}"),
    }
    assert_eq!(bodies(&server).await.len(), 1);
}

#[tokio::test]
async fn invalid_params_surface_the_server_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "error": {"code": -32602, "message": "Pagination Error. Limit should not be greater than 1000."},
            "id": 1
        })))
        .mount(&server)
        .await;
    let err = client(&server)
        .get_assets_by_owner(&GetAssetsByOwner::new("o").limit(5000))
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("Limit should not be greater than 1000"),
        "{err}"
    );
}
