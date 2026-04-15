#![cfg(feature = "jetstream")]

use orbitflare_sdk::JetstreamClientBuilder;

#[tokio::test]
async fn subscribe_yaml_missing_file() {
    let client = JetstreamClientBuilder::new()
        .url("http://localhost:10000")
        .build()
        .unwrap();
    let err = client
        .subscribe_yaml("nonexistent.yml")
        .err()
        .expect("expected error");
    match err {
        orbitflare_sdk::Error::Config(msg) => {
            assert!(msg.contains("failed to read"));
        }
        other => panic!("expected Config error, got: {other}"),
    }
}

#[tokio::test]
async fn subscribe_yaml_valid_config_loads() {
    let path = std::env::temp_dir().join("orbitflare_jet_good.yml");
    std::fs::write(
        &path,
        "transactions:\n  test:\n    account_include:\n      - \"abc\"\n",
    )
    .unwrap();

    let client = JetstreamClientBuilder::new()
        .url("http://localhost:10000")
        .build()
        .unwrap();
    let result = client.subscribe_yaml(path.to_str().unwrap());

    let _ = std::fs::remove_file(&path);

    assert!(result.is_ok());
}
