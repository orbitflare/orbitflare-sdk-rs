#![cfg(feature = "jetstream")]

use orbitflare_sdk::jetstream::v2::{JetstreamClientBuilder, TransactionFilter};

#[test]
fn builder_requires_url() {
    let prev = std::env::var("ORBITFLARE_JETSTREAM_URL").ok();
    unsafe { std::env::remove_var("ORBITFLARE_JETSTREAM_URL") };

    let err = JetstreamClientBuilder::new()
        .build()
        .err()
        .expect("expected error");
    assert!(matches!(err, orbitflare_sdk::Error::Config(_)));

    if let Some(v) = prev {
        unsafe { std::env::set_var("ORBITFLARE_JETSTREAM_URL", v) };
    }
}

#[test]
fn filter_builder_sets_account_include() {
    let f = TransactionFilter::new()
        .account_include(["abc"])
        .with_id("f1");
    assert_eq!(f.filter_id, "f1");
    assert_eq!(f.filter.unwrap().account_include, vec!["abc".to_string()]);
}

#[tokio::test]
async fn subscribe_transactions_builds_stream() {
    let client = JetstreamClientBuilder::new()
        .url("http://localhost:10000")
        .build()
        .unwrap();
    let filter = TransactionFilter::new()
        .account_include(["abc"])
        .with_id("f1");
    let mut stream = client.subscribe_transactions(vec![filter]);

    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next()).await;
}

#[tokio::test]
async fn subscribe_slots_builds_stream() {
    let client = JetstreamClientBuilder::new()
        .url("http://localhost:10000")
        .build()
        .unwrap();
    let mut stream = client.subscribe_slots();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next()).await;
}

#[tokio::test]
async fn filter_handle_after_close_errors() {
    let client = JetstreamClientBuilder::new()
        .url("http://localhost:10000")
        .build()
        .unwrap();
    let stream = client.subscribe_transactions(vec![
        TransactionFilter::new()
            .account_include(["abc"])
            .with_id("f1"),
    ]);
    let handle = stream.handle();
    stream.close();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let res = handle.add_filters(vec![
        TransactionFilter::new()
            .account_include(["xyz"])
            .with_id("f2"),
    ]);
    assert!(res.is_err());
}
