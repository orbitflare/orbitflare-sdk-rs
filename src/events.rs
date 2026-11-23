use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::broadcast;

use crate::error::sanitize_url;

const CAPACITY: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionEvent {
    Connecting {
        endpoint: String,
        attempt: u32,
    },
    Connected {
        endpoint: String,
    },
    Disconnected {
        endpoint: String,
        error: String,
        retry_in: Option<Duration>,
    },
    ReplayClamped {
        requested: u64,
        available: u64,
    },
    ReplayUnavailable {
        requested: u64,
    },
    Lagged {
        missed: u64,
    },
}

pub struct ConnectionEvents {
    rx: broadcast::Receiver<ConnectionEvent>,
}

impl ConnectionEvents {
    pub async fn recv(&mut self) -> Option<ConnectionEvent> {
        match self.rx.recv().await {
            Ok(event) => Some(event),
            Err(broadcast::error::RecvError::Lagged(missed)) => {
                Some(ConnectionEvent::Lagged { missed })
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }

    pub fn try_recv(&mut self) -> Option<ConnectionEvent> {
        match self.rx.try_recv() {
            Ok(event) => Some(event),
            Err(broadcast::error::TryRecvError::Lagged(missed)) => {
                Some(ConnectionEvent::Lagged { missed })
            }
            Err(_) => None,
        }
    }
}

pub(crate) struct EventHub {
    tx: broadcast::Sender<ConnectionEvent>,
    first: Mutex<Option<broadcast::Receiver<ConnectionEvent>>>,
}

impl EventHub {
    pub fn new() -> Self {
        let (tx, rx) = broadcast::channel(CAPACITY);
        Self {
            tx,
            first: Mutex::new(Some(rx)),
        }
    }

    pub fn sender(&self) -> EventSender {
        EventSender(self.tx.clone())
    }

    pub fn subscribe(&self) -> ConnectionEvents {
        let first = self.first.lock().unwrap_or_else(|e| e.into_inner()).take();
        ConnectionEvents {
            rx: first.unwrap_or_else(|| self.tx.subscribe()),
        }
    }
}

#[derive(Clone)]
pub(crate) struct EventSender(broadcast::Sender<ConnectionEvent>);

impl EventSender {
    pub fn send(&self, event: ConnectionEvent) {
        let _ = self.0.send(event);
    }
}

pub(crate) fn endpoint_label(url: &str) -> String {
    let url = sanitize_url(url);
    match url.split_once("://") {
        Some((scheme, rest)) => {
            let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            format!("{scheme}://{host}{path}")
        }
        None => url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn first_reader_sees_events_sent_before_it_subscribed() {
        let hub = EventHub::new();
        hub.sender().send(ConnectionEvent::Connected {
            endpoint: "a".into(),
        });
        let mut first = hub.subscribe();
        let mut second = hub.subscribe();
        hub.sender().send(ConnectionEvent::Connected {
            endpoint: "b".into(),
        });
        assert_eq!(
            first.recv().await,
            Some(ConnectionEvent::Connected {
                endpoint: "a".into()
            })
        );
        assert_eq!(
            first.recv().await,
            Some(ConnectionEvent::Connected {
                endpoint: "b".into()
            })
        );
        assert_eq!(
            second.recv().await,
            Some(ConnectionEvent::Connected {
                endpoint: "b".into()
            })
        );
    }

    #[tokio::test]
    async fn slow_readers_are_told_what_they_missed() {
        let hub = EventHub::new();
        let mut events = hub.subscribe();
        for i in 0..(CAPACITY as u32 + 10) {
            hub.sender().send(ConnectionEvent::Connecting {
                endpoint: "a".into(),
                attempt: i,
            });
        }
        assert_eq!(
            events.recv().await,
            Some(ConnectionEvent::Lagged { missed: 10 })
        );
    }

    #[test]
    fn endpoint_labels_hide_credentials() {
        assert_eq!(
            endpoint_label("https://user:pw@ams.rpc.orbitflare.com:10000/?api_key=x"),
            "https://ams.rpc.orbitflare.com:10000/"
        );
        assert_eq!(
            endpoint_label("http://ams.jetstream.orbitflare.com"),
            "http://ams.jetstream.orbitflare.com"
        );
    }

    #[tokio::test]
    async fn ends_when_the_stream_is_gone() {
        let hub = EventHub::new();
        let mut events = hub.subscribe();
        drop(hub);
        assert_eq!(events.recv().await, None);
    }
}
