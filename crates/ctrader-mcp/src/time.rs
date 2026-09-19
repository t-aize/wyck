//! Timestamp helpers.
//!
//! The two servers disagree on timestamp encoding (`references/local-http-server.md`
//! "Time encoding on Local", `references/remote-http-server.md` "Time encoding on
//! Remote"):
//!
//! - **Local** requires ISO 8601 with a **mandatory `Z` suffix** on every time-typed
//!   input (`from`/`to` on `get_trendbars`, drawing-object time anchors, `expiresAt`).
//!   A string without `Z` is silently coerced to the *client's* local time zone by the
//!   underlying desktop client (`Q-L8`), producing a range offset by the local UTC
//!   offset. [`to_local_iso8601_z`] guarantees the suffix is present.
//! - **Remote** accepts either epoch milliseconds or an ISO 8601 string on history
//!   window endpoints, but requires **integer epoch milliseconds only** on
//!   `expirationTimestamp` (`Q-R2`): an ISO string there is rejected by Zod validation.
//!   [`RemoteTimestamp`] models this distinction in the type system so a caller cannot
//!   accidentally send an ISO string where only an integer is accepted.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Returns the current wall-clock time as epoch milliseconds (UTC).
///
/// Used by workflow W0 (session bootstrap) to compute the offset between the agent's
/// local clock and the server's `get_server_time` response, so subsequent time-window
/// requests are anchored to the server's clock rather than a potentially-skewed local
/// clock.
pub fn now_epoch_millis() -> i64 {
    (OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}

/// Converts epoch milliseconds to an ISO 8601 / RFC 3339 string with a guaranteed
/// trailing `Z` (never a numeric `+00:00` offset), for use on every Local time-typed
/// input field. See `Q-L8`.
pub fn epoch_millis_to_local_iso8601_z(epoch_millis: i64) -> Result<String, TimeError> {
    let dt = OffsetDateTime::from_unix_timestamp_nanos(i128::from(epoch_millis) * 1_000_000)
        .map_err(|_| TimeError::OutOfRange(epoch_millis))?;
    to_local_iso8601_z(dt)
}

/// Formats an [`OffsetDateTime`] as ISO 8601 with a guaranteed trailing `Z`.
///
/// `time`'s [`Rfc3339`] well-known formatter renders a zero UTC offset as `+00:00`, not
/// `Z`; since Local's parser has been observed to coerce a non-`Z`-suffixed string to
/// local time regardless of an explicit `+00:00` offset (`Q-L8`), this function
/// normalizes the tail explicitly rather than relying on the formatter's own choice.
pub fn to_local_iso8601_z(dt: OffsetDateTime) -> Result<String, TimeError> {
    let dt = dt.to_offset(time::UtcOffset::UTC);
    let formatted = dt.format(&Rfc3339).map_err(TimeError::Format)?;
    Ok(match formatted.strip_suffix("+00:00") {
        Some(head) => format!("{head}Z"),
        None => formatted,
    })
}

/// Parses a Local-style ISO 8601 timestamp (with or without an explicit `Z`/offset)
/// back into epoch milliseconds. Accepts both `Z`-suffixed and offset-suffixed forms so
/// round-tripping a Local response (which always includes an explicit offset) works
/// without a second code path.
pub fn local_iso8601_to_epoch_millis(iso: &str) -> Result<i64, TimeError> {
    let dt = OffsetDateTime::parse(iso, &Rfc3339).map_err(TimeError::Parse)?;
    Ok((dt.unix_timestamp_nanos() / 1_000_000) as i64)
}

/// A Remote-server timestamp value.
///
/// `references/remote-http-server.md` "Time encoding on Remote" documents that
/// `fromTimestamp`/`toTimestamp` on history-window endpoints accept **either** form,
/// while `expirationTimestamp` on `create_order`/`amend_order` accepts **only** the
/// integer-epoch-milliseconds form (`Q-R2`: an ISO string there is rejected by Zod
/// validation). Modeling both shapes as one `#[serde(untagged)]` enum lets a single DTO
/// field type serialize correctly for whichever endpoint uses it, while
/// [`RemoteTimestamp::epoch_millis`] is the only constructor exposed on
/// `expirationTimestamp`-shaped fields in this crate's request DTOs, so the Q-R2 mistake
/// is structurally unrepresentable at those call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RemoteTimestamp {
    /// Epoch milliseconds (UTC). The only form accepted on `expirationTimestamp`.
    EpochMillis(i64),
}

impl RemoteTimestamp {
    /// Builds a Remote timestamp from epoch milliseconds. This is the required form for
    /// `expirationTimestamp` (`Q-R2`) and is always accepted on history-window
    /// endpoints too.
    pub fn epoch_millis(epoch_millis: i64) -> Self {
        Self::EpochMillis(epoch_millis)
    }

    /// Returns the underlying epoch-millisecond value.
    pub fn as_epoch_millis(self) -> i64 {
        match self {
            Self::EpochMillis(ms) => ms,
        }
    }
}

/// Errors from timestamp formatting/parsing.
#[derive(Debug, thiserror::Error)]
pub enum TimeError {
    #[error("epoch milliseconds value {0} is out of range for a valid timestamp")]
    OutOfRange(i64),
    #[error("failed to format timestamp: {0}")]
    Format(time::error::Format),
    #[error("failed to parse timestamp: {0}")]
    Parse(time::error::Parse),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_millis_round_trips_through_local_iso8601_z() {
        let original = 1_778_000_000_123; // arbitrary, non-round millisecond value
        let iso = epoch_millis_to_local_iso8601_z(original).unwrap();
        assert!(iso.ends_with('Z'), "expected a Z suffix, got: {iso}");
        assert!(!iso.contains("+00:00"));
        let round_tripped = local_iso8601_to_epoch_millis(&iso).unwrap();
        assert_eq!(round_tripped, original);
    }

    #[test]
    fn known_instant_formats_with_z_suffix() {
        let dt = time::macros::datetime!(2026-05-14 12:00:00 UTC);
        let epoch_millis = dt.unix_timestamp() * 1000;
        let iso = epoch_millis_to_local_iso8601_z(epoch_millis).unwrap();
        assert_eq!(iso, "2026-05-14T12:00:00Z");
    }
}
