use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio_stream::StreamExt;

use orbitflare_sdk_proto::jetstream::v2::{
    AddFilters, GetVersionRequest, Ping, PingRequest, RemoveFilters, SlotEvent,
    SubscribeRequestFilterTransactions, SubscribeSlotsRequest, SubscribeTransactionsRequest,
    SubscribeTransactionsResponse, TxFilter,
    jetstream_v2_client::JetstreamV2Client as ProtoV2Client,
    subscribe_transactions_request::Payload as RequestPayload,
};

use crate::error::{Error, Result};
use crate::retry::RetryPolicy;
use crate::streaming::{Reconnector, StreamConfig, StreamConfigBuilder, unary_with_failover};

#[derive(Clone, Debug, Default)]
pub struct TransactionFilter {
    account_include: Vec<String>,
    account_exclude: Vec<String>,
    account_required: Vec<String>,
    include_enrichment: bool,
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

    pub fn include_enrichment(mut self, value: bool) -> Self {
        self.include_enrichment = value;
        self
    }

    pub fn with_id(self, filter_id: impl Into<String>) -> TxFilter {
        TxFilter {
            filter_id: filter_id.into(),
            filter: Some(self.into()),
        }
    }
}

impl From<TransactionFilter> for SubscribeRequestFilterTransactions {
    fn from(f: TransactionFilter) -> Self {
        Self {
            account_include: f.account_include,
            account_exclude: f.account_exclude,
            account_required: f.account_required,
            include_enrichment: f.include_enrichment,
        }
    }
}

pub struct JetstreamClient {
    inner: Arc<StreamConfig>,
}

enum FilterCommand {
    Add(Vec<TxFilter>),
    Remove(Vec<String>),
    Ping(i32),
}

#[derive(Clone)]
pub struct TxFilterHandle {
    cmd_tx: mpsc::UnboundedSender<FilterCommand>,
}

impl TxFilterHandle {
    pub fn add_filters(&self, filters: Vec<TxFilter>) -> Result<()> {
        self.send(FilterCommand::Add(filters))
    }

    pub fn remove_filters(&self, filter_ids: Vec<String>) -> Result<()> {
        self.send(FilterCommand::Remove(filter_ids))
    }

    pub fn ping(&self, ping_id: i32) -> Result<()> {
        self.send(FilterCommand::Ping(ping_id))
    }

    fn send(&self, cmd: FilterCommand) -> Result<()> {
        self.cmd_tx
            .send(cmd)
            .map_err(|_| Error::Stream("stream closed".into()))
    }
}

pub struct TransactionStream {
    rx: mpsc::Receiver<Result<SubscribeTransactionsResponse>>,
    handle: TxFilterHandle,
    _shutdown: watch::Sender<bool>,
}

impl TransactionStream {
    pub async fn next(&mut self) -> Option<Result<SubscribeTransactionsResponse>> {
        self.rx.recv().await
    }

    pub fn handle(&self) -> TxFilterHandle {
        self.handle.clone()
    }

    pub fn add_filters(&self, filters: Vec<TxFilter>) -> Result<()> {
        self.handle.add_filters(filters)
    }

    pub fn remove_filters(&self, filter_ids: Vec<String>) -> Result<()> {
        self.handle.remove_filters(filter_ids)
    }

    pub fn close(self) {
        let _ = self._shutdown.send(true);
    }
}

pub struct SlotStream {
    rx: mpsc::Receiver<Result<SlotEvent>>,
    _shutdown: watch::Sender<bool>,
}

impl SlotStream {
    pub async fn next(&mut self) -> Option<Result<SlotEvent>> {
        self.rx.recv().await
    }

    pub fn close(self) {
        let _ = self._shutdown.send(true);
    }
}

impl JetstreamClient {
    pub fn subscribe_transactions(&self, filters: Vec<TxFilter>) -> TransactionStream {
        let (tx, rx) = mpsc::channel(self.inner.channel_capacity);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let rc = Reconnector::new(Arc::clone(&self.inner), shutdown_rx, "v2 tx");
        tokio::spawn(tx_stream_task(rc, filters, cmd_rx, tx));

        TransactionStream {
            rx,
            handle: TxFilterHandle { cmd_tx },
            _shutdown: shutdown_tx,
        }
    }

    pub fn subscribe_slots(&self) -> SlotStream {
        let (tx, rx) = mpsc::channel(self.inner.channel_capacity);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let rc = Reconnector::new(Arc::clone(&self.inner), shutdown_rx, "v2 slots");
        tokio::spawn(slot_stream_task(rc, tx));

        SlotStream {
            rx,
            _shutdown: shutdown_tx,
        }
    }

    pub async fn ping(&self, count: i32) -> Result<i32> {
        unary_with_failover(&self.inner, |channel| async move {
            let resp = ProtoV2Client::new(channel)
                .ping(PingRequest { count })
                .await?;
            Ok(resp.into_inner().count)
        })
        .await
    }

    pub async fn get_version(&self) -> Result<String> {
        unary_with_failover(&self.inner, |channel| async move {
            let resp = ProtoV2Client::new(channel)
                .get_version(GetVersionRequest {})
                .await?;
            Ok(resp.into_inner().version)
        })
        .await
    }
}

