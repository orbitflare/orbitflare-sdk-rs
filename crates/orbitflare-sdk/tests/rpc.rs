use orbitflare_sdk::{
    GetTransactionsFilters, GetTransactionsOptions, RangeFilter, RetryPolicy, RpcClientBuilder,
};
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Respond, ResponseTemplate};

async fn mock_rpc(body: serde_json::Value) -> (MockServer, orbitflare_sdk::rpc::RpcClient) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&server)
        .await;
    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .retry(RetryPolicy { max_attempts: 1, ..Default::default() })
        .build()
        .unwrap();
    (server, client)
}

#[tokio::test]
async fn get_slot_returns_u64() {
    let (_, client) = mock_rpc(json!({"jsonrpc":"2.0","result":12345,"id":1})).await;
    let slot = client.get_slot().await.unwrap();
    assert_eq!(slot, 12345);
}

#[tokio::test]
async fn get_balance_returns_lamports() {
    let (_, client) = mock_rpc(json!({"jsonrpc":"2.0","result":{"value":999000000},"id":1})).await;
    let balance = client.get_balance("addr").await.unwrap();
    assert_eq!(balance, 999000000);
}

#[tokio::test]
async fn get_latest_blockhash_returns_tuple() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":{"value":{"blockhash":"abc123","lastValidBlockHeight":500}},
        "id":1
    })).await;
    let (hash, height) = client.get_latest_blockhash().await.unwrap();
    assert_eq!(hash, "abc123");
    assert_eq!(height, 500);
}

#[tokio::test]
async fn get_account_info_returns_none_for_missing() {
    let (_, client) = mock_rpc(json!({"jsonrpc":"2.0","result":{"value":null},"id":1})).await;
    let acct = client.get_account_info("missing").await.unwrap();
    assert!(acct.is_none());
}

#[tokio::test]
async fn get_account_info_returns_some() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":{"value":{"lamports":100,"owner":"11111","data":["","base64"],"executable":false}},
        "id":1
    })).await;
    let acct = client.get_account_info("addr").await.unwrap();
    assert!(acct.is_some());
    assert_eq!(acct.unwrap()["lamports"], 100);
}

#[tokio::test]
async fn get_multiple_accounts_returns_vec() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":{"value":[{"lamports":10},null,{"lamports":30}]},
        "id":1
    })).await;
    let accounts = client.get_multiple_accounts(&["a", "b", "c"]).await.unwrap();
    assert_eq!(accounts.len(), 3);
    assert!(accounts[0].is_some());
    assert!(accounts[1].is_none());
    assert!(accounts[2].is_some());
    assert_eq!(accounts[0].as_ref().unwrap()["lamports"], 10);
    assert_eq!(accounts[2].as_ref().unwrap()["lamports"], 30);
}

#[tokio::test]
async fn get_transaction_returns_value() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":{"slot":100,"meta":{"fee":5000},"transaction":{}},
        "id":1
    })).await;
    let tx = client.get_transaction("sig123").await.unwrap();
    assert_eq!(tx["slot"], 100);
    assert_eq!(tx["meta"]["fee"], 5000);
}

#[tokio::test]
async fn get_signatures_for_address_returns_array() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":[{"signature":"aaa","slot":1},{"signature":"bbb","slot":2}],
        "id":1
    })).await;
    let sigs = client.get_signatures_for_address("addr", 2).await.unwrap();
    assert_eq!(sigs.len(), 2);
    assert_eq!(sigs[0]["signature"], "aaa");
    assert_eq!(sigs[1]["slot"], 2);
}

#[tokio::test]
async fn get_program_accounts_returns_array() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":[{"pubkey":"a","account":{"lamports":1}},{"pubkey":"b","account":{"lamports":2}}],
        "id":1
    })).await;
    let accts = client.get_program_accounts("program").await.unwrap();
    assert_eq!(accts.len(), 2);
}

