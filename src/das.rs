//! Digital Asset Standard (DAS) API: NFTs, compressed NFTs, MPL Core assets and SPL tokens
//! through one set of JSON-RPC methods, served on the same endpoint as standard RPC.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::rpc::RpcClient;

/// Most IDs a batch method takes per request. Larger batches are split automatically.
pub const MAX_BATCH: usize = 1000;
/// Largest page size, and the server's default.
pub const MAX_LIMIT: u32 = 1000;

impl RpcClient {
    pub async fn get_asset(&self, id: &str) -> Result<Asset> {
        self.get_asset_with_options(id, &DisplayOptions::default())
            .await
    }

    pub async fn get_asset_with_options(
        &self,
        id: &str,
        options: &DisplayOptions,
    ) -> Result<Asset> {
        self.das("getAsset", with_options(json!({ "id": id }), options))
            .await
    }

    /// One entry per ID, in order, with `None` for IDs that are not indexed assets.
    pub async fn get_asset_batch(&self, ids: &[&str]) -> Result<Vec<Option<Asset>>> {
        self.get_asset_batch_with_options(ids, &DisplayOptions::default())
            .await
    }

    pub async fn get_asset_batch_with_options(
        &self,
        ids: &[&str],
        options: &DisplayOptions,
    ) -> Result<Vec<Option<Asset>>> {
        let mut out = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(MAX_BATCH) {
            let assets: Vec<Option<Asset>> = self
                .das(
                    "getAssetBatch",
                    with_options(json!({ "ids": chunk }), options),
                )
                .await?;
            out.extend(assets);
        }
        Ok(out)
    }

    pub async fn get_asset_proof(&self, id: &str) -> Result<AssetProof> {
        self.das("getAssetProof", json!({ "id": id })).await
    }

    /// Proofs keyed by asset ID, with `None` where no proof was found.
    pub async fn get_asset_proof_batch(
        &self,
        ids: &[&str],
    ) -> Result<HashMap<String, Option<AssetProof>>> {
        let mut out = HashMap::with_capacity(ids.len());
        for chunk in ids.chunks(MAX_BATCH) {
            let proofs: HashMap<String, Option<AssetProof>> = self
                .das("getAssetProofBatch", json!({ "ids": chunk }))
                .await?;
            out.extend(proofs);
        }
        Ok(out)
    }

    /// A compressed asset and its Merkle proof in one call: everything a Bubblegum
    /// transfer, burn or delegate instruction needs.
    pub async fn get_asset_with_proof(&self, id: &str) -> Result<AssetWithProof> {
        self.get_asset_with_proof_with_options(id, &DisplayOptions::default())
            .await
    }

    pub async fn get_asset_with_proof_with_options(
        &self,
        id: &str,
        options: &DisplayOptions,
    ) -> Result<AssetWithProof> {
        self.das(
            "getAssetWithProof",
            with_options(json!({ "id": id }), options),
        )
        .await
    }

    pub async fn get_assets_by_owner(&self, req: &GetAssetsByOwner) -> Result<AssetList> {
        self.das(GetAssetsByOwner::METHOD, req).await
    }

    pub async fn get_assets_by_group(&self, req: &GetAssetsByGroup) -> Result<AssetList> {
        self.das(GetAssetsByGroup::METHOD, req).await
    }

    pub async fn get_assets_by_creator(&self, req: &GetAssetsByCreator) -> Result<AssetList> {
        self.das(GetAssetsByCreator::METHOD, req).await
    }

    pub async fn get_assets_by_authority(&self, req: &GetAssetsByAuthority) -> Result<AssetList> {
        self.das(GetAssetsByAuthority::METHOD, req).await
    }

    pub async fn search_assets(&self, req: &SearchAssets) -> Result<AssetList> {
        self.das(SearchAssets::METHOD, req).await
    }

