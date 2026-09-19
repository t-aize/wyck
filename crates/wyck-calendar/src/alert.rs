//! Non-blocking "a big release is imminent" warnings.
//!
//! Same philosophy as wyck's prop-firm guardrails: **warn, never block**. This module
//! only *reports* which relevant events are close; it has no way to veto an order, and
//! nothing in the crate calls it implicitly. A UI polls [`imminent_events`] on its own
//! tick (it is a cheap pure function over an already-sorted slice) and decides how loud
//! to be.
//!
//! `now` is a parameter, not read from the system clock, deliberately: cTrader's docs
//! recommend anchoring any time-window computation to the broker's `get_server_time`
//! rather than the local clock, and the caller is the one who knows the offset. It also
//! makes the logic deterministic to test.

use time::{Duration, OffsetDateTime};

use crate::event::{CalendarEvent, Impact};
use crate::filter::EventFilter;

/// When and for what to warn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertPolicy {
    /// How far ahead of an event to start warning. Default 30 minutes.
    pub lead_time: Duration,
    /// How long after its scheduled time an event still counts as "underway" — the
    /// volatile window right after a release. Default 15 minutes.
    pub grace: Duration,
    /// Which events are relevant. Default: [`Impact::High`] and above, any currency.
    /// Point [`EventFilter::currencies`] at the currencies of the symbols in play (see
    /// [`crate::currencies_from_symbols`]) so a JPY release doesn't warn an EURUSD
    /// trader; the watchlist can pull in specific events regardless of impact.
    pub filter: EventFilter,
}

impl Default for AlertPolicy {
    fn default() -> Self {
        Self {
            lead_time: Duration::minutes(30),
            grace: Duration::minutes(15),
            filter: EventFilter::new().with_min_impact(Impact::High),
        }
    }
}

/// Where an event is relative to `now`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    /// Not yet released; happens in `starts_in` (always positive).
    Upcoming {
        /// Time until the scheduled instant.
        starts_in: Duration,
    },
    /// Scheduled instant has passed within the policy's grace window.
    Underway {
        /// Time since the scheduled instant (zero or positive).
        since: Duration,
    },
}

/// One event the policy considers worth a heads-up.
#[derive(Debug, Clone, PartialEq)]
pub struct EventWarning {
    /// The event.
    pub event: CalendarEvent,
    /// Where it is relative to `now`.
    pub timing: Timing,
}

