use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use crate::credentials;
use crate::endpoint::EndpointSet;
use crate::error::{Error, Result, SanitizedUrl};
use crate::retry::RetryPolicy;

const DEFAULT_PING_INTERVAL_SECS: u64 = 10;
const DEFAULT_MAX_MISSED_PONGS: u32 = 3;

pub struct WsClient {
    cmd_tx: mpsc::Sender<WsCommand>,
}

pub struct WsSubscription {
    rx: mpsc::Receiver<Value>,
    sub_id: u64,
    unsub_method: String,
    cmd_tx: mpsc::Sender<WsCommand>,
}

impl WsSubscription {
    pub async fn next(&mut self) -> Option<Value> {
        self.rx.recv().await
    }

    pub async fn unsubscribe(self) {
        let _ = self
            .cmd_tx
            .send(WsCommand::Unsubscribe {
                sub_id: self.sub_id,
                method: self.unsub_method,
            })
            .await;
    }
}

enum WsCommand {
    Subscribe {
        method: String,
        params: Value,
        event_tx: mpsc::Sender<Value>,
        result_tx: oneshot::Sender<Result<u64>>,
    },
    Unsubscribe {
        sub_id: u64,
        method: String,
    },
}

struct ActiveSub {
    method: String,
    params: Value,
    event_tx: mpsc::Sender<Value>,
}

pub struct WsClientBuilder {
    url: Option<String>,
    fallbacks: Vec<String>,
    api_key: Option<String>,
    retry: RetryPolicy,
    ping_interval_secs: u64,
    max_missed_pongs: u32,
}

