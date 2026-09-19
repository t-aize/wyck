//! The calendar's domain model.

use std::fmt;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::currency::Currency;
use crate::reading::Reading;

/// How market-moving the source rates an event.
///
/// Ordered by significance (`Unknown < Holiday < Low < Medium < High`) so
/// `impact >= Impact::Medium` reads the way it sounds. Only `Low`, `Medium` and `High`
/// were observed in the live feed; [`Impact::Holiday`] is the value the source's own
/// site uses for bank holidays and is handled defensively, and any other string maps to
/// [`Impact::Unknown`] instead of failing the record, so a new tier never blanks the
/// calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Impact {
    /// A value this crate does not recognize.
    Unknown,
    /// A non-trading day (bank holiday); no data release.
    Holiday,
    /// Minor release.
    Low,
    /// Moderately market-moving.
    Medium,
    /// Highly market-moving (rate decisions, CPI, payrolls, …).
    High,
}

impl Impact {
    /// Maps the feed's `impact` string (case-insensitive) to an [`Impact`].
    #[must_use]
    pub fn from_feed(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "holiday" | "non-economic" => Self::Holiday,
            _ => Self::Unknown,
        }
    }
}

impl fmt::Display for Impact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unknown => "Unknown",
            Self::Holiday => "Holiday",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        })
    }
}

/// What an event applies to.
///
/// The feed's `country` field is normally a currency code but is the literal `"All"` for
/// events that are not tied to one currency (`BRICS Summit`, `ECOFIN Meetings` …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scope {
    /// Not tied to a single currency.
    Global,
    /// Affects one currency.
    Currency(Currency),
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Global => f.write_str("All"),
            Self::Currency(c) => c.fmt(f),
        }
    }
}

/// A stable identity for an event: the same release keeps the same id across refreshes,
/// so a UI can remember "already warned about this one". Built from the instant, scope
/// and title (the feed has no id field).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventId {
    /// Unix timestamp of the event, in seconds.
    pub unix_seconds: i64,
    /// What the event applies to.
    pub scope: Scope,
    /// The event's title.
    pub title: String,
}

/// One scheduled economic release or speech.
#[derive(Debug, Clone, PartialEq)]
pub struct CalendarEvent {
    /// Human-readable name, e.g. `"CPI m/m"`.
    pub title: String,
    /// Which currency (or none) the event applies to.
    pub scope: Scope,
    /// When it happens. Carries the feed's own UTC offset (US Eastern, DST-aware);
    /// compare instants freely, and call [`OffsetDateTime::to_offset`] to display in
    /// another zone.
    pub time: OffsetDateTime,
    /// The source's significance rating.
    pub impact: Impact,
    /// The consensus forecast as published, or `None` when the feed leaves it blank.
    pub forecast: Option<String>,
    /// The previous release's value as published, or `None` when blank.
    pub previous: Option<String>,
}

impl CalendarEvent {
    /// The event's stable identity. See [`EventId`].
    #[must_use]
    pub fn id(&self) -> EventId {
        EventId {
            unix_seconds: self.time.unix_timestamp(),
            scope: self.scope,
            title: self.title.clone(),
        }
    }

    /// [`Reading::parse`] of the forecast, if there is one.
    #[must_use]
    pub fn forecast_reading(&self) -> Option<Reading> {
        self.forecast.as_deref().map(Reading::parse)
    }

    /// [`Reading::parse`] of the previous value, if there is one.
    #[must_use]
    pub fn previous_reading(&self) -> Option<Reading> {
        self.previous.as_deref().map(Reading::parse)
    }
}

/// Events whose time lies in `[from, to)`. `events` must be sorted by time — the order
/// [`crate::parse_feed`] returns and [`crate::CalendarState::events`] holds.
#[must_use]
pub fn between(
    events: &[CalendarEvent],
    from: OffsetDateTime,
    to: OffsetDateTime,
) -> &[CalendarEvent] {
    let start = events.partition_point(|e| e.time < from);
    let end = events.partition_point(|e| e.time < to);
    &events[start..end.max(start)]
}

/// The next `limit` events at or after `now`. `events` must be sorted by time.
#[must_use]
pub fn upcoming(events: &[CalendarEvent], now: OffsetDateTime, limit: usize) -> &[CalendarEvent] {
    let start = events.partition_point(|e| e.time < now);
    &events[start..(start + limit).min(events.len())]
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    pub(crate) fn event(
        title: &str,
        scope: Scope,
        time: OffsetDateTime,
        impact: Impact,
    ) -> CalendarEvent {
        CalendarEvent {
            title: title.to_owned(),
            scope,
            time,
            impact,
            forecast: None,
            previous: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::testing::event;
    use super::*;

    #[test]
    fn impact_orders_by_significance() {
        assert!(Impact::High > Impact::Medium);
        assert!(Impact::Medium > Impact::Low);
        assert!(Impact::Low > Impact::Holiday);
        assert!(Impact::Holiday > Impact::Unknown);
    }

    #[test]
    fn impact_parses_every_known_value_and_tolerates_new_ones() {
        assert_eq!(Impact::from_feed("High"), Impact::High);
        assert_eq!(Impact::from_feed("medium"), Impact::Medium);
        assert_eq!(Impact::from_feed(" LOW "), Impact::Low);
        assert_eq!(Impact::from_feed("Holiday"), Impact::Holiday);
        assert_eq!(Impact::from_feed("Extreme"), Impact::Unknown);
        assert_eq!(Impact::from_feed(""), Impact::Unknown);
    }

    #[test]
    fn id_is_stable_across_offsets_of_the_same_instant() {
        let a = event(
            "CPI m/m",
            Scope::Currency(Currency::CAD),
            datetime!(2026-09-14 08:30 -4),
            Impact::High,
        );
        let mut b = a.clone();
        b.time = datetime!(2026-09-14 12:30 UTC);
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn slicing_helpers_respect_bounds() {
        let t = |h: u8| datetime!(2026-09-14 00:00 UTC).replace_hour(h).unwrap();
        let events: Vec<_> = [1, 3, 3, 5, 9]
            .into_iter()
            .map(|h| event("e", Scope::Global, t(h), Impact::Low))
            .collect();

        assert_eq!(between(&events, t(3), t(9)).len(), 3);
        assert_eq!(between(&events, t(0), t(1)).len(), 0);
        assert_eq!(
            between(&events, t(9), t(3)).len(),
            0,
            "inverted range is empty"
        );
        assert_eq!(upcoming(&events, t(2), 2).len(), 2);
        assert_eq!(upcoming(&events, t(10), 5).len(), 0);
        assert_eq!(upcoming(&events, t(0), 99).len(), 5);
    }
}
