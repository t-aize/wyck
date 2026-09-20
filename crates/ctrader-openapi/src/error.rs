//! The errors of this crate, and how to react to each.
//!
//! Everything that can go wrong is an [`OpenApiError`]. Most callers do not need the variants:
//! [`OpenApiError::kind`] sorts them into a short list of [`ErrorKind`]s, and
//! [`OpenApiError::is_retryable`] says whether trying again later can work. The error codes are
//! the strings the server sends in `ProtoOAErrorRes.errorCode`, taken from the official
//! `ProtoOAErrorCode` list.

use std::time::Duration;

/// A specialized `Result` for this crate.
pub type Result<T> = std::result::Result<T, OpenApiError>;

/// What went wrong, in a few words a caller can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The request rate was exceeded. Wait and try again.
    RateLimited,
    /// The server is under maintenance. Try again after the announced end.
    Maintenance,
    /// The access token is expired or was invalidated: refresh it, or sign in again.
    TokenInvalid,
    /// The application or the account is not (or no longer) authorized on this connection.
    NotAuthorized,
    /// The server understood the request and refused it (a bad symbol, a bad range, ...).
    Rejected,
    /// The connection could not be made or dropped.
    Transport,
    /// No answer came in time.
    Timeout,
    /// The server sent something this client cannot read, or the sign in exchange failed.
    Protocol,
    /// The client was already closed.
    Closed,
    /// The configuration is unusable.
    Config,
}

/// Every failure of the Open API client.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OpenApiError {
    /// The configuration cannot be used (an address that is not a WebSocket URL, and so on).
    #[error("invalid configuration: {0}")]
    Config(String),

    /// The connection failed: the address, TLS, or the WebSocket handshake.
    #[error("connection failed: {0}")]
    Transport(String),

    /// The connection ended, or the client was closed, before the answer came.
    #[error("the connection is closed")]
    Closed,

    /// No answer arrived within the request timeout.
    #[error("timed out waiting for {operation}")]
    Timeout {
        /// What was being waited for.
        operation: &'static str,
    },

    /// The server answered with an error (`ProtoOAErrorRes`).
    #[error("the server refused the request: {code}{}", description.as_deref().map(|d| format!(" ({d})")).unwrap_or_default())]
    Server {
        /// The `errorCode` string, for example `REQUEST_FREQUENCY_EXCEEDED`.
        code: String,
        /// The server's explanation, when it gives one.
        description: Option<String>,
        /// How long to wait before trying again, when the server says (it sends seconds). With
        /// `BLOCKED_PAYLOAD_TYPE` it is the time until that type of request is unblocked.
        retry_after: Option<Duration>,
        /// When maintenance ends, as a Unix time in seconds, when the server says.
        maintenance_end: Option<i64>,
    },

    /// A message could not be read, or was not the one expected.
    #[error("unexpected message: {0}")]
    Protocol(String),

    /// The OAuth exchange (token endpoint or callback) failed. The text never holds a secret.
    #[error("sign in failed: {0}")]
    Auth(String),
}

impl OpenApiError {
    /// A server error from the fields of `ProtoOAErrorRes`. `retry_after_secs` is in seconds and
    /// `maintenance_end` is a Unix time in seconds, both as the server sends them.
    #[must_use]
    pub fn server(
        code: impl Into<String>,
        description: Option<String>,
        retry_after_secs: Option<i64>,
        maintenance_end: Option<i64>,
    ) -> Self {
        Self::Server {
            code: code.into(),
            description,
            retry_after: retry_after_secs
                .and_then(|s| u64::try_from(s).ok())
                .map(Duration::from_secs),
            maintenance_end,
        }
    }

