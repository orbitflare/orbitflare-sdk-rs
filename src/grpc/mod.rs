use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio_stream::StreamExt;

use orbitflare_sdk_proto::geyser::{
    SubscribeReplayInfoRequest, SubscribeRequest, SubscribeRequestPing, SubscribeUpdate,
    geyser_client::GeyserClient as ProtoGeyserClient, subscribe_update::UpdateOneof,
};

pub mod filters;
pub use crate::filters::{Lamports, Memcmp, MemcmpData};
pub use filters::{
    AccountFilter, BlockFilter, BlockFooterFilter, Commitment, DeshredTransactionFilter,
    SlotFilter, SubscribeRequestBuilder, TransactionFilter,
};

use crate::error::{Error, Result};
use crate::events::{ConnectionEvent, ConnectionEvents, EventHub};
use crate::retry::RetryPolicy;
use crate::streaming::{AuthChannel, Reconnector, StreamConfig, StreamConfigBuilder};

const REPLAY_INFO_TIMEOUT: Duration = Duration::from_secs(10);

pub struct GeyserClient {
    inner: Arc<StreamConfig>,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ConnectInfo {
    pub endpoint: String,
    pub attempt: u32,
    pub reconnect: bool,
    pub last_slot: Option<u64>,
}

#[derive(Clone)]
pub struct GeyserHandle {
    cmd_tx: mpsc::UnboundedSender<SubscribeRequest>,
}

impl GeyserHandle {
    pub fn update(&self, request: SubscribeRequest) -> Result<()> {
        self.cmd_tx
            .send(request)
            .map_err(|_| Error::Stream("stream closed".into()))
    }
}

pub struct GeyserStream {
    rx: mpsc::Receiver<Result<SubscribeUpdate>>,
    handle: GeyserHandle,
    events: EventHub,
    _shutdown: watch::Sender<bool>,
}

impl GeyserStream {
    pub async fn next(&mut self) -> Option<Result<SubscribeUpdate>> {
        self.rx.recv().await
    }

    pub fn handle(&self) -> GeyserHandle {
        self.handle.clone()
    }

    pub fn update(&self, request: SubscribeRequest) -> Result<()> {
        self.handle.update(request)
    }

    pub fn events(&self) -> ConnectionEvents {
        self.events.subscribe()
    }

    pub fn close(self) {
        let _ = self._shutdown.send(true);
    }
}

type BuildRequest = Box<dyn FnMut(&ConnectInfo) -> SubscribeRequest + Send>;

enum RequestSource {
    Fixed(Box<SubscribeRequest>),
    Build(BuildRequest),
}

impl RequestSource {
    fn request(&mut self, info: &ConnectInfo) -> SubscribeRequest {
        match self {
            RequestSource::Fixed(request) => (**request).clone(),
            RequestSource::Build(build) => build(info),
        }
    }

    fn updated(&mut self, request: &SubscribeRequest) {
        if let RequestSource::Fixed(current) = self {
            **current = request.clone();
        }
    }
}

struct Session {
    source: RequestSource,
    commands: mpsc::UnboundedReceiver<SubscribeRequest>,
    connected_before: bool,
    last_slot: Option<u64>,
}

impl GeyserClient {
    pub fn subscribe_yaml(&self, path: &str) -> Result<GeyserStream> {
        let cfg: crate::config::GeyserStreamConfig = crate::config::read_yaml(path)?;
        Ok(self.subscribe(cfg.to_subscribe_request()))
    }

    pub fn subscribe(&self, request: SubscribeRequest) -> GeyserStream {
        self.spawn(RequestSource::Fixed(Box::new(request)))
    }

    pub fn subscribe_with<F>(&self, build: F) -> GeyserStream
    where
        F: FnMut(&ConnectInfo) -> SubscribeRequest + Send + 'static,
    {
        self.spawn(RequestSource::Build(Box::new(build)))
    }

