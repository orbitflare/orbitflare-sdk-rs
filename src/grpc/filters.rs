use std::collections::HashMap;

use orbitflare_sdk_proto::geyser as pb;

use crate::filters::{Lamports, Memcmp, MemcmpData};

#[derive(Clone, Debug, Default)]
pub struct TransactionFilter {
    vote: Option<bool>,
    failed: Option<bool>,
    signature: Option<String>,
    account_include: Vec<String>,
    account_exclude: Vec<String>,
    account_required: Vec<String>,
}

impl TransactionFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vote(mut self, vote: bool) -> Self {
        self.vote = Some(vote);
        self
    }

    pub fn failed(mut self, failed: bool) -> Self {
        self.failed = Some(failed);
        self
    }

    pub fn signature(mut self, signature: impl Into<String>) -> Self {
        self.signature = Some(signature.into());
        self
    }

    pub fn account_include<I, S>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.account_include
            .extend(accounts.into_iter().map(Into::into));
        self
    }

    pub fn account_exclude<I, S>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.account_exclude
            .extend(accounts.into_iter().map(Into::into));
        self
    }

    pub fn account_required<I, S>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.account_required
            .extend(accounts.into_iter().map(Into::into));
        self
    }
}

impl From<TransactionFilter> for pb::SubscribeRequestFilterTransactions {
    fn from(f: TransactionFilter) -> Self {
        Self {
            vote: f.vote,
            failed: f.failed,
            signature: f.signature,
            account_include: f.account_include,
            account_exclude: f.account_exclude,
            account_required: f.account_required,
        }
    }
}

#[derive(Clone, Debug)]
enum Condition {
    Memcmp(Memcmp),
    Datasize(u64),
    TokenAccountState(bool),
    Lamports(Lamports),
}

#[derive(Clone, Debug, Default)]
pub struct AccountFilter {
    account: Vec<String>,
    owner: Vec<String>,
    conditions: Vec<Condition>,
    nonempty_txn_signature: Option<bool>,
}

impl AccountFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn account<I, S>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.account.extend(accounts.into_iter().map(Into::into));
        self
    }

    pub fn owner<I, S>(mut self, owners: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.owner.extend(owners.into_iter().map(Into::into));
        self
    }

    pub fn memcmp(mut self, memcmp: Memcmp) -> Self {
        self.conditions.push(Condition::Memcmp(memcmp));
        self
    }

    pub fn datasize(mut self, datasize: u64) -> Self {
        self.conditions.push(Condition::Datasize(datasize));
        self
    }

    pub fn token_account_state(mut self, state: bool) -> Self {
        self.conditions.push(Condition::TokenAccountState(state));
        self
    }

    pub fn lamports(mut self, lamports: Lamports) -> Self {
        self.conditions.push(Condition::Lamports(lamports));
        self
    }

    pub fn nonempty_txn_signature(mut self, value: bool) -> Self {
        self.nonempty_txn_signature = Some(value);
        self
    }
}

fn memcmp_to_pb(m: Memcmp) -> pb::SubscribeRequestFilterAccountsFilterMemcmp {
    use pb::subscribe_request_filter_accounts_filter_memcmp::Data;
    let data = match m.data {
        MemcmpData::Bytes(b) => Data::Bytes(b),
        MemcmpData::Base58(s) => Data::Base58(s),
        MemcmpData::Base64(s) => Data::Base64(s),
    };
    pb::SubscribeRequestFilterAccountsFilterMemcmp {
        offset: m.offset,
        data: Some(data),
    }
}

fn lamports_to_pb(l: Lamports) -> pb::SubscribeRequestFilterAccountsFilterLamports {
    use pb::subscribe_request_filter_accounts_filter_lamports::Cmp;
    let cmp = match l {
        Lamports::Eq(v) => Cmp::Eq(v),
        Lamports::Ne(v) => Cmp::Ne(v),
        Lamports::Lt(v) => Cmp::Lt(v),
        Lamports::Gt(v) => Cmp::Gt(v),
    };
    pb::SubscribeRequestFilterAccountsFilterLamports { cmp: Some(cmp) }
}

