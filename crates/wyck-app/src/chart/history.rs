//! Deciding what to ask the server for.
//!
//! The chart shows what the cache holds at once and then fills the difference. This module works
//! out *which spans* to request and what to record as fetched afterwards. It does no I/O of its
//! own: the caller passes the fetch as a closure, so the same code runs against the engine in the
//! app and against a stub in tests.
//!
//! # Pages
//!
//! Requests are made a page at a time, [`PAGE_BARS`] bars of the period wide. A page is small
//! enough to answer within the engine's request timeout on both servers (Local caps a call at
//! 1000 bars, Remote a window at 720 hours) and big enough that a few pages fill a screen at any
//! zoom. Two kinds of span come up:
//!
//! - the **tail**, from the newest thing we hold up to now. It runs on every open of a chart and
//!   is what makes the last bar current.
//! - an **older page**, just before the oldest bar loaded, when the user scrolls left.
//!
//! # What counts as fetched
//!
//! The newest bar of an answer may still be forming, and a span with no bars at all may only mean
//! the market is closed. [`covered_for`] settles both: coverage stops at the open of the newest
//! bar returned (that bar is fetched again next time, which is the point), and an answer with no
//! bars covers the span only up to one period before now. Older spans are complete as asked.

use std::future::Future;

use wyck_engine::domain::{Candle, Period, UnixMillis};

use super::coverage::{Coverage, Span};
use super::series::period_millis;

/// How many bars one request covers, at most.
pub const PAGE_BARS: i64 = 1000;

/// How many pages the tail may span. A cache older than this is not caught up in one go: the
/// missing middle stays uncovered and is fetched if the user scrolls back to it.
pub const MAX_TAIL_PAGES: i64 = 3;

/// The length of one period in milliseconds, with a calendar month taken as thirty days for
/// sizing requests.
#[must_use]
pub fn unit_millis(period: Period) -> i64 {
    period_millis(period).unwrap_or(30 * 24 * 3_600_000)
}

/// The width of one request.
#[must_use]
pub fn page_millis(period: Period) -> i64 {
    PAGE_BARS * unit_millis(period)
}

/// The span to fetch to bring a series up to date at `now`: from the end of what is covered (or
/// one page back when nothing is), to one period after `now` so the forming bar is included.
///
/// A gap wider than [`MAX_TAIL_PAGES`] pages is shortened from the old end.
#[must_use]
pub fn tail_span(coverage: &Coverage, now: UnixMillis, period: Period) -> Span {
    let page = page_millis(period);
    let to = now + unit_millis(period);
    let floor = to - MAX_TAIL_PAGES * page;
    let from = coverage
        .latest()
        .map_or(to - page, |latest| latest.max(floor));
    (from, to)
}

/// The span of the page just before `before` (the open of the oldest bar loaded).
#[must_use]
pub fn older_span(before: UnixMillis, period: Period) -> Span {
    (before - page_millis(period), before)
}

/// What to record as fetched after `span` was answered with `bars`. See the module docs.
///
/// Returns `None` when nothing can be said to be covered (a span that came back empty and is
/// entirely within the last period).
#[must_use]
pub fn covered_for(span: Span, bars: &[Candle], now: UnixMillis, period: Period) -> Option<Span> {
    let (from, to) = span;
    let settled = now - unit_millis(period);
    let newest = bars.iter().map(|b| b.time).max();
    let end = if to > settled {
        // The span reaches into the last period. The newest bar may be forming, so cover up to its
        // open only; when the newest bar is older (a closed market) cover up to the settled time.
        newest.map_or(settled, |newest| newest.max(settled)).min(to)
    } else {
        to
    };
    (end > from).then_some((from, end))
}

/// The result of one request.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    /// The bars the server returned, oldest first.
    pub bars: Vec<Candle>,
    /// The span to record as fetched, if any.
    pub covered: Option<Span>,
}

