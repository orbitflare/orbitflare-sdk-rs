#![cfg(feature = "grpc")]

use std::collections::HashSet;
use std::time::Duration;

use orbitflare_sdk::GeyserClientBuilder;
use orbitflare_sdk::grpc::{
    BlockFooterFilter, Commitment, SlotFilter, SubscribeRequestBuilder, TransactionFilter,
};
use orbitflare_sdk::proto::geyser::SubscribeRequest;
use orbitflare_sdk::proto::geyser::SubscribeUpdate;

async fn collect(request: SubscribeRequest, dur: Duration) -> Vec<SubscribeUpdate> {
    let client = GeyserClientBuilder::new()
        .build()
        .expect("set ORBITFLARE_GRPC_URL");
    let mut stream = client.subscribe(request);
    let mut out = Vec::new();
    let _ = tokio::time::timeout(dur, async {
        while let Some(u) = stream.next().await {
            out.push(u.expect("stream error"));
        }
    })
    .await;
    out
}

use orbitflare_sdk::proto::geyser::subscribe_update::UpdateOneof as U;

#[tokio::test]
#[ignore]
async fn live_block_footers_with_certificates() {
    let req = SubscribeRequestBuilder::new()
        .block_footer("f", BlockFooterFilter::new().include_certificates(true))
        .commitment(Commitment::Processed)
        .build();
    let updates = collect(req, Duration::from_secs(20)).await;

    let mut slots = HashSet::new();
    let mut with_final = 0;
    for u in &updates {
        let Some(U::BlockFooter(f)) = &u.update_oneof else {
            continue;
        };
        assert_eq!(u.filters, vec!["f".to_string()]);
        assert!(
            slots.insert((f.slot, f.bank_id)),
            "duplicate footer {}",
            f.slot
        );
        assert!(f.bank_id > 0);
        assert_eq!(f.bank_hash.len(), 32);
        assert!(f.block_user_agent.len() <= 255);
        assert!(f.block_producer_time_nanos > 0);
        if let Some(cert) = &f.block_final_cert {
            with_final += 1;
            assert!(cert.slot <= f.slot);
            for agg in [&cert.final_aggregate, &cert.notar_aggregate]
                .into_iter()
                .flatten()
            {
                assert_eq!(agg.signature.len(), 96);
                assert!(agg.block_id.is_empty() || agg.block_id.len() == 32);
                assert!(!agg.signer_bitmap.is_empty());
            }
        }
    }
    assert!(!slots.is_empty(), "no footers in {} updates", updates.len());
    assert!(with_final > 0, "no footer carried a final cert");
    println!("{} footers, {} with final cert", slots.len(), with_final);
}

#[tokio::test]
#[ignore]
async fn live_block_footers_without_certificates() {
    let req = SubscribeRequestBuilder::new()
        .block_footer("f", BlockFooterFilter::new())
        .commitment(Commitment::Processed)
        .build();
    let updates = collect(req, Duration::from_secs(10)).await;

    let footers: Vec<_> = updates
        .iter()
        .filter_map(|u| match &u.update_oneof {
            Some(U::BlockFooter(f)) => Some(f),
            _ => None,
        })
        .collect();
    assert!(!footers.is_empty(), "no footers");
    for f in footers {
        assert_eq!(f.bank_hash.len(), 32);
        assert!(f.block_final_cert.is_none());
        assert!(f.skip_reward_cert.is_none());
        assert!(f.notar_reward_cert.is_none());
    }
}

#[tokio::test]
#[ignore]
async fn live_bank_ids_and_v1_transactions() {
    let req = SubscribeRequestBuilder::new()
        .slots("s", SlotFilter::new().filter_by_commitment(false))
        .transactions(
            "t",
            TransactionFilter::new()
                .vote(false)
                .account_include(["ComputeBudget111111111111111111111111111111"]),
        )
        .commitment(Commitment::Processed)
        .build();
    let updates = collect(req, Duration::from_secs(20)).await;

    let (mut txs, mut v1, mut slots) = (0, 0, 0);
    for u in &updates {
        match &u.update_oneof {
            Some(U::Transaction(t)) => {
                txs += 1;
                assert!(t.bank_id > 0, "transaction without bank_id");
                let msg = t
                    .transaction
                    .as_ref()
                    .and_then(|i| i.transaction.as_ref())
                    .and_then(|x| x.message.as_ref())
                    .expect("transaction without message");
                if msg.config.is_some() {
                    v1 += 1;
                    assert!(msg.versioned, "v1 tx with versioned=false");
                }
            }
            Some(U::Slot(s)) => {
                slots += 1;
                assert!(s.slot > 0);
            }
            _ => {}
        }
    }
    assert!(txs > 0 && slots > 0, "txs={txs} slots={slots}");
    println!("{txs} txs ({v1} v1), {slots} slot updates");
}
