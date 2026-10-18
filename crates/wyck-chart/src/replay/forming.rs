//! A forming candle, built from the minute bars seen so far.

use wyck_market::{Bar, Tick};

use super::path::replay_partial_bar;

const MINUTE_MS: i64 = 60_000;

/// The candle that opened at `from_ms`, as the minute bars (sorted, oldest first) show it when the
/// cursor is at `cursor_ms`: whole minutes ended by then, plus the minute in progress along its
/// synthetic path. `None` when no minute has started yet.
///
/// The cost is the number of minutes in the candle, so a monthly candle folds up to 45 000 bars.
#[must_use]
pub fn replay_fold_minutes(minutes: &[Bar], from_ms: i64, cursor_ms: i64) -> Option<Bar> {
    let lo = minutes.partition_point(|m| m.time_ms < from_ms);
    let hi = minutes.partition_point(|m| m.time_ms < cursor_ms);
    let mut folded: Option<Bar> = None;
    for minute in minutes.get(lo..hi)? {
        let part = if minute.time_ms + MINUTE_MS > cursor_ms {
            let Some(part) = replay_partial_bar(*minute, cursor_ms - minute.time_ms, MINUTE_MS)
            else {
                continue;
            };
            part
        } else {
            *minute
        };
        folded = Some(match folded {
            None => Bar {
                time_ms: from_ms,
                ..part
            },
            Some(so_far) => Bar {
                high: so_far.high.max(part.high),
                low: so_far.low.min(part.low),
                close: part.close,
                volume: so_far.volume + part.volume,
                ..so_far
            },
        });
    }
    folded
}

/// The candle that opened at `from_ms`, as the ticks (sorted, oldest first) show it when the cursor is
/// at `cursor_ms`: the ticks from `from_ms` up to, not including, the cursor. `None` when there is
/// none. The volume is the number of ticks.
#[must_use]
pub fn replay_fold_ticks(ticks: &[Tick], from_ms: i64, cursor_ms: i64) -> Option<Bar> {
    let lo = ticks.partition_point(|t| t.time_ms < from_ms);
    let hi = ticks.partition_point(|t| t.time_ms < cursor_ms);
    let span = ticks.get(lo..hi)?;
    let (first, last) = (span.first()?, span.last()?);
    let (low, high) = span
        .iter()
        .fold((first.price, first.price), |(low, high), t| {
            (low.min(t.price), high.max(t.price))
        });
    Some(Bar {
        time_ms: from_ms,
        open: first.price,
        high,
        low,
        close: last.price,
        volume: wyck_core::math::count_i64(span.len()),
    })
}

