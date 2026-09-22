//! Whole ranges of history, fetched page by page.
//!
//! One request returns only part of a range: the server caps what a response holds and says so
//! with `hasMore`. These functions turn that into "give me everything between `from` and `to`":
//! they split the range where the server needs it, follow `hasMore`, and put the pages together
//! in order. Every request goes through the client's historical rate limit (5 per second), so a
//! long range takes a while and never triggers `REQUEST_FREQUENCY_EXCEEDED`.
//!
//! # Ticks
//!
//! A tick request may span at most one week, so a longer range is cut into windows of just under a
//! week ([`tick_windows`]). Inside a window the server returns the **newest** ticks first, so when
//! `hasMore` is set the next request asks for the part before the oldest tick received. Bid and ask
//! ticks are separate requests; [`crate::openapi::market::merge_sides`] joins them into quotes.
//!
//! # Bars
//!
//! The documentation limits the range of a bar request per period but does not say which end of the
//! range a truncated answer holds. [`fetch_bars`] works either way: it looks at where the page falls
//! in the range and continues from the missing side. A forum report describes silent gaps when many
//! full size bar requests are chained, so a caller filling a long history should ask in modest
//! ranges and check the seams.

use super::bars::{Bar, Period};
use super::client::MarketClient;
use super::ticks::{QuoteType, Tick};
use crate::openapi::error::{OpenApiError, Result};

/// The longest range of one tick request: one week.
pub const MAX_TICK_RANGE_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

/// A tick window is kept a little under the limit, so an off by one at its edges cannot be refused.
const TICK_WINDOW_MS: i64 = MAX_TICK_RANGE_MS - 60_000;

/// The most requests one range may take: a guard against a server that keeps saying `hasMore`
/// without making progress.
const MAX_PAGES: usize = 5_000;

/// ```
/// use wyck::openapi::market::tick_windows;
///
/// let day = 86_400_000;
/// let windows = tick_windows(0, 20 * day);
/// assert!(windows.len() >= 3);                       // under a week each
/// assert_eq!(windows[1].0, windows[0].1 + 1);        // no gap, no overlap
/// ```
///
/// Splits `[from_ms, to_ms]` into chronological windows the tick endpoint accepts. Windows do not
/// overlap: each starts one millisecond after the one before ends. An empty or backwards range
/// gives no window.
#[must_use]
pub fn tick_windows(from_ms: i64, to_ms: i64) -> Vec<(i64, i64)> {
    let mut windows = Vec::new();
    if to_ms < from_ms {
        return windows;
    }
    let mut start = from_ms;
    loop {
        let end = start.saturating_add(TICK_WINDOW_MS).min(to_ms);
        windows.push((start, end));
        if end >= to_ms {
            break;
        }
        start = end + 1;
    }
    windows
}