impl From<AccountFilter> for pb::SubscribeRequestFilterAccounts {
    fn from(f: AccountFilter) -> Self {
        use pb::subscribe_request_filter_accounts_filter::Filter;
        let filters = f
            .conditions
            .into_iter()
            .map(|c| {
                let filter = match c {
                    Condition::Memcmp(m) => Filter::Memcmp(memcmp_to_pb(m)),
                    Condition::Datasize(d) => Filter::Datasize(d),
                    Condition::TokenAccountState(s) => Filter::TokenAccountState(s),
                    Condition::Lamports(l) => Filter::Lamports(lamports_to_pb(l)),
                };
                pb::SubscribeRequestFilterAccountsFilter {
                    filter: Some(filter),
                }
            })
            .collect();
        Self {
            account: f.account,
            owner: f.owner,
            filters,
            nonempty_txn_signature: f.nonempty_txn_signature,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SlotFilter {
    filter_by_commitment: Option<bool>,
    interslot_updates: Option<bool>,
}

impl SlotFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn filter_by_commitment(mut self, value: bool) -> Self {
        self.filter_by_commitment = Some(value);
        self
    }

    pub fn interslot_updates(mut self, value: bool) -> Self {
        self.interslot_updates = Some(value);
        self
    }
}

impl From<SlotFilter> for pb::SubscribeRequestFilterSlots {
    fn from(f: SlotFilter) -> Self {
        Self {
            filter_by_commitment: f.filter_by_commitment,
            interslot_updates: f.interslot_updates,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct BlockFilter {
    account_include: Vec<String>,
    include_transactions: Option<bool>,
    include_accounts: Option<bool>,
    include_entries: Option<bool>,
}

impl BlockFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn account_include<I, S>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.account_include
            .extend(accounts.into_iter().map(Into::into));
        self
    }

    pub fn include_transactions(mut self, value: bool) -> Self {
        self.include_transactions = Some(value);
        self
    }

    pub fn include_accounts(mut self, value: bool) -> Self {
        self.include_accounts = Some(value);
        self
    }

    pub fn include_entries(mut self, value: bool) -> Self {
        self.include_entries = Some(value);
        self
    }
}

impl From<BlockFilter> for pb::SubscribeRequestFilterBlocks {
    fn from(f: BlockFilter) -> Self {
        Self {
            account_include: f.account_include,
            include_transactions: f.include_transactions,
            include_accounts: f.include_accounts,
            include_entries: f.include_entries,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DeshredTransactionFilter {
    vote: Option<bool>,
    account_include: Vec<String>,
    account_exclude: Vec<String>,
    account_required: Vec<String>,
}

impl DeshredTransactionFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vote(mut self, vote: bool) -> Self {
        self.vote = Some(vote);
        self
    }

    pub fn account_include<I, S>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.account_include
            .extend(accounts.into_iter().map(Into::into));
        self
    }

    pub fn account_exclude<I, S>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.account_exclude
            .extend(accounts.into_iter().map(Into::into));
        self
    }

    pub fn account_required<I, S>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.account_required
            .extend(accounts.into_iter().map(Into::into));
        self
    }
}

impl From<DeshredTransactionFilter> for pb::SubscribeRequestFilterDeshredTransactions {
    fn from(f: DeshredTransactionFilter) -> Self {
        Self {
            vote: f.vote,
            account_include: f.account_include,
            account_exclude: f.account_exclude,
            account_required: f.account_required,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Commitment {
    Processed,
    Confirmed,
    Finalized,
}

impl Commitment {
    pub fn as_i32(self) -> i32 {
        match self {
            Commitment::Processed => 0,
            Commitment::Confirmed => 1,
            Commitment::Finalized => 2,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SubscribeRequestBuilder {
    accounts: HashMap<String, AccountFilter>,
    slots: HashMap<String, SlotFilter>,
    transactions: HashMap<String, TransactionFilter>,
    transactions_status: HashMap<String, TransactionFilter>,
    blocks: HashMap<String, BlockFilter>,
    blocks_meta: Vec<String>,
    entry: Vec<String>,
    commitment: Option<Commitment>,
    accounts_data_slice: Vec<(u64, u64)>,
    from_slot: Option<u64>,
}

impl SubscribeRequestBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn accounts(mut self, name: impl Into<String>, filter: AccountFilter) -> Self {
        self.accounts.insert(name.into(), filter);
        self
    }

    pub fn slots(mut self, name: impl Into<String>, filter: SlotFilter) -> Self {
        self.slots.insert(name.into(), filter);
        self
    }

    pub fn transactions(mut self, name: impl Into<String>, filter: TransactionFilter) -> Self {
        self.transactions.insert(name.into(), filter);
        self
    }

    pub fn transactions_status(
        mut self,
        name: impl Into<String>,
        filter: TransactionFilter,
    ) -> Self {
        self.transactions_status.insert(name.into(), filter);
        self
    }

    pub fn blocks(mut self, name: impl Into<String>, filter: BlockFilter) -> Self {
        self.blocks.insert(name.into(), filter);
        self
    }

    pub fn blocks_meta(mut self, name: impl Into<String>) -> Self {
        self.blocks_meta.push(name.into());
        self
    }

    pub fn entry(mut self, name: impl Into<String>) -> Self {
        self.entry.push(name.into());
        self
    }

    pub fn commitment(mut self, commitment: Commitment) -> Self {
        self.commitment = Some(commitment);
        self
    }

    pub fn accounts_data_slice(mut self, offset: u64, length: u64) -> Self {
        self.accounts_data_slice.push((offset, length));
        self
    }

    pub fn from_slot(mut self, slot: u64) -> Self {
        self.from_slot = Some(slot);
        self
    }

    pub fn build(self) -> pb::SubscribeRequest {
        let accounts = self
            .accounts
            .into_iter()
            .map(|(k, v)| (k, v.into()))
            .collect();
        let slots = self.slots.into_iter().map(|(k, v)| (k, v.into())).collect();
        let transactions = self
            .transactions
            .into_iter()
            .map(|(k, v)| (k, v.into()))
            .collect();
        let transactions_status = self
            .transactions_status
            .into_iter()
            .map(|(k, v)| (k, v.into()))
            .collect();
        let blocks = self
            .blocks
            .into_iter()
            .map(|(k, v)| (k, v.into()))
            .collect();
        let blocks_meta = self
            .blocks_meta
            .into_iter()
            .map(|k| (k, pb::SubscribeRequestFilterBlocksMeta {}))
            .collect();
        let entry = self
            .entry
            .into_iter()
            .map(|k| (k, pb::SubscribeRequestFilterEntry {}))
            .collect();
        let accounts_data_slice = self
            .accounts_data_slice
            .into_iter()
            .map(|(offset, length)| pb::SubscribeRequestAccountsDataSlice { offset, length })
            .collect();

        pb::SubscribeRequest {
            accounts,
            slots,
            transactions,
            transactions_status,
            blocks,
            blocks_meta,
            entry,
            commitment: self.commitment.map(Commitment::as_i32),
            accounts_data_slice,
            ping: Some(pb::SubscribeRequestPing { id: 1 }),
            from_slot: self.from_slot,
        }
    }
}
