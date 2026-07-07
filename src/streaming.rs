//! Shared scaffolding for the gRPC streaming clients (gRPC/Geyser and
//! JetStream v1/v2).
//!
//! These pieces capture the parts that are identical across every streaming
//! client — connection setup, the endpoint/retry configuration, the builder
//! plumbing, and the reconnect/failover loop — so each client only has to
//! implement the bit that is actually unique to it: what it does on a live
//! connection.

use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tonic::transport::Channel;

use crate::endpoint::EndpointSet;
use crate::error::{Error, Result, SanitizedUrl};
use crate::retry::RetryPolicy;

pub(crate) const DEFAULT_PING_INTERVAL_SECS: u64 = 10;
pub(crate) const DEFAULT_MAX_MISSED_PONGS: u32 = 3;

pub(crate) struct StreamConfig {
    pub endpoints: EndpointSet,
    pub retry: RetryPolicy,
    pub timeout_secs: u64,
    pub keepalive_secs: u64,
    pub ping_interval_secs: u64,
    pub max_missed_pongs: u32,
    pub channel_capacity: usize,
}

impl StreamConfig {
    pub async fn connect(&self, url: &str) -> Result<Channel> {
        let endpoint = Channel::from_shared(url.to_string())
            .map_err(|e| Error::Config(format!("invalid endpoint: {e}")))?;
        endpoint
            .timeout(Duration::from_secs(self.timeout_secs))
            .tcp_keepalive(Some(Duration::from_secs(self.keepalive_secs)))
            .connect_timeout(Duration::from_secs(self.timeout_secs))
            .connect()
            .await
            .map_err(Error::transport)
    }
}

pub(crate) struct StreamConfigBuilder {
    url: Option<String>,
    fallbacks: Vec<String>,
    retry: RetryPolicy,
    timeout_secs: u64,
    keepalive_secs: u64,
    ping_interval_secs: u64,
    max_missed_pongs: u32,
    channel_capacity: usize,
}

impl Default for StreamConfigBuilder {
    fn default() -> Self {
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
}

impl StreamConfigBuilder {
    pub fn url(&mut self, url: &str) {
        self.url = Some(url.to_string());
    }

    pub fn urls(&mut self, urls: &[&str]) {
        if let Some((first, rest)) = urls.split_first() {
            self.url = Some(first.to_string());
            self.fallbacks = rest.iter().map(|s| s.to_string()).collect();
        }
    }

    pub fn fallback_url(&mut self, url: &str) {
        self.fallbacks.push(url.to_string());
    }

    pub fn fallback_urls(&mut self, urls: &[&str]) {
        self.fallbacks.extend(urls.iter().map(|s| s.to_string()));
    }

    pub fn retry(&mut self, policy: RetryPolicy) {
        self.retry = policy;
    }

    pub fn timeout_secs(&mut self, secs: u64) {
        self.timeout_secs = secs;
    }

    pub fn keepalive_secs(&mut self, secs: u64) {
        self.keepalive_secs = secs;
    }

    pub fn ping_interval_secs(&mut self, secs: u64) {
        self.ping_interval_secs = secs;
    }

    pub fn max_missed_pongs(&mut self, n: u32) {
        self.max_missed_pongs = n;
    }

    pub fn channel_capacity(&mut self, cap: usize) {
        self.channel_capacity = cap;
    }

    pub fn build(self, env_var: &str) -> Result<StreamConfig> {
        let url = self
            .url
            .or_else(|| std::env::var(env_var).ok())
            .ok_or_else(|| {
                Error::Config(format!(
                    "no URL provided. Pass .url() to the builder or set {env_var} in your environment"
                ))
            })?;

        Ok(StreamConfig {
            endpoints: EndpointSet::new(&url, &self.fallbacks),
            retry: self.retry,
            timeout_secs: self.timeout_secs,
            keepalive_secs: self.keepalive_secs,
            ping_interval_secs: self.ping_interval_secs,
            max_missed_pongs: self.max_missed_pongs,
            channel_capacity: self.channel_capacity,
        })
    }
}

pub(crate) struct Reconnector {
    pub cfg: Arc<StreamConfig>,
    shutdown_rx: watch::Receiver<bool>,
    attempt: u32,
    label: &'static str,
}

impl Reconnector {
    pub fn new(
        cfg: Arc<StreamConfig>,
        shutdown_rx: watch::Receiver<bool>,
        label: &'static str,
    ) -> Self {
        Self {
            cfg,
            shutdown_rx,
            attempt: 0,
            label,
        }
    }

    pub fn should_stop(&self) -> bool {
        *self.shutdown_rx.borrow()
    }

    pub fn shutdown(&self) -> watch::Receiver<bool> {
        self.shutdown_rx.clone()
    }

    pub fn pick(&mut self) -> (usize, String) {
        self.attempt += 1;
        let idx = self.cfg.endpoints.pick();
        let url = self.cfg.endpoints.get(idx);
        tracing::info!(endpoint = %SanitizedUrl(&url), attempt = self.attempt, "connecting ({})", self.label);
        (idx, url)
    }

    pub fn on_connected(&mut self, idx: usize) {
        self.cfg.endpoints.mark_success(idx);
        self.attempt = 0;
        tracing::info!("connected, streaming ({})", self.label);
    }

    pub async fn after_error<T>(
        &mut self,
        idx: usize,
        url: &str,
        err: Error,
        tx: &mpsc::Sender<Result<T>>,
    ) -> ControlFlow<()> {
        self.cfg.endpoints.mark_failure(idx);

        if !self.cfg.retry.has_attempts_left(self.attempt) {
            let _ = tx.send(Err(err.with_endpoint(url))).await;
            return ControlFlow::Break(());
        }

        let delay = self.cfg.retry.delay_for_attempt(self.attempt);
        tracing::warn!(
            error = %err,
            endpoint = %SanitizedUrl(url),
            attempt = self.attempt,
            delay_ms = delay.as_millis() as u64,
            "reconnecting ({})",
            self.label,
        );

        let mut shutdown_rx = self.shutdown_rx.clone();
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            res = shutdown_rx.changed() => {
                if res.is_err() || *shutdown_rx.borrow() {
                    return ControlFlow::Break(());
                }
            }
        }
        ControlFlow::Continue(())
    }
}

#[cfg(feature = "jetstream")]
pub(crate) async fn unary_with_failover<F, Fut, T>(cfg: &StreamConfig, call: F) -> Result<T>
where
    F: Fn(Channel) -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let idx = cfg.endpoints.pick();
        let url = cfg.endpoints.get(idx);

        let result = async {
            let channel = cfg.connect(&url).await?;
            call(channel).await
        }
        .await;

        match result {
            Ok(v) => {
                cfg.endpoints.mark_success(idx);
                return Ok(v);
            }
            Err(e) => {
                cfg.endpoints.mark_failure(idx);
                if !e.is_retryable() || !cfg.retry.has_attempts_left(attempt) {
                    return Err(e.with_endpoint(&url));
                }
                let delay = cfg.retry.delay_for_attempt(attempt);
                tokio::time::sleep(delay).await;
            }
        }
    }
}
