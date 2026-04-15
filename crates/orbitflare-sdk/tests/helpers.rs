use orbitflare_sdk::RetryPolicy;
use orbitflare_sdk::error::Error;
use std::time::Duration;

#[test]
fn retryable_classification() {
    let retryable: Vec<Error> = vec![
        Error::Transport(Box::new(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "x",
        ))),
        Error::Timeout,
        Error::RateLimited {
            retry_after: Some(Duration::from_secs(5)),
        },
        Error::RateLimited { retry_after: None },
        Error::Stream("closed".into()),
        Error::Rpc {
            code: -32005,
            message: "x".into(),
        },
        Error::Rpc {
            code: -32007,
            message: "x".into(),
        },
        Error::Rpc {
            code: -32014,
            message: "x".into(),
        },
        Error::Rpc {
            code: -32015,
            message: "x".into(),
        },
        Error::Rpc {
            code: -32016,
            message: "x".into(),
        },
    ];
    for e in retryable {
        assert!(e.is_retryable(), "should be retryable: {e}");
    }

    let not_retryable: Vec<Error> = vec![
        Error::Auth("bad".into()),
        Error::Config("bad".into()),
        Error::Serialization("bad".into()),
        Error::Rpc {
            code: -32600,
            message: "x".into(),
        },
        Error::Rpc {
            code: -32602,
            message: "x".into(),
        },
        Error::Rpc {
            code: 0,
            message: "x".into(),
        },
        Error::Grpc {
            code: 16,
            message: "x".into(),
        },
    ];
    for e in not_retryable {
        assert!(!e.is_retryable(), "should not be retryable: {e}");
    }
}

#[test]
fn retry_after_only_set_on_rate_limited() {
    assert_eq!(
        Error::RateLimited {
            retry_after: Some(Duration::from_secs(30))
        }
        .retry_after(),
        Some(Duration::from_secs(30)),
    );
    assert_eq!(Error::RateLimited { retry_after: None }.retry_after(), None);
    assert_eq!(Error::Timeout.retry_after(), None);
    assert_eq!(
        Error::Rpc {
            code: -32005,
            message: "x".into()
        }
        .retry_after(),
        None
    );
}

#[test]
fn with_endpoint_prepends_host_on_wrapping_variants() {
    let cases = [
        Error::Rpc {
            code: -32404,
            message: "rejected".into(),
        },
        Error::Grpc {
            code: 16,
            message: "rejected".into(),
        },
        Error::Transport(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "rejected",
        ))),
    ];

    for e in cases {
        let e = e.with_endpoint("http://fra.rpc.orbitflare.com");
        let msg = format!("{e}");
        assert!(
            msg.contains("[http://fra.rpc.orbitflare.com]"),
            "missing endpoint: {msg}"
        );
        assert!(msg.contains("rejected"), "lost original message: {msg}");
    }
}

#[test]
fn with_endpoint_passes_through_rate_limited() {
    let e = Error::RateLimited {
        retry_after: Some(Duration::from_secs(10)),
    };
    let e = e.with_endpoint("http://fra.rpc.orbitflare.com");
    match e {
        Error::RateLimited { retry_after } => {
            assert_eq!(retry_after, Some(Duration::from_secs(10)));
        }
        other => panic!("expected RateLimited, got: {other}"),
    }
}

#[test]
fn with_endpoint_strips_api_key_from_host() {
    let e = Error::Rpc {
        code: 0,
        message: "fail".into(),
    };
    let e = e.with_endpoint("http://fra.rpc.orbitflare.com?api_key=ORBIT-SECRET-123");
    let msg = format!("{e}");
    assert!(!msg.contains("ORBIT-SECRET-123"), "leaked api_key: {msg}");
    assert!(msg.contains("fra.rpc.orbitflare.com"));
}

#[test]
fn retry_policy_defaults() {
    let p = RetryPolicy::default();
    assert_eq!(p.initial_delay, Duration::from_millis(100));
    assert_eq!(p.max_delay, Duration::from_secs(30));
    assert_eq!(p.multiplier, 2.0);
    assert_eq!(p.max_attempts, 0);
}

#[test]
fn retry_attempts_left() {
    let finite = RetryPolicy {
        max_attempts: 3,
        ..Default::default()
    };
    assert!(finite.has_attempts_left(1));
    assert!(finite.has_attempts_left(2));
    assert!(!finite.has_attempts_left(3));

    let infinite = RetryPolicy {
        max_attempts: 0,
        ..Default::default()
    };
    assert!(infinite.has_attempts_left(999));
    assert!(infinite.has_attempts_left(u32::MAX));
}

#[test]
fn retry_delay_grows_and_caps() {
    let p = RetryPolicy {
        initial_delay: Duration::from_millis(100),
        max_delay: Duration::from_millis(500),
        multiplier: 2.0,
        max_attempts: 0,
    };

    let d1 = p.delay_for_attempt(1);
    let d2 = p.delay_for_attempt(2);
    let d3 = p.delay_for_attempt(3);
    assert!(d2 > d1);
    assert!(d3 > d2);

    let dn = p.delay_for_attempt(20);
    assert!(dn.as_millis() <= 625, "should cap near max_delay: {dn:?}");
}