/// Every tick of one side in `[from_ms, to_ms]`, oldest first.
///
/// A tick that shares its millisecond with the boundary of two pages may be missed when the server
/// cuts a page in the middle of that millisecond; the ticks of one millisecond are never split in
/// practice, and the next page starts one millisecond before the oldest tick received.
///
/// # Errors
///
/// Any error of [`MarketClient::tick_page`], or a protocol error if paging stops before
/// the requested range is complete. Ticks already fetched are dropped with it.
pub async fn fetch_ticks(
    market: &MarketClient,
    symbol_id: i64,
    side: QuoteType,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<Tick>> {
    let mut all: Vec<Tick> = Vec::new();
    let mut requests = 0usize;
    for (window_start, window_end) in tick_windows(from_ms, to_ms) {
        let mut upper = window_end;
        loop {
            requests += 1;
            let (page, has_more) = market
                .tick_page(symbol_id, side, window_start, upper)
                .await?;
            let oldest = page.first().map(|t| t.time_ms);
            all.extend(page);
            if !has_more {
                break;
            }
            match oldest {
                Some(oldest) if oldest > window_start && requests < MAX_PAGES => {
                    upper = oldest - 1;
                }
                _ => {
                    return Err(OpenApiError::Protocol(format!(
                        "tick history for {symbol_id} is incomplete in [{window_start}, {upper}]: \
                         the server reported more ticks but paging could not continue"
                    )));
                }
            }
        }
    }
    all.sort_by_key(|t| t.time_ms);
    Ok(all)
}

/// Every bar of `period` opening in `[from_ms, to_ms]`, oldest first, one per open time.
///
/// # Errors
///
/// Any error of [`MarketClient::bars_page`], or a protocol error if paging stops before
/// the requested range is complete. Bars already fetched are dropped with it.
pub async fn fetch_bars(
    market: &MarketClient,
    symbol_id: i64,
    period: Period,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<Bar>> {
    let mut all: Vec<Bar> = Vec::new();
    let (mut low, mut high) = (from_ms, to_ms);
    for page_number in 0..MAX_PAGES {
        if high < low {
            break;
        }
        let (page, has_more) = market.bars_page(symbol_id, period, low, high).await?;
        let (Some(first), Some(last)) = (
            page.first().map(|b| b.time_ms),
            page.last().map(|b| b.time_ms),
        ) else {
            if has_more {
                return Err(OpenApiError::Protocol(format!(
                    "bar history for {symbol_id} is incomplete in [{low}, {high}]: \
                     the server reported more bars but returned an empty page"
                )));
            }
            break;
        };
        all.extend(page);
        if !has_more {
            break;
        }
        if page_number + 1 == MAX_PAGES {
            return Err(OpenApiError::Protocol(format!(
                "bar history for {symbol_id} is incomplete: reached the {MAX_PAGES}-page limit"
            )));
        }
        // Which end of the range does this page hold? The one that reaches the top does, and the
        // rest is below it; otherwise the page starts at the bottom and the rest is above.
        match continuation(low, high, first, last, period) {
            Some((next_low, next_high)) => (low, high) = (next_low, next_high),
            None => {
                return Err(OpenApiError::Protocol(format!(
                    "bar history for {symbol_id} is incomplete in [{low}, {high}]: \
                     the server reported more bars without advancing"
                )));
            }
        }
    }
    all.retain(|b| b.time_ms >= from_ms && b.time_ms <= to_ms);
    all.sort_by_key(|b| b.time_ms);
    all.dedup_by_key(|b| b.time_ms);
    Ok(all)
}

/// The range still to fetch after a truncated page whose bars run from `first` to `last`, or
/// `None` when the page made no progress (which would loop forever).
#[must_use]
pub fn continuation(
    low: i64,
    high: i64,
    first: i64,
    last: i64,
    period: Period,
) -> Option<(i64, i64)> {
    let holds_newest = last + period.millis() > high;
    if holds_newest {
        (first > low).then_some((low, first - 1))
    } else {
        (last < high).then_some((last + 1, high))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400_000;

    #[test]
    fn a_short_range_is_one_window() {
        assert_eq!(tick_windows(1_000, 5_000), vec![(1_000, 5_000)]);
        assert_eq!(tick_windows(10, 10), vec![(10, 10)]);
    }

    #[test]
    fn a_long_range_is_cut_into_windows_under_a_week() {
        let windows = tick_windows(0, 20 * DAY);
        assert!(windows.len() >= 3);
        assert_eq!(windows.first().unwrap().0, 0);
        assert_eq!(windows.last().unwrap().1, 20 * DAY);
        for (start, end) in &windows {
            assert!(end - start < MAX_TICK_RANGE_MS, "{start} to {end}");
        }
        for pair in windows.windows(2) {
            assert_eq!(pair[1].0, pair[0].1 + 1, "no gap and no overlap");
        }
    }

    #[test]
    fn a_backwards_range_has_no_window() {
        assert!(tick_windows(5, 4).is_empty());
    }

    #[test]
    fn windows_survive_extreme_values() {
        let windows = tick_windows(i64::MAX - 10, i64::MAX);
        assert_eq!(windows, vec![(i64::MAX - 10, i64::MAX)]);
    }

    #[test]
    fn a_page_at_the_top_of_the_range_leaves_the_part_below() {
        // M1 bars, range 0..=100 minutes; the page holds the newest bars 60..=100.
        let m = Period::M1.millis();
        let next = continuation(0, 100 * m, 60 * m, 100 * m, Period::M1);
        assert_eq!(next, Some((0, 60 * m - 1)));
    }

    #[test]
    fn a_page_at_the_bottom_of_the_range_leaves_the_part_above() {
        let m = Period::M1.millis();
        let next = continuation(0, 100 * m, 0, 40 * m, Period::M1);
        assert_eq!(next, Some((40 * m + 1, 100 * m)));
    }

    #[test]
    fn a_page_that_makes_no_progress_stops_the_loop() {
        let m = Period::M1.millis();
        assert_eq!(continuation(0, 100 * m, 0, 100 * m, Period::M1), None);
        assert_eq!(
            continuation(50 * m, 100 * m, 50 * m, 100 * m, Period::M1),
            None
        );
    }
}
