use serde::Deserialize;
use std::collections::HashMap;

use crate::error::{Error, Result};

#[derive(Deserialize)]
pub struct GeyserStreamConfig {
    #[serde(default)]
    pub transactions: HashMap<String, TransactionFilter>,
    #[serde(default)]
    pub accounts: HashMap<String, AccountFilter>,
    #[serde(default)]
    pub slots: HashMap<String, SlotFilter>,
    #[serde(default = "default_commitment")]
    pub commitment: String,
}

#[derive(Deserialize)]
pub struct JetstreamStreamConfig {
    #[serde(default)]
    pub transactions: HashMap<String, TransactionFilter>,
    #[serde(default)]
    pub accounts: HashMap<String, AccountFilter>,
}

#[derive(Deserialize)]
pub struct TransactionFilter {
    #[serde(default)]
    pub vote: Option<bool>,
    #[serde(default)]
    pub failed: Option<bool>,
    #[serde(default)]
    pub account_include: Vec<String>,
    #[serde(default)]
    pub account_exclude: Vec<String>,
    #[serde(default)]
    pub account_required: Vec<String>,
}

#[derive(Deserialize)]
pub struct AccountFilter {
    #[serde(default)]
    pub account: Vec<String>,
    #[serde(default)]
    pub owner: Vec<String>,
}

#[derive(Deserialize)]
pub struct SlotFilter {
    #[serde(default)]
    pub filter_by_commitment: Option<bool>,
}

fn default_commitment() -> String {
    "confirmed".into()
}

pub(crate) fn read_yaml<T: serde::de::DeserializeOwned>(path: &str) -> Result<T> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| Error::Config(format!("failed to read {path}: {e}")))?;
    let expanded = shellexpand::env(&raw)
        .map_err(|e| Error::Config(format!("failed to expand env vars: {e}")))?;
    serde_yml::from_str(&expanded).map_err(|e| Error::Config(format!("failed to parse yaml: {e}")))
}

fn parse_commitment(s: &str) -> i32 {
    match s.to_lowercase().as_str() {
        "processed" => 0,
        "finalized" => 2,
        _ => 1,
    }
}

impl GeyserStreamConfig {
    pub fn to_subscribe_request(&self) -> orbitflare_sdk_proto::geyser::SubscribeRequest {
        use orbitflare_sdk_proto::geyser::*;

        let transactions = self
            .transactions
            .iter()
            .map(|(name, f)| {
                (
                    name.clone(),
                    SubscribeRequestFilterTransactions {
                        vote: f.vote,
                        failed: f.failed,
                        signature: None,
                        account_include: f.account_include.clone(),
                        account_exclude: f.account_exclude.clone(),
                        account_required: f.account_required.clone(),
                    },
                )
            })
            .collect();

        let accounts = self
            .accounts
            .iter()
            .map(|(name, f)| {
                (
                    name.clone(),
                    SubscribeRequestFilterAccounts {
                        account: f.account.clone(),
                        owner: f.owner.clone(),
                        filters: vec![],
                        nonempty_txn_signature: None,
                    },
                )
            })
            .collect();

        let slots = self
            .slots
            .iter()
            .map(|(name, f)| {
                (
                    name.clone(),
                    SubscribeRequestFilterSlots {
                        filter_by_commitment: f.filter_by_commitment,
                        interslot_updates: None,
                    },
                )
            })
            .collect();

        SubscribeRequest {
            accounts,
            slots,
            transactions,
            transactions_status: HashMap::new(),
            blocks: HashMap::new(),
            blocks_meta: HashMap::new(),
            entry: HashMap::new(),
            commitment: Some(parse_commitment(&self.commitment)),
            accounts_data_slice: vec![],
            ping: Some(SubscribeRequestPing { id: 1 }),
            from_slot: None,
        }
    }
}

impl JetstreamStreamConfig {
    pub fn to_subscribe_request(&self) -> orbitflare_sdk_proto::jetstream::SubscribeRequest {
        use orbitflare_sdk_proto::jetstream::*;

        let transactions = self
            .transactions
            .iter()
            .map(|(name, f)| {
                (
                    name.clone(),
                    SubscribeRequestFilterTransactions {
                        account_include: f.account_include.clone(),
                        account_exclude: f.account_exclude.clone(),
                        account_required: f.account_required.clone(),
                    },
                )
            })
            .collect();

        let accounts = self
            .accounts
            .iter()
            .map(|(name, f)| {
                (
                    name.clone(),
                    SubscribeRequestFilterAccounts {
                        account: f.account.clone(),
                        owner: f.owner.clone(),
                        filters: vec![],
                    },
                )
            })
            .collect();

        SubscribeRequest {
            transactions,
            accounts,
            ping: Some(SubscribeRequestPing { id: 1 }),
        }
    }
}