#[tokio::test]
async fn get_recent_prioritization_fees_returns_array() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":[{"slot":1,"prioritizationFee":100},{"slot":2,"prioritizationFee":200}],
        "id":1
    })).await;
    let fees = client.get_recent_prioritization_fees(&["addr"]).await.unwrap();
    assert_eq!(fees.len(), 2);
    assert_eq!(fees[0]["prioritizationFee"], 100);
}

#[tokio::test]
async fn simulate_transaction_returns_value() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":{"value":{"err":null,"logs":["log1"],"unitsConsumed":5000}},
        "id":1
    })).await;
    let result = client.simulate_transaction("base64tx").await.unwrap();
    assert!(result["value"]["err"].is_null());
}

#[tokio::test]
async fn get_token_accounts_by_owner_returns_array() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":{"value":[{"pubkey":"tok1","account":{"data":{"parsed":{"info":{"mint":"m1"}}}}}]},
        "id":1
    })).await;
    let tokens = client.get_token_accounts_by_owner("owner", None, None).await.unwrap();
    assert_eq!(tokens.len(), 1);
}

#[tokio::test]
async fn get_transactions_for_address_basic() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":{
            "data":[{"signature":"aaa","slot":1},{"signature":"bbb","slot":2}],
        },
        "id":1
    })).await;

    let result = client
        .get_transactions_for_address("addr", GetTransactionsOptions::new())
        .await
        .unwrap();

    assert_eq!(result.data.len(), 2);
    assert_eq!(result.data[0]["signature"], "aaa");
    assert!(result.pagination_token.is_none());
}

#[tokio::test]
async fn get_transactions_for_address_with_pagination_token() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":{
            "data":[{"signature":"aaa","slot":1}],
            "paginationToken":"387936002:541"
        },
        "id":1
    })).await;

    let result = client
        .get_transactions_for_address("addr", GetTransactionsOptions::new().limit(1))
        .await
        .unwrap();

    assert_eq!(result.data.len(), 1);
    assert_eq!(result.pagination_token, Some("387936002:541".into()));
}

#[tokio::test]
async fn get_transactions_for_address_sends_all_options() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc":"2.0",
            "result":{"data":[]},
            "id":1
        })))
        .mount(&server)
        .await;

    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .build()
        .unwrap();

    let options = GetTransactionsOptions::new()
        .transaction_details("full")
        .limit(50)
        .sort_order("asc")
        .pagination_token("100:5")
        .commitment("finalized")
        .filters(
            GetTransactionsFilters::new()
                .token_accounts("all")
                .status("succeeded")
                .block_time(RangeFilter {
                    gte: Some(1704067200),
                    lte: Some(1706745600),
                    ..Default::default()
                }),
        );

    client
        .get_transactions_for_address("addr", options)
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let params = &body["params"][1];

    assert_eq!(params["transactionDetails"], "full");
    assert_eq!(params["limit"], 50);
    assert_eq!(params["sortOrder"], "asc");
    assert_eq!(params["paginationToken"], "100:5");
    assert_eq!(params["commitment"], "finalized");
    assert_eq!(params["filters"]["tokenAccounts"], "all");
    assert_eq!(params["filters"]["status"], "succeeded");
    assert_eq!(params["filters"]["blockTime"]["gte"], 1704067200);
    assert_eq!(params["filters"]["blockTime"]["lte"], 1706745600);
}

#[tokio::test]
async fn get_transactions_for_address_slot_filter() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc":"2.0",
            "result":{"data":[]},
            "id":1
        })))
        .mount(&server)
        .await;

    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .build()
        .unwrap();

    let options = GetTransactionsOptions::new().filters(
        GetTransactionsFilters::new().slot(RangeFilter {
            gt: Some(300_000_000u64),
            lt: Some(400_000_000u64),
            ..Default::default()
        }),
    );

    client
        .get_transactions_for_address("addr", options)
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let slot = &body["params"][1]["filters"]["slot"];
    assert_eq!(slot["gt"], 300_000_000u64);
    assert_eq!(slot["lt"], 400_000_000u64);
    assert!(slot.get("gte").is_none());
    assert!(slot.get("lte").is_none());
}