    pub async fn get_token_accounts(&self, req: &GetTokenAccounts) -> Result<TokenAccountList> {
        if req.owner.is_none() && req.mint.is_none() {
            return Err(Error::Config(
                "getTokenAccounts needs an owner, a mint or both".into(),
            ));
        }
        self.das(GetTokenAccounts::METHOD, req).await
    }

    pub async fn get_nft_editions(&self, req: &GetNftEditions) -> Result<NftEditions> {
        self.das(GetNftEditions::METHOD, req).await
    }

    pub async fn get_signatures_for_asset(
        &self,
        req: &GetSignaturesForAsset,
    ) -> Result<AssetSignatures> {
        if req.id.is_none() && (req.tree.is_none() || req.leaf_index.is_none()) {
            return Err(Error::Config(
                "getSignaturesForAsset needs an id, or a tree and leaf index".into(),
            ));
        }
        self.das(GetSignaturesForAsset::METHOD, req).await
    }

    /// Walks every page of a list request. Requests sorted by `id` page by cursor;
    /// other sorts, and requests that set `page`, page by number.
    ///
    /// ```no_run
    /// # async fn run(client: orbitflare_sdk::RpcClient) -> orbitflare_sdk::Result<()> {
    /// use orbitflare_sdk::das::GetAssetsByGroup;
    ///
    /// let req = GetAssetsByGroup::collection("CDDyz6MHYY8972DPCXiEeReCxGqJSCbxHnimrvoyWrFF");
    /// let mut pages = client.pages(req);
    /// while let Some(page) = pages.next_page().await {
    ///     for asset in page?.items {
    ///         println!("{}", asset.id);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn pages<R: PagedRequest>(&self, mut req: R) -> Pages<'_, R> {
        if (R::PAGES_ONLY || !req.sorted_by_id()) && req.paging_mut().page.is_none() {
            req.paging_mut().page = Some(1);
        }
        Pages {
            client: self,
            req,
            done: false,
        }
    }

    async fn das<T: DeserializeOwned>(&self, method: &str, params: impl Serialize) -> Result<T> {
        let result = self.request(method, serde_json::to_value(params)?).await?;
        serde_json::from_value(result).map_err(|e| Error::Serialization(format!("{method}: {e}")))
    }
}

fn with_options(mut params: Value, options: &DisplayOptions) -> Value {
    if !options.is_empty() {
        params["options"] = json!(options);
    }
    params
}

/// Treats `null` as the type's default.
fn nullable<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(
    d: D,
) -> std::result::Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// Accepts a string, number or bool, since metadata JSON is written by hand.
fn loose_string<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Null => None,
        Value::String(s) => Some(s),
        other => Some(other.to_string()),
    })
}

/* ---------- Requests ---------- */

/// What a response includes. Every option defaults to off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_fungible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_native_balance: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_zero_balance: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_unverified_collections: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_collection_metadata: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_inscription: Option<bool>,
}

impl DisplayOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn show_fungible(mut self, v: bool) -> Self {
        self.show_fungible = Some(v);
        self
    }

    pub fn show_native_balance(mut self, v: bool) -> Self {
        self.show_native_balance = Some(v);
        self
    }

    pub fn show_zero_balance(mut self, v: bool) -> Self {
        self.show_zero_balance = Some(v);
        self
    }

    pub fn show_unverified_collections(mut self, v: bool) -> Self {
        self.show_unverified_collections = Some(v);
        self
    }

    pub fn show_collection_metadata(mut self, v: bool) -> Self {
        self.show_collection_metadata = Some(v);
        self
    }

    pub fn show_inscription(mut self, v: bool) -> Self {
        self.show_inscription = Some(v);
        self
    }
}

/// Paging parameters shared by the list methods. Use one of `page`, `cursor`, or `before`/`after`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Paging {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetSortBy {
    #[default]
    Id,
    Created,
    Updated,
    RecentAction,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SortBy {
    pub sort_by: AssetSortBy,
    pub sort_direction: SortDirection,
}

