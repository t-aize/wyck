//! The crate's error type.

use std::time::Duration;

/// The crate-wide result alias.
pub type Result<T> = std::result::Result<T, CalendarError>;

/// Everything that can go wrong fetching or decoding the calendar feed.
///
/// None of these are fatal to a caller: [`crate::CalendarService`] logs them, records
/// them in [`crate::CalendarState::last_error`], and keeps serving the last good data.
#[derive(Debug, thiserror::Error)]
pub enum CalendarError {
    /// The HTTP request never produced a response (DNS, TLS, connect, timeout, or a
    /// body read that broke mid-stream).
    #[error("calendar request failed: {0}")]
    Transport(#[source] reqwest::Error),

    /// The feed answered with a non-success status other than `304`/`429`.
    #[error("calendar feed returned HTTP {status}")]
    Status {
        /// The HTTP status code.
        status: u16,
    },

    /// The feed answered `429 Too Many Requests`. The public feed throttles clients
    /// that poll it aggressively, so this is an expected condition, not a bug: the
    /// service backs off (honoring `retry_after` when the server supplied one).
    #[error("calendar feed rate-limited the request (retry after {retry_after:?})")]
    RateLimited {
        /// The server's `Retry-After` hint, if it sent a delta-seconds value.
        retry_after: Option<Duration>,
    },

    /// The response body was larger than [`crate::ClientConfig::max_body_bytes`]. A real
    /// week of events is ~15 KB; anything near the cap means the URL is not the feed.
    #[error("calendar response exceeded the {limit}-byte limit")]
    TooLarge {
        /// The configured limit, in bytes.
        limit: usize,
    },

    /// A success status, but the body is an HTML page rather than JSON: the shape of the
    /// feed's "Request Denied: you've exceeded the limit for Calendar Export requests"
    /// block page, or of a CDN/maintenance page. Treated like a rate limit (transient,
    /// long hold) rather than as schema drift.
    #[error("calendar feed returned an HTML page instead of JSON (blocked or maintenance page)")]
    HtmlResponse,

    /// The body was not a JSON array of objects.
    #[error("calendar feed is not a JSON array of events: {0}")]
    Decode(#[source] serde_json::Error),

    /// The body was a JSON array with records in it, but **none** could be interpreted
    /// as an event. Almost certainly schema drift; reported as an error (rather than an
    /// empty calendar) so a stale-but-correct cache is never replaced by nothing.
    #[error("calendar feed contained {skipped} record(s) but none were valid events")]
    NoValidEvents {
        /// How many records were rejected.
        skipped: usize,
    },

    /// The [`crate::ClientConfig`] was rejected before any request was made.
    #[error("invalid calendar configuration: {0}")]
    Config(String),
}

impl CalendarError {
    /// Whether retrying the same request later can plausibly succeed. `false` for
    /// errors that point at a misconfiguration or a permanently different feed shape.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Transport(_) | Self::RateLimited { .. } | Self::HtmlResponse => true,
            Self::Status { status } => *status >= 500 || *status == 408,
            Self::TooLarge { .. }
            | Self::Decode(_)
            | Self::NoValidEvents { .. }
            | Self::Config(_) => false,
        }
    }
}