#[tokio::test]
async fn get_transactions_for_address_omits_unset_options() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc":"2.0",
            "result":{"data":[]},
            "id":1
        })))
        .mount(&server)
        .await;

    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .build()
        .unwrap();

    client
        .get_transactions_for_address("addr", GetTransactionsOptions::new())
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let params = &body["params"][1];

    assert!(params.get("transactionDetails").is_none());
    assert!(params.get("limit").is_none());
    assert!(params.get("filters").is_none());
}

#[tokio::test]
async fn request_custom_method() {
    let (_, client) = mock_rpc(json!({"jsonrpc":"2.0","result":{"epoch":100},"id":1})).await;
    let result = client.request("getEpochInfo", json!([])).await.unwrap();
    assert_eq!(result["epoch"], 100);
}

#[tokio::test]
async fn request_raw_body() {
    let (_, client) = mock_rpc(json!({"jsonrpc":"2.0","result":"ok","id":1})).await;
    let result = client
        .request_raw(r#"{"jsonrpc":"2.0","id":1,"method":"getHealth","params":[]}"#)
        .await
        .unwrap();
    assert_eq!(result, "ok");
}

#[tokio::test]
async fn send_transaction_returns_signature() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "result":"5K8F2jABCDEFtestsignature123456789",
        "id":1
    })).await;
    let sig = client.send_transaction("base64txdata").await.unwrap();
    assert_eq!(sig, "5K8F2jABCDEFtestsignature123456789");
}

#[tokio::test]
async fn commitment_defaults_and_override() {
    let (_, default_client) = mock_rpc(json!({"jsonrpc":"2.0","result":0,"id":1})).await;
    assert_eq!(default_client.commitment(), "confirmed");

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","result":0,"id":1})))
        .mount(&server)
        .await;
    let finalized = RpcClientBuilder::new()
        .url(&server.uri())
        .commitment("finalized")
        .build()
        .unwrap();
    assert_eq!(finalized.commitment(), "finalized");
}

#[tokio::test]
async fn rpc_error_extracted() {
    let (_, client) = mock_rpc(json!({
        "jsonrpc":"2.0",
        "error":{"code":-32600,"message":"invalid request"},
        "id":1
    })).await;
    let err = client.get_slot().await.unwrap_err();
    match err {
        orbitflare_sdk::Error::Rpc { code, message } => {
            assert_eq!(code, -32600);
            assert!(message.contains("invalid request"));
        }
        other => panic!("expected Rpc error, got: {other}"),
    }
}

#[tokio::test]
async fn rpc_error_from_non_200_json_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"jsonrpc":"2.0","error":{"code":-32404,"message":"IP not whitelisted"}}))
        )
        .mount(&server)
        .await;
    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .retry(RetryPolicy { max_attempts: 1, ..Default::default() })
        .build()
        .unwrap();
    let err = client.get_slot().await.unwrap_err();
    match err {
        orbitflare_sdk::Error::Rpc { code, message } => {
            assert_eq!(code, -32404);
            assert!(message.contains("IP not whitelisted"));
        }
        other => panic!("expected Rpc error, got: {other}"),
    }
}

#[tokio::test]
async fn server_error_is_retryable() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;
    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .retry(RetryPolicy { max_attempts: 1, ..Default::default() })
        .build()
        .unwrap();
    let err = client.get_slot().await.unwrap_err();
    assert!(err.is_retryable());
}

#[tokio::test]
async fn rate_limit_429() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(429)
                .append_header("Retry-After", "10")
        )
        .mount(&server)
        .await;
    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .retry(RetryPolicy { max_attempts: 1, ..Default::default() })
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let err = client.get_slot().await.unwrap_err();
    match err {
        orbitflare_sdk::Error::RateLimited { retry_after } => {
            assert_eq!(retry_after, Some(Duration::from_secs(10)));
        }
        other => panic!("expected RateLimited, got: {other}"),
    }
}

