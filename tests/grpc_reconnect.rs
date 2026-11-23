#![cfg(feature = "grpc")]

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use orbitflare_sdk::proto::geyser::{
    SubscribeReplayInfoRequest, SubscribeReplayInfoResponse, SubscribeRequest,
    SubscribeRequestFilterTransactions, SubscribeUpdate, SubscribeUpdatePong, SubscribeUpdateSlot,
    subscribe_update::UpdateOneof,
};
use orbitflare_sdk::{
    ConnectInfo, ConnectionEvent, ConnectionEvents, GeyserClient, GeyserClientBuilder, RetryPolicy,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::codegen::{BoxFuture, Service, http};
use tonic::server::NamedService;

type UpdateStream =
    Pin<Box<dyn tokio_stream::Stream<Item = Result<SubscribeUpdate, tonic::Status>> + Send>>;

#[derive(Default)]
struct State {
    requests: Mutex<Vec<(u32, SubscribeRequest)>>,
    connections: AtomicU32,
    next_slot: AtomicU64,
    drop_after: AtomicU64,
    replay: Mutex<Option<Option<u64>>>,
}

impl State {
    fn requests(&self) -> Vec<(u32, SubscribeRequest)> {
        self.requests.lock().unwrap().clone()
    }

    fn subscriptions(&self) -> Vec<(u32, SubscribeRequest)> {
        self.requests()
            .into_iter()
            .filter(|(_, r)| r.ping.is_none())
            .collect()
    }
}

#[derive(Clone)]
struct MockGeyser(Arc<State>);

impl NamedService for MockGeyser {
    const NAME: &'static str = "geyser.Geyser";
}

struct Subscribe(Arc<State>);

impl tonic::server::StreamingService<SubscribeRequest> for Subscribe {
    type Response = SubscribeUpdate;
    type ResponseStream = UpdateStream;
    type Future = BoxFuture<tonic::Response<UpdateStream>, tonic::Status>;

    fn call(
        &mut self,
        request: tonic::Request<tonic::Streaming<SubscribeRequest>>,
    ) -> Self::Future {
        let state = Arc::clone(&self.0);
        Box::pin(async move {
            let connection = state.connections.fetch_add(1, Ordering::SeqCst) + 1;
            let mut inbound = request.into_inner();
            let (tx, rx) = mpsc::channel(64);
            tokio::spawn(async move {
                let mut sent = 0u64;
                let mut ticker = tokio::time::interval(Duration::from_millis(10));
                let mut subscribed = false;
                loop {
                    tokio::select! {
                        msg = inbound.next() => match msg {
                            Some(Ok(req)) => {
                                if let Some(ping) = &req.ping {
                                    let pong = SubscribeUpdate {
                                        update_oneof: Some(UpdateOneof::Pong(SubscribeUpdatePong { id: ping.id })),
                                        ..Default::default()
                                    };
                                    let _ = tx.send(Ok(pong)).await;
                                } else {
                                    subscribed = true;
                                }
                                state.requests.lock().unwrap().push((connection, req));
                            }
                            _ => return,
                        },
                        _ = ticker.tick(), if subscribed => {
                            let limit = state.drop_after.load(Ordering::SeqCst);
                            if limit > 0 && sent >= limit {
                                let _ = tx.send(Err(tonic::Status::unavailable("mock drop"))).await;
                                return;
                            }
                            let slot = state.next_slot.fetch_add(1, Ordering::SeqCst);
                            let update = SubscribeUpdate {
                                update_oneof: Some(UpdateOneof::Slot(SubscribeUpdateSlot { slot, ..Default::default() })),
                                ..Default::default()
                            };
                            if tx.send(Ok(update)).await.is_err() {
                                return;
                            }
                            sent += 1;
                        }
                    }
                }
            });
            Ok(tonic::Response::new(
                Box::pin(ReceiverStream::new(rx)) as UpdateStream
            ))
        })
    }
}

struct ReplayInfo(Arc<State>);

impl tonic::server::UnaryService<SubscribeReplayInfoRequest> for ReplayInfo {
    type Response = SubscribeReplayInfoResponse;
    type Future = BoxFuture<tonic::Response<SubscribeReplayInfoResponse>, tonic::Status>;

    fn call(&mut self, _: tonic::Request<SubscribeReplayInfoRequest>) -> Self::Future {
        let answer = *self.0.replay.lock().unwrap();
        Box::pin(async move {
            match answer {
                Some(first_available) => Ok(tonic::Response::new(SubscribeReplayInfoResponse {
                    first_available,
                })),
                None => Err(tonic::Status::unimplemented("no replay info")),
            }
        })
    }
}

impl<B> Service<http::Request<B>> for MockGeyser
where
    B: tonic::codegen::Body + Send + 'static,
    B::Error: Into<tonic::codegen::StdError> + Send + 'static,
{
    type Response = http::Response<tonic::body::Body>;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let state = Arc::clone(&self.0);
        match req.uri().path() {
            "/geyser.Geyser/Subscribe" => Box::pin(async move {
                let mut grpc = tonic::server::Grpc::new(tonic_prost::ProstCodec::default());
                Ok(grpc.streaming(Subscribe(state), req).await)
            }),
            "/geyser.Geyser/SubscribeReplayInfo" => Box::pin(async move {
                let mut grpc = tonic::server::Grpc::new(tonic_prost::ProstCodec::default());
                Ok(grpc.unary(ReplayInfo(state), req).await)
            }),
            _ => Box::pin(async { Ok(tonic::Status::unimplemented("").into_http()) }),
        }
    }
}

