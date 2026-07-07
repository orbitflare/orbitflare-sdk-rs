#[cfg(feature = "grpc")]
#[test]
fn grpc_builder_maps_all_fields() {
    use orbitflare_sdk::grpc::{
        AccountFilter, BlockFilter, Commitment, Lamports, Memcmp, SlotFilter,
        SubscribeRequestBuilder, TransactionFilter,
    };

    let req = SubscribeRequestBuilder::new()
        .transactions(
            "tx",
            TransactionFilter::new()
                .vote(false)
                .failed(false)
                .signature("sig")
                .account_include(["a", "b"])
                .account_exclude(["c"]),
        )
        .accounts(
            "acc",
            AccountFilter::new()
                .owner(["owner"])
                .memcmp(Memcmp::base58(0, "data"))
                .datasize(165)
                .lamports(Lamports::Gt(100))
                .token_account_state(true)
                .nonempty_txn_signature(true),
        )
        .slots("sl", SlotFilter::new().filter_by_commitment(true))
        .blocks("bl", BlockFilter::new().include_transactions(true))
        .blocks_meta("bm")
        .entry("en")
        .accounts_data_slice(8, 32)
        .commitment(Commitment::Confirmed)
        .from_slot(42)
        .build();

    assert_eq!(req.commitment, Some(1));
    assert_eq!(req.from_slot, Some(42));

    let tx = &req.transactions["tx"];
    assert_eq!(tx.vote, Some(false));
    assert_eq!(tx.signature.as_deref(), Some("sig"));
    assert_eq!(tx.account_include, vec!["a", "b"]);
    assert_eq!(tx.account_exclude, vec!["c"]);

    let acc = &req.accounts["acc"];
    assert_eq!(acc.owner, vec!["owner"]);
    assert_eq!(acc.filters.len(), 4);
    assert_eq!(acc.nonempty_txn_signature, Some(true));

    assert_eq!(req.slots["sl"].filter_by_commitment, Some(true));
    assert_eq!(req.blocks["bl"].include_transactions, Some(true));
    assert!(req.blocks_meta.contains_key("bm"));
    assert!(req.entry.contains_key("en"));
    assert_eq!(req.accounts_data_slice.len(), 1);
    assert_eq!(req.accounts_data_slice[0].offset, 8);
    assert_eq!(req.accounts_data_slice[0].length, 32);
}

#[cfg(feature = "jetstream")]
#[test]
fn jetstream_v1_builder_maps_fields() {
    use orbitflare_sdk::jetstream::{SubscribeRequestBuilder, TransactionFilter};

    let req = SubscribeRequestBuilder::new()
        .transactions(
            "tx",
            TransactionFilter::new()
                .account_include(["a"])
                .account_required(["r"]),
        )
        .build();

    assert_eq!(req.transactions["tx"].account_include, vec!["a"]);
    assert_eq!(req.transactions["tx"].account_required, vec!["r"]);
    assert!(req.accounts.is_empty());
    assert!(req.ping.is_some());
}

#[cfg(feature = "jetstream")]
#[test]
fn jetstream_v2_filter_with_id() {
    use orbitflare_sdk::jetstream::v2::TransactionFilter;

    let tx = TransactionFilter::new()
        .account_include(["a", "b"])
        .account_exclude(["c"])
        .include_enrichment(true)
        .with_id("f1");

    assert_eq!(tx.filter_id, "f1");
    let filter = tx.filter.expect("filter set");
    assert_eq!(filter.account_include, vec!["a", "b"]);
    assert_eq!(filter.account_exclude, vec!["c"]);
    assert!(filter.include_enrichment);
}