async fn tx_stream_task(
    mut rc: Reconnector,
    initial: Vec<TxFilter>,
    mut cmd_rx: mpsc::UnboundedReceiver<FilterCommand>,
    tx: mpsc::Sender<Result<SubscribeTransactionsResponse>>,
) {
    let mut active: HashMap<String, TxFilter> = initial
        .into_iter()
        .map(|f| (f.filter_id.clone(), f))
        .collect();

    loop {
        if rc.should_stop() {
            return;
        }
        let (idx, url) = rc.pick();

        match run_tx_connection(&url, idx, &mut rc, &mut active, &mut cmd_rx, &tx).await {
            Ok(()) => return,
            Err(e) => {
                if rc.after_error(idx, &url, e, &tx).await == ControlFlow::Break(()) {
                    return;
                }
            }
        }
    }
}

async fn run_tx_connection(
    url: &str,
    idx: usize,
    rc: &mut Reconnector,
    active: &mut HashMap<String, TxFilter>,
    cmd_rx: &mut mpsc::UnboundedReceiver<FilterCommand>,
    tx: &mpsc::Sender<Result<SubscribeTransactionsResponse>>,
) -> Result<()> {
    let channel = rc.cfg.connect(url).await?;
    let mut client = ProtoV2Client::new(channel);

    let (outbound_tx, outbound_rx) = mpsc::channel::<SubscribeTransactionsRequest>(16);

    if !active.is_empty() {
        let filters = active.values().cloned().collect();
        outbound_tx
            .send(add_filters_req(filters))
            .await
            .map_err(|_| Error::Stream("outbound closed".into()))?;
    }

    let outbound_stream = tokio_stream::wrappers::ReceiverStream::new(outbound_rx);
    let response = client.subscribe_transactions(outbound_stream).await?;
    let mut inbound = response.into_inner();

    rc.on_connected(idx);

    let mut shutdown_rx = rc.shutdown();
    let mut ping_interval = tokio::time::interval(Duration::from_secs(rc.cfg.ping_interval_secs));
    ping_interval.tick().await;
    let mut ping_id: i32 = 1;
    let mut intervals_since_traffic: u32 = 0;

    loop {
        tokio::select! {
            msg = inbound.next() => {
                match msg {
                    Some(Ok(resp)) => {
                        intervals_since_traffic = 0;
                        if tx.send(Ok(resp)).await.is_err() {
                            return Ok(());
                        }
                    }
                    Some(Err(e)) => return Err(Error::from(e)),
                    None => return Err(Error::Stream("stream ended".into())),
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(FilterCommand::Add(filters)) => {
                        for f in &filters {
                            active.insert(f.filter_id.clone(), f.clone());
                        }
                        send_outbound(&outbound_tx, add_filters_req(filters)).await?;
                    }
                    Some(FilterCommand::Remove(ids)) => {
                        for id in &ids {
                            active.remove(id);
                        }
                        let req = SubscribeTransactionsRequest {
                            payload: Some(RequestPayload::RemoveFilters(RemoveFilters {
                                filter_ids: ids,
                            })),
                        };
                        send_outbound(&outbound_tx, req).await?;
                    }
                    Some(FilterCommand::Ping(id)) => {
                        let req = SubscribeTransactionsRequest {
                            payload: Some(RequestPayload::Ping(Ping { ping_id: id })),
                        };
                        send_outbound(&outbound_tx, req).await?;
                    }

                    None => {}
                }
            }
            _ = ping_interval.tick() => {
                if intervals_since_traffic >= rc.cfg.max_missed_pongs {
                    return Err(Error::Stream(format!(
                        "no traffic after {} pings",
                        rc.cfg.max_missed_pongs
                    )));
                }
                let req = SubscribeTransactionsRequest {
                    payload: Some(RequestPayload::Ping(Ping { ping_id })),
                };
                send_outbound(&outbound_tx, req).await?;
                ping_id = ping_id.wrapping_add(1);
                intervals_since_traffic += 1;
            }
            res = shutdown_rx.changed() => {
                if res.is_err() || *shutdown_rx.borrow() { return Ok(()); }
            }
        }
    }
}

async fn send_outbound(
    outbound_tx: &mpsc::Sender<SubscribeTransactionsRequest>,
    req: SubscribeTransactionsRequest,
) -> Result<()> {
    outbound_tx
        .send(req)
        .await
        .map_err(|_| Error::Stream("outbound closed".into()))
}

fn add_filters_req(filters: Vec<TxFilter>) -> SubscribeTransactionsRequest {
    SubscribeTransactionsRequest {
        payload: Some(RequestPayload::AddFilters(AddFilters { filters })),
    }
}

async fn slot_stream_task(mut rc: Reconnector, tx: mpsc::Sender<Result<SlotEvent>>) {
    loop {
        if rc.should_stop() {
            return;
        }
        let (idx, url) = rc.pick();

        match run_slot_connection(&url, idx, &mut rc, &tx).await {
            Ok(()) => return,
            Err(e) => {
                if rc.after_error(idx, &url, e, &tx).await == ControlFlow::Break(()) {
                    return;
                }
            }
        }
    }
}

async fn run_slot_connection(
    url: &str,
    idx: usize,
    rc: &mut Reconnector,
    tx: &mpsc::Sender<Result<SlotEvent>>,
) -> Result<()> {
    let channel = rc.cfg.connect(url).await?;
    let mut client = ProtoV2Client::new(channel);

    let response = client.subscribe_slots(SubscribeSlotsRequest {}).await?;
    let mut inbound = response.into_inner();

    rc.on_connected(idx);

    let mut shutdown_rx = rc.shutdown();
    loop {
        tokio::select! {
            msg = inbound.next() => {
                match msg {
                    Some(Ok(event)) => {
                        if tx.send(Ok(event)).await.is_err() {
                            return Ok(());
                        }
                    }
                    Some(Err(e)) => return Err(Error::from(e)),
                    None => return Err(Error::Stream("stream ended".into())),
                }
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
