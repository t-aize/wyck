//! How much one replay update covers: a time from a menu, a time the user typed, or some ticks.

use std::borrow::Cow;

const SECOND_MS: i64 = 1_000;
const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;
/// The shortest and longest step in time the user may type.
const MIN_CUSTOM_MS: i64 = SECOND_MS;
const MAX_CUSTOM_MS: i64 = 7 * DAY_MS;
/// The most ticks one update may cover.
const MAX_TICKS: u32 = 10_000;

/// What the user may write, for the message that says a step was not understood.
pub const REPLAY_STEP_HELP: &str =
    "Write a step like 90s, 7 min, 2 h, 3 days or 50 ticks (1 second to 7 days, 1 to 10000 ticks).";

/// How much market time one update covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplayStep {
    /// The shortest timeframe among the charts (see `replay_auto_step`).
    #[default]
    Auto,
    Seconds15,
    Minute1,
    Minutes5,
    Minutes15,
    Minutes30,
    Hour1,
    Hours4,
    Day1,
    /// A length typed by the user, in milliseconds.
    Custom(i64),
    /// This many ticks of the data (the first tick not yet seen, then the next ones).
    Ticks(u32),
}

impl ReplayStep {
    /// The steps the menu offers.
    pub const PRESETS: [Self; 9] = [
        Self::Auto,
        Self::Seconds15,
        Self::Minute1,
        Self::Minutes5,
        Self::Minutes15,
        Self::Minutes30,
        Self::Hour1,
        Self::Hours4,
        Self::Day1,
    ];

    /// The length in milliseconds, or `None` for [`ReplayStep::Auto`] and a step in ticks.
    #[must_use]
    pub fn millis(self) -> Option<i64> {
        Some(match self {
            Self::Auto | Self::Ticks(_) => return None,
            Self::Seconds15 => 15 * SECOND_MS,
            Self::Minute1 => MINUTE_MS,
            Self::Minutes5 => 5 * MINUTE_MS,
            Self::Minutes15 => 15 * MINUTE_MS,
            Self::Minutes30 => 30 * MINUTE_MS,
            Self::Hour1 => HOUR_MS,
            Self::Hours4 => 4 * HOUR_MS,
            Self::Day1 => DAY_MS,
            Self::Custom(ms) => ms,
        })
    }

    /// How many ticks one update covers, for a step in ticks.
    #[must_use]
    pub fn ticks(self) -> Option<u32> {
        match self {
            Self::Ticks(count) => Some(count),
            _ => None,
        }
    }

    #[must_use]
    pub fn label(self) -> Cow<'static, str> {
        match self {
            Self::Auto => Cow::Borrowed("Auto"),
            Self::Seconds15 => Cow::Borrowed("15 seconds"),
            Self::Minute1 => Cow::Borrowed("1 minute"),
            Self::Minutes5 => Cow::Borrowed("5 minutes"),
            Self::Minutes15 => Cow::Borrowed("15 minutes"),
            Self::Minutes30 => Cow::Borrowed("30 minutes"),
            Self::Hour1 => Cow::Borrowed("1 hour"),
            Self::Hours4 => Cow::Borrowed("4 hours"),
            Self::Day1 => Cow::Borrowed("1 day"),
            Self::Custom(ms) => Cow::Owned(duration_label(ms)),
            Self::Ticks(1) => Cow::Borrowed("1 tick"),
            Self::Ticks(count) => Cow::Owned(format!("{count} ticks")),
        }
    }
}

/// A length written in the largest unit that holds it whole.
fn duration_label(ms: i64) -> String {
    let plural = |count: i64, unit: &str| {
        if count == 1 {
            format!("1 {unit}")
        } else {
            format!("{count} {unit}s")
        }
    };
    if ms % DAY_MS == 0 {
        plural(ms / DAY_MS, "day")
    } else if ms % HOUR_MS == 0 {
        plural(ms / HOUR_MS, "hour")
    } else if ms % MINUTE_MS == 0 {
        plural(ms / MINUTE_MS, "minute")
    } else if ms % SECOND_MS == 0 {
        plural(ms / SECOND_MS, "second")
    } else {
        format!("{ms} ms")
    }
}

