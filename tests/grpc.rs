#![cfg(feature = "grpc")]

use orbitflare_sdk::GeyserClientBuilder;

#[tokio::test]
async fn subscribe_yaml_missing_file() {
    let client = GeyserClientBuilder::new()
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
async fn subscribe_yaml_invalid_yaml() {
    let path = std::env::temp_dir().join("orbitflare_grpc_bad.yml");
    std::fs::write(&path, "{{{{not yaml").unwrap();

    let client = GeyserClientBuilder::new()
        .url("http://localhost:10000")
        .build()
        .unwrap();
    let err = client
        .subscribe_yaml(path.to_str().unwrap())
        .err()
        .expect("expected error");

    let _ = std::fs::remove_file(&path);

    match err {
        orbitflare_sdk::Error::Config(msg) => {
            assert!(msg.contains("failed to parse"));
        }
        other => panic!("expected Config error, got: {other}"),
    }
}

#[tokio::test]
async fn subscribe_yaml_valid_config_loads() {
    let path = std::env::temp_dir().join("orbitflare_grpc_good.yml");
    std::fs::write(
        &path,
        "transactions:\n  test:\n    account_include:\n      - \"abc\"\ncommitment: confirmed\n",
    )
    .unwrap();

    let client = GeyserClientBuilder::new()
        .url("http://localhost:10000")
        .build()
        .unwrap();
    let result = client.subscribe_yaml(path.to_str().unwrap());

    let _ = std::fs::remove_file(&path);

    assert!(result.is_ok());
}