impl Default for WsClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WsClientBuilder {
    pub fn new() -> Self {
        Self {
            url: None,
            fallbacks: Vec::new(),
            api_key: None,
            retry: RetryPolicy::default(),
            ping_interval_secs: DEFAULT_PING_INTERVAL_SECS,
            max_missed_pongs: DEFAULT_MAX_MISSED_PONGS,
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

    pub fn api_key(mut self, key: &str) -> Self {
        self.api_key = Some(key.to_string());
        self
    }

    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = policy;
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

    pub async fn build(self) -> Result<WsClient> {
        let url = self
            .url
            .or_else(|| std::env::var("ORBITFLARE_WS_URL").ok())
            .ok_or_else(|| {
                Error::Config(
                    "no WebSocket URL provided. Pass .url() to the builder \
                     or set ORBITFLARE_WS_URL in your environment"
                        .into(),
                )
            })?;

        let api_key = self
            .api_key
            .or_else(|| std::env::var("ORBITFLARE_LICENSE_KEY").ok());

        let mut endpoint_urls = Vec::with_capacity(1 + self.fallbacks.len());
        let apply = |u: &str| match &api_key {
            Some(k) => credentials::apply_api_key(u, k),
            None => u.to_string(),
        };
        endpoint_urls.push(apply(&url));
        for fb in &self.fallbacks {
            endpoint_urls.push(apply(fb));
        }

        let endpoints = EndpointSet::new(&endpoint_urls[0], &endpoint_urls[1..]);

        let (ws, _) = try_connect(&endpoints).await?;

        let (cmd_tx, cmd_rx) = mpsc::channel(64);

        tokio::spawn(background_task(
            endpoints,
            ws,
            cmd_rx,
            self.retry,
            self.ping_interval_secs,
            self.max_missed_pongs,
        ));

        Ok(WsClient { cmd_tx })
    }
}

impl WsClient {
    pub async fn account_subscribe(
        &self,
        address: &str,
        commitment: &str,
    ) -> Result<WsSubscription> {
        self.subscribe_inner(
            "accountSubscribe",
            "accountUnsubscribe",
            json!([address, {"encoding": "base64", "commitment": commitment}]),
        )
        .await
    }

    pub async fn logs_subscribe(
        &self,
        mentions: &[&str],
        commitment: &str,
    ) -> Result<WsSubscription> {
        let filter = if mentions.is_empty() {
            json!("all")
        } else {
            json!({"mentions": mentions})
        };
        self.subscribe_inner(
            "logsSubscribe",
            "logsUnsubscribe",
            json!([filter, {"commitment": commitment}]),
        )
        .await
    }

    pub async fn slot_subscribe(&self) -> Result<WsSubscription> {
        self.subscribe_inner("slotSubscribe", "slotUnsubscribe", json!([]))
            .await
    }

    pub async fn signature_subscribe(
        &self,
        signature: &str,
        commitment: &str,
    ) -> Result<WsSubscription> {
        self.subscribe_inner(
            "signatureSubscribe",
            "signatureUnsubscribe",
            json!([signature, {"commitment": commitment}]),
        )
        .await
    }

    async fn subscribe_inner(
        &self,
        method: &str,
        unsub_method: &str,
        params: Value,
    ) -> Result<WsSubscription> {
        let (event_tx, event_rx) = mpsc::channel(256);
        let (result_tx, result_rx) = oneshot::channel();

        self.cmd_tx
            .send(WsCommand::Subscribe {
                method: method.to_string(),
                params,
                event_tx,
                result_tx,
            })
            .await
            .map_err(|_| Error::Stream("ws background task gone".into()))?;

        let sub_id = result_rx
            .await
            .map_err(|_| Error::Stream("ws subscribe response lost".into()))??;

        Ok(WsSubscription {
            rx: event_rx,
            sub_id,
            unsub_method: unsub_method.to_string(),
            cmd_tx: self.cmd_tx.clone(),
        })
    }
}

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn try_connect(endpoints: &EndpointSet) -> Result<(WsStream, usize)> {
    for i in 0..endpoints.len() {
        let url = endpoints.get(i);
        match tokio_tungstenite::connect_async(&url).await {
            Ok((ws, _)) => {
                tracing::info!(endpoint = %SanitizedUrl(&url), "ws connected");
                return Ok((ws, i));
            }
            Err(e) => {
                tracing::warn!(endpoint = %SanitizedUrl(&url), error = %e, "ws connect failed");
                endpoints.mark_failure(i);
            }
        }
    }
    Err(Error::Transport(
        "all ws endpoints failed".to_string().into(),
    ))
}

#[allow(clippy::too_many_arguments)]
async fn background_task(
    endpoints: EndpointSet,
    initial_ws: WsStream,
    mut cmd_rx: mpsc::Receiver<WsCommand>,
    retry: RetryPolicy,
    ping_interval_secs: u64,
    max_missed_pongs: u32,
) {
    let mut subs: Vec<ActiveSub> = Vec::new();
    let mut attempt = 0u32;

    let result = run_connection(
        initial_ws,
        &mut cmd_rx,
        &mut subs,
        ping_interval_secs,
        max_missed_pongs,
    )
    .await;

    if result.is_ok() {
        return;
    }

    loop {
        attempt += 1;
        if !retry.has_attempts_left(attempt) {
            tracing::error!("ws reconnect attempts exhausted");
            return;
        }

        let delay = retry.delay_for_attempt(attempt);
        tracing::warn!(
            attempt,
            delay_ms = delay.as_millis() as u64,
            "ws reconnecting",
        );
        tokio::time::sleep(delay).await;

        let (ws, idx) = match try_connect(&endpoints).await {
            Ok(pair) => {
                attempt = 0;
                endpoints.mark_success(pair.1);
                pair
            }
            Err(e) => {
                tracing::warn!(error = %e, "ws reconnect failed");
                continue;
            }
        };

        match run_connection(
            ws,
            &mut cmd_rx,
            &mut subs,
            ping_interval_secs,
            max_missed_pongs,
        )
        .await
        {
            Ok(()) => return,
            Err(e) => {
                endpoints.mark_failure(idx);
                tracing::warn!(error = %e, "ws connection lost");
            }
        }
    }
}

async fn run_connection(
    ws: WsStream,
    cmd_rx: &mut mpsc::Receiver<WsCommand>,
    subs: &mut Vec<ActiveSub>,
    ping_interval_secs: u64,
    max_missed_pongs: u32,
) -> Result<()> {
    let (mut write, mut read) = ws.split();
    let mut sub_id_to_idx: HashMap<u64, usize> = HashMap::new();
    let mut pending: HashMap<u64, PendingKind> = HashMap::new();
    let mut next_id: u64 = 1;

    for (idx, sub) in subs.iter().enumerate() {
        let id = next_id;
        next_id += 1;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": &sub.method,
            "params": &sub.params,
        });
        write
            .send(Message::text(msg.to_string()))
            .await
            .map_err(Error::transport)?;
        pending.insert(id, PendingKind::Resubscribe(idx));
    }

