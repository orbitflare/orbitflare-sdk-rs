use orbitflare_sdk::grpc::{
    AccountFilter, Commitment, Lamports, Memcmp, SlotFilter, SubscribeRequestBuilder,
    TransactionFilter,
};
use orbitflare_sdk::proto::geyser::subscribe_update::UpdateOneof;
use orbitflare_sdk::{GeyserClientBuilder, Result};

const PUMP_FUN: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const WSOL: &str = "So11111111111111111111111111111111111111112";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let url = std::env::var("ORBITFLARE_GRPC_URL")
        .unwrap_or_else(|_| "http://slc.rpc.orbitflare.com:10000".to_string());
    let client = GeyserClientBuilder::new().url(&url).build()?;

    let request = SubscribeRequestBuilder::new()
        .transactions(
            "pumpfun",
            TransactionFilter::new()
                .vote(false)
                .failed(false)
                .account_include([PUMP_FUN])
                .account_required([WSOL]),
        )
        .accounts(
            "wsol-token-accounts",
            AccountFilter::new()
                .owner([TOKEN_PROGRAM])
                .datasize(165)
                .memcmp(Memcmp::base58(0, WSOL))
                .lamports(Lamports::Gt(0)),
        )
        .slots("slots", SlotFilter::new().filter_by_commitment(true))
        .commitment(Commitment::Confirmed)
        .build();

    let mut stream = client.subscribe(request);

    while let Some(update) = stream.next().await {
        let update = update?;
        match update.update_oneof {
            Some(UpdateOneof::Transaction(tx)) => {
                if let Some(info) = &tx.transaction {
                    let sig = bs58::encode(&info.signature).into_string();
                    println!(
                        "tx slot={} sig={} filters={:?}",
                        tx.slot,
                        &sig[..12.min(sig.len())],
                        update.filters
                    );
                }
            }
            Some(UpdateOneof::Account(acc)) => {
                println!("account slot={} filters={:?}", acc.slot, update.filters);
            }
            Some(UpdateOneof::Slot(slot)) => {
                println!("slot {}", slot.slot);
            }
            _ => {}
        }
    }

    Ok(())
}
