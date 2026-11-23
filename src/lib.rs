#[cfg(any(feature = "rpc", feature = "ws"))]
mod credentials;
mod endpoint;

pub mod error;
pub mod retry;

#[cfg(feature = "rpc")]
pub mod rpc;

#[cfg(feature = "rpc")]
pub mod das;

#[cfg(feature = "ws")]
pub mod ws;

#[cfg(any(feature = "grpc", feature = "jetstream"))]
pub mod config;

#[cfg(any(feature = "grpc", feature = "jetstream"))]
pub mod events;

#[cfg(feature = "grpc")]
pub(crate) mod filters;

#[cfg(any(feature = "grpc", feature = "jetstream"))]
pub(crate) mod streaming;

#[cfg(feature = "grpc")]
pub mod grpc;

#[cfg(feature = "jetstream")]
pub mod jetstream;

pub use error::{Error, Result};
pub use retry::RetryPolicy;

#[cfg(feature = "rpc")]
pub use rpc::{
    GetTransactionsFilters, GetTransactionsOptions, GetTransactionsResult, RangeFilter, RpcClient,
    RpcClientBuilder,
};

#[cfg(feature = "ws")]
pub use ws::{WsClient, WsClientBuilder, WsSubscription};

#[cfg(feature = "grpc")]
pub use grpc::{ConnectInfo, GeyserClient, GeyserClientBuilder, GeyserHandle, GeyserStream};

#[cfg(feature = "jetstream")]
pub use jetstream::{JetstreamClient, JetstreamClientBuilder, JetstreamStream};

#[cfg(any(feature = "grpc", feature = "jetstream"))]
pub use events::{ConnectionEvent, ConnectionEvents};

#[cfg(any(feature = "grpc", feature = "jetstream"))]
pub use orbitflare_sdk_proto as proto;
