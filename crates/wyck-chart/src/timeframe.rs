//! The timeframes the chart offers: tick by tick, seconds, the server's own bar periods, and
//! multiples of them.
//!
//! The Open API only serves bars from one minute up, in fourteen periods. Anything shorter is
//! built here from the tick stream: `Ticks` plots every price change, `Seconds(n)` groups the
//! ticks into bars of `n` seconds. Any other length (45 minutes, 2 hours, 3 days) is built by
//! grouping the bars of the longest server period that divides it: `Multiple(H1, 2)` is two
//! hours made of one-hour bars.
//!
//! A timeframe is saved as a short code: `T`, `S15`, `M45`, `H2`, `D3`, `W2`, `MN3`. The same
//! length always gets the same code (120 minutes is `H2`), so a custom one typed twice is one.

use chrono::{Datelike, TimeZone, Utc};
use wyck_openapi_model::market::Period;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timeframe {
    /// One point per price change.
    Ticks,
    /// Bars of this many seconds, built from ticks.
    Seconds(u32),
    /// Bars the server keeps.
    Bars(Period),
    /// Bars of this many bars of a server period, grouped here. The count is at least 2.
    Multiple(Period, u32),
}

const DAY_MS: i64 = 86_400_000;
const WEEK_MS: i64 = 7 * DAY_MS;

/// The longest custom lengths: a day in seconds is far too many ticks, and a bar longer than
/// these is not a chart anyone reads.
const MAX_SECONDS: u32 = 300;
const MAX_DAYS: u32 = 365;
const MAX_WEEKS: u32 = 52;
const MAX_MONTHS: u32 = 24;

/// The timeframes of the "more" menu, by section.
pub const GROUPS: [(&str, &[Timeframe]); 5] = [
    ("Ticks", &[Timeframe::Ticks]),
    (
        "Seconds",
        &[
            Timeframe::Seconds(1),
            Timeframe::Seconds(2),
            Timeframe::Seconds(3),
            Timeframe::Seconds(5),
            Timeframe::Seconds(10),
            Timeframe::Seconds(15),
            Timeframe::Seconds(20),
            Timeframe::Seconds(30),
            Timeframe::Seconds(45),
        ],
    ),
    (
        "Minutes",
        &[
            Timeframe::Bars(Period::M1),
            Timeframe::Bars(Period::M2),
            Timeframe::Bars(Period::M3),
            Timeframe::Bars(Period::M4),
            Timeframe::Bars(Period::M5),
            Timeframe::Bars(Period::M10),
            Timeframe::Bars(Period::M15),
            Timeframe::Multiple(Period::M10, 2),
            Timeframe::Bars(Period::M30),
            Timeframe::Multiple(Period::M15, 3),
        ],
    ),
    (
        "Hours",
        &[
            Timeframe::Bars(Period::H1),
            Timeframe::Multiple(Period::H1, 2),
            Timeframe::Multiple(Period::H1, 3),
            Timeframe::Bars(Period::H4),
            Timeframe::Multiple(Period::H1, 6),
            Timeframe::Multiple(Period::H4, 2),
            Timeframe::Bars(Period::H12),
        ],
    ),
    (
        "Days and up",
        &[
            Timeframe::Bars(Period::D1),
            Timeframe::Multiple(Period::D1, 2),
            Timeframe::Multiple(Period::D1, 3),
            Timeframe::Bars(Period::W1),
            Timeframe::Multiple(Period::W1, 2),
            Timeframe::Bars(Period::MN1),
            Timeframe::Multiple(Period::MN1, 3),
            Timeframe::Multiple(Period::MN1, 6),
            Timeframe::Multiple(Period::MN1, 12),
        ],
    ),
];

/// The ones the header shows as buttons.
pub const QUICK: [Timeframe; 7] = [
    Timeframe::Ticks,
    Timeframe::Bars(Period::M1),
    Timeframe::Bars(Period::M5),
    Timeframe::Bars(Period::M15),
    Timeframe::Bars(Period::H1),
    Timeframe::Bars(Period::H4),
    Timeframe::Bars(Period::D1),
];

