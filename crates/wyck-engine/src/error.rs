//! The engine's error type.
//!
//! [`EngineError`] is what every fallible [`EngineHandle`](crate::EngineHandle) method
//! returns. It deliberately does not expose `ctrader_mcp::CTraderError` as-is: that type
//! describes the wire (schema mismatches, envelope shapes), while a front end needs to know
//! what to *do* (retry, show a message, ask the user to reconnect). [`EngineError::kind`]
//! and [`EngineError::is_retryable`] answer exactly that and stay stable as variants are
//! added.

use ctrader_mcp::CTraderError;

/// The crate-wide result alias.
pub type Result<T> = std::result::Result<T, EngineError>;

/// A coarse, stable classification of an [`EngineError`], for choosing how to present it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The engine configuration is invalid.
    Config,
    /// The engine is in the wrong state for the request (not connected, not armed, busy).
    State,
    /// The request itself is invalid (bad volume, missing stop loss, unknown symbol).
    Validation,
    /// The broker or the connection to it failed.
    Broker,
    /// A deadline passed before the broker answered.
    Timeout,
    /// The engine is shutting down.
    Shutdown,
    /// A bug or an unexpected condition inside the engine.
    Internal,
}

/// What kind of broker-side failure occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BrokerErrorKind {
    /// Could not reach the server, or the connection dropped.
    Connection,
    /// The server understood the request and refused it.
    Rejected,
    /// The server or the upstream broker is temporarily unable to answer.
    Unavailable,
    /// The server answered with something this engine cannot interpret.
    Protocol,
    /// Anything else.
    Unknown,
}

/// Everything that can go wrong in an engine call.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    /// The [`EngineConfig`](crate::EngineConfig) was rejected.
    #[error("invalid engine configuration: {0}")]
    Config(String),

    /// The request needs a connected session and there is none.
    #[error("not connected to a broker")]
    NotConnected,

    /// The session exists but is not `Ready` (still connecting, reconnecting, failed).
    #[error("the session is not ready ({state})")]
    NotReady {
        /// A short description of the current session state.
        state: String,
    },

    /// A real order was requested while the engine is in dry-run mode.
    #[error("trading is not armed: the engine is in dry-run mode")]
    NotArmed,

    /// Arming was refused.
    #[error("cannot arm trading: {0}")]
    ArmRefused(String),

    /// The connected session cannot trade (read-only data profile).
    #[error("this session cannot place orders: {0}")]
    TradingUnavailable(String),

    /// The request is not a valid order or command.
    #[error("invalid request: {0}")]
    Invalid(String),

    /// An order for the same symbol is already in flight, or one was submitted too
    /// recently.
    #[error("busy: {0}")]
    Busy(String),

    /// A confirmation token (flatten preview, arming) is unknown, used, or expired.
    #[error("confirmation rejected: {0}")]
    ConfirmationRejected(String),

    /// The broker or the connection failed.
    #[error("broker error ({kind:?}): {message}")]
    Broker {
        /// What kind of failure it was.
        kind: BrokerErrorKind,
        /// Whether trying the same read again later can plausibly succeed. Never a licence
        /// to retry a mutating call automatically.
        retryable: bool,
        /// A description for logs and messages. Never contains a secret.
        message: String,
    },

    /// The broker did not answer in time.
    #[error("timed out waiting for {operation}")]
    Timeout {
        /// What was being waited for.
        operation: &'static str,
    },

    /// The engine is shutting down and no longer accepts requests.
    #[error("the engine is shutting down")]
    ShuttingDown,

    /// A bug or an unexpected condition inside the engine.
    #[error("internal error: {0}")]
    Internal(String),
}

impl EngineError {
    /// The coarse classification. See [`ErrorKind`].
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Config(_) => ErrorKind::Config,
            Self::NotConnected
            | Self::NotReady { .. }
            | Self::NotArmed
            | Self::ArmRefused(_)
            | Self::TradingUnavailable(_)
            | Self::Busy(_)
            | Self::ConfirmationRejected(_) => ErrorKind::State,
            Self::Invalid(_) => ErrorKind::Validation,
            Self::Broker { .. } => ErrorKind::Broker,
            Self::Timeout { .. } => ErrorKind::Timeout,
            Self::ShuttingDown => ErrorKind::Shutdown,
            Self::Internal(_) => ErrorKind::Internal,
        }
    }

    /// Whether repeating the same *read* later can plausibly succeed.
    ///
    /// This says nothing about mutating calls: an order is never retried automatically,
    /// whatever this returns (see the crate docs, "Safety model").
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Broker { retryable, .. } => *retryable,
            Self::Timeout { .. } | Self::NotReady { .. } | Self::Busy(_) => true,
            _ => false,
        }
    }
}

