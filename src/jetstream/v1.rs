use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio_stream::StreamExt;

use orbitflare_sdk_proto::jetstream::{
    SubscribeRequest, SubscribeRequestFilterTransactions, SubscribeRequestPing, SubscribeUpdate,
    jetstream_client::JetstreamClient as ProtoJetstreamClient, subscribe_update::UpdateOneof,
};

use crate::error::{Error, Result};
use crate::retry::RetryPolicy;
use crate::streaming::{Reconnector, StreamConfig, StreamConfigBuilder};

#[derive(Clone, Debug, Default)]
pub struct TransactionFilter {
    account_include: Vec<String>,
    account_exclude: Vec<String>,
    account_required: Vec<String>,
}

impl TransactionFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn account_include<I, S>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.account_include
            .extend(accounts.into_iter().map(Into::into));
        self
    }

    pub fn account_exclude<I, S>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.account_exclude
            .extend(accounts.into_iter().map(Into::into));
        self
    }

    pub fn account_required<I, S>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.account_required
            .extend(accounts.into_iter().map(Into::into));
        self
    }
}

impl From<TransactionFilter> for SubscribeRequestFilterTransactions {
    fn from(f: TransactionFilter) -> Self {
        Self {
            account_include: f.account_include,
            account_exclude: f.account_exclude,
            account_required: f.account_required,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SubscribeRequestBuilder {
    transactions: HashMap<String, TransactionFilter>,
}

impl SubscribeRequestBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn transactions(mut self, name: impl Into<String>, filter: TransactionFilter) -> Self {
        self.transactions.insert(name.into(), filter);
        self
    }

    pub fn build(self) -> SubscribeRequest {
        let transactions = self
            .transactions
            .into_iter()
            .map(|(k, v)| (k, v.into()))
            .collect();
        SubscribeRequest {
            transactions,
            accounts: HashMap::new(),
            ping: Some(SubscribeRequestPing { id: 1 }),
        }
    }
}

pub struct JetstreamClient {
    inner: Arc<StreamConfig>,
}

pub struct JetstreamStream {
    rx: mpsc::Receiver<Result<SubscribeUpdate>>,
    _shutdown: watch::Sender<bool>,
}

impl JetstreamStream {
    pub async fn next(&mut self) -> Option<Result<SubscribeUpdate>> {
        self.rx.recv().await
    }

    pub fn close(self) {
        let _ = self._shutdown.send(true);
    }
}

impl JetstreamClient {
    pub fn subscribe_yaml(&self, path: &str) -> Result<JetstreamStream> {
        let cfg: crate::config::JetstreamStreamConfig = crate::config::read_yaml(path)?;
        Ok(self.subscribe(cfg.to_subscribe_request()))
    }

    pub fn subscribe(&self, request: SubscribeRequest) -> JetstreamStream {
        let (tx, rx) = mpsc::channel(self.inner.channel_capacity);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let rc = Reconnector::new(Arc::clone(&self.inner), shutdown_rx, "jetstream");
        tokio::spawn(stream_task(rc, request, tx));

        JetstreamStream {
            rx,
            _shutdown: shutdown_tx,
        }
    }
}

async fn stream_task(
    mut rc: Reconnector,
    request: SubscribeRequest,
    tx: mpsc::Sender<Result<SubscribeUpdate>>,
) {
    loop {
        if rc.should_stop() {
            return;
        }
        let (idx, url) = rc.pick();

        match run_connection(&url, idx, &mut rc, &request, &tx).await {
            Ok(()) => return,
            Err(e) => {
                if rc.after_error(idx, &url, e, &tx).await == ControlFlow::Break(()) {
                    return;
                }
            }
        }
    }
}

async fn run_connection(
    url: &str,
    idx: usize,
    rc: &mut Reconnector,
    request: &SubscribeRequest,
    tx: &mpsc::Sender<Result<SubscribeUpdate>>,
) -> Result<()> {
    let channel = rc.cfg.connect(url).await?;
    let mut client = ProtoJetstreamClient::new(channel);

    let (outbound_tx, outbound_rx) = mpsc::channel::<SubscribeRequest>(4);
    outbound_tx
        .send(request.clone())
        .await
        .map_err(|_| Error::Stream("outbound closed".into()))?;

    let outbound_stream = tokio_stream::wrappers::ReceiverStream::new(outbound_rx);
    let response: tonic::Response<tonic::Streaming<SubscribeUpdate>> =
        client.subscribe(outbound_stream).await?;
    let mut inbound = response.into_inner();

    rc.on_connected(idx);

    let mut shutdown_rx = rc.shutdown();
    let mut ping_interval = tokio::time::interval(Duration::from_secs(rc.cfg.ping_interval_secs));
    ping_interval.tick().await;
    let mut ping_id: i32 = 1;
    let mut missed_pongs: u32 = 0;

    loop {
        tokio::select! {
            msg = inbound.next() => {
                match msg {
                    Some(Ok(update)) => {
                        if let Some(UpdateOneof::Pong(_)) = &update.update_oneof {
                            missed_pongs = 0;
                            continue;
                        }
                        if tx.send(Ok(update)).await.is_err() {
                            return Ok(());
                        }
                    }
                    Some(Err(e)) => return Err(Error::from(e)),
                    None => return Err(Error::Stream("stream ended".into())),
                }
            }
            _ = ping_interval.tick() => {
                if missed_pongs >= rc.cfg.max_missed_pongs {
                    return Err(Error::Stream(format!(
                        "no pong after {} pings",
                        rc.cfg.max_missed_pongs
                    )));
                }
                let ping_req = SubscribeRequest {
                    ping: Some(SubscribeRequestPing { id: ping_id }),
                    ..Default::default()
                };
                if outbound_tx.send(ping_req).await.is_err() {
                    return Err(Error::Stream("outbound closed".into()));
                }
                ping_id = ping_id.wrapping_add(1);
                missed_pongs += 1;
            }
            res = shutdown_rx.changed() => {
                if res.is_err() || *shutdown_rx.borrow() { return Ok(()); }
            }
        }
    }
}

#[derive(Default)]
pub struct JetstreamClientBuilder {
    cfg: StreamConfigBuilder,
}

impl JetstreamClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn url(mut self, url: &str) -> Self {
        self.cfg.url(url);
        self
    }

    pub fn urls(mut self, urls: &[&str]) -> Self {
        self.cfg.urls(urls);
        self
    }

    pub fn fallback_url(mut self, url: &str) -> Self {
        self.cfg.fallback_url(url);
        self
    }

    pub fn fallback_urls(mut self, urls: &[&str]) -> Self {
        self.cfg.fallback_urls(urls);
        self
    }

    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.cfg.retry(policy);
        self
    }

    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.cfg.timeout_secs(secs);
        self
    }

    pub fn keepalive_secs(mut self, secs: u64) -> Self {
        self.cfg.keepalive_secs(secs);
        self
    }

    pub fn ping_interval_secs(mut self, secs: u64) -> Self {
        self.cfg.ping_interval_secs(secs);
        self
    }

    pub fn max_missed_pongs(mut self, n: u32) -> Self {
        self.cfg.max_missed_pongs(n);
        self
    }

    pub fn channel_capacity(mut self, cap: usize) -> Self {
        self.cfg.channel_capacity(cap);
        self
    }

    pub fn build(self) -> Result<JetstreamClient> {
        Ok(JetstreamClient {
            inner: Arc::new(self.cfg.build("ORBITFLARE_JETSTREAM_URL")?),
        })
    }
}
