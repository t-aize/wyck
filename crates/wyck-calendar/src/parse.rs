//! Decoding the feed's JSON into [`CalendarEvent`]s.
//!
//! Deliberately tolerant at the *record* level and strict at the *document* level: one
//! malformed record (a new date format, a missing field) is skipped and counted rather
//! than discarding the other ~100 events, but a body that is not an array at all (or an
//! array where nothing decodes) is an error, so schema drift is loud and never replaces
//! a good cache with an empty calendar.

use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::currency::Currency;
use crate::error::{CalendarError, Result};
use crate::event::{CalendarEvent, Impact, Scope};

/// A decoded feed.
#[derive(Debug, Clone, PartialEq)]
pub struct Feed {
    /// Every valid event, sorted by time (then scope, then title) with exact duplicates
    /// removed.
    pub events: Vec<CalendarEvent>,
    /// How many records were rejected (each is logged at `warn` with the reason).
    pub skipped: usize,
}

/// The wire shape of one record. Only the fields the feed is documented to carry;
/// unknown extra fields are ignored so additive changes never break decoding.
#[derive(Deserialize)]
struct RawEvent {
    title: String,
    country: String,
    date: String,
    impact: String,
    #[serde(default)]
    forecast: Option<String>,
    #[serde(default)]
    previous: Option<String>,
}

/// Decodes the raw response body of the feed. Tolerant per record (a malformed record is
/// skipped and counted in [`Feed::skipped`]), strict per document.
///
/// # Errors
///
/// [`CalendarError::Decode`] if `body` is not a JSON array;
/// [`CalendarError::NoValidEvents`] if it holds records but none are valid. An empty
/// array is valid and yields an empty [`Feed`].
pub fn parse_feed(body: &[u8]) -> Result<Feed> {
    let records: Vec<serde_json::Value> =
        serde_json::from_slice(body).map_err(CalendarError::Decode)?;

    let mut events = Vec::with_capacity(records.len());
    let mut skipped = 0;
    for record in records {
        match decode_record(record) {
            Ok(event) => events.push(event),
            Err(reason) => {
                skipped += 1;
                tracing::warn!(%reason, "skipping malformed calendar record");
            }
        }
    }

    if events.is_empty() && skipped > 0 {
        return Err(CalendarError::NoValidEvents { skipped });
    }

    events.sort_by(|a, b| (a.time, a.scope, &a.title).cmp(&(b.time, b.scope, &b.title)));
    events.dedup();
    Ok(Feed { events, skipped })
}

fn decode_record(record: serde_json::Value) -> std::result::Result<CalendarEvent, String> {
    let raw: RawEvent = serde_json::from_value(record).map_err(|e| e.to_string())?;

    let title = raw.title.trim();
    if title.is_empty() {
        return Err("empty title".to_owned());
    }
    let scope = parse_scope(&raw.country)
        .ok_or_else(|| format!("`{title}`: unrecognized country `{}`", raw.country))?;
    let time = OffsetDateTime::parse(raw.date.trim(), &Rfc3339)
        .map_err(|e| format!("`{title}`: bad date `{}`: {e}", raw.date))?;

    let impact = Impact::from_feed(&raw.impact);
    if impact == Impact::Unknown {
        tracing::warn!(title, impact = %raw.impact, "calendar record has an unrecognized impact level");
    }

    Ok(CalendarEvent {
        title: title.to_owned(),
        scope,
        time,
        impact,
        forecast: non_empty(raw.forecast),
        previous: non_empty(raw.previous),
    })
}

fn parse_scope(country: &str) -> Option<Scope> {
    let country = country.trim();
    if country.eq_ignore_ascii_case("all") {
        Some(Scope::Global)
    } else {
        country.parse::<Currency>().ok().map(Scope::Currency)
    }
}