impl SortBy {
    pub fn new(sort_by: AssetSortBy, sort_direction: SortDirection) -> Self {
        Self {
            sort_by,
            sort_direction,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TokenType {
    #[serde(rename = "fungible")]
    Fungible,
    #[serde(rename = "nonFungible")]
    NonFungible,
    #[serde(rename = "regularNFT")]
    RegularNft,
    #[serde(rename = "compressedNFT")]
    CompressedNft,
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConditionType {
    #[default]
    All,
    Any,
}

/// A list request that [`RpcClient::pages`] can walk.
pub trait PagedRequest: Serialize {
    type Page: Page;
    const METHOD: &'static str;
    /// Pages by number only; the method takes no cursor.
    const PAGES_ONLY: bool = false;

    fn paging_mut(&mut self) -> &mut Paging;

    /// Cursors and ranges only work when sorting by `id`.
    fn sorted_by_id(&self) -> bool {
        true
    }
}

/// One page of a list response.
pub trait Page: DeserializeOwned {
    type Item;
    fn item_count(&self) -> usize;
    fn next_cursor(&self) -> Option<&str>;
    fn into_items(self) -> Vec<Self::Item>;
}

/// Pages of a list request. See [`RpcClient::pages`].
pub struct Pages<'a, R: PagedRequest> {
    client: &'a RpcClient,
    req: R,
    done: bool,
}

impl<R: PagedRequest> Pages<'_, R> {
    pub async fn next_page(&mut self) -> Option<Result<R::Page>> {
        if self.done {
            return None;
        }
        let page: R::Page = match self.client.das(R::METHOD, &self.req).await {
            Ok(page) => page,
            Err(e) => {
                self.done = true;
                return Some(Err(e));
            }
        };
        let paging = self.req.paging_mut();
        let full = page.item_count() >= paging.limit.unwrap_or(MAX_LIMIT) as usize;
        if !full || page.item_count() == 0 {
            self.done = true;
        } else if let Some(n) = paging.page {
            paging.page = Some(n + 1);
        } else if paging.before.is_some() || paging.after.is_some() {
            // A range is one slice, not a walk.
            self.done = true;
        } else {
            match page.next_cursor() {
                Some(c) if !c.is_empty() => paging.cursor = Some(c.to_string()),
                _ => self.done = true,
            }
        }
        Some(Ok(page))
    }

    /// Every item on every remaining page.
    pub async fn collect_items(mut self) -> Result<Vec<<R::Page as Page>::Item>> {
        let mut items = Vec::new();
        while let Some(page) = self.next_page().await {
            items.extend(page?.into_items());
        }
        Ok(items)
    }
}

macro_rules! paging_setters {
    ($ty:ty) => {
        impl $ty {
            pub fn page(mut self, page: u32) -> Self {
                self.paging.page = Some(page);
                self
            }

            pub fn limit(mut self, limit: u32) -> Self {
                self.paging.limit = Some(limit);
                self
            }

            pub fn cursor(mut self, cursor: &str) -> Self {
                self.paging.cursor = Some(cursor.to_string());
                self
            }

            pub fn before(mut self, id: &str) -> Self {
                self.paging.before = Some(id.to_string());
                self
            }

            pub fn after(mut self, id: &str) -> Self {
                self.paging.after = Some(id.to_string());
                self
            }
        }
    };
}

macro_rules! asset_list_request {
    ($ty:ty, $method:literal) => {
        paging_setters!($ty);

        impl $ty {
            pub fn sort_by(mut self, sort_by: SortBy) -> Self {
                self.sort_by = Some(sort_by);
                self
            }

            pub fn options(mut self, options: DisplayOptions) -> Self {
                self.options = (!options.is_empty()).then_some(options);
                self
            }
        }

        impl PagedRequest for $ty {
            type Page = AssetList;
            const METHOD: &'static str = $method;

            fn paging_mut(&mut self) -> &mut Paging {
                &mut self.paging
            }

            fn sorted_by_id(&self) -> bool {
                self.sort_by.is_none_or(|s| s.sort_by == AssetSortBy::Id)
            }
        }
    };
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAssetsByOwner {
    pub owner_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<SortBy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<DisplayOptions>,
    #[serde(flatten)]
    pub paging: Paging,
}

impl GetAssetsByOwner {
    pub fn new(owner: &str) -> Self {
        Self {
            owner_address: owner.to_string(),
            ..Default::default()
        }
    }
}

asset_list_request!(GetAssetsByOwner, "getAssetsByOwner");

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAssetsByGroup {
    pub group_key: String,
    pub group_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<SortBy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<DisplayOptions>,
    #[serde(flatten)]
    pub paging: Paging,
}

impl GetAssetsByGroup {
    pub fn new(group_key: &str, group_value: &str) -> Self {
        Self {
            group_key: group_key.to_string(),
            group_value: group_value.to_string(),
            ..Default::default()
        }
    }

    /// The assets in a collection.
    pub fn collection(collection: &str) -> Self {
        Self::new("collection", collection)
    }
}

asset_list_request!(GetAssetsByGroup, "getAssetsByGroup");

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAssetsByCreator {
    pub creator_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<SortBy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<DisplayOptions>,
    #[serde(flatten)]
    pub paging: Paging,
}

impl GetAssetsByCreator {
    pub fn new(creator: &str) -> Self {
        Self {
            creator_address: creator.to_string(),
            ..Default::default()
        }
    }

    pub fn only_verified(mut self, v: bool) -> Self {
        self.only_verified = Some(v);
        self
    }
}

asset_list_request!(GetAssetsByCreator, "getAssetsByCreator");

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAssetsByAuthority {
    pub authority_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<SortBy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<DisplayOptions>,
    #[serde(flatten)]
    pub paging: Paging,
}

impl GetAssetsByAuthority {
    pub fn new(authority: &str) -> Self {
        Self {
            authority_address: authority.to_string(),
            ..Default::default()
        }
    }
}

asset_list_request!(GetAssetsByAuthority, "getAssetsByAuthority");

/// Combines filters in one query. Every filter must match unless `condition_type` is `Any`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchAssets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_address: Option<String>,
    /// Group key and value, such as `("collection", <address>)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grouping: Option<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface: Option<Interface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<TokenType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_type: Option<OwnershipModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frozen: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burnt: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supply: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supply_mint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty_target_type: Option<RoyaltyModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty_target: Option<String>,
    /// Royalty in basis points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty_amount: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_uri: Option<String>,
    /// Needs `owner_address`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition_type: Option<ConditionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub negate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<SortBy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<DisplayOptions>,
    #[serde(flatten)]
    pub paging: Paging,
}

impl SearchAssets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn owner(mut self, owner: &str) -> Self {
        self.owner_address = Some(owner.to_string());
        self
    }

    pub fn creator(mut self, creator: &str, verified_only: bool) -> Self {
        self.creator_address = Some(creator.to_string());
        self.creator_verified = verified_only.then_some(true);
        self
    }

    pub fn authority(mut self, authority: &str) -> Self {
        self.authority_address = Some(authority.to_string());
        self
    }

    pub fn grouping(mut self, key: &str, value: &str) -> Self {
        self.grouping = Some((key.to_string(), value.to_string()));
        self
    }

    pub fn collection(self, collection: &str) -> Self {
        self.grouping("collection", collection)
    }

    pub fn delegate(mut self, delegate: &str) -> Self {
        self.delegate = Some(delegate.to_string());
        self
    }

    pub fn interface(mut self, interface: Interface) -> Self {
        self.interface = Some(interface);
        self
    }

    pub fn token_type(mut self, token_type: TokenType) -> Self {
        self.token_type = Some(token_type);
        self
    }

    pub fn owner_type(mut self, owner_type: OwnershipModel) -> Self {
        self.owner_type = Some(owner_type);
        self
    }

    pub fn compressed(mut self, v: bool) -> Self {
        self.compressed = Some(v);
        self
    }

    pub fn compressible(mut self, v: bool) -> Self {
        self.compressible = Some(v);
        self
    }

    pub fn frozen(mut self, v: bool) -> Self {
        self.frozen = Some(v);
        self
    }

    pub fn burnt(mut self, v: bool) -> Self {
        self.burnt = Some(v);
        self
    }

    pub fn supply(mut self, supply: u64) -> Self {
        self.supply = Some(supply);
        self
    }

    pub fn supply_mint(mut self, mint: &str) -> Self {
        self.supply_mint = Some(mint.to_string());
        self
    }

    pub fn royalty_target_type(mut self, model: RoyaltyModel) -> Self {
        self.royalty_target_type = Some(model);
        self
    }

    pub fn royalty_target(mut self, target: &str) -> Self {
        self.royalty_target = Some(target.to_string());
        self
    }

    pub fn royalty_amount(mut self, basis_points: u32) -> Self {
        self.royalty_amount = Some(basis_points);
        self
    }

    pub fn json_uri(mut self, uri: &str) -> Self {
        self.json_uri = Some(uri.to_string());
        self
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn condition_type(mut self, condition: ConditionType) -> Self {
        self.condition_type = Some(condition);
        self
    }

    pub fn negate(mut self, v: bool) -> Self {
        self.negate = Some(v);
        self
    }
}

asset_list_request!(SearchAssets, "searchAssets");

#[derive(Debug, Clone, Default, Serialize)]
pub struct GetTokenAccounts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<DisplayOptions>,
    #[serde(flatten)]
    pub paging: Paging,
}

impl GetTokenAccounts {
    pub fn by_owner(owner: &str) -> Self {
        Self::default().owner(owner)
    }