async fn serve(state: Arc<State>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(MockGeyser(state))
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );
    format!("http://{addr}")
}

fn client(url: &str) -> GeyserClient {
    GeyserClientBuilder::new()
        .url(url)
        .retry(RetryPolicy {
            initial_delay: Duration::from_millis(20),
            max_delay: Duration::from_millis(50),
            ..RetryPolicy::default()
        })
        .build()
        .unwrap()
}

fn named(filter: &str) -> SubscribeRequest {
    SubscribeRequest {
        transactions: HashMap::from([(
            filter.to_string(),
            SubscribeRequestFilterTransactions::default(),
        )]),
        ..Default::default()
    }
}

fn filter_of(request: &SubscribeRequest) -> String {
    request
        .transactions
        .keys()
        .next()
        .cloned()
        .unwrap_or_default()
}

fn slot(update: &SubscribeUpdate) -> u64 {
    match &update.update_oneof {
        Some(UpdateOneof::Slot(s)) => s.slot,
        other => panic!("expected a slot update, got {other:?}"),
    }
}

async fn drain(events: &mut ConnectionEvents) -> Vec<ConnectionEvent> {
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(200);
    while let Ok(Some(e)) = tokio::time::timeout_at(deadline, events.recv()).await {
        out.push(e);
    }
    out
}

async fn eventually(what: &str, check: impl Fn() -> bool) {
    for _ in 0..200 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test]
async fn reconnects_rebuild_the_request_and_resume_after_the_last_slot() {
    let state = Arc::new(State {
        next_slot: AtomicU64::new(1000),
        drop_after: AtomicU64::new(5),
        replay: Mutex::new(Some(Some(0))),
        ..Default::default()
    });
    let url = serve(Arc::clone(&state)).await;
    let infos: Arc<Mutex<Vec<ConnectInfo>>> = Arc::default();
    let seen = Arc::clone(&infos);
    let mut stream = client(&url).subscribe_with(move |info| {
        seen.lock().unwrap().push(info.clone());
        let mut request = named("resume");
        request.from_slot = info.last_slot.map(|s| s + 1);
        request
    });
    let mut events = stream.events();

    let mut slots = Vec::new();
    while slots.len() < 15 {
        slots.push(slot(&stream.next().await.unwrap().unwrap()));
    }
    assert_eq!(slots, (1000..1015).collect::<Vec<_>>());

    let infos = infos.lock().unwrap().clone();
    assert!(infos.len() >= 3);
    assert!(!infos[0].reconnect && infos[0].last_slot.is_none());
    assert!(infos[1].reconnect && infos[1].last_slot == Some(1004));
    assert_eq!(infos[2].last_slot, Some(1009));

    let subs = state.subscriptions();
    assert_eq!(subs[0].1.from_slot, None);
    assert_eq!((subs[1].0, subs[1].1.from_slot), (2, Some(1005)));
    assert_eq!((subs[2].0, subs[2].1.from_slot), (3, Some(1010)));

    let events = drain(&mut events).await;
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| match e {
            ConnectionEvent::Connecting { .. } => "connecting",
            ConnectionEvent::Connected { .. } => "connected",
            ConnectionEvent::Disconnected { .. } => "disconnected",
            _ => "other",
        })
        .collect();
    assert_eq!(
        &kinds[..7],
        [
            "connecting",
            "connected",
            "disconnected",
            "connecting",
            "connected",
            "disconnected",
            "connecting"
        ]
    );
    match &events[2] {
        ConnectionEvent::Disconnected {
            endpoint,
            error,
            retry_in,
        } => {
            assert_eq!(endpoint, &url);
            assert!(error.contains("mock drop"), "{error}");
            assert!(retry_in.is_some());
        }
        other => panic!("{other:?}"),
    }
    stream.close();
}

#[tokio::test]
async fn updates_apply_in_place_and_survive_a_reconnect() {
    let state = Arc::new(State::default());
    let url = serve(Arc::clone(&state)).await;
    let mut stream = client(&url).subscribe(named("first"));
    stream.next().await.unwrap().unwrap();

    let mut update = named("second");
    update.from_slot = Some(7);
    stream.handle().update(update).unwrap();
    eventually("the update", || state.subscriptions().len() == 2).await;
    let subs = state.subscriptions();
    assert_eq!(state.connections.load(Ordering::SeqCst), 1);
    assert_eq!(
        (subs[1].0, filter_of(&subs[1].1), subs[1].1.from_slot),
        (1, "second".into(), None)
    );

    state.drop_after.store(1, Ordering::SeqCst);
    eventually("a reconnect", || {
        state.connections.load(Ordering::SeqCst) >= 2
    })
    .await;
    eventually("the resubscription", || {
        state.subscriptions().iter().any(|(c, _)| *c == 2)
    })
    .await;
    let resubscribed = state
        .subscriptions()
        .into_iter()
        .find(|(c, _)| *c == 2)
        .unwrap()
        .1;
    assert_eq!(filter_of(&resubscribed), "second");
    assert_eq!(resubscribed.from_slot, None);
    stream.close();
}

