//! Where the axis labels go and what they say.
//!
//! Price labels sit on round numbers (1, 2 and 5 times a power of ten). Time labels are placed on
//! the bars where a round unit of time changes (a new minute, hour, day, month), so they read
//! well however the chart is zoomed, and they stay correct across weekend gaps because they look
//! at the times of the bars, not at their positions.

use time::OffsetDateTime;

const SECOND: i64 = 1_000;
const MINUTE: i64 = 60 * SECOND;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// A step of the time axis.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Unit {
    Fixed(i64),
    Months(i64),
    Years(i64),
}

impl Unit {
    /// The length it stands for, for choosing between steps.
    fn nominal_ms(self) -> i64 {
        match self {
            Self::Fixed(ms) => ms,
            Self::Months(n) => n * 30 * DAY,
            Self::Years(n) => n * 365 * DAY,
        }
    }
}

const LADDER: [Unit; 28] = [
    Unit::Fixed(SECOND),
    Unit::Fixed(2 * SECOND),
    Unit::Fixed(5 * SECOND),
    Unit::Fixed(10 * SECOND),
    Unit::Fixed(15 * SECOND),
    Unit::Fixed(30 * SECOND),
    Unit::Fixed(MINUTE),
    Unit::Fixed(2 * MINUTE),
    Unit::Fixed(5 * MINUTE),
    Unit::Fixed(10 * MINUTE),
    Unit::Fixed(15 * MINUTE),
    Unit::Fixed(30 * MINUTE),
    Unit::Fixed(HOUR),
    Unit::Fixed(2 * HOUR),
    Unit::Fixed(3 * HOUR),
    Unit::Fixed(4 * HOUR),
    Unit::Fixed(6 * HOUR),
    Unit::Fixed(12 * HOUR),
    Unit::Fixed(DAY),
    Unit::Fixed(2 * DAY),
    Unit::Fixed(7 * DAY),
    Unit::Months(1),
    Unit::Months(2),
    Unit::Months(3),
    Unit::Months(6),
    Unit::Years(1),
    Unit::Years(2),
    Unit::Years(5),
];

/// A label of the time axis.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeLabel {
    /// Which point it belongs to.
    pub index: usize,
    pub text: String,
    /// Whether it names a bigger unit than its neighbours (a new day among hours).
    pub major: bool,
}

/// A step for a price axis that shows about `target` labels over `range`.
pub fn nice_step(range: f64, target: usize) -> f64 {
    if !(range.is_finite() && range > 0.0) {
        return 1.0;
    }
    let raw = range / target.max(1) as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let n = raw / magnitude;
    let multiple = if n <= 1.0 {
        1.0
    } else if n <= 2.0 {
        2.0
    } else if n <= 5.0 {
        5.0
    } else {
        10.0
    };
    multiple * magnitude
}

/// The values to label between `lo` and `hi`. `min_step` is the smallest step that still means
/// something for the symbol (one unit of its last decimal).
pub fn price_ticks(lo: f64, hi: f64, target: usize, min_step: f64) -> Vec<f64> {
    if !(lo.is_finite() && hi.is_finite() && hi > lo) {
        return Vec::new();
    }
    let step = nice_step(hi - lo, target).max(min_step.max(f64::MIN_POSITIVE));
    let mut value = (lo / step).ceil() * step;
    let mut ticks = Vec::new();
    while value <= hi && ticks.len() < 200 {
        ticks.push(value);
        value += step;
    }
    ticks
}

struct Civil {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    millisecond: u16,
}

fn civil(time_ms: i64) -> Option<Civil> {
    let seconds = time_ms.div_euclid(1_000);
    let dt = OffsetDateTime::from_unix_timestamp(seconds).ok()?;
    Some(Civil {
        year: dt.year(),
        month: dt.month() as u8,
        day: dt.day(),
        hour: dt.hour(),
        minute: dt.minute(),
        second: dt.second(),
        millisecond: time_ms.rem_euclid(1_000) as u16,
    })
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn month_name(month: u8) -> &'static str {
    MONTHS[usize::from(month.clamp(1, 12)) - 1]
}

