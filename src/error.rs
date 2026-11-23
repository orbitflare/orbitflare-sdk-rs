use std::fmt;
use std::time::Duration;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}", chain(.0.as_ref()))]
    Transport(Box<dyn std::error::Error + Send + Sync>),

    #[error("rpc error {code}: {message}")]
    Rpc { code: i64, message: String },

    #[error("grpc error ({code}): {message}")]
    Grpc { code: i32, message: String },

    #[error("rate limited{}", match .retry_after {
        Some(d) => format!(" (retry after {}s)", d.as_secs()),
        None => String::new(),
    })]
    RateLimited { retry_after: Option<Duration> },

    #[error("auth: {0}")]
    Auth(String),

    #[error("request timed out")]
    Timeout,

    #[error("stream closed: {0}")]
    Stream(String),

    #[error("{0}")]
    Config(String),

    #[error("{0}")]
    Serialization(String),
}

impl Error {
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Transport(_) | Error::Timeout | Error::RateLimited { .. } => true,
            Error::Rpc { code, .. } => matches!(code, -32005 | -32007 | -32014 | -32015 | -32016),
            Error::Stream(_) => true,
            _ => false,
        }
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Error::RateLimited { retry_after } => *retry_after,
            _ => None,
        }
    }

    pub(crate) fn transport(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Error::Transport(Box::new(err))
    }

    pub fn with_endpoint(self, endpoint: &str) -> Self {
        let endpoint = sanitize_url(endpoint);
        match self {
            Error::Transport(e) => {
                Error::Transport(format!("[{endpoint}] {}", chain(e.as_ref())).into())
            }
            Error::Rpc { code, message } => Error::Rpc {
                code,
                message: format!("[{endpoint}] {message}"),
            },
            Error::Grpc { code, message } => Error::Grpc {
                code,
                message: format!("[{endpoint}] {message}"),
            },
            Error::RateLimited { retry_after } => Error::RateLimited { retry_after },
            other => other,
        }
    }
}

#[cfg(feature = "rpc")]
impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Error::Timeout
        } else {
            Error::Transport(Box::new(e))
        }
    }
}

#[cfg(feature = "rpc")]
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialization(e.to_string())
    }
}

#[cfg(any(feature = "grpc", feature = "jetstream"))]
impl From<tonic::Status> for Error {
    fn from(s: tonic::Status) -> Self {
        Error::Grpc {
            code: s.code() as i32,
            message: s.message().to_string(),
        }
    }
}

fn chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut out = err.to_string();
    let mut source = err.source();
    while let Some(inner) = source {
        let text = inner.to_string();
        if !out.ends_with(&text) {
            out.push_str(": ");
            out.push_str(&text);
        }
        source = inner.source();
    }
    out
}

pub(crate) fn sanitize_url(s: &str) -> String {
    let mut result = s.to_string();
    for needle in ["?api_key=", "&api_key="] {
        while let Some(start) = result.find(needle) {
            let value_start = start + needle.len();
            let end = result[value_start..]
                .find(|c: char| c == '&' || c == ')' || c.is_whitespace())
                .map(|i| i + value_start)
                .unwrap_or(result.len());
            result = format!("{}{}", &result[..start], &result[end..]);
        }
    }
    result
}

pub(crate) struct SanitizedUrl<'a>(pub &'a str);

impl fmt::Display for SanitizedUrl<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&sanitize_url(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Outer(std::io::Error);

    impl fmt::Display for Outer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("transport error")
        }
    }

    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn transport_errors_show_their_cause() {
        let err = Error::Transport(Box::new(Outer(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "connection refused",
        ))));
        assert_eq!(err.to_string(), "transport error: connection refused");
    }
}