/// The unit a custom timeframe is typed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Seconds,
    Minutes,
    Hours,
    Days,
    Weeks,
    Months,
}

impl Unit {
    pub const ALL: [Self; 6] = [
        Self::Seconds,
        Self::Minutes,
        Self::Hours,
        Self::Days,
        Self::Weeks,
        Self::Months,
    ];

    /// The letter a code is written with: `s`, `m`, `h`, `D`, `W`, `M`.
    pub fn short(self) -> &'static str {
        match self {
            Self::Seconds => "s",
            Self::Minutes => "m",
            Self::Hours => "h",
            Self::Days => "D",
            Self::Weeks => "W",
            Self::Months => "M",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Seconds => "Seconds",
            Self::Minutes => "Minutes",
            Self::Hours => "Hours",
            Self::Days => "Days",
            Self::Weeks => "Weeks",
            Self::Months => "Months",
        }
    }
}

/// The server periods up to half a day, which a length within a day is grouped from.
const INTRADAY: [Period; 11] = [
    Period::M1,
    Period::M2,
    Period::M3,
    Period::M4,
    Period::M5,
    Period::M10,
    Period::M15,
    Period::M30,
    Period::H1,
    Period::H4,
    Period::H12,
];

impl Timeframe {
    pub const DEFAULT: Self = Self::Bars(Period::M5);

    /// The timeframe of `count` units, or `None` when there is none that long: zero, more
    /// than the longest offered, or minutes past a day that are not whole days.
    pub fn of(count: u32, unit: Unit) -> Option<Self> {
        if count == 0 {
            return None;
        }
        match unit {
            Unit::Seconds if count.is_multiple_of(60) => Self::of(count / 60, Unit::Minutes),
            Unit::Seconds => (count <= MAX_SECONDS).then_some(Self::Seconds(count)),
            Unit::Minutes => {
                let minutes = i64::from(count);
                if minutes % 1_440 == 0 {
                    return Self::of(count / 1_440, Unit::Days);
                }
                if minutes > 1_440 {
                    return None;
                }
                // The longest server period that divides it; M1 always does.
                let base = INTRADAY
                    .iter()
                    .rev()
                    .copied()
                    .find(|p| minutes % p.minutes() == 0)?;
                Some(Self::grouped(
                    base,
                    u32::try_from(minutes / base.minutes()).ok()?,
                ))
            }
            Unit::Hours => Self::of(count.checked_mul(60)?, Unit::Minutes),
            Unit::Days if count.is_multiple_of(7) => Self::of(count / 7, Unit::Weeks),
            Unit::Days => (count <= MAX_DAYS).then(|| Self::grouped(Period::D1, count)),
            Unit::Weeks => (count <= MAX_WEEKS).then(|| Self::grouped(Period::W1, count)),
            Unit::Months => (count <= MAX_MONTHS).then(|| Self::grouped(Period::MN1, count)),
        }
    }

    fn grouped(base: Period, count: u32) -> Self {
        if count == 1 {
            Self::Bars(base)
        } else {
            Self::Multiple(base, count)
        }
    }

    /// The short name: `Tick`, `15s`, `M5`, `H4`, `M45`.
    pub fn label(self) -> String {
        match self {
            Self::Ticks => "Tick".to_owned(),
            Self::Seconds(n) => format!("{n}s"),
            _ => self.code(),
        }
    }

    /// The short, stable name written in saved files: `T`, `S15`, `M5`, `H4`, `D1`, `H2`. It
    /// never changes, so a file saved by one version is read by the next.
    pub fn code(self) -> String {
        match self {
            Self::Ticks => "T".to_owned(),
            Self::Seconds(n) => format!("S{n}"),
            Self::Bars(period) => period.label().to_owned(),
            Self::Multiple(base, n) => match base {
                Period::D1 => format!("D{n}"),
                Period::W1 => format!("W{n}"),
                Period::MN1 => format!("MN{n}"),
                _ => {
                    let minutes = base.minutes() * i64::from(n);
                    if minutes % 60 == 0 {
                        format!("H{}", minutes / 60)
                    } else {
                        format!("M{minutes}")
                    }
                }
            },
        }
    }