/// Requests `span` with `fetch` and works out what was covered.
///
/// # Errors
///
/// Whatever `fetch` returns; nothing is recorded then.
pub async fn fetch_span<E, F, Fut>(
    span: Span,
    now: UnixMillis,
    period: Period,
    fetch: F,
) -> Result<Fetched, E>
where
    F: FnOnce(UnixMillis, UnixMillis) -> Fut,
    Fut: Future<Output = Result<Vec<Candle>, E>>,
{
    let bars = fetch(span.0, span.1).await?;
    let covered = covered_for(span, &bars, now, period);
    Ok(Fetched { bars, covered })
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;
    const NOW: i64 = 1_000_000 * H;

    fn bar(hour: i64) -> Candle {
        Candle::at(hour * H, 1.0)
    }

    fn cov(from: i64, to: i64) -> Coverage {
        let mut c = Coverage::new();
        c.add(from, to);
        c
    }

    #[test]
    fn a_fresh_series_starts_one_page_back() {
        let (from, to) = tail_span(&Coverage::new(), NOW, Period::H1);
        assert_eq!(to, NOW + H);
        assert_eq!(to - from, page_millis(Period::H1));
    }

    #[test]
    fn a_covered_series_only_asks_for_the_new_part() {
        let (from, to) = tail_span(&cov(NOW - 50 * H, NOW - 2 * H), NOW, Period::H1);
        assert_eq!((from, to), (NOW - 2 * H, NOW + H));
    }

    #[test]
    fn a_very_stale_cache_is_only_caught_up_a_few_pages() {
        let (from, to) = tail_span(&cov(0, 10 * H), NOW, Period::H1);
        assert_eq!(to - from, MAX_TAIL_PAGES * page_millis(Period::H1));
    }

    #[test]
    fn the_older_page_ends_where_the_data_starts() {
        let (from, to) = older_span(500 * H, Period::H1);
        assert_eq!(to, 500 * H);
        assert_eq!(to - from, PAGE_BARS * H);
    }

    #[test]
    fn coverage_stops_at_the_open_of_the_forming_bar() {
        let span = (NOW - 10 * H, NOW + H);
        let bars = [bar(999_990), bar(999_995), bar(1_000_000)];
        let covered = covered_for(span, &bars, NOW, Period::H1).unwrap();
        assert_eq!(covered, (NOW - 10 * H, 1_000_000 * H));
    }

    #[test]
    fn an_empty_answer_covers_up_to_the_last_settled_period() {
        let span = (NOW - 10 * H, NOW + H);
        assert_eq!(
            covered_for(span, &[], NOW, Period::H1),
            Some((NOW - 10 * H, NOW - H))
        );
        // Nothing to say when the whole span is inside the last period.
        assert_eq!(
            covered_for((NOW - H / 2, NOW + H), &[], NOW, Period::H1),
            None
        );
    }

    #[test]
    fn a_closed_market_does_not_cover_the_future() {
        // The newest bar is from last Friday; the span runs to now plus one period.
        let span = (NOW - 100 * H, NOW + H);
        let covered = covered_for(span, &[bar(999_950)], NOW, Period::H1).unwrap();
        assert_eq!(covered, (NOW - 100 * H, NOW - H));
    }

    #[test]
    fn an_older_page_is_covered_as_asked_even_when_empty() {
        let span = (100 * H, 200 * H);
        assert_eq!(covered_for(span, &[], NOW, Period::H1), Some(span));
        assert_eq!(covered_for(span, &[bar(150)], NOW, Period::H1), Some(span));
    }

    #[tokio::test]
    async fn fetching_passes_the_span_and_reports_what_was_covered() {
        let span = (100 * H, 200 * H);
        let fetched = fetch_span(span, NOW, Period::H1, |from, to| async move {
            assert_eq!((from, to), (100 * H, 200 * H));
            Ok::<_, String>(vec![bar(150)])
        })
        .await
        .unwrap();
        assert_eq!(fetched.bars.len(), 1);
        assert_eq!(fetched.covered, Some(span));
    }

    #[tokio::test]
    async fn a_failed_fetch_records_nothing() {
        let result = fetch_span((0, H), NOW, Period::H1, |_, _| async {
            Err::<Vec<Candle>, _>("boom".to_owned())
        })
        .await;
        assert_eq!(result, Err("boom".to_owned()));
    }
}