/// Reads a step the user typed: `90s`, `90 seconds`, `7m`, `7 min`, `2h`, `3 days`, `50 ticks`.
/// `None` when it is not one, or out of range (a time from 1 second to 7 days, 1 to 10000 ticks).
#[must_use]
pub fn parse_step(text: &str) -> Option<ReplayStep> {
    let text = text.trim().to_ascii_lowercase();
    let split = text.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let (number, unit) = text.split_at(split);
    let unit = unit.trim();
    if matches!(unit, "t" | "tick" | "ticks") {
        let count: u32 = number.parse().ok()?;
        return (1..=MAX_TICKS)
            .contains(&count)
            .then_some(ReplayStep::Ticks(count));
    }
    let unit_ms = match unit {
        "s" | "sec" | "secs" | "second" | "seconds" => SECOND_MS,
        "m" | "min" | "mins" | "minute" | "minutes" => MINUTE_MS,
        "h" | "hr" | "hrs" | "hour" | "hours" => HOUR_MS,
        "d" | "day" | "days" => DAY_MS,
        _ => return None,
    };
    let value: f64 = number.parse().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    let ms = wyck_core::math::f64_to_i64((value * wyck_core::math::i64_to_f64(unit_ms)).round());
    (MIN_CUSTOM_MS..=MAX_CUSTOM_MS)
        .contains(&ms)
        .then_some(ReplayStep::Custom(ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_is_read_in_every_spelling() {
        for (text, ms) in [
            ("90s", 90_000),
            ("90 sec", 90_000),
            ("90 Seconds", 90_000),
            ("7m", 7 * MINUTE_MS),
            ("7 min", 7 * MINUTE_MS),
            ("  2 h ", 2 * HOUR_MS),
            ("2hours", 2 * HOUR_MS),
            ("3d", 3 * DAY_MS),
            ("3 days", 3 * DAY_MS),
            ("1.5h", 90 * MINUTE_MS),
            ("0.5 min", 30_000),
        ] {
            assert_eq!(parse_step(text), Some(ReplayStep::Custom(ms)), "{text}");
        }
    }

    #[test]
    fn a_step_in_ticks_is_read_too() {
        assert_eq!(parse_step("50 ticks"), Some(ReplayStep::Ticks(50)));
        assert_eq!(parse_step("1t"), Some(ReplayStep::Ticks(1)));
        assert_eq!(parse_step("1 tick"), Some(ReplayStep::Ticks(1)));
    }

    #[test]
    fn what_is_not_a_step_or_is_out_of_range_is_refused() {
        for text in [
            "",
            "s",
            "5",
            "five min",
            "-5m",
            "0s",
            "0 ticks",
            "10001 ticks",
            "2.5 ticks",
            "500ms",
            "8 days",
            "0.9s",
            "10 weeks",
            "1e3s",
        ] {
            assert_eq!(parse_step(text), None, "{text}");
        }
    }

    #[test]
    fn the_longest_and_shortest_steps_are_allowed() {
        assert_eq!(parse_step("1s"), Some(ReplayStep::Custom(SECOND_MS)));
        assert_eq!(parse_step("7d"), Some(ReplayStep::Custom(7 * DAY_MS)));
        assert_eq!(parse_step("10000 ticks"), Some(ReplayStep::Ticks(10_000)));
    }

    #[test]
    fn a_step_is_written_in_its_largest_whole_unit() {
        assert_eq!(ReplayStep::Custom(90_000).label(), "90 seconds");
        assert_eq!(ReplayStep::Custom(7 * MINUTE_MS).label(), "7 minutes");
        assert_eq!(ReplayStep::Custom(HOUR_MS).label(), "1 hour");
        assert_eq!(ReplayStep::Custom(2 * DAY_MS).label(), "2 days");
        assert_eq!(ReplayStep::Custom(1_500).label(), "1500 ms");
        assert_eq!(ReplayStep::Ticks(1).label(), "1 tick");
        assert_eq!(ReplayStep::Ticks(50).label(), "50 ticks");
    }

    #[test]
    fn a_menu_step_has_a_length_except_auto_and_ticks() {
        assert_eq!(ReplayStep::Auto.millis(), None);
        assert_eq!(ReplayStep::Ticks(5).millis(), None);
        assert!(
            ReplayStep::PRESETS[1..]
                .iter()
                .all(|s| s.millis().is_some())
        );
        assert_eq!(ReplayStep::Custom(7_000).millis(), Some(7_000));
        assert_eq!(ReplayStep::Ticks(5).ticks(), Some(5));
    }

    #[test]
    fn a_written_step_reads_back_as_itself() {
        for step in [
            ReplayStep::Custom(90_000),
            ReplayStep::Custom(2 * HOUR_MS),
            ReplayStep::Custom(3 * DAY_MS),
            ReplayStep::Ticks(50),
            ReplayStep::Ticks(1),
        ] {
            assert_eq!(parse_step(&step.label()), Some(step));
        }
    }
}