#[tokio::test]
async fn updates_sent_while_disconnected_apply_on_the_next_connection() {
    let state = Arc::new(State {
        drop_after: AtomicU64::new(1),
        ..Default::default()
    });
    let url = serve(Arc::clone(&state)).await;
    let client = GeyserClientBuilder::new()
        .url(&url)
        .retry(RetryPolicy {
            initial_delay: Duration::from_millis(300),
            max_delay: Duration::from_millis(300),
            ..RetryPolicy::default()
        })
        .build()
        .unwrap();
    let stream = client.subscribe(named("first"));
    let mut events = stream.events();
    loop {
        if let Some(ConnectionEvent::Disconnected { .. }) = events.recv().await {
            break;
        }
    }
    stream.update(named("second")).unwrap();
    eventually("the reconnect", || {
        state.subscriptions().iter().any(|(c, _)| *c == 2)
    })
    .await;
    let subs = state.subscriptions();
    let second: Vec<_> = subs.iter().filter(|(c, _)| *c == 2).collect();
    assert_eq!(second.len(), 1, "subscribed once, already updated");
    assert_eq!(filter_of(&second[0].1), "second");
    stream.close();
}

#[tokio::test]
async fn from_slot_older_than_the_replay_window_is_clamped() {
    let state = Arc::new(State {
        replay: Mutex::new(Some(Some(500))),
        ..Default::default()
    });
    let url = serve(Arc::clone(&state)).await;
    let mut stream = client(&url).subscribe_with(|_| SubscribeRequest {
        from_slot: Some(100),
        ..named("x")
    });
    let mut events = stream.events();
    stream.next().await.unwrap().unwrap();
    assert_eq!(state.subscriptions()[0].1.from_slot, Some(500));
    assert!(
        drain(&mut events)
            .await
            .contains(&ConnectionEvent::ReplayClamped {
                requested: 100,
                available: 500
            })
    );
}

#[tokio::test]
async fn from_slot_inside_the_window_is_kept() {
    let state = Arc::new(State {
        replay: Mutex::new(Some(Some(50))),
        ..Default::default()
    });
    let url = serve(Arc::clone(&state)).await;
    let mut stream = client(&url).subscribe_with(|_| SubscribeRequest {
        from_slot: Some(100),
        ..named("x")
    });
    let mut events = stream.events();
    stream.next().await.unwrap().unwrap();
    assert_eq!(state.subscriptions()[0].1.from_slot, Some(100));
    assert!(!drain(&mut events).await.iter().any(|e| matches!(
        e,
        ConnectionEvent::ReplayClamped { .. } | ConnectionEvent::ReplayUnavailable { .. }
    )));
}

#[tokio::test]
async fn servers_without_replay_start_live() {
    let state = Arc::new(State {
        replay: Mutex::new(Some(None)),
        ..Default::default()
    });
    let url = serve(Arc::clone(&state)).await;
    let mut stream = client(&url).subscribe_with(|_| SubscribeRequest {
        from_slot: Some(100),
        ..named("x")
    });
    let mut events = stream.events();
    stream.next().await.unwrap().unwrap();
    assert_eq!(state.subscriptions()[0].1.from_slot, None);
    assert!(
        drain(&mut events)
            .await
            .contains(&ConnectionEvent::ReplayUnavailable { requested: 100 })
    );
}

#[tokio::test]
async fn a_failed_replay_lookup_leaves_from_slot_to_the_server() {
    let state = Arc::new(State::default());
    let url = serve(Arc::clone(&state)).await;
    let mut stream = client(&url).subscribe_with(|_| SubscribeRequest {
        from_slot: Some(100),
        ..named("x")
    });
    stream.next().await.unwrap().unwrap();
    assert_eq!(state.subscriptions()[0].1.from_slot, Some(100));
}

#[tokio::test]
async fn an_exhausted_retry_policy_ends_the_stream_and_says_so() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = GeyserClientBuilder::new()
        .url(&url)
        .timeout_secs(2)
        .retry(RetryPolicy {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(10),
            max_attempts: 2,
            ..RetryPolicy::default()
        })
        .build()
        .unwrap();
    let mut stream = client.subscribe(named("x"));
    let mut events = stream.events();
    assert!(stream.next().await.unwrap().is_err());
    assert!(stream.next().await.is_none());
    let events = drain(&mut events).await;
    let retries: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            ConnectionEvent::Disconnected { retry_in, .. } => Some(retry_in.is_some()),
            _ => None,
        })
        .collect();
    assert_eq!(retries, [true, false], "{events:?}");
}