    pub fn by_mint(mint: &str) -> Self {
        Self::default().mint(mint)
    }

    pub fn owner(mut self, owner: &str) -> Self {
        self.owner = Some(owner.to_string());
        self
    }

    pub fn mint(mut self, mint: &str) -> Self {
        self.mint = Some(mint.to_string());
        self
    }

    pub fn show_zero_balance(mut self, v: bool) -> Self {
        self.options = Some(DisplayOptions::new().show_zero_balance(v));
        self
    }
}

paging_setters!(GetTokenAccounts);

impl PagedRequest for GetTokenAccounts {
    type Page = TokenAccountList;
    const METHOD: &'static str = "getTokenAccounts";

    fn paging_mut(&mut self) -> &mut Paging {
        &mut self.paging
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GetNftEditions {
    pub mint: String,
    #[serde(flatten)]
    pub paging: Paging,
}

impl GetNftEditions {
    pub fn new(master_mint: &str) -> Self {
        Self {
            mint: master_mint.to_string(),
            ..Default::default()
        }
    }
}

paging_setters!(GetNftEditions);

impl PagedRequest for GetNftEditions {
    type Page = NftEditions;
    const METHOD: &'static str = "getNftEditions";

    fn paging_mut(&mut self) -> &mut Paging {
        &mut self.paging
    }
}

/// Identify the asset by `id`, or by `tree` and `leaf_index` together. Pages by number only.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSignaturesForAsset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_direction: Option<SortDirection>,
    #[serde(flatten)]
    pub paging: Paging,
}

impl GetSignaturesForAsset {
    pub fn new(id: &str) -> Self {
        Self {
            id: Some(id.to_string()),
            ..Default::default()
        }
    }

