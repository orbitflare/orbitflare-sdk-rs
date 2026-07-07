//! JetStream clients.
//!
//! Two protocol versions live here, organized identically: [`v1`] (the original
//! fixed-filter server stream) and [`v2`] (runtime-managed filters, per-message
//! sequence numbers, opt-in enrichment, and slot events). For convenience the
//! v1 client and its typed filters are re-exported at this module's root (and
//! at the crate root), so `jetstream::JetstreamClient` and
//! `jetstream::v1::JetstreamClient` are the same type and nothing breaks for
//! old users.

pub mod v1;
pub mod v2;

pub use v1::{
    JetstreamClient, JetstreamClientBuilder, JetstreamStream, SubscribeRequestBuilder,
    TransactionFilter,
};