    fn spawn(&self, source: RequestSource) -> GeyserStream {
        let (tx, rx) = mpsc::channel(self.inner.channel_capacity);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let events = EventHub::new();

        let rc = Reconnector::new(
            Arc::clone(&self.inner),
            shutdown_rx,
            "grpc",
            events.sender(),
        );
        let session = Session {
            source,
            commands: cmd_rx,
            connected_before: false,
            last_slot: None,
        };
        tokio::spawn(stream_task(rc, session, tx));

        GeyserStream {
            rx,
            handle: GeyserHandle { cmd_tx },
            events,
            _shutdown: shutdown_tx,
        }
    }
}

async fn stream_task(
    mut rc: Reconnector,
    mut session: Session,
    tx: mpsc::Sender<Result<SubscribeUpdate>>,
) {
    loop {
        if rc.should_stop() {
            return;
        }
        let (idx, url) = rc.pick();

        match run_connection(&url, idx, &mut rc, &mut session, &tx).await {
            Ok(()) => return,
            Err(e) => {
                if rc.after_error(idx, &url, e, &tx).await == ControlFlow::Break(()) {
                    return;
                }
            }
        }
    }
}

fn slot_of(update: &UpdateOneof) -> Option<u64> {
    match update {
        UpdateOneof::Slot(u) => Some(u.slot),
        UpdateOneof::Transaction(u) => Some(u.slot),
        UpdateOneof::TransactionStatus(u) => Some(u.slot),
        UpdateOneof::Account(u) => Some(u.slot),
        UpdateOneof::Block(u) => Some(u.slot),
        UpdateOneof::BlockMeta(u) => Some(u.slot),
        UpdateOneof::Entry(u) => Some(u.slot),
        _ => None,
    }
}

async fn replay_from(
    client: &mut ProtoGeyserClient<AuthChannel>,
    rc: &Reconnector,
    wanted: u64,
) -> Option<u64> {
    let info = tokio::time::timeout(
        REPLAY_INFO_TIMEOUT,
        client.subscribe_replay_info(SubscribeReplayInfoRequest {}),
    )
    .await;
    match info {
        Ok(Ok(response)) => match response.into_inner().first_available {
            Some(available) if wanted < available => {
                tracing::warn!(
                    requested = wanted,
                    available,
                    "from_slot is older than the replay window; starting at the oldest available slot"
                );
                rc.emit(ConnectionEvent::ReplayClamped {
                    requested: wanted,
                    available,
                });
                Some(available)
            }
            Some(_) => Some(wanted),
            None => {
                tracing::warn!(
                    requested = wanted,
                    "endpoint does not support replay; starting live"
                );
                rc.emit(ConnectionEvent::ReplayUnavailable { requested: wanted });
                None
            }
        },
        Ok(Err(status)) => {
            tracing::debug!(error = %status, "replay window lookup failed; using from_slot as is");
            Some(wanted)
        }
        Err(_) => {
            tracing::debug!("replay window lookup timed out; using from_slot as is");
            Some(wanted)
        }
    }
}

async fn run_connection(
    url: &str,
    idx: usize,
    rc: &mut Reconnector,
    session: &mut Session,
    tx: &mpsc::Sender<Result<SubscribeUpdate>>,
) -> Result<()> {
    let channel = rc.cfg.connect(url).await?;
    let mut client = ProtoGeyserClient::new(channel);

    while let Ok(mut update) = session.commands.try_recv() {
        update.from_slot = None;
        session.source.updated(&update);
    }
    let info = ConnectInfo {
        endpoint: crate::events::endpoint_label(url),
        attempt: rc.attempt(),
        reconnect: session.connected_before,
        last_slot: session.last_slot,
    };
    let mut request = session.source.request(&info);
    if let Some(wanted) = request.from_slot {
        request.from_slot = replay_from(&mut client, rc, wanted).await;
    }

    let (outbound_tx, outbound_rx) = mpsc::channel::<SubscribeRequest>(4);
    outbound_tx
        .send(request)
        .await
        .map_err(|_| Error::Stream("outbound closed".into()))?;

    let outbound_stream = tokio_stream::wrappers::ReceiverStream::new(outbound_rx);
    let response: tonic::Response<tonic::Streaming<SubscribeUpdate>> =
        client.subscribe(outbound_stream).await?;
    let mut inbound = response.into_inner();

    rc.on_connected(idx);
    session.connected_before = true;

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
                        missed_pongs = 0;
                        if let Some(oneof) = &update.update_oneof {
                            if let UpdateOneof::Pong(_) = oneof {
                                continue;
                            }
                            if let Some(slot) = slot_of(oneof) {
                                session.last_slot = Some(session.last_slot.map_or(slot, |s| s.max(slot)));
                            }
                        }
                        if tx.send(Ok(update)).await.is_err() {
                            return Ok(());
                        }
                    }
                    Some(Err(e)) => return Err(Error::from(e)),
                    None => return Err(Error::Stream("stream ended".into())),
                }
            }
            Some(mut update) = session.commands.recv() => {
                update.from_slot = None;
                session.source.updated(&update);
                if outbound_tx.send(update).await.is_err() {
                    return Err(Error::Stream("outbound closed".into()));
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
pub struct GeyserClientBuilder {
    cfg: StreamConfigBuilder,
}

impl GeyserClientBuilder {
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

    pub fn api_key(mut self, key: &str) -> Self {
        self.cfg.api_key(key);
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

    pub fn build(self) -> Result<GeyserClient> {
        Ok(GeyserClient {
            inner: Arc::new(self.cfg.build("ORBITFLARE_GRPC_URL")?),
        })
    }
}