    pub fn by_leaf(tree: &str, leaf_index: u64) -> Self {
        Self {
            tree: Some(tree.to_string()),
            leaf_index: Some(leaf_index),
            ..Default::default()
        }
    }

    pub fn sort_direction(mut self, direction: SortDirection) -> Self {
        self.sort_direction = Some(direction);
        self
    }

    pub fn page(mut self, page: u32) -> Self {
        self.paging.page = Some(page);
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.paging.limit = Some(limit);
        self
    }
}

impl PagedRequest for GetSignaturesForAsset {
    type Page = AssetSignatures;
    const METHOD: &'static str = "getSignaturesForAsset";
    const PAGES_ONLY: bool = true;

    fn paging_mut(&mut self) -> &mut Paging {
        &mut self.paging
    }
}

/* ---------- Responses ---------- */

/// The asset's standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Interface {
    #[serde(rename = "V1_NFT")]
    V1Nft,
    #[serde(rename = "V1_PRINT")]
    V1Print,
    #[serde(rename = "LEGACY_NFT")]
    LegacyNft,
    #[serde(rename = "V2_NFT")]
    V2Nft,
    #[serde(rename = "ProgrammableNFT")]
    ProgrammableNft,
    FungibleAsset,
    FungibleToken,
    MplCoreAsset,
    MplCoreCollection,
    MplCoreGroup,
    MplBubblegumV2,
    Identity,
    Executable,
    Custom,
    /// A value this SDK doesn't know yet.
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OwnershipModel {
    /// NFTs.
    #[default]
    Single,
    /// Fungible tokens.
    Token,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoyaltyModel {
    #[default]
    Creators,
    Fanout,
    Single,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Full,
    Royalty,
    Metadata,
    Extension,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Asset {
    pub interface: Interface,
    pub id: String,
    pub last_indexed_slot: Option<u64>,
    pub content: Option<Content>,
    #[serde(deserialize_with = "nullable")]
    pub authorities: Vec<Authority>,
    pub compression: Option<Compression>,
    #[serde(deserialize_with = "nullable")]
    pub grouping: Vec<Group>,
    pub royalty: Option<Royalty>,
    #[serde(deserialize_with = "nullable")]
    pub creators: Vec<Creator>,
    pub ownership: Option<Ownership>,
    pub supply: Option<Supply>,
    #[serde(deserialize_with = "nullable")]
    pub mutable: bool,
    #[serde(deserialize_with = "nullable")]
    pub burnt: bool,
    pub token_info: Option<TokenInfo>,
    /// Token-2022 mint extensions.
    pub mint_extensions: Option<Value>,
    pub inscription: Option<Value>,
    /// MPL Core plugins.
    pub plugins: Option<Value>,
    pub unknown_plugins: Option<Value>,
    pub external_plugins: Option<Value>,
    pub unknown_external_plugins: Option<Value>,
    pub mpl_core_info: Option<MplCoreInfo>,
    pub uses: Option<Uses>,
}

impl Asset {
    pub fn name(&self) -> Option<&str> {
        self.content
            .as_ref()
            .map(|c| c.metadata.name.as_str())
            .filter(|n| !n.is_empty())
    }

    pub fn symbol(&self) -> Option<&str> {
        self.content
            .as_ref()
            .map(|c| c.metadata.symbol.as_str())
            .filter(|s| !s.is_empty())
    }

    pub fn image(&self) -> Option<&str> {
        self.content.as_ref()?.links.image.as_deref()
    }

    /// The owner, or `None` for fungible tokens, which have many holders.
    pub fn owner(&self) -> Option<&str> {
        self.ownership
            .as_ref()
            .map(|o| o.owner.as_str())
            .filter(|o| !o.is_empty())
    }

    /// The collection address, if the asset is in a collection.
    pub fn collection(&self) -> Option<&str> {
        self.grouping
            .iter()
            .find(|g| g.group_key == "collection")
            .and_then(|g| g.group_value.as_deref())
    }

    pub fn is_compressed(&self) -> bool {
        self.compression.as_ref().is_some_and(|c| c.compressed)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Content {
    #[serde(rename = "$schema", deserialize_with = "nullable")]
    pub schema: String,
    #[serde(deserialize_with = "nullable")]
    pub json_uri: String,
    #[serde(deserialize_with = "nullable")]
    pub files: Vec<AssetFile>,
    #[serde(deserialize_with = "nullable")]
    pub metadata: Metadata,
    #[serde(deserialize_with = "nullable")]
    pub links: Links,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetFile {
    pub uri: Option<String>,
    pub cdn_uri: Option<String>,
    pub mime: Option<String>,
    pub quality: Option<Value>,
    pub contexts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Metadata {
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub symbol: String,
    pub description: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub attributes: Vec<Attribute>,
    pub token_standard: Option<String>,
    /// The name in the off-chain JSON.
    pub json_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Attribute {
    #[serde(deserialize_with = "loose_string")]
    pub trait_type: Option<String>,
    pub value: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Links {
    pub image: Option<String>,
    pub animation_url: Option<String>,
    pub external_url: Option<String>,
    #[serde(flatten)]
    pub other: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Authority {
    pub address: String,
    #[serde(default, deserialize_with = "nullable")]
    pub scopes: Vec<Scope>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Compression {
    #[serde(deserialize_with = "nullable")]
    pub eligible: bool,
    #[serde(deserialize_with = "nullable")]
    pub compressed: bool,
    #[serde(deserialize_with = "nullable")]
    pub data_hash: String,
    #[serde(deserialize_with = "nullable")]
    pub creator_hash: String,
    #[serde(deserialize_with = "nullable")]
    pub asset_hash: String,
    #[serde(deserialize_with = "nullable")]
    pub tree: String,
    #[serde(deserialize_with = "nullable")]
    pub seq: u64,
    #[serde(deserialize_with = "nullable")]
    pub leaf_id: u64,
    /// Bubblegum v2 only.
    pub collection_hash: Option<String>,
    /// Bubblegum v2 only.
    pub asset_data_hash: Option<String>,
    /// Bubblegum v2 only.
    pub flags: Option<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Group {
    pub group_key: String,
    pub group_value: Option<String>,
    /// Set to `false` on unverified collections, which appear only with `show_unverified_collections`.
    pub verified: Option<bool>,
    /// With `show_collection_metadata`.
    pub collection_metadata: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Royalty {
    pub royalty_model: RoyaltyModel,
    pub target: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub percent: f64,
    #[serde(deserialize_with = "nullable")]
    pub basis_points: u32,
    #[serde(deserialize_with = "nullable")]
    pub primary_sale_happened: bool,
    #[serde(deserialize_with = "nullable")]
    pub locked: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Creator {
    pub address: String,
    /// Percent of royalties.
    #[serde(deserialize_with = "nullable")]
    pub share: u8,
    #[serde(deserialize_with = "nullable")]
    pub verified: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Ownership {
    /// Empty for fungible tokens.
    #[serde(deserialize_with = "nullable")]
    pub owner: String,
    pub ownership_model: OwnershipModel,
    #[serde(deserialize_with = "nullable")]
    pub frozen: bool,
    #[serde(deserialize_with = "nullable")]
    pub delegated: bool,
    pub delegate: Option<String>,
    pub non_transferable: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Supply {
    pub print_max_supply: Option<u64>,
    pub print_current_supply: Option<u64>,
    pub edition_nonce: Option<u8>,
    /// Prints only.
    pub edition_number: Option<u64>,
    /// Prints only.
    pub master_edition_mint: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TokenInfo {
    pub symbol: Option<String>,
    /// The queried owner's balance, in base units.
    pub balance: Option<u64>,
    /// Total supply, in base units.
    pub supply: Option<u64>,
    pub decimals: Option<u8>,
    pub token_program: Option<String>,
    pub associated_token_address: Option<String>,
    pub mint_authority: Option<String>,
    pub freeze_authority: Option<String>,
    pub price_info: Option<PriceInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PriceInfo {
    #[serde(deserialize_with = "nullable")]
    pub price_per_token: f64,
    /// Value of the owner's balance.
    pub total_price: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    pub currency: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MplCoreInfo {
    pub num_minted: Option<u64>,
    pub current_size: Option<u64>,
    pub plugins_json_version: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Uses {
    #[serde(deserialize_with = "nullable")]
    pub use_method: String,
    #[serde(deserialize_with = "nullable")]
    pub remaining: u64,
    #[serde(deserialize_with = "nullable")]
    pub total: u64,
}

/// The owner's SOL balance, with `show_native_balance`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NativeBalance {
    #[serde(deserialize_with = "nullable")]
    pub lamports: u64,
    pub price_per_sol: Option<f64>,
    /// In USD.
    pub total_price: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetProof {
    pub root: String,
    /// Sibling hashes from the leaf up to the root.
    #[serde(deserialize_with = "nullable")]
    pub proof: Vec<String>,
    pub node_index: u64,
    pub leaf: String,
    pub tree_id: String,
    pub last_indexed_slot: Option<u64>,
}

impl AssetProof {
    /// The proof nodes a transaction must pass, leaving out the `canopy_depth` top levels the
    /// tree already stores on chain.
    pub fn proof_path(&self, canopy_depth: usize) -> &[String] {
        &self.proof[..self.proof.len().saturating_sub(canopy_depth)]
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetWithProof {
    pub asset: Asset,
    pub proof: AssetProof,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetList {
    pub last_indexed_slot: Option<u64>,
    /// Items in this page.
    pub total: u32,
    pub limit: u32,
    pub page: Option<u32>,
    pub cursor: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub items: Vec<Asset>,
    #[serde(rename = "nativeBalance")]
    pub native_balance: Option<NativeBalance>,
}

impl Page for AssetList {
    type Item = Asset;

    fn item_count(&self) -> usize {
        self.items.len()
    }

    fn next_cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    fn into_items(self) -> Vec<Asset> {
        self.items
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TokenAccount {
    pub address: String,
    pub mint: String,
    pub owner: String,
    /// In base units.
    #[serde(deserialize_with = "nullable")]
    pub amount: u64,
    pub delegate: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub delegated_amount: u64,
    pub close_authority: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub frozen: bool,
    /// Token-2022 account extensions.
    pub token_extensions: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TokenAccountList {
    pub last_indexed_slot: Option<u64>,
    pub total: u32,
    pub limit: u32,
    pub page: Option<u32>,
    pub cursor: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub token_accounts: Vec<TokenAccount>,
}

impl Page for TokenAccountList {
    type Item = TokenAccount;

    fn item_count(&self) -> usize {
        self.token_accounts.len()
    }

    fn next_cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    fn into_items(self) -> Vec<TokenAccount> {
        self.token_accounts
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NftEdition {
    pub mint: String,
    pub edition_address: String,
    pub edition: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NftEditions {
    pub master_edition_address: String,
    /// Editions printed so far.
    pub supply: u64,
    /// `None` when unlimited.
    pub max_supply: Option<u64>,
    pub total: u32,
    pub limit: u32,
    pub page: Option<u32>,
    pub cursor: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub editions: Vec<NftEdition>,
}

impl Page for NftEditions {
    type Item = NftEdition;

    fn item_count(&self) -> usize {
        self.editions.len()
    }

    fn next_cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    fn into_items(self) -> Vec<NftEdition> {
        self.editions
    }
}

/// A transaction that changed a compressed asset, sent as a `[signature, instruction]` pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "(String, String)", into = "(String, String)")]
pub struct AssetSignature {
    pub signature: String,
    /// The Bubblegum instruction, such as `MintToCollectionV1`, `Transfer` or `Burn`.
    pub instruction: String,
}

impl From<(String, String)> for AssetSignature {
    fn from((signature, instruction): (String, String)) -> Self {
        Self {
            signature,
            instruction,
        }
    }
}

impl From<AssetSignature> for (String, String) {
    fn from(s: AssetSignature) -> Self {
        (s.signature, s.instruction)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetSignatures {
    pub last_indexed_slot: Option<u64>,
    pub total: u32,
    pub limit: u32,
    pub page: Option<u32>,
    #[serde(deserialize_with = "nullable")]
    pub items: Vec<AssetSignature>,
}

impl Page for AssetSignatures {
    type Item = AssetSignature;

    fn item_count(&self) -> usize {
        self.items.len()
    }

    fn next_cursor(&self) -> Option<&str> {
        None
    }

    fn into_items(self) -> Vec<AssetSignature> {
        self.items
    }
}
