//! A forming candle, built from the minute bars seen so far.

use wyck_market::Bar;

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
