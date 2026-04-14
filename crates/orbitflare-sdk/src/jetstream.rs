use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio_stream::StreamExt;
use tonic::transport::Channel;

use orbitflare_sdk_proto::jetstream::{
    jetstream_client::JetstreamClient as ProtoJetstreamClient, SubscribeRequest,
    SubscribeRequestPing, SubscribeUpdate, subscribe_update::UpdateOneof,
};

use crate::endpoint::EndpointSet;
use crate::error::{Error, Result, SanitizedUrl};
use crate::retry::RetryPolicy;

const DEFAULT_PING_INTERVAL_SECS: u64 = 10;
const DEFAULT_MAX_MISSED_PONGS: u32 = 3;

struct JetstreamClientInner {
    endpoints: EndpointSet,
    retry: RetryPolicy,
    timeout_secs: u64,
    keepalive_secs: u64,
    ping_interval_secs: u64,
    max_missed_pongs: u32,
    channel_capacity: usize,
}

pub struct JetstreamClient {
    inner: Arc<JetstreamClientInner>,
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

        tokio::spawn(stream_task(Arc::clone(&self.inner), request, tx, shutdown_rx));

        JetstreamStream {
            rx,
            _shutdown: shutdown_tx,
        }
    }
}

async fn stream_task(
    inner: Arc<JetstreamClientInner>,
    request: SubscribeRequest,
    tx: mpsc::Sender<Result<SubscribeUpdate>>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut attempt = 0u32;

    loop {
        if *shutdown_rx.borrow() {
            return;
        }

        attempt += 1;
        let idx = inner.endpoints.pick();
        let url = inner.endpoints.get(idx);

        tracing::info!(endpoint = %SanitizedUrl(&url), attempt, "connecting");

        match connect_and_stream(&url, &inner, &request, &tx, &mut shutdown_rx).await {
            Ok(()) => {
                inner.endpoints.mark_success(idx);
                return;
            }
            Err(e) => {
                inner.endpoints.mark_failure(idx);

                if !inner.retry.has_attempts_left(attempt) {
                    let _ = tx.send(Err(e.with_endpoint(&url))).await;
                    return;
                }

                let delay = inner.retry.delay_for_attempt(attempt);
                tracing::warn!(
                    error = %e,
                    endpoint = %SanitizedUrl(&url),
                    attempt,
                    delay_ms = delay.as_millis() as u64,
                    "reconnecting",
                );
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() { return; }
                    }
                }
            }
        }
    }
}

async fn connect_and_stream(
    url: &str,
    inner: &JetstreamClientInner,
    request: &SubscribeRequest,
    tx: &mpsc::Sender<Result<SubscribeUpdate>>,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Result<()> {
    let endpoint = Channel::from_shared(url.to_string())
        .map_err(|e| Error::Config(format!("invalid endpoint: {e}")))?;
    let channel = endpoint
        .timeout(Duration::from_secs(inner.timeout_secs))
        .tcp_keepalive(Some(Duration::from_secs(inner.keepalive_secs)))
        .connect_timeout(Duration::from_secs(inner.timeout_secs))
        .connect()
        .await
        .map_err(Error::transport)?;

    let mut client = ProtoJetstreamClient::new(channel);

    let (outbound_tx, outbound_rx) = mpsc::channel::<SubscribeRequest>(4);
    outbound_tx.send(request.clone()).await.map_err(|_| Error::Stream("outbound closed".into()))?;

    let outbound_stream = tokio_stream::wrappers::ReceiverStream::new(outbound_rx);
    let response: tonic::Response<tonic::Streaming<SubscribeUpdate>> =
        client.subscribe(outbound_stream).await?;
    let mut inbound = response.into_inner();

    tracing::info!("connected, streaming");

    let mut ping_interval = tokio::time::interval(Duration::from_secs(inner.ping_interval_secs));
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
                if missed_pongs >= inner.max_missed_pongs {
                    return Err(Error::Stream(format!(
                        "no pong after {} pings",
                        inner.max_missed_pongs
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
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() { return Ok(()); }
            }
        }
    }
}

pub struct JetstreamClientBuilder {
    url: Option<String>,
    fallbacks: Vec<String>,
    retry: RetryPolicy,
    timeout_secs: u64,
    keepalive_secs: u64,
    ping_interval_secs: u64,
    max_missed_pongs: u32,
    channel_capacity: usize,
}

impl Default for JetstreamClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl JetstreamClientBuilder {
    pub fn new() -> Self {
        Self {
            url: None,
            fallbacks: Vec::new(),
            retry: RetryPolicy::default(),
            timeout_secs: 30,
            keepalive_secs: 60,
            ping_interval_secs: DEFAULT_PING_INTERVAL_SECS,
            max_missed_pongs: DEFAULT_MAX_MISSED_PONGS,
            channel_capacity: 4096,
        }
    }

    pub fn url(mut self, url: &str) -> Self {
        self.url = Some(url.to_string());
        self
    }

    pub fn urls(mut self, urls: &[&str]) -> Self {
        if let Some((first, rest)) = urls.split_first() {
            self.url = Some(first.to_string());
            self.fallbacks = rest.iter().map(|s| s.to_string()).collect();
        }
        self
    }

    pub fn fallback_url(mut self, url: &str) -> Self {
        self.fallbacks.push(url.to_string());
        self
    }

    pub fn fallback_urls(mut self, urls: &[&str]) -> Self {
        self.fallbacks.extend(urls.iter().map(|s| s.to_string()));
        self
    }

    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = policy;
        self
    }

    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn keepalive_secs(mut self, secs: u64) -> Self {
        self.keepalive_secs = secs;
        self
    }

    pub fn ping_interval_secs(mut self, secs: u64) -> Self {
        self.ping_interval_secs = secs;
        self
    }

    pub fn max_missed_pongs(mut self, n: u32) -> Self {
        self.max_missed_pongs = n;
        self
    }

    pub fn channel_capacity(mut self, cap: usize) -> Self {
        self.channel_capacity = cap;
        self
    }

    pub fn build(self) -> Result<JetstreamClient> {
        let url = self
            .url
            .or_else(|| std::env::var("ORBITFLARE_JETSTREAM_URL").ok())
            .ok_or_else(|| {
                Error::Config(
                    "no JetStream URL provided. Pass .url() to the builder \
                     or set ORBITFLARE_JETSTREAM_URL in your environment"
                        .into(),
                )
            })?;

        let endpoints = EndpointSet::new(&url, &self.fallbacks);

        Ok(JetstreamClient {
            inner: Arc::new(JetstreamClientInner {
                endpoints,
                retry: self.retry,
                timeout_secs: self.timeout_secs,
                keepalive_secs: self.keepalive_secs,
                ping_interval_secs: self.ping_interval_secs,
                max_missed_pongs: self.max_missed_pongs,
                channel_capacity: self.channel_capacity,
            }),
        })
    }
}