/// The feed uses `""` for "no value"; surface that as `None`.
fn non_empty(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/ff_calendar_thisweek.json");

    fn one(json: &str) -> CalendarEvent {
        let feed = parse_feed(json.as_bytes()).unwrap();
        assert_eq!(feed.events.len(), 1);
        feed.events.into_iter().next().unwrap()
    }

    #[test]
    fn decodes_the_documented_sample_record() {
        let e = one(
            r#"[{"title":"CPI m/m","country":"CAD","date":"2026-09-14T08:30:00-04:00",
                 "impact":"High","forecast":"-0.1%","previous":"0.5%"}]"#,
        );
        assert_eq!(e.title, "CPI m/m");
        assert_eq!(e.scope, Scope::Currency(Currency::CAD));
        assert_eq!(e.time, datetime!(2026-09-14 08:30 -4));
        assert_eq!(e.impact, Impact::High);
        assert_eq!(e.forecast.as_deref(), Some("-0.1%"));
        assert_eq!(e.previous.as_deref(), Some("0.5%"));
    }

    #[test]
    fn empty_forecast_and_previous_become_none() {
        let e = one(r#"[{"title":"RBA Assist Gov Hunter Speaks","country":"AUD",
                 "date":"2026-09-13T22:30:00-04:00","impact":"Low","forecast":"","previous":""}]"#);
        assert_eq!((e.forecast, e.previous), (None, None));
    }

    #[test]
    fn missing_or_null_forecast_and_previous_are_none() {
        let e = one(
            r#"[{"title":"X","country":"USD","date":"2026-09-13T22:30:00-04:00",
                 "impact":"Low","forecast":null}]"#,
        );
        assert_eq!((e.forecast, e.previous), (None, None));
    }

    #[test]
    fn country_all_is_global_scope() {
        let e = one(
            r#"[{"title":"BRICS Summit","country":"All","date":"2026-09-13T04:15:00-04:00",
                 "impact":"Low","forecast":"","previous":""}]"#,
        );
        assert_eq!(e.scope, Scope::Global);
    }

    #[test]
    fn compound_values_are_kept_verbatim() {
        let e = one(r#"[{"title":"German 30-y Bond Auction","country":"EUR",
                 "date":"2026-09-16T05:35:00-04:00","impact":"Low","forecast":"","previous":"3.65|1.3"}]"#);
        assert_eq!(e.previous.as_deref(), Some("3.65|1.3"));
        assert!(matches!(
            e.previous_reading(),
            Some(crate::Reading::Compound(_))
        ));
    }

    #[test]
    fn every_impact_level_decodes() {
        for (raw, want) in [
            ("High", Impact::High),
            ("Medium", Impact::Medium),
            ("Low", Impact::Low),
            ("Holiday", Impact::Holiday),
            ("Brand New Tier", Impact::Unknown),
        ] {
            let json = format!(
                r#"[{{"title":"t","country":"USD","date":"2026-09-13T04:15:00-04:00","impact":"{raw}"}}]"#
            );
            assert_eq!(one(&json).impact, want, "{raw}");
        }
    }

    #[test]
    fn dates_with_any_utc_offset_compare_as_instants() {
        let feed = parse_feed(
            br#"[
              {"title":"b","country":"USD","date":"2026-09-14T13:00:00Z","impact":"Low"},
              {"title":"a","country":"USD","date":"2026-09-14T08:30:00-04:00","impact":"Low"}
            ]"#,
        )
        .unwrap();
        // 08:30-04:00 == 12:30Z, so it sorts before 13:00Z.
        let titles: Vec<_> = feed.events.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["a", "b"]);
    }

    #[test]
    fn one_bad_record_is_skipped_without_losing_the_rest() {
        let feed = parse_feed(
            br#"[
              {"title":"good","country":"USD","date":"2026-09-14T08:30:00-04:00","impact":"Low"},
              {"title":"bad date","country":"USD","date":"14/09/2026","impact":"Low"},
              {"title":"bad country","country":"United States","date":"2026-09-14T08:30:00-04:00","impact":"Low"},
              {"title":"  ","country":"USD","date":"2026-09-14T08:30:00-04:00","impact":"Low"},
              {"country":"USD"}
            ]"#,
        )
        .unwrap();
        assert_eq!(feed.events.len(), 1);
        assert_eq!(feed.skipped, 4);
    }

    #[test]
    fn exact_duplicates_are_collapsed() {
        let record =
            r#"{"title":"t","country":"USD","date":"2026-09-14T08:30:00-04:00","impact":"Low"}"#;
        let feed = parse_feed(format!("[{record},{record}]").as_bytes()).unwrap();
        assert_eq!(feed.events.len(), 1);
    }

    #[test]
    fn empty_array_is_a_valid_empty_calendar() {
        let feed = parse_feed(b"[]").unwrap();
        assert!(feed.events.is_empty());
        assert_eq!(feed.skipped, 0);
    }

    #[test]
    fn non_array_documents_are_decode_errors() {
        for body in ["", "not json", "{}", r#"{"events":[]}"#, "null"] {
            assert!(
                matches!(parse_feed(body.as_bytes()), Err(CalendarError::Decode(_))),
                "{body:?}"
            );
        }
    }

    #[test]
    fn an_array_with_no_valid_record_is_an_error_not_an_empty_calendar() {
        assert!(matches!(
            parse_feed(br#"[{"foo":1},{"bar":2}]"#),
            Err(CalendarError::NoValidEvents { skipped: 2 })
        ));
    }

    #[test]
    fn unknown_extra_fields_are_ignored() {
        let e = one(
            r#"[{"title":"t","country":"USD","date":"2026-09-14T08:30:00-04:00",
                 "impact":"Low","url":"https://example.invalid","actual":"1.0"}]"#,
        );
        assert_eq!(e.title, "t");
    }

    #[test]
    fn the_real_week_sample_decodes_completely_and_sorted() {
        let feed = parse_feed(FIXTURE.as_bytes()).unwrap();
        assert_eq!(feed.skipped, 0);
        assert!(feed.events.len() > 50, "got {}", feed.events.len());
        assert!(feed.events.windows(2).all(|w| w[0].time <= w[1].time));
        assert!(feed.events.iter().any(|e| e.scope == Scope::Global));
        assert!(feed.events.iter().any(|e| e.impact == Impact::High));
        assert!(feed.events.iter().all(|e| e.impact != Impact::Unknown));
    }
}