/// The candle that opened at `from_ms` when real ticks are held: the minute bars that ended before
/// the cursor, and the real ticks of the minute in progress. The bar of the minute in progress is
/// never read, not even its open: it holds the price of ticks that have not happened yet.
///
/// The volume of the last minute is a count of bid ticks, not the server's (which counts bid and
/// ask changes), so the two parts do not quite add up. `None` when neither part has anything.
#[must_use]
pub fn replay_fold_hybrid(
    minutes: &[Bar],
    ticks: &[Tick],
    from_ms: i64,
    cursor_ms: i64,
) -> Option<Bar> {
    let current = cursor_ms - cursor_ms.rem_euclid(MINUTE_MS);
    let closed = replay_fold_minutes(minutes, from_ms, current);
    let live = replay_fold_ticks(ticks, from_ms.max(current), cursor_ms);
    match (closed, live) {
        (Some(closed), Some(live)) => Some(Bar {
            high: closed.high.max(live.high),
            low: closed.low.min(live.low),
            close: live.close,
            volume: closed.volume + live.volume,
            ..closed
        }),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minute(index: i64, open: i64, high: i64, low: i64, close: i64) -> Bar {
        Bar {
            time_ms: index * MINUTE_MS,
            open,
            high,
            low,
            close,
            volume: 10,
        }
    }

    fn minutes() -> Vec<Bar> {
        vec![
            minute(0, 100, 105, 99, 104),
            minute(1, 104, 110, 103, 108),
            minute(2, 108, 109, 95, 96),
        ]
    }

    #[test]
    fn no_minute_has_started_before_the_candle_opens() {
        assert_eq!(replay_fold_minutes(&minutes(), 0, 0), None);
        assert_eq!(
            replay_fold_minutes(&minutes(), 3 * MINUTE_MS, 4 * MINUTE_MS),
            None
        );
    }

    #[test]
    fn the_candle_is_the_whole_minutes_ended_by_the_cursor() {
        let candle = replay_fold_minutes(&minutes(), 0, 2 * MINUTE_MS).expect("a candle");
        assert_eq!(
            (
                candle.time_ms,
                candle.open,
                candle.high,
                candle.low,
                candle.close
            ),
            (0, 100, 110, 99, 108)
        );
        assert_eq!(candle.volume, 20);
    }

    #[test]
    fn the_minute_in_progress_follows_its_synthetic_path() {
        let candle = replay_fold_minutes(&minutes(), 0, 2 * MINUTE_MS + 20_000).expect("a candle");
        assert_eq!(
            candle.high, 110,
            "the third minute closed down: it goes up first"
        );
        assert_eq!(candle.low, 99);
        assert!(candle.close > 108 && candle.close <= 109);
    }

    #[test]
    fn a_minute_after_the_cursor_is_never_seen() {
        let candle = replay_fold_minutes(&minutes(), 0, MINUTE_MS).expect("a candle");
        assert_eq!((candle.high, candle.low, candle.close), (105, 99, 104));
    }

    #[test]
    fn a_later_candle_starts_at_its_own_opening_minute() {
        let candle = replay_fold_minutes(&minutes(), MINUTE_MS, 3 * MINUTE_MS).expect("a candle");
        assert_eq!(
            (candle.time_ms, candle.open, candle.low),
            (MINUTE_MS, 104, 95)
        );
    }
}

#[cfg(test)]
mod tick_tests {
    use super::*;

    fn tick(time_ms: i64, price: i64) -> Tick {
        Tick { time_ms, price }
    }

    fn minute(index: i64, open: i64, high: i64, low: i64, close: i64) -> Bar {
        Bar {
            time_ms: index * MINUTE_MS,
            open,
            high,
            low,
            close,
            volume: 10,
        }
    }

    #[test]
    fn a_candle_from_ticks_is_what_the_ticks_before_the_cursor_show() {
        let ticks = [
            tick(0, 100),
            tick(10_000, 105),
            tick(20_000, 98),
            tick(30_000, 101),
        ];
        let candle = replay_fold_ticks(&ticks, 0, 25_000).expect("a candle");
        assert_eq!(
            (
                candle.open,
                candle.high,
                candle.low,
                candle.close,
                candle.volume
            ),
            (100, 105, 98, 98, 3)
        );
        assert_eq!(
            replay_fold_ticks(&ticks, 0, 0),
            None,
            "nothing happened before the cursor"
        );
        assert_eq!(replay_fold_ticks(&ticks, 40_000, 90_000), None);
    }

    #[test]
    fn a_tick_exactly_at_the_cursor_is_not_seen() {
        let ticks = [tick(0, 100), tick(10_000, 999)];
        let candle = replay_fold_ticks(&ticks, 0, 10_000).expect("a candle");
        assert_eq!(candle.high, 100);
    }

    #[test]
    fn the_hybrid_candle_takes_ended_minutes_and_the_ticks_of_the_minute_in_progress() {
        let minutes = [
            minute(0, 100, 105, 99, 104),
            minute(1, 104, 110, 103, 108),
            minute(2, 108, 150, 50, 77),
        ];
        let ticks = [
            tick(2 * MINUTE_MS + 5_000, 109),
            tick(2 * MINUTE_MS + 20_000, 106),
        ];
        let candle =
            replay_fold_hybrid(&minutes, &ticks, 0, 2 * MINUTE_MS + 30_000).expect("candle");
        assert_eq!(
            (candle.open, candle.high, candle.low, candle.close),
            (100, 110, 99, 106),
            "the final range of minute 2 (150, 50) is not read"
        );
    }

    #[test]
    fn the_hybrid_candle_never_reads_the_minute_in_progress() {
        let ticks = [tick(2 * MINUTE_MS + 5_000, 109)];
        let poisoned = [
            minute(0, 100, 105, 99, 104),
            minute(1, 104, 110, 103, 108),
            minute(2, 1, 1_000_000, 0, 1),
        ];
        let clean = [
            minute(0, 100, 105, 99, 104),
            minute(1, 104, 110, 103, 108),
            minute(2, 2, 2, 2, 2),
        ];
        let cursor = 2 * MINUTE_MS + 30_000;
        assert_eq!(
            replay_fold_hybrid(&poisoned, &ticks, 0, cursor),
            replay_fold_hybrid(&clean, &ticks, 0, cursor)
        );
    }

    #[test]
    fn at_the_start_of_a_minute_the_candle_is_the_ended_minutes_alone() {
        let minutes = [minute(0, 100, 105, 99, 104), minute(1, 1, 999, 0, 2)];
        let candle = replay_fold_hybrid(&minutes, &[], 0, MINUTE_MS).expect("a candle");
        assert_eq!((candle.high, candle.close), (105, 104));
    }

    #[test]
    fn a_candle_that_opens_in_the_minute_in_progress_is_made_of_its_ticks() {
        let ticks = [tick(MINUTE_MS + 1_000, 50), tick(MINUTE_MS + 2_000, 60)];
        let candle =
            replay_fold_hybrid(&[], &ticks, MINUTE_MS, MINUTE_MS + 3_000).expect("a candle");
        assert_eq!(
            (candle.time_ms, candle.open, candle.close),
            (MINUTE_MS, 50, 60)
        );
    }
}