    /// The timeframe a code stands for: a saved one, or one typed by hand (`45m`, `2h`, `3D`
    /// and `90s` are read too).
    pub fn from_code(code: &str) -> Option<Self> {
        let code = code.trim();
        if code == "T" {
            return Some(Self::Ticks);
        }
        let upper = code.to_ascii_uppercase();
        // `MN3` before `M3`; a trailing unit (`45m`, `2h`) as well as a leading one. A lower
        // case trailing `m` is minutes and an upper case one months, as traders write them.
        let (unit, digits) = if let Some(rest) = upper.strip_prefix("MN") {
            (Unit::Months, rest.to_owned())
        } else if let Some(rest) = upper.strip_suffix("MN") {
            (Unit::Months, rest.to_owned())
        } else if code.ends_with('M') && code.len() > 1 && !code.starts_with('M') {
            (Unit::Months, upper[..upper.len() - 1].to_owned())
        } else {
            let lead = upper.chars().next()?;
            let tail = upper.chars().last()?;
            let unit_of = |c: char| match c {
                'S' => Some(Unit::Seconds),
                'M' => Some(Unit::Minutes),
                'H' => Some(Unit::Hours),
                'D' => Some(Unit::Days),
                'W' => Some(Unit::Weeks),
                _ => None,
            };
            if let Some(unit) = unit_of(lead) {
                (unit, upper[1..].to_owned())
            } else {
                (unit_of(tail)?, upper[..upper.len() - 1].to_owned())
            }
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Self::of(digits.parse().ok()?, unit)
    }

    /// Whether the menu offers it without it being added.
    pub fn is_builtin(self) -> bool {
        GROUPS.iter().any(|(_, items)| items.contains(&self))
    }

    /// The name written out, for tooltips.
    pub fn name(self) -> String {
        let plural = |n: i64, unit: &str| {
            if n == 1 {
                format!("1 {unit}")
            } else {
                format!("{n} {unit}s")
            }
        };
        match self {
            Self::Ticks => "Tick by tick".to_owned(),
            Self::Seconds(n) => plural(i64::from(n), "second"),
            Self::Bars(Period::W1) => "1 week".to_owned(),
            Self::Bars(Period::MN1) => "1 month".to_owned(),
            Self::Multiple(Period::D1, n) => plural(i64::from(n), "day"),
            Self::Multiple(Period::W1, n) => plural(i64::from(n), "week"),
            Self::Multiple(Period::MN1, n) => plural(i64::from(n), "month"),
            Self::Bars(_) | Self::Multiple(..) => {
                let minutes = self.bar_ms().unwrap_or(0) / 60_000;
                if minutes % 1_440 == 0 {
                    plural(minutes / 1_440, "day")
                } else if minutes % 60 == 0 {
                    plural(minutes / 60, "hour")
                } else {
                    plural(minutes, "minute")
                }
            }
        }
    }

    /// Sorts timeframes shortest first, ticks before everything.
    pub fn sort_key(self) -> i64 {
        self.bar_ms().unwrap_or(0)
    }

    /// The length of one bar in milliseconds, `None` for ticks.
    pub fn bar_ms(self) -> Option<i64> {
        match self {
            Self::Ticks => None,
            Self::Seconds(n) => Some(i64::from(n) * 1_000),
            Self::Bars(period) => Some(period.millis()),
            Self::Multiple(period, n) => Some(period.millis() * i64::from(n)),
        }
    }

    /// The server period the bars come from: their own, or the one they are grouped from.
    pub fn period(self) -> Option<Period> {
        match self {
            Self::Bars(period) | Self::Multiple(period, _) => Some(period),
            _ => None,
        }
    }

    /// Which bar of a grouped timeframe a server bar opening at `time_ms` belongs to: the same
    /// key for the bars of one group, a greater one for a later group. A length within a day
    /// starts again each day (UTC), as the server's own periods do.
    pub fn group_key(self, time_ms: i64) -> i64 {
        let Self::Multiple(base, n) = self else {
            return time_ms;
        };
        let n = i64::from(n);
        match base {
            Period::D1 => time_ms.div_euclid(DAY_MS * n),
            Period::W1 => time_ms.div_euclid(WEEK_MS * n),
            Period::MN1 => {
                let date = Utc
                    .timestamp_millis_opt(time_ms)
                    .single()
                    .unwrap_or_default();
                (i64::from(date.year()) * 12 + i64::from(date.month0())).div_euclid(n)
            }
            _ => {
                let span = base.millis() * n;
                let day = time_ms.div_euclid(DAY_MS);
                day * (DAY_MS / span + 1) + (time_ms - day * DAY_MS) / span
            }
        }
    }

    /// When the group holding a server bar that opens at `time_ms` opens, or `None` to use the
    /// time of its first bar (weeks and months, whose server bars open at the broker's own hour).
    pub fn group_open(self, time_ms: i64) -> Option<i64> {
        let Self::Multiple(base, n) = self else {
            return Some(time_ms);
        };
        let n = i64::from(n);
        match base {
            Period::W1 | Period::MN1 => None,
            Period::D1 => Some(time_ms - time_ms.rem_euclid(DAY_MS * n)),
            _ => {
                let span = base.millis() * n;
                let day = time_ms - time_ms.rem_euclid(DAY_MS);
                Some(day + (time_ms - day) / span * span)
            }
        }
    }

    /// How many server bars make one bar: 1 but for a grouped timeframe.
    pub fn group_size(self) -> u32 {
        match self {
            Self::Multiple(_, n) => n,
            _ => 1,
        }
    }

    /// Whether the bars of this timeframe are built here from ticks.
    pub fn is_tick_built(self) -> bool {
        matches!(self, Self::Ticks | Self::Seconds(_))
    }

    /// How far back the first load of a tick-built timeframe reaches, and the size of each older
    /// step after it.
    pub fn tick_span_ms(self) -> i64 {
        const MINUTE: i64 = 60_000;
        match self {
            Self::Ticks => 10 * MINUTE,
            Self::Seconds(1) => 60 * MINUTE,
            Self::Seconds(n) => i64::from(n) * 8 * MINUTE,
            Self::Bars(_) | Self::Multiple(..) => 0,
        }
    }

    /// Pixels per bar when the timeframe is first shown.
    pub fn default_bar_px(self) -> f64 {
        match self {
            Self::Ticks => 3.0,
            _ => 8.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_entry_has_a_unique_label() {
        let mut labels: Vec<String> = GROUPS
            .iter()
            .flat_map(|(_, items)| items.iter().map(|tf| tf.label()))
            .collect();
        let total = labels.len();
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), total);
    }

    #[test]
    fn every_timeframe_survives_being_saved_and_read_back() {
        for (_, items) in GROUPS {
            for timeframe in items.iter().copied() {
                assert_eq!(Timeframe::from_code(&timeframe.code()), Some(timeframe));
            }
        }
        assert_eq!(
            Timeframe::from_code("M7"),
            Some(Timeframe::Multiple(Period::M1, 7))
        );
        assert_eq!(Timeframe::from_code(""), None);
        assert_eq!(Timeframe::from_code("X9"), None);
        assert_eq!(Timeframe::from_code("S7"), Some(Timeframe::Seconds(7)));
    }

    #[test]
    fn a_length_is_grouped_from_the_longest_period_that_divides_it() {
        let code = |text: &str| Timeframe::from_code(text).map(Timeframe::code);
        assert_eq!(
            Timeframe::from_code("45m"),
            Some(Timeframe::Multiple(Period::M15, 3))
        );
        assert_eq!(
            Timeframe::from_code("H8"),
            Some(Timeframe::Multiple(Period::H4, 2))
        );
        assert_eq!(
            Timeframe::from_code("H6"),
            Some(Timeframe::Multiple(Period::H1, 6))
        );
        // The same length is always written the same way.
        assert_eq!(code("120m").as_deref(), Some("H2"));
        assert_eq!(code("2h").as_deref(), Some("H2"));
        assert_eq!(code("S60").as_deref(), Some("M1"));
        assert_eq!(code("90s").as_deref(), Some("S90"));
        assert_eq!(code("H24").as_deref(), Some("D1"));
        assert_eq!(code("D7").as_deref(), Some("W1"));
        assert_eq!(code("3D").as_deref(), Some("D3"));
        assert_eq!(code("3M").as_deref(), Some("MN3"));
        assert_eq!(code("MN12").as_deref(), Some("MN12"));
        // Nothing, too long, or minutes past a day that are not whole days.
        assert_eq!(code("0h"), None);
        assert_eq!(code("M1500"), None);
        assert_eq!(code("D400"), None);
        assert_eq!(code("S1000"), None);
        assert_eq!(code("H"), None);
        assert_eq!(Timeframe::from_code("H2").unwrap().name(), "2 hours");
        assert_eq!(Timeframe::from_code("M45").unwrap().name(), "45 minutes");
        assert_eq!(Timeframe::from_code("W2").unwrap().name(), "2 weeks");
    }

    #[test]
    fn grouped_bars_start_again_each_day() {
        const H: i64 = 3_600_000;
        let day = 20_000 * 24 * H;
        let m45 = Timeframe::from_code("M45").unwrap();
        // 00:00, 00:15 and 00:30 are one bar; 00:45 starts the next.
        let first = m45.group_key(day);
        assert_eq!(m45.group_key(day + H / 4), first);
        assert_eq!(m45.group_key(day + H / 2), first);
        assert!(m45.group_key(day + 3 * H / 4) > first);
        assert_eq!(m45.group_open(day + H / 2), Some(day));
        // H5 does not divide a day: the last bar is short and the next day starts afresh.
        let h5 = Timeframe::from_code("H5").unwrap();
        assert_eq!(h5.group_open(day + 23 * H), Some(day + 20 * H));
        assert_eq!(h5.group_open(day + 24 * H), Some(day + 24 * H));
        assert!(h5.group_key(day + 24 * H) > h5.group_key(day + 23 * H));
        // Months are counted on the calendar.
        let quarter = Timeframe::from_code("MN3").unwrap();
        let jan = chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mar = chrono::Utc.with_ymd_and_hms(2026, 3, 31, 0, 0, 0).unwrap();
        let apr = chrono::Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap();
        let key = |d: chrono::DateTime<chrono::Utc>| quarter.group_key(d.timestamp_millis());
        assert_eq!(key(jan), key(mar));
        assert!(key(apr) > key(mar));
        assert_eq!(quarter.group_open(0), None);
    }

    #[test]
    fn the_saved_codes_are_the_ones_written_in_files_today() {
        // These are a file format: changing one would orphan every saved layout.
        assert_eq!(Timeframe::Ticks.code(), "T");
        assert_eq!(Timeframe::Seconds(15).code(), "S15");
        assert_eq!(Timeframe::Bars(Period::M5).code(), "M5");
        assert_eq!(Timeframe::Bars(Period::MN1).code(), "MN1");
    }

    #[test]
    fn quick_timeframes_are_in_the_menu() {
        for quick in QUICK {
            assert!(
                GROUPS.iter().any(|(_, items)| items.contains(&quick)),
                "{quick:?}"
            );
        }
    }

    #[test]
    fn bar_lengths_and_sources() {
        assert_eq!(Timeframe::Ticks.bar_ms(), None);
        assert_eq!(Timeframe::Seconds(5).bar_ms(), Some(5_000));
        assert_eq!(Timeframe::Bars(Period::H1).bar_ms(), Some(3_600_000));
        assert!(Timeframe::Seconds(1).is_tick_built());
        assert!(!Timeframe::Bars(Period::M1).is_tick_built());
        assert!(!Timeframe::Multiple(Period::H1, 2).is_tick_built());
        assert_eq!(Timeframe::Multiple(Period::H1, 2).bar_ms(), Some(7_200_000));
        assert_eq!(Timeframe::Bars(Period::H4).name(), "4 hours");
        assert_eq!(Timeframe::Bars(Period::MN1).name(), "1 month");
    }
}
