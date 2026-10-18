//! The synthetic path of a bar: how its price moves between its open and its close.

use wyck_market::Bar;

/// The bar as it stands `elapsed_ms` into a span of `length_ms`, or `None` before anything moved.
///
/// The path is O, L, H, C for a bar that closed up and O, H, L, C for one that closed down, with
/// the price moving in a straight line between the four points, each third of the span. It is
/// built from the final range, so it is plausible, not what really happened inside the bar. The
/// high only grows, the low only falls, and the volume grows in proportion to the time.
#[must_use]
pub fn replay_partial_bar(bar: Bar, elapsed_ms: i64, length_ms: i64) -> Option<Bar> {
    if elapsed_ms <= 0 || length_ms <= 0 {
        return None;
    }
    if elapsed_ms >= length_ms {
        return Some(bar);
    }
    let points = if bar.close >= bar.open {
        [bar.open, bar.low, bar.high, bar.close]
    } else {
        [bar.open, bar.high, bar.low, bar.close]
    };
    let length = i128::from(length_ms);
    let scaled = i128::from(elapsed_ms) * 3;
    let leg = usize::try_from(scaled / length).unwrap_or(2).min(2);
    let rest = scaled - i128::from(i64::try_from(leg).unwrap_or(2)) * length;
    let from = i128::from(*points.get(leg)?);
    let to = i128::from(*points.get(leg + 1)?);
    let price = i64::try_from(from + (to - from) * rest / length).ok()?;
    let visited = points.get(..=leg)?;
    let high = visited.iter().copied().chain([price]).max()?;
    let low = visited.iter().copied().chain([price]).min()?;
    let volume = i64::try_from(i128::from(bar.volume) * i128::from(elapsed_ms) / length)
        .unwrap_or(bar.volume);
    Some(Bar {
        time_ms: bar.time_ms,
        open: bar.open,
        high,
        low,
        close: price,
        volume,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up() -> Bar {
        Bar {
            time_ms: 0,
            open: 100,
            high: 130,
            low: 90,
            close: 120,
            volume: 30,
        }
    }

    #[test]
    fn nothing_has_moved_before_the_span_starts() {
        assert_eq!(replay_partial_bar(up(), 0, 60_000), None);
        assert_eq!(replay_partial_bar(up(), -5, 60_000), None);
    }

    #[test]
    fn a_partial_bar_at_the_end_of_the_span_is_the_whole_bar() {
        assert_eq!(replay_partial_bar(up(), 60_000, 60_000), Some(up()));
        assert_eq!(replay_partial_bar(up(), 90_000, 60_000), Some(up()));
    }

    #[test]
    fn a_bar_that_closed_up_visits_its_low_before_its_high() {
        let third = replay_partial_bar(up(), 20_000, 60_000).expect("a bar");
        assert_eq!((third.low, third.high), (90, 100));
        let two_thirds = replay_partial_bar(up(), 40_000, 60_000).expect("a bar");
        assert_eq!((two_thirds.low, two_thirds.high), (90, 130));
        assert_eq!(two_thirds.close, 130);
    }

    #[test]
    fn a_bar_that_closed_down_visits_its_high_before_its_low() {
        let down = Bar {
            time_ms: 0,
            open: 120,
            high: 130,
            low: 90,
            close: 100,
            volume: 30,
        };
        let third = replay_partial_bar(down, 20_000, 60_000).expect("a bar");
        assert_eq!((third.low, third.high), (120, 130));
        let two_thirds = replay_partial_bar(down, 40_000, 60_000).expect("a bar");
        assert_eq!((two_thirds.low, two_thirds.high), (90, 130));
    }

    #[test]
    fn the_range_only_widens_and_the_close_stays_inside_it() {
        for bar in [
            up(),
            Bar {
                open: 120,
                close: 100,
                ..up()
            },
        ] {
            let mut previous: Option<Bar> = None;
            for elapsed in (1..=60).map(|s| s * 1_000) {
                let now = replay_partial_bar(bar, elapsed, 60_000).expect("a bar");
                assert!(now.low <= now.close && now.close <= now.high);
                assert!(now.low >= bar.low && now.high <= bar.high);
                if let Some(before) = previous {
                    assert!(now.high >= before.high && now.low <= before.low);
                    assert!(now.volume >= before.volume);
                }
                previous = Some(now);
            }
        }
    }
}