    /// The error code the server sent, if this is a server error.
    #[must_use]
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Server { code, .. } => Some(code),
            _ => None,
        }
    }

    /// Sorts the error into an [`ErrorKind`].
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Config(_) => ErrorKind::Config,
            Self::Transport(_) => ErrorKind::Transport,
            Self::Closed => ErrorKind::Closed,
            Self::Timeout { .. } => ErrorKind::Timeout,
            Self::Protocol(_) | Self::Auth(_) => ErrorKind::Protocol,
            Self::Server { code, .. } => match code.as_str() {
                // `BLOCKED_PAYLOAD_TYPE` is the server blocking one type of request for a while after
                // too many, with `retryAfter` seconds until it is unblocked.
                "REQUEST_FREQUENCY_EXCEEDED" | "BLOCKED_PAYLOAD_TYPE" => ErrorKind::RateLimited,
                "SERVER_IS_UNDER_MAINTENANCE" => ErrorKind::Maintenance,
                "OA_AUTH_TOKEN_EXPIRED" | "CH_ACCESS_TOKEN_INVALID" => ErrorKind::TokenInvalid,
                "ACCOUNT_NOT_AUTHORIZED"
                | "CH_CLIENT_AUTH_FAILURE"
                | "CH_CLIENT_NOT_AUTHENTICATED"
                | "CH_OA_CLIENT_NOT_FOUND"
                | "CH_CTID_TRADER_ACCOUNT_NOT_FOUND" => ErrorKind::NotAuthorized,
                _ => ErrorKind::Rejected,
            },
        }
    }

    /// Whether the same request may work if tried again later, unchanged.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::RateLimited
                | ErrorKind::Maintenance
                | ErrorKind::Transport
                | ErrorKind::Timeout
        )
    }

    /// How long to wait before a retry: the server's own advice when there is one.
    #[must_use]
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Server { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(code: &str) -> OpenApiError {
        OpenApiError::server(code, None, None, None)
    }

    #[test]
    fn server_codes_are_sorted_into_kinds() {
        assert_eq!(
            server("REQUEST_FREQUENCY_EXCEEDED").kind(),
            ErrorKind::RateLimited
        );
        assert_eq!(
            server("SERVER_IS_UNDER_MAINTENANCE").kind(),
            ErrorKind::Maintenance
        );
        assert_eq!(
            server("OA_AUTH_TOKEN_EXPIRED").kind(),
            ErrorKind::TokenInvalid
        );
        assert_eq!(
            server("CH_ACCESS_TOKEN_INVALID").kind(),
            ErrorKind::TokenInvalid
        );
        assert_eq!(
            server("ACCOUNT_NOT_AUTHORIZED").kind(),
            ErrorKind::NotAuthorized
        );
        assert_eq!(server("SYMBOL_NOT_FOUND").kind(), ErrorKind::Rejected);
        assert_eq!(server("SOMETHING_NEW").kind(), ErrorKind::Rejected);
    }

    #[test]
    fn only_transient_failures_are_retryable() {
        assert!(server("REQUEST_FREQUENCY_EXCEEDED").is_retryable());
        assert!(server("SERVER_IS_UNDER_MAINTENANCE").is_retryable());
        assert!(OpenApiError::Timeout { operation: "x" }.is_retryable());
        assert!(OpenApiError::Transport("x".into()).is_retryable());
        assert!(!server("SYMBOL_NOT_FOUND").is_retryable());
        assert!(!server("CH_ACCESS_TOKEN_INVALID").is_retryable());
        assert!(!OpenApiError::Closed.is_retryable());
        assert!(!OpenApiError::Protocol("x".into()).is_retryable());
    }

    #[test]
    fn the_server_advice_on_waiting_is_kept() {
        let e = OpenApiError::server("BLOCKED_PAYLOAD_TYPE", None, Some(2), None);
        assert_eq!(e.retry_after(), Some(Duration::from_secs(2)));
        assert_eq!(e.kind(), ErrorKind::RateLimited);
        assert_eq!(e.code(), Some("BLOCKED_PAYLOAD_TYPE"));
        assert_eq!(OpenApiError::Closed.retry_after(), None);
        let negative = OpenApiError::server("X", None, Some(-5), None);
        assert_eq!(negative.retry_after(), None, "a nonsense value is dropped");
    }

    #[test]
    fn the_message_names_the_code_and_the_description() {
        let e = OpenApiError::server(
            "SYMBOL_NOT_FOUND",
            Some("no such symbol".into()),
            None,
            None,
        );
        let text = e.to_string();
        assert!(text.contains("SYMBOL_NOT_FOUND") && text.contains("no such symbol"));
        assert!(server("A").to_string().ends_with('A'));
    }
}