/// The relevant events that are imminent or just released at `now`, soonest first.
///
/// An event is reported when `now < time <= now + lead_time` ([`Timing::Upcoming`]) or
/// `time <= now < time + grace` ([`Timing::Underway`]) *and* it passes the policy's
/// filter. `events` must be sorted by time (as [`crate::parse_feed`] returns them).
///
/// ```
/// use time::macros::datetime;
/// use wyck_calendar::{AlertPolicy, Timing, imminent_events, parse_feed};
///
/// let feed = parse_feed(br#"[{"title":"FOMC Statement","country":"USD",
///     "date":"2026-09-16T14:00:00-04:00","impact":"High"}]"#)?;
/// let now = datetime!(2026-09-16 13:45 -4);
///
/// let warnings = imminent_events(&feed.events, now, &AlertPolicy::default());
/// assert_eq!(warnings.len(), 1);
/// assert!(matches!(warnings[0].timing, Timing::Upcoming { .. }));
/// # Ok::<(), wyck_calendar::CalendarError>(())
/// ```
#[must_use]
pub fn imminent_events(
    events: &[CalendarEvent],
    now: OffsetDateTime,
    policy: &AlertPolicy,
) -> Vec<EventWarning> {
    let from = now - policy.grace;
    let start = events.partition_point(|e| e.time <= from);
    // `time <= now + lead` (inclusive), hence the partition on `<=`.
    let end = events.partition_point(|e| e.time <= now + policy.lead_time);

    events[start..end.max(start)]
        .iter()
        .filter(|e| policy.filter.matches(e))
        .map(|e| EventWarning {
            timing: if e.time > now {
                Timing::Upcoming {
                    starts_in: e.time - now,
                }
            } else {
                Timing::Underway {
                    since: now - e.time,
                }
            },
            event: e.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;
    use crate::currency::Currency;
    use crate::event::Scope;
    use crate::event::testing::event;

    fn at(minute_offset: i64) -> OffsetDateTime {
        datetime!(2026-09-16 14:00 -4) + Duration::minutes(minute_offset)
    }

    fn high(title: &str, when: OffsetDateTime) -> CalendarEvent {
        event(title, Scope::Currency(Currency::USD), when, Impact::High)
    }

    fn titles(w: &[EventWarning]) -> Vec<&str> {
        w.iter().map(|w| w.event.title.as_str()).collect()
    }

    #[test]
    fn warns_inside_the_lead_window_only() {
        let events = [
            high("early", at(31)),
            high("edge", at(30)),
            high("soon", at(10)),
        ];
        let mut sorted = events.to_vec();
        sorted.sort_by_key(|e| e.time);
        let w = imminent_events(&sorted, at(0), &AlertPolicy::default());
        assert_eq!(
            titles(&w),
            ["soon", "edge"],
            "31 min out is outside, 30 is inclusive"
        );
    }

    #[test]
    fn reports_timing_relative_to_now() {
        let events = [high("later", at(20)), high("just-out", at(-5))];
        let mut sorted = events.to_vec();
        sorted.sort_by_key(|e| e.time);
        let w = imminent_events(&sorted, at(0), &AlertPolicy::default());
        assert_eq!(
            w[0].timing,
            Timing::Underway {
                since: Duration::minutes(5)
            }
        );
        assert_eq!(
            w[1].timing,
            Timing::Upcoming {
                starts_in: Duration::minutes(20)
            }
        );
    }

    #[test]
    fn an_event_exactly_now_is_underway_with_zero_elapsed() {
        let w = imminent_events(&[high("now", at(0))], at(0), &AlertPolicy::default());
        assert_eq!(
            w[0].timing,
            Timing::Underway {
                since: Duration::ZERO
            }
        );
    }

    #[test]
    fn stops_warning_once_the_grace_window_has_passed() {
        let events = [high("old", at(-15)), high("fresh", at(-14))];
        let mut sorted = events.to_vec();
        sorted.sort_by_key(|e| e.time);
        let w = imminent_events(&sorted, at(0), &AlertPolicy::default());
        assert_eq!(
            titles(&w),
            ["fresh"],
            "grace window is exclusive at its far edge"
        );
    }

    #[test]
    fn respects_the_filter() {
        let policy = AlertPolicy {
            filter: EventFilter::new()
                .with_min_impact(Impact::High)
                .with_currencies([Currency::EUR]),
            ..AlertPolicy::default()
        };
        let events = [
            high("usd-high", at(5)),
            event(
                "eur-medium",
                Scope::Currency(Currency::EUR),
                at(6),
                Impact::Medium,
            ),
            event(
                "eur-high",
                Scope::Currency(Currency::EUR),
                at(7),
                Impact::High,
            ),
        ];
        assert_eq!(
            titles(&imminent_events(&events, at(0), &policy)),
            ["eur-high"]
        );
    }

    #[test]
    fn watchlisted_events_warn_regardless_of_impact() {
        let policy = AlertPolicy {
            filter: EventFilter::new()
                .with_min_impact(Impact::High)
                .watch("speaks"),
            ..AlertPolicy::default()
        };
        let events = [event("Lagarde Speaks", Scope::Global, at(5), Impact::Low)];
        assert_eq!(imminent_events(&events, at(0), &policy).len(), 1);
    }

    #[test]
    fn clustered_releases_all_warn_in_time_order() {
        let events = [
            high("Federal Funds Rate", at(0)),
            high("FOMC Statement", at(0)),
            high("Press Conference", at(20)),
        ];
        let w = imminent_events(&events, at(-10), &AlertPolicy::default());
        assert_eq!(w.len(), 3);
        assert!(w.windows(2).all(|p| p[0].event.time <= p[1].event.time));
    }

    #[test]
    fn empty_and_far_away_calendars_yield_nothing() {
        assert!(imminent_events(&[], at(0), &AlertPolicy::default()).is_empty());
        assert!(
            imminent_events(&[high("later", at(600))], at(0), &AlertPolicy::default()).is_empty()
        );
    }
}