fn bucket(unit: Unit, time_ms: i64) -> i64 {
    match unit {
        Unit::Fixed(ms) => time_ms.div_euclid(ms),
        Unit::Months(n) => civil(time_ms)
            .map(|c| (i64::from(c.year) * 12 + i64::from(c.month) - 1).div_euclid(n))
            .unwrap_or(0),
        Unit::Years(n) => civil(time_ms)
            .map(|c| i64::from(c.year).div_euclid(n))
            .unwrap_or(0),
    }
}

/// Picks the labels of the time axis for the points `first..last`.
///
/// `time_at` gives the time of a point, `x_of` its x, and `min_gap` is the least space between
/// two labels in pixels.
pub fn time_labels(
    first: usize,
    last: usize,
    time_at: impl Fn(usize) -> Option<i64>,
    x_of: impl Fn(usize) -> f64,
    plot_w: f64,
    min_gap: f64,
) -> Vec<TimeLabel> {
    let (Some(t0), Some(t1)) = (time_at(first), last.checked_sub(1).and_then(&time_at)) else {
        return Vec::new();
    };
    let slots = (plot_w / min_gap).floor().max(1.0);
    let wanted = ((t1 - t0).max(1) as f64 / slots) as i64;
    let unit = LADDER
        .iter()
        .copied()
        .find(|u| u.nominal_ms() >= wanted)
        .unwrap_or(Unit::Years(10));

    let mut labels: Vec<TimeLabel> = Vec::new();
    let mut previous_bucket = first
        .checked_sub(1)
        .and_then(&time_at)
        .map(|t| bucket(unit, t));
    let mut previous_time: Option<i64> = first.checked_sub(1).and_then(&time_at);
    let mut last_x = f64::NEG_INFINITY;
    for index in first..last {
        let Some(time) = time_at(index) else { continue };
        let key = bucket(unit, time);
        let starts = previous_bucket.is_none_or(|p| p != key);
        let before = previous_time;
        previous_bucket = Some(key);
        previous_time = Some(time);
        if !starts {
            continue;
        }
        let x = x_of(index);
        if x - last_x < min_gap || x < 0.0 || x > plot_w {
            continue;
        }
        last_x = x;
        let (text, major) = label_text(unit, time, before);
        labels.push(TimeLabel { index, text, major });
    }
    labels
}

/// The text of a label and whether it is a major one, given the time of the point before it.
fn label_text(unit: Unit, time: i64, before: Option<i64>) -> (String, bool) {
    let Some(c) = civil(time) else {
        return (String::new(), false);
    };
    let earlier = before.and_then(civil);
    let new_day = earlier
        .as_ref()
        .is_none_or(|e| (e.year, e.month, e.day) != (c.year, c.month, c.day));
    let new_year = earlier.as_ref().is_none_or(|e| e.year != c.year);
    let new_month = earlier
        .as_ref()
        .is_none_or(|e| (e.year, e.month) != (c.year, c.month));
    match unit {
        Unit::Fixed(ms) if ms < DAY => {
            if new_day {
                (format!("{} {}", c.day, month_name(c.month)), true)
            } else if ms < MINUTE {
                (
                    format!("{:02}:{:02}:{:02}", c.hour, c.minute, c.second),
                    false,
                )
            } else {
                (format!("{:02}:{:02}", c.hour, c.minute), false)
            }
        }
        Unit::Fixed(_) => {
            if new_year {
                (c.year.to_string(), true)
            } else if new_month {
                (month_name(c.month).to_owned(), true)
            } else {
                (c.day.to_string(), false)
            }
        }
        Unit::Months(_) => {
            if new_year {
                (c.year.to_string(), true)
            } else {
                (month_name(c.month).to_owned(), false)
            }
        }
        Unit::Years(_) => (c.year.to_string(), true),
    }
}

