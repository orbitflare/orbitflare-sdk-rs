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

const JUPITER: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
const PUMP: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

fn watching(account: &str) -> SubscribeRequest {
    SubscribeRequestBuilder::new()
        .transactions(
            "t",
            TransactionFilter::new()
                .vote(false)
                .account_include([account]),
        )
        .slots("s", SlotFilter::new())
        .commitment(Commitment::Processed)
        .build()
}

fn accounts_of(update: &SubscribeUpdate) -> Option<Vec<String>> {
    let Some(U::Transaction(tx)) = &update.update_oneof else {
        return None;
    };
    let info = tx.transaction.as_ref()?;
    let mut keys: Vec<Vec<u8>> = info
        .transaction
        .as_ref()?
        .message
        .as_ref()?
        .account_keys
        .clone();
    if let Some(meta) = &info.meta {
        keys.extend(meta.loaded_writable_addresses.iter().cloned());
        keys.extend(meta.loaded_readonly_addresses.iter().cloned());
    }
    Some(keys.iter().map(|k| bs58::encode(k).into_string()).collect())
}

#[tokio::test]
#[ignore]
async fn live_update_replaces_filters_without_reconnecting() {
    use orbitflare_sdk::ConnectionEvent;
    let client = GeyserClientBuilder::new()
        .build()
        .expect("set ORBITFLARE_GRPC_URL");
    let mut stream = client.subscribe(watching(JUPITER));
    let mut events = stream.events();
    let next_tx = async |stream: &mut orbitflare_sdk::GeyserStream| loop {
        let update = stream.next().await.unwrap().expect("stream error");
        if let Some(accounts) = accounts_of(&update) {
            return accounts;
        }
    };
    let first = tokio::time::timeout(Duration::from_secs(30), next_tx(&mut stream))
        .await
        .expect("no Jupiter transaction");
    assert!(first.iter().any(|a| a == JUPITER));

    stream.update(watching(PUMP)).unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        let _ = tokio::time::timeout_at(deadline, stream.next()).await;
    }
    for _ in 0..20 {
        let accounts = tokio::time::timeout(Duration::from_secs(30), next_tx(&mut stream))
            .await
            .expect("no pump.fun transaction after the update");
        assert!(
            accounts.iter().any(|a| a == PUMP),
            "old filter still applied"
        );
    }
    let mut connects = 0;
    while let Some(event) = events.try_recv() {
        match event {
            ConnectionEvent::Connected { .. } => connects += 1,
            ConnectionEvent::Disconnected { error, .. } => panic!("disconnected: {error}"),
            _ => {}
        }
    }
    assert_eq!(connects, 1, "the update must not reconnect");
    stream.close();
}

#[tokio::test]
#[ignore]
async fn live_subscribe_with_replays_from_the_requested_slot() {
    use orbitflare_sdk::ConnectionEvent;
    let client = GeyserClientBuilder::new()
        .build()
        .expect("set ORBITFLARE_GRPC_URL");
    let mut probe = client.subscribe(watching(JUPITER));
    let now = loop {
        if let Some(U::Slot(s)) = probe.next().await.unwrap().unwrap().update_oneof {
            break s.slot;
        }
    };
    probe.close();

    let from = now - 150;
    let mut stream = client.subscribe_with(move |info| {
        assert!(!info.reconnect);
        SubscribeRequest {
            from_slot: Some(from),
            ..watching(JUPITER)
        }
    });
    let first_tx_slot = loop {
        let update = stream.next().await.unwrap().expect("stream error");
        if let Some(U::Transaction(tx)) = update.update_oneof {
            break tx.slot;
        }
    };
    assert!(
        first_tx_slot >= from && first_tx_slot < now,
        "replay starts at {from}, got a first transaction at {first_tx_slot} (now {now})"
    );
    stream.close();

    let mut old = client.subscribe_with(|_| SubscribeRequest {
        from_slot: Some(1),
        ..watching(JUPITER)
    });
    let mut events = old.events();
    old.next().await.unwrap().expect("stream error");
    let mut clamped = None;
    while let Some(event) = events.try_recv() {
        if let ConnectionEvent::ReplayClamped {
            requested,
            available,
        } = event
        {
            clamped = Some((requested, available));
        }
    }
    let (requested, available) = clamped.expect("expected the slot to be clamped");
    assert_eq!(requested, 1);
    assert!(available > 1 && available <= now, "{available}");
    old.close();
}
