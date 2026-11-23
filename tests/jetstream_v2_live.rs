#![cfg(feature = "jetstream")]

use std::time::Duration;

use orbitflare_sdk::jetstream::v2::{JetstreamClient, JetstreamClientBuilder, TransactionFilter};
use orbitflare_sdk::proto::jetstream::v2::subscribe_transactions_response::Payload;
use orbitflare_sdk::proto::jetstream::v2::{SlotEvent, SlotStatus, TransactionMessage};

const COMPUTE_BUDGET: &str = "ComputeBudget111111111111111111111111111111";

fn client() -> JetstreamClient {
    JetstreamClientBuilder::new()
        .build()
        .expect("set ORBITFLARE_JETSTREAM_URL")
}

async fn collect_slots(
    mut stream: orbitflare_sdk::jetstream::v2::SlotStream,
    dur: Duration,
) -> Vec<SlotEvent> {
    let mut events = Vec::new();
    let _ = tokio::time::timeout(dur, async {
        while let Some(ev) = stream.next().await {
            events.push(ev.expect("slot stream error"));
        }
    })
    .await;
    stream.close();
    events
}

#[tokio::test]
#[ignore]
async fn live_unary() {
    let c = client();
    assert!(!c.get_version().await.unwrap().is_empty());
    assert_eq!(c.ping(7).await.unwrap(), 7);
}

#[tokio::test]
#[ignore]
async fn live_slots_without_footers() {
    let events = collect_slots(client().subscribe_slots(), Duration::from_secs(15)).await;
    assert!(events.len() > 10, "too few slot events: {}", events.len());
    assert!(events.windows(2).all(|w| w[1].sequence > w[0].sequence));
    for ev in &events {
        assert_ne!(
            ev.status(),
            SlotStatus::BlockFooter,
            "footer without opt-in"
        );
        assert!(ev.block_footer.is_none());
    }
}

#[tokio::test]
#[ignore]
async fn live_slots_with_footers() {
    let events = collect_slots(
        client().subscribe_slots_with_block_footers(),
        Duration::from_secs(30),
    )
    .await;
    assert!(events.windows(2).all(|w| w[1].sequence > w[0].sequence));

    let mut footers = 0;
    let mut seen = std::collections::HashSet::new();
    for ev in &events {
        if ev.status() != SlotStatus::BlockFooter {
            assert!(ev.block_footer.is_none(), "footer on {:?}", ev.status());
            continue;
        }
        footers += 1;
        assert!(seen.insert(ev.slot), "two footers for slot {}", ev.slot);
        let f = ev
            .block_footer
            .as_ref()
            .expect("BLOCK_FOOTER event without footer");
        assert_eq!(f.slot, ev.slot);
        assert_eq!(f.bank_hash.len(), 32);
        assert!(f.block_id.is_empty() || f.block_id.len() == 32);
        assert!(f.block_user_agent.len() <= 255);
        assert!(f.block_producer_time_nanos > 0);
        if let Some(cert) = &f.block_final_cert {
            for agg in [&cert.final_aggregate, &cert.notar_aggregate]
                .into_iter()
                .flatten()
            {
                assert_eq!(agg.signature.len(), 96);
                assert!(agg.block_id.is_empty() || agg.block_id.len() == 32);
            }
        }
    }
    assert!(footers > 0, "no footers in {} events", events.len());
}

#[tokio::test]
#[ignore]
async fn live_transactions_enriched() {
    let filter = TransactionFilter::new()
        .account_include([COMPUTE_BUDGET])
        .include_enrichment(true)
        .with_id("cb");
    let mut stream = client().subscribe_transactions(vec![filter]);

    let mut accepted = false;
    let mut txs: Vec<TransactionMessage> = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(resp) = stream.next().await {
            match resp.expect("tx stream error").payload {
                Some(Payload::FilterValidation(r)) => {
                    assert!(r.accepted, "filter rejected: {}", r.rejection_reason);
                    accepted = true;
                }
                Some(Payload::Transaction(ft)) => {
                    assert_eq!(ft.filter_ids, vec!["cb".to_string()]);
                    txs.push(ft.transaction.expect("empty transaction"));
                    if txs.len() >= 5000 {
                        break;
                    }
                }
                _ => {}
            }
        }
    })
    .await;
    stream.close();

    assert!(accepted, "no filter_validation");
    assert!(!txs.is_empty(), "no transactions");
    assert!(
        txs.iter().all(|t| t.fee_payer.len() == 32),
        "enrichment missing"
    );
    assert!(txs.iter().any(|t| t.priority_fee_lamports > 0));

    let v1: Vec<_> = txs.iter().filter(|t| t.config.is_some()).collect();
    for t in &v1 {
        let cfg = t.config.as_ref().unwrap();
        assert!(t.versioned, "v1 tx with versioned=false");
        assert_eq!(t.priority_fee_lamports, cfg.priority_fee);
        assert_eq!(t.compute_unit_price, 0, "v1 tx with compute_unit_price");
        assert!(matches!(cfg.config_mask & 0b11, 0 | 0b11));
        assert!(
            cfg.heap_size == 0
                || (cfg.heap_size % 1024 == 0 && (32768..=262144).contains(&cfg.heap_size))
        );
    }
    println!("{} txs, {} v1", txs.len(), v1.len());
}