/// The full date and time of a point, for the crosshair. UTC.
pub fn full_time(time_ms: i64, with_seconds: bool, with_millis: bool) -> String {
    let Some(c) = civil(time_ms) else {
        return String::new();
    };
    let weekday = OffsetDateTime::from_unix_timestamp(time_ms.div_euclid(1_000))
        .map(|dt| dt.weekday().to_string())
        .unwrap_or_default();
    let weekday: String = weekday.chars().take(3).collect();
    let mut text = format!(
        "{weekday} {} {} {}  {:02}:{:02}",
        c.day,
        month_name(c.month),
        c.year,
        c.hour,
        c.minute
    );
    if with_seconds || with_millis {
        text.push_str(&format!(":{:02}", c.second));
    }
    if with_millis {
        text.push_str(&format!(".{:03}", c.millisecond));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_one_two_or_five_times_a_power_of_ten() {
        for range in [0.0003, 0.07, 1.0, 13.0, 4_500.0, 9e7] {
            let step = nice_step(range, 6);
            let mantissa = step / 10f64.powf(step.log10().floor());
            assert!(
                [1.0, 2.0, 5.0].iter().any(|m| (mantissa - m).abs() < 1e-9)
                    || (mantissa - 10.0).abs() < 1e-9,
                "{range} -> {step}"
            );
        }
        assert_eq!(nice_step(-1.0, 5), 1.0);
        assert_eq!(nice_step(f64::NAN, 5), 1.0);
    }

    #[test]
    fn price_ticks_land_inside_the_range_on_round_numbers() {
        let ticks = price_ticks(108_412.0, 108_988.0, 6, 1.0);
        assert!(!ticks.is_empty() && ticks.len() <= 10);
        assert!(ticks.iter().all(|t| *t >= 108_412.0 && *t <= 108_988.0));
        let step = ticks[1] - ticks[0];
        assert!(
            ticks
                .windows(2)
                .all(|w| ((w[1] - w[0]) - step).abs() < 1e-6)
        );
    }

    #[test]
    fn price_ticks_never_go_below_the_quote_step() {
        let ticks = price_ticks(100.0, 100.4, 8, 1.0);
        assert!(ticks.windows(2).all(|w| w[1] - w[0] >= 1.0 - 1e-9));
        assert!(price_ticks(5.0, 5.0, 8, 1.0).is_empty());
        assert!(price_ticks(f64::NAN, 5.0, 8, 1.0).is_empty());
    }

    fn minutes(range: std::ops::Range<i64>) -> Vec<i64> {
        // From 2026-01-05 00:00 UTC, one bar a minute.
        range.map(|m| 1_767_571_200_000 + m * MINUTE).collect()
    }

    #[test]
    fn hourly_labels_appear_on_the_hour_and_keep_their_spacing() {
        let times = minutes(0..600);
        let labels = time_labels(
            0,
            times.len(),
            |i| times.get(i).copied(),
            |i| i as f64 * 2.0,
            1_200.0,
            80.0,
        );
        assert!(labels.len() >= 3, "{labels:?}");
        for pair in labels.windows(2) {
            assert!(pair[1].index - pair[0].index >= 40);
        }
        // Every label after the first sits on a whole unit boundary.
        for label in labels.iter().skip(1) {
            assert_eq!(label.index % 10, 0, "{label:?}");
        }
        assert!(labels[0].major, "the first label names the day");
    }

    #[test]
    fn a_new_day_is_a_major_label() {
        let times = minutes(0..3_000);
        let labels = time_labels(
            0,
            times.len(),
            |i| times.get(i).copied(),
            |i| i as f64 * 0.5,
            1_500.0,
            90.0,
        );
        let majors: Vec<_> = labels.iter().filter(|l| l.major).collect();
        assert!(majors.len() >= 2, "{labels:?}");
        assert!(majors.iter().any(|l| l.text == "6 Jan"), "{majors:?}");
    }

    #[test]
    fn no_points_no_labels() {
        assert!(time_labels(0, 0, |_| None, |_| 0.0, 500.0, 80.0).is_empty());
    }

    #[test]
    fn the_crosshair_time_is_readable() {
        let text = full_time(1_767_571_200_000 + 61_500, true, true);
        assert_eq!(text, "Mon 5 Jan 2026  00:01:01.500");
        assert_eq!(
            full_time(1_767_571_200_000, false, false),
            "Mon 5 Jan 2026  00:00"
        );
    }

    #[test]
    fn times_before_the_epoch_do_not_panic() {
        assert!(!full_time(-86_400_000, false, false).is_empty());
    }
}