#[tokio::test]
async fn failover_to_second_endpoint() {
    let bad = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("down"))
        .mount(&bad)
        .await;

    let good = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","result":42,"id":1})))
        .mount(&good)
        .await;

    let client = RpcClientBuilder::new()
        .urls(&[&bad.uri(), &good.uri()])
        .retry(RetryPolicy { max_attempts: 1, ..Default::default() })
        .build()
        .unwrap();
    let slot = client.get_slot().await.unwrap();
    assert_eq!(slot, 42);
}

#[tokio::test]
async fn failover_on_non_retryable_error() {
    let bad = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"jsonrpc":"2.0","error":{"code":-32404,"message":"IP not whitelisted"}}))
        )
        .mount(&bad)
        .await;

    let good = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","result":99,"id":1})))
        .mount(&good)
        .await;

    let client = RpcClientBuilder::new()
        .urls(&[&bad.uri(), &good.uri()])
        .retry(RetryPolicy { max_attempts: 1, ..Default::default() })
        .build()
        .unwrap();
    let slot = client.get_slot().await.unwrap();
    assert_eq!(slot, 99);
}

#[tokio::test]
async fn retries_before_failing() {
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();

    struct CountingResponder(Arc<AtomicU32>);
    impl Respond for CountingResponder {
        fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
            let n = self.0.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                ResponseTemplate::new(500).set_body_string("down")
            } else {
                ResponseTemplate::new(200)
                    .set_body_json(json!({"jsonrpc":"2.0","result":777,"id":1}))
            }
        }
    }

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(CountingResponder(counter_clone))
        .mount(&server)
        .await;

    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .retry(RetryPolicy {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            multiplier: 1.5,
            max_attempts: 5,
        })
        .build()
        .unwrap();

    let slot = client.get_slot().await.unwrap();
    assert_eq!(slot, 777);
    assert!(counter.load(Ordering::SeqCst) >= 3);
}

#[tokio::test]
async fn quarantined_endpoint_skipped_on_second_request() {
    let bad = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("down"))
        .mount(&bad)
        .await;

    let good = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","result":1,"id":1})))
        .mount(&good)
        .await;

    let client = RpcClientBuilder::new()
        .urls(&[&bad.uri(), &good.uri()])
        .retry(RetryPolicy { max_attempts: 1, ..Default::default() })
        .build()
        .unwrap();

    let _ = client.get_slot().await.unwrap();
    let slot = client.get_slot().await.unwrap();
    assert_eq!(slot, 1);
}

#[tokio::test]
async fn error_includes_endpoint_context() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .retry(RetryPolicy { max_attempts: 1, ..Default::default() })
        .build()
        .unwrap();

    let err = client.get_slot().await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("127.0.0.1"), "error should contain endpoint: {msg}");
}

#[tokio::test]
async fn api_key_injected_into_url() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","result":1,"id":1})))
        .mount(&server)
        .await;

    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .api_key("ORBIT-TEST-KEY")
        .build()
        .unwrap();

    client.get_slot().await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let req_url = requests[0].url.to_string();
    assert!(req_url.contains("api_key=ORBIT-TEST-KEY"), "URL should contain api_key: {req_url}");
}

#[tokio::test]
async fn fallback_url_method() {
    let bad = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("down"))
        .mount(&bad)
        .await;

    let good = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","result":55,"id":1})))
        .mount(&good)
        .await;

    let client = RpcClientBuilder::new()
        .url(&bad.uri())
        .fallback_url(&good.uri())
        .retry(RetryPolicy { max_attempts: 1, ..Default::default() })
        .build()
        .unwrap();

    let slot = client.get_slot().await.unwrap();
    assert_eq!(slot, 55);
}

#[tokio::test]
async fn timeout_triggers_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200)
            .set_body_json(json!({"jsonrpc":"2.0","result":1,"id":1}))
            .set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;

    let client = RpcClientBuilder::new()
        .url(&server.uri())
        .timeout(Duration::from_millis(100))
        .retry(RetryPolicy { max_attempts: 1, ..Default::default() })
        .build()
        .unwrap();

    let err = client.get_slot().await.unwrap_err();
    assert!(err.is_retryable());
}