impl From<CTraderError> for EngineError {
    fn from(error: CTraderError) -> Self {
        let (kind, retryable) = match &error {
            CTraderError::Connect { .. } | CTraderError::Transport { .. } => {
                (BrokerErrorKind::Connection, true)
            }
            CTraderError::Encode { .. }
            | CTraderError::Decode { .. }
            | CTraderError::EmptyResponse { .. }
            | CTraderError::SchemaMismatch { .. } => (BrokerErrorKind::Protocol, false),
            CTraderError::ServerRejection { .. } | CTraderError::PreFlightRejected { .. } => {
                (BrokerErrorKind::Rejected, false)
            }
            CTraderError::UpstreamBrokerError { .. }
            | CTraderError::LocalFault { .. }
            | CTraderError::ResourceUnavailable { .. } => (BrokerErrorKind::Unavailable, true),
            CTraderError::UnclassifiedToolError { .. }
            | CTraderError::Invariant(_)
            | CTraderError::Other(_) => (BrokerErrorKind::Unknown, false),
        };
        Self::Broker {
            kind,
            retryable,
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    fn tool() -> Cow<'static, str> {
        Cow::Borrowed("get_positions")
    }

    fn broker(kind: BrokerErrorKind, retryable: bool) -> EngineError {
        EngineError::Broker {
            kind,
            retryable,
            message: String::new(),
        }
    }

    fn map(error: CTraderError) -> (BrokerErrorKind, bool) {
        match EngineError::from(error) {
            EngineError::Broker {
                kind, retryable, ..
            } => (kind, retryable),
            other => panic!("expected a broker error, got {other:?}"),
        }
    }

    #[test]
    fn every_ctrader_error_variant_maps_to_a_classified_broker_error() {
        let json_err = || serde_json::from_str::<i32>("x").unwrap_err();
        let cases = [
            (
                CTraderError::Connect {
                    uri: "u".into(),
                    message: "m".into(),
                },
                (BrokerErrorKind::Connection, true),
            ),
            (
                CTraderError::Transport {
                    tool: tool(),
                    message: "m".into(),
                },
                (BrokerErrorKind::Connection, true),
            ),
            (
                CTraderError::Encode {
                    tool: tool(),
                    source: json_err(),
                },
                (BrokerErrorKind::Protocol, false),
            ),
            (
                CTraderError::Decode {
                    tool: tool(),
                    source: json_err(),
                },
                (BrokerErrorKind::Protocol, false),
            ),
            (
                CTraderError::EmptyResponse { tool: tool() },
                (BrokerErrorKind::Protocol, false),
            ),
            (
                CTraderError::SchemaMismatch {
                    tool: tool(),
                    message: "m".into(),
                },
                (BrokerErrorKind::Protocol, false),
            ),
            (
                CTraderError::ServerRejection {
                    tool: tool(),
                    code: None,
                    http_status: None,
                    message: "m".into(),
                },
                (BrokerErrorKind::Rejected, false),
            ),
            (
                CTraderError::UpstreamBrokerError {
                    tool: tool(),
                    code: "502".into(),
                    message: "m".into(),
                },
                (BrokerErrorKind::Unavailable, true),
            ),
            (
                CTraderError::LocalFault {
                    tool: tool(),
                    message: "m".into(),
                },
                (BrokerErrorKind::Unavailable, true),
            ),
            (
                CTraderError::ResourceUnavailable { tool: tool() },
                (BrokerErrorKind::Unavailable, true),
            ),
            (
                CTraderError::UnclassifiedToolError {
                    tool: tool(),
                    message: "m".into(),
                },
                (BrokerErrorKind::Unknown, false),
            ),
            (
                CTraderError::PreFlightRejected {
                    tool: tool(),
                    message: "m".into(),
                },
                (BrokerErrorKind::Rejected, false),
            ),
            (
                CTraderError::Invariant("m".into()),
                (BrokerErrorKind::Unknown, false),
            ),
            (
                CTraderError::Other("m".into()),
                (BrokerErrorKind::Unknown, false),
            ),
        ];
        for (error, expected) in cases {
            let label = error.to_string();
            assert_eq!(map(error), expected, "{label}");
        }
    }

    #[test]
    fn kinds_are_stable() {
        assert_eq!(EngineError::NotConnected.kind(), ErrorKind::State);
        assert_eq!(EngineError::NotArmed.kind(), ErrorKind::State);
        assert_eq!(
            EngineError::Invalid("x".into()).kind(),
            ErrorKind::Validation
        );
        assert_eq!(
            EngineError::Timeout { operation: "x" }.kind(),
            ErrorKind::Timeout
        );
        assert_eq!(EngineError::ShuttingDown.kind(), ErrorKind::Shutdown);
        assert_eq!(
            broker(BrokerErrorKind::Rejected, false).kind(),
            ErrorKind::Broker
        );
    }

    #[test]
    fn retryability() {
        assert!(broker(BrokerErrorKind::Connection, true).is_retryable());
        assert!(!broker(BrokerErrorKind::Rejected, false).is_retryable());
        assert!(EngineError::Timeout { operation: "x" }.is_retryable());
        assert!(!EngineError::NotArmed.is_retryable());
        assert!(!EngineError::Invalid("x".into()).is_retryable());
    }

    #[test]
    fn messages_carry_the_tool_but_no_secret_shape() {
        let text = EngineError::from(CTraderError::Transport {
            tool: tool(),
            message: "connection reset".into(),
        })
        .to_string();
        assert!(text.contains("get_positions") && text.contains("connection reset"));
    }
}