    let mut ping_timer = tokio::time::interval(Duration::from_secs(ping_interval_secs));
    ping_timer.tick().await;
    let mut missed_pongs: u32 = 0;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        missed_pongs = 0;
                        handle_text(
                            text.as_str(),
                            subs,
                            &mut sub_id_to_idx,
                            &mut pending,
                            &mut write,
                        ).await;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = write.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        missed_pongs = 0;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        return Err(Error::Stream("ws closed".into()));
                    }
                    Some(Err(e)) => {
                        return Err(Error::transport(e));
                    }
                    _ => {}
                }
            }
            _ = ping_timer.tick() => {
                if missed_pongs >= max_missed_pongs {
                    return Err(Error::Stream(format!(
                        "no pong after {} pings", max_missed_pongs
                    )));
                }
                if write.send(Message::Ping(vec![].into())).await.is_err() {
                    return Err(Error::Stream("ws ping send failed".into()));
                }
                missed_pongs += 1;
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(WsCommand::Subscribe { method, params, event_tx, result_tx }) => {
                        let id = next_id;
                        next_id += 1;
                        let msg = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "method": &method,
                            "params": &params,
                        });
                        if write.send(Message::text(msg.to_string())).await.is_err() {
                            let _ = result_tx.send(Err(Error::Stream("ws send failed".into())));
                            return Err(Error::Stream("ws send failed".into()));
                        }
                        pending.insert(id, PendingKind::NewSubscribe {
                            method,
                            params,
                            event_tx,
                            result_tx,
                        });
                    }
                    Some(WsCommand::Unsubscribe { sub_id, method }) => {
                        if let Some(idx) = sub_id_to_idx.remove(&sub_id) {
                            subs.remove(idx);
                            rebuild_index(subs, &mut sub_id_to_idx);
                        }
                        let id = next_id;
                        next_id += 1;
                        let msg = json!({"jsonrpc":"2.0","id":id,"method":method,"params":[sub_id]});
                        let _ = write.send(Message::text(msg.to_string())).await;
                    }
                    None => return Ok(()),
                }
            }
        }
    }
}

enum PendingKind {
    NewSubscribe {
        method: String,
        params: Value,
        event_tx: mpsc::Sender<Value>,
        result_tx: oneshot::Sender<Result<u64>>,
    },
    Resubscribe(usize),
}

type WsWrite = futures_util::stream::SplitSink<WsStream, Message>;

async fn handle_text(
    text: &str,
    subs: &mut Vec<ActiveSub>,
    sub_id_to_idx: &mut HashMap<u64, usize>,
    pending: &mut HashMap<u64, PendingKind>,
    write: &mut WsWrite,
) {
    let parsed: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };

    if let Some(req_id) = parsed.get("id").and_then(|i| i.as_u64()) {
        if let Some(kind) = pending.remove(&req_id) {
            match kind {
                PendingKind::NewSubscribe {
                    method,
                    params,
                    event_tx,
                    result_tx,
                } => {
                    if let Some(error) = parsed.get("error") {
                        let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
                        let msg = error
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("subscribe failed");
                        let _ = result_tx.send(Err(Error::Rpc {
                            code,
                            message: msg.to_string(),
                        }));
                        return;
                    }
                    if let Some(sub_id) = parsed.get("result").and_then(|r| r.as_u64()) {
                        let idx = subs.len();
                        subs.push(ActiveSub {
                            method,
                            params,
                            event_tx,
                        });
                        sub_id_to_idx.insert(sub_id, idx);
                        let _ = result_tx.send(Ok(sub_id));
                    } else {
                        let _ = result_tx.send(Err(Error::Rpc {
                            code: 0,
                            message: "missing subscription id in response".to_string(),
                        }));
                    }
                }
                PendingKind::Resubscribe(idx) => {
                    if let Some(sub_id) = parsed.get("result").and_then(|r| r.as_u64()) {
                        sub_id_to_idx.insert(sub_id, idx);
                        tracing::debug!(sub_id, idx, "resubscribed");
                    }
                }
            }
        }
        return;
    }

    if let Some(params) = parsed.get("params") {
        if let Some(sub_id) = params.get("subscription").and_then(|s| s.as_u64()) {
            if let Some(&idx) = sub_id_to_idx.get(&sub_id) {
                if let Some(sub) = subs.get(idx) {
                    if let Some(result) = params.get("result") {
                        let payload =
                            if result.get("context").is_some() && result.get("value").is_some() {
                                result["value"].clone()
                            } else {
                                result.clone()
                            };
                        if sub.event_tx.send(payload).await.is_err() {
                            auto_unsubscribe(sub_id, &parsed, write).await;
                        }
                    }
                }
            } else {
                auto_unsubscribe(sub_id, &parsed, write).await;
            }
        }
    }
}

async fn auto_unsubscribe(sub_id: u64, parsed: &Value, write: &mut WsWrite) {
    if let Some(method) = parsed.get("method").and_then(|m| m.as_str()) {
        if let Some(operation) = method.strip_suffix("Notification") {
            let unsub_method = format!("{operation}Unsubscribe");
            let msg = json!({"jsonrpc":"2.0","id":0,"method":unsub_method,"params":[sub_id]});
            let _ = write.send(Message::text(msg.to_string())).await;
            tracing::debug!(
                sub_id,
                unsub_method,
                "auto-unsubscribed orphaned subscription"
            );
        }
    }
}

fn rebuild_index(subs: &[ActiveSub], sub_id_to_idx: &mut HashMap<u64, usize>) {
    let old: Vec<(u64, usize)> = sub_id_to_idx.drain().collect();
    for (sub_id, old_idx) in old {
        if old_idx < subs.len() {
            sub_id_to_idx.insert(sub_id, old_idx);
        }
    }
}
