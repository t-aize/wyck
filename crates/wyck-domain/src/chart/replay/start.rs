//! Where a replay starts: a random moment, or the bar a time falls in.

use crate::market::Bar;

/// A random time in `[first_ms, last_ms]`, rounded down to `step_ms`. The same seed gives the same
/// time. `None` when the range is empty.
#[must_use]
pub fn replay_random_start(seed: u64, first_ms: i64, last_ms: i64, step_ms: i64) -> Option<i64> {
    let width = u64::try_from(last_ms.checked_sub(first_ms)?).ok()?;
    let pick = splitmix64(seed) % width.checked_add(1)?;
    let time = first_ms.checked_add(i64::try_from(pick).ok()?)?;
    let step = step_ms.max(1);
    Some((time - time.rem_euclid(step)).max(first_ms))
}

/// The opening time of the newest bar that opened at or before `time_ms`.
#[must_use]
pub fn replay_align_to_bar(bars: &[Bar], time_ms: i64) -> Option<i64> {
    let at = bars.partition_point(|b| b.time_ms <= time_ms);
    at.checked_sub(1)
        .and_then(|i| bars.get(i))
        .map(|b| b.time_ms)
}

fn splitmix64(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(time_ms: i64) -> Bar {
        Bar {
            time_ms,
            open: 1,
            high: 1,
            low: 1,
            close: 1,
            volume: 1,
        }
    }

    #[test]
    fn a_random_start_is_the_same_for_the_same_seed() {
        assert_eq!(
            replay_random_start(7, 1_000, 9_000_000, 60_000),
            replay_random_start(7, 1_000, 9_000_000, 60_000)
        );
        assert_ne!(
            replay_random_start(7, 1_000, 9_000_000, 60_000),
            replay_random_start(8, 1_000, 9_000_000, 60_000)
        );
    }

    #[test]
    fn a_random_start_stays_in_range_and_on_the_step() {
        for seed in 0..500 {
            let start = replay_random_start(seed, 120_000, 5_000_000, 60_000).expect("a start");
            assert!((120_000..=5_000_000).contains(&start));
            assert_eq!(start % 60_000, 0);
        }
    }

    #[test]
    fn an_empty_range_has_no_random_start() {
        assert_eq!(replay_random_start(1, 10, 5, 60_000), None);
        assert_eq!(replay_random_start(1, 10, 10, 1), Some(10));
    }

    #[test]
    fn a_time_aligns_to_the_bar_it_falls_in() {
        let bars = [bar(0), bar(300_000), bar(600_000)];
        assert_eq!(replay_align_to_bar(&bars, 450_000), Some(300_000));
        assert_eq!(replay_align_to_bar(&bars, 300_000), Some(300_000));
        assert_eq!(replay_align_to_bar(&bars, 9_000_000), Some(600_000));
        assert_eq!(replay_align_to_bar(&bars, -1), None);
    }
}
