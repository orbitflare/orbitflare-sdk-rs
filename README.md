<p align="center">
  <img src="https://raw.githubusercontent.com/orbitflare/orbitflare-sdk-rs/main/assets/banner.png" alt="orbitflare-sdk" width="100%">
</p>

<p align="center">
  <a href="https://crates.io/crates/orbitflare-sdk"><img src="https://img.shields.io/crates/v/orbitflare-sdk.svg?style=flat-square&color=3CAB9C&labelColor=041815" alt="crates.io"></a>
  <a href="https://docs.orbitflare.com/sdk/overview"><img src="https://img.shields.io/badge/docs-orbitflare.com-3CAB9C?style=flat-square&labelColor=041815" alt="Documentation"></a>
</p>

# orbitflare-sdk

Rust SDK for [OrbitFlare](https://orbitflare.com) - RPC, gRPC (Yellowstone Geyser), JetStream, and WebSocket clients for Solana.

## Install

```bash
cargo add orbitflare-sdk
```

Only the RPC client is enabled by default. Enable what you need:

```bash
cargo add orbitflare-sdk --features grpc         # gRPC streaming (Yellowstone Geyser)
cargo add orbitflare-sdk --features jetstream    # JetStream streaming
cargo add orbitflare-sdk --features ws           # WebSocket subscriptions
cargo add orbitflare-sdk --features all          # Everything
```

Or in your `Cargo.toml`:

```toml
[dependencies]
orbitflare-sdk = "0.3.0"                                          # RPC only (default)
orbitflare-sdk = { version = "0.3.0", features = ["grpc"] }       # gRPC
orbitflare-sdk = { version = "0.3.0", features = ["all"] }        # Everything
```

## RPC

```rust
use orbitflare_sdk::{RpcClientBuilder, Result};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    let client = RpcClientBuilder::new()
        .url("http://ny.rpc.orbitflare.com")
        .commitment("confirmed")
        .build()?;

    let slot = client.get_slot().await?;
    let balance = client.get_balance("CKs1E69a2e9TmH4mKKLrXFF8kD3ZnwKjoEuXa6sz9WqX").await?;
    let (blockhash, last_valid) = client.get_latest_blockhash().await?;

    let inflation = client.request("getInflationRate", json!([])).await?;

    let raw = client
        .request_raw(r#"{"jsonrpc":"2.0","id":1,"method":"getHealth","params":[]}"#)
        .await?;

    Ok(())
}
```

### Typed helpers

| Method | Returns |
|---|---|
| `get_slot()` | Current slot (`u64`) |
| `get_balance(address)` | Lamports (`u64`) |
| `get_account_info(address)` | Account data or `None` |
| `get_multiple_accounts(addresses)` | Vec of accounts (auto-chunks to 100) |
| `get_latest_blockhash()` | `(blockhash, last_valid_block_height)` |
| `get_transaction(signature)` | Full transaction with metadata |
| `get_signatures_for_address(address, limit)` | Recent signatures |
| `get_program_accounts(program_id)` | All accounts owned by a program |
| `get_recent_prioritization_fees(addresses)` | Recent priority fees |
| `send_transaction(tx_base64)` | Signature string |
| `simulate_transaction(tx_base64)` | Simulation result |
| `get_token_accounts_by_owner(owner, mint, program)` | Token accounts |
| `get_transactions_for_address(address, options)` | Full transaction history with filters and pagination (OrbitFlare-specific) |
| `request(method, params)` | Any RPC method by name |
| `request_raw(body)` | Raw JSON-RPC body string |

## gRPC (Yellowstone Geyser)

Subscribe using a YAML config file:

```yaml
# grpc.yml
transactions:
  pumpfun:
    vote: false
    failed: false
    account_include:
      - "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"

commitment: confirmed
```

```rust
use orbitflare_sdk::{GeyserClientBuilder, Result};
use orbitflare_sdk::proto::geyser::subscribe_update::UpdateOneof;

#[tokio::main]
async fn main() -> Result<()> {
    let client = GeyserClientBuilder::new()
        .url("http://ny.rpc.orbitflare.com:10000")
        .build()?;

    let mut stream = client.subscribe_yaml("grpc.yml")?;

    while let Some(update) = stream.next().await {
        let update = update?;
        if let Some(UpdateOneof::Transaction(tx)) = update.update_oneof {
            println!("slot={}", tx.slot);
        }
    }

    Ok(())
}
```

The YAML format supports `${ENV_VAR}` expansion.

### Typed filters

Build the request programmatically with the typed builder instead of YAML:

```rust
use orbitflare_sdk::grpc::{
    AccountFilter, Commitment, Lamports, Memcmp, SlotFilter, SubscribeRequestBuilder,
    TransactionFilter,
};

let request = SubscribeRequestBuilder::new()
    .transactions(
        "pumpfun",
        TransactionFilter::new()
            .vote(false)
            .failed(false)
            .account_include(["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"])
            .account_required(["So11111111111111111111111111111111111111112"]),
    )
    .accounts(
        "wsol-token-accounts",
        AccountFilter::new()
            .owner(["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"])
            .datasize(165)
            .memcmp(Memcmp::base58(0, "So11111111111111111111111111111111111111112"))
            .lamports(Lamports::Gt(0)),
    )
    .slots("slots", SlotFilter::new().filter_by_commitment(true))
    .commitment(Commitment::Confirmed)
    .build();

let mut stream = client.subscribe(request);
```

`client.subscribe(request)` also accepts a raw proto `SubscribeRequest` if you'd rather construct it by hand.

## JetStream

```yaml
# jetstream.yml
transactions:
  raydium:
    account_include:
      - "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8"
```

```rust
use orbitflare_sdk::{JetstreamClientBuilder, Result};
use orbitflare_sdk::proto::jetstream::subscribe_update::UpdateOneof;

#[tokio::main]
async fn main() -> Result<()> {
    let client = JetstreamClientBuilder::new()
        .url("http://ny.jetstream.orbitflare.com")
        .build()?;

    let mut stream = client.subscribe_yaml("jetstream.yml")?;

    while let Some(update) = stream.next().await {
        let update = update?;
        if let Some(UpdateOneof::Transaction(tx)) = update.update_oneof {
            println!("slot={}", tx.slot);
        }
    }

    Ok(())
}
```

Or build the request with the typed builder:

```rust
use orbitflare_sdk::jetstream::{SubscribeRequestBuilder, TransactionFilter};

let request = SubscribeRequestBuilder::new()
    .transactions(
        "pumpfun",
        TransactionFilter::new()
            .account_include(["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"])
            .account_required(["So11111111111111111111111111111111111111112"]),
    )
    .build();

let mut stream = client.subscribe(request);
```

### JetStream v2

v2 (`jetstream::v2`) runs on the same endpoint and auth as v1 and is fully additive. Filters are managed at runtime over a bidirectional stream (no reconnect to change them), every response carries a monotonic `sequence` for gap detection, slot lifecycle events are a separate stream, and transactions can opt into enrichment (fee payer, program ids, compute-unit price, loaded address-table addresses, and more).

```rust
use orbitflare_sdk::jetstream::v2::{JetstreamClientBuilder, TransactionFilter};
use orbitflare_sdk::proto::jetstream::v2::subscribe_transactions_response::Payload;
use orbitflare_sdk::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let client = JetstreamClientBuilder::new()
        .url("http://ny.jetstream.orbitflare.com")
        .build()?;

    println!("version={}", client.get_version().await?);

    let filter = TransactionFilter::new()
        .account_include(["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"])
        .include_enrichment(true)
        .with_id("pumpfun");

    let mut stream = client.subscribe_transactions(vec![filter]);

    while let Some(resp) = stream.next().await {
        match resp?.payload {
            Some(Payload::Transaction(ft)) => {
                if let Some(tx) = &ft.transaction {
                    println!("slot={} cu_price={}", tx.slot, tx.compute_unit_price);
                }
            }
            Some(Payload::FilterValidation(r)) => {
                println!("filter {} accepted={}", r.filter_id, r.accepted);
            }
            _ => {}
        }
    }

    Ok(())
}
```

Add or remove filters on a live stream without reconnecting:

```rust
let handle = stream.handle();
handle.add_filters(vec![
    TransactionFilter::new().account_include(["..."]).with_id("raydium"),
])?;
handle.remove_filters(vec!["pumpfun".to_string()])?;
```

Slot lifecycle events are a separate server stream:

```rust
let mut slots = client.subscribe_slots();
while let Some(event) = slots.next().await {
    let event = event?;
    println!("slot={} status={:?}", event.slot, event.status());
}
```

## WebSocket

```rust
use orbitflare_sdk::{WsClientBuilder, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = WsClientBuilder::new()
        .url("ws://ny.rpc.orbitflare.com")
        .build()
        .await?;

    let mut sub = client.slot_subscribe().await?;

    while let Some(slot) = sub.next().await {
        println!("{slot}");
    }

    Ok(())
}
```

Subscription types: `account_subscribe`, `logs_subscribe`, `slot_subscribe`, `signature_subscribe`. All auto-resubscribe on reconnect.

## Endpoint failover

All clients support multiple endpoints with automatic failover and health tracking.

```rust
let client = RpcClientBuilder::new()
    .urls(&[
        "http://ny.rpc.orbitflare.com",
        "http://fra.rpc.orbitflare.com",
        "http://ams.rpc.orbitflare.com",
    ])
    .build()?;
```

Failing endpoints are quarantined with exponential cooldown (10s, 20s, 40s, max 60s) and automatically retried once the cooldown expires. Healthy endpoints are always preferred.

## Retry

RPC calls retry on transient errors (5xx, 429, connection resets, Solana error codes -32005, -32007, -32014, -32015, -32016) with exponential backoff before failing over to the next endpoint. 429 responses with a `Retry-After` header are respected.

gRPC, JetStream, and WebSocket connections use active ping/pong to detect dead connections. Configurable via the builder:

```rust
let client = GeyserClientBuilder::new()
    .url("http://ny.rpc.orbitflare.com:10000")
    .ping_interval_secs(15)   // send a ping every 15s (default: 10)
    .max_missed_pongs(5)      // kill connection after 5 missed pongs (default: 3)
    .build()?;
```

All three streaming clients reconnect automatically on disconnection. WebSocket also re-subscribes all active subscriptions after reconnecting.

Configure retry behavior:

```rust
use orbitflare_sdk::RetryPolicy;
use std::time::Duration;

let client = RpcClientBuilder::new()
    .url("http://ny.rpc.orbitflare.com")
    .retry(RetryPolicy {
        initial_delay: Duration::from_millis(200),
        max_delay: Duration::from_secs(15),
        multiplier: 2.0,
        max_attempts: 5,
    })
    .build()?;
```

## Environment variables

| Variable | Used by | Purpose |
|---|---|---|
| `ORBITFLARE_LICENSE_KEY` | RPC, WebSocket | API key appended to endpoint URLs |
| `ORBITFLARE_RPC_URL` | RPC | Default endpoint if `.url()` is not called |
| `ORBITFLARE_WS_URL` | WebSocket | Default endpoint if `.url()` is not called |
| `ORBITFLARE_GRPC_URL` | gRPC | Default endpoint if `.url()` is not called |
| `ORBITFLARE_JETSTREAM_URL` | JetStream | Default endpoint if `.url()` is not called |
