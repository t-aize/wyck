//! Hosting the economic calendar: turns `wyck-calendar` data into the engine's
//! [`NewsView`] and into non-blocking [`Warning`](crate::state::Warning)s.
//!
//! Every few seconds (and whenever the calendar service publishes) the engine works out
//! which currencies matter (the base and quote currencies of the symbols with open positions
//! or on the watch list), filters the calendar to high-impact events for those currencies,
//! and warns about the ones that are close. Timing uses the broker's clock, estimated when
//! the session was established, not this computer's.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use time::OffsetDateTime;
use wyck_calendar::{
    AlertPolicy, CalendarEvent, CalendarHandle, CalendarState, EventFilter, Freshness, Impact,
    Scope, Timing, currencies_from_symbols, imminent_events, upcoming,
};

use crate::core::Inner;
use crate::domain::UnixMillis;
use crate::state::{NewsFreshness, NewsItem, NewsView, WarningKind};

/// How often the news view and warnings are recomputed when nothing else prompts it.
const TICK: Duration = Duration::from_secs(15);
/// How many upcoming events the view carries.
const VIEW_LEN: usize = 20;

fn to_datetime(ms: UnixMillis) -> Option<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000).ok()
}

fn to_millis(dt: OffsetDateTime) -> UnixMillis {
    i64::try_from(dt.unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
}

fn freshness(f: Freshness) -> NewsFreshness {
    match f {
        Freshness::Loading => NewsFreshness::Loading,
        Freshness::Unavailable => NewsFreshness::Unavailable,
        Freshness::Fresh => NewsFreshness::Fresh,
        Freshness::Stale => NewsFreshness::Stale,
    }
}

fn item(event: &CalendarEvent) -> NewsItem {
    NewsItem {
        title: event.title.clone(),
        currency: match event.scope {
            Scope::Currency(c) => Some(c.to_string()),
            _ => None,
        },
        impact: event.impact.to_string(),
        at: to_millis(event.time),
        forecast: event.forecast.clone(),
        previous: event.previous.clone(),
    }
}

/// The filter for "what this person trades": high impact, for the currencies of the symbols
/// with positions or on the watch list. With no symbols at all, every currency.
pub(crate) fn news_filter(symbols: &[String]) -> EventFilter {
    EventFilter::new()
        .with_min_impact(Impact::High)
        .with_currencies(currencies_from_symbols(symbols))
}

/// The pure part: given calendar data and the current time, what should the view and the
/// active news warnings be.
pub(crate) struct NewsComputation {
    pub(crate) view: NewsView,
    /// `(warning id, message)` for every event that is imminent or just released.
    pub(crate) warnings: Vec<(String, String)>,
}

pub(crate) fn compute(
    calendar: &CalendarState,
    symbols: &[String],
    now_ms: UnixMillis,
    lead: Duration,
    grace: Duration,
) -> NewsComputation {
    let filter = news_filter(symbols);
    let mut view = NewsView {
        freshness: freshness(calendar.freshness()),
        fetched_at: calendar.fetched_at.map(to_millis),
        last_error: calendar.last_error.clone(),
        upcoming: Vec::new(),
    };
    let Some(now) = to_datetime(now_ms) else {
        return NewsComputation {
            view,
            warnings: Vec::new(),
        };
    };
    let grace_td = time::Duration::try_from(grace).unwrap_or(time::Duration::minutes(15));
    let start = now - grace_td;
    view.upcoming = filter
        .apply(upcoming(&calendar.events, start, calendar.events.len()))
        .into_iter()
        .take(VIEW_LEN)
        .map(item)
        .collect();

    let policy = AlertPolicy {
        lead_time: time::Duration::try_from(lead).unwrap_or(time::Duration::minutes(30)),
        grace: grace_td,
        filter,
    };
    let warnings = imminent_events(&calendar.events, now, &policy)
        .into_iter()
        .map(|w| {
            let id = format!("news:{}:{}", w.event.time.unix_timestamp(), w.event.title);
            let ccy = match w.event.scope {
                Scope::Currency(c) => format!(" ({c})"),
                _ => String::new(),
            };
            let when = match w.timing {
                Timing::Upcoming { starts_in } => {
                    format!("in {} min", (starts_in.whole_seconds() + 59) / 60)
                }
                Timing::Underway { since } if since.whole_seconds() < 60 => "just now".to_owned(),
                Timing::Underway { since } => format!("{} min ago", since.whole_minutes()),
            };
            (id, format!("{}{ccy}: {when}", w.event.title))
        })
        .collect();
    NewsComputation { view, warnings }
}

/// Starts the bridge task. It ends with the engine.
pub(crate) fn spawn(inner: &Arc<Inner>, calendar: CalendarHandle) {
    let me = Arc::clone(inner);
    let mut updates = calendar.subscribe();
    inner.tracker.spawn(async move {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut active: HashSet<String> = HashSet::new();
        loop {
            tokio::select! {
                () = me.shutdown.cancelled() => break,
                _ = tick.tick() => {}
                changed = updates.changed() => {
                    if changed.is_err() {
                        break;
                    }
                }
            }
            let state = calendar.state();
            let symbols = me.quote_symbols();
            let computed = compute(
                &state,
                &symbols,
                me.server_now_ms(),
                me.config.guardrails.news_lead_time,
                me.config.guardrails.news_grace,
            );
            me.update(|s| s.news = computed.view.clone());

            let desired: HashSet<String> =
                computed.warnings.iter().map(|(id, _)| id.clone()).collect();
            for (id, message) in &computed.warnings {
                me.raise_warning(id.clone(), WarningKind::News, message.clone());
            }
            for stale in active.difference(&desired) {
                me.clear_warning(stale);
            }
            active = desired;
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use time::macros::datetime;
    use wyck_calendar::{CalendarState, parse_feed};

    use super::*;

    fn state(json: &str) -> CalendarState {
        let feed = parse_feed(json.as_bytes()).unwrap();
        CalendarState {
            events: Arc::from(feed.events),
            fetched_at: Some(datetime!(2026-09-16 12:00 UTC)),
            last_error: None,
            consecutive_failures: 0,
            skipped_records: 0,
        }
    }

    const FEED: &str = r#"[
      {"title":"Federal Funds Rate","country":"USD","date":"2026-09-16T14:00:00-04:00","impact":"High"},
      {"title":"BOJ Policy Rate","country":"JPY","date":"2026-09-16T14:10:00-04:00","impact":"High"},
      {"title":"Building Permits","country":"USD","date":"2026-09-16T14:05:00-04:00","impact":"Low"},
      {"title":"BRICS Summit","country":"All","date":"2026-09-16T14:20:00-04:00","impact":"High"}
    ]"#;

    fn now(hh: u8, mm: u8) -> UnixMillis {
        let t = datetime!(2026-09-16 00:00 -4)
            .replace_hour(hh)
            .unwrap()
            .replace_minute(mm)
            .unwrap();
        to_millis(t)
    }

    fn compute_at(symbols: &[&str], n: UnixMillis) -> NewsComputation {
        let symbols: Vec<String> = symbols.iter().map(|s| (*s).to_owned()).collect();
        compute(
            &state(FEED),
            &symbols,
            n,
            Duration::from_secs(1800),
            Duration::from_secs(900),
        )
    }

    #[test]
    fn only_high_impact_events_for_traded_currencies_warn() {
        let c = compute_at(&["EURUSD"], now(13, 45));
        let messages: Vec<&str> = c.warnings.iter().map(|(_, m)| m.as_str()).collect();
        assert_eq!(
            messages,
            ["Federal Funds Rate (USD): in 15 min"],
            "{messages:?}"
        );
    }

    #[test]
    fn a_jpy_trader_hears_about_the_boj_not_the_fed() {
        let c = compute_at(&["GBPJPY"], now(13, 45));
        let messages: Vec<&str> = c.warnings.iter().map(|(_, m)| m.as_str()).collect();
        assert_eq!(messages, ["BOJ Policy Rate (JPY): in 25 min"]);
    }

    #[test]
    fn after_the_release_it_says_so_until_the_grace_window_ends() {
        let c = compute_at(&["EURUSD"], now(14, 5));
        assert_eq!(c.warnings.len(), 1);
        assert!(c.warnings[0].1.contains("5 min ago"), "{}", c.warnings[0].1);
        assert!(compute_at(&["EURUSD"], now(14, 20)).warnings.is_empty());
    }

    #[test]
    fn with_no_symbols_every_currency_counts() {
        let c = compute_at(&[], now(13, 50));
        assert_eq!(c.warnings.len(), 3, "USD, JPY and the global event");
    }

    #[test]
    fn the_view_lists_upcoming_filtered_events_soonest_first() {
        let c = compute_at(&["EURUSD"], now(13, 0));
        let titles: Vec<&str> = c.view.upcoming.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["Federal Funds Rate"]);
        assert_eq!(c.view.upcoming[0].currency.as_deref(), Some("USD"));
        assert_eq!(c.view.freshness, NewsFreshness::Fresh);
    }

    #[test]
    fn warning_ids_are_stable_for_the_same_event() {
        let a = compute_at(&["EURUSD"], now(13, 45));
        let b = compute_at(&["EURUSD"], now(13, 50));
        assert_eq!(a.warnings[0].0, b.warnings[0].0);
    }
}
