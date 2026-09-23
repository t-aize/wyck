//! The timeframes the chart offers: tick by tick, seconds, and the server's own bar periods.
//!
//! The Open API only serves bars from one minute up. Anything shorter is built here from the tick
//! stream: `Ticks` plots every price change, `Seconds(n)` groups the ticks into bars of `n`
//! seconds.

use wyck::openapi::market::Period;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timeframe {
    /// One point per price change.
    Ticks,
    /// Bars of this many seconds, built from ticks.
    Seconds(u32),
    /// Bars the server keeps.
    Bars(Period),
}

/// The timeframes of the "more" menu, by section.
pub const GROUPS: [(&str, &[Timeframe]); 5] = [
    ("Ticks", &[Timeframe::Ticks]),
    (
        "Seconds",
        &[
            Timeframe::Seconds(1),
            Timeframe::Seconds(5),
            Timeframe::Seconds(10),
            Timeframe::Seconds(15),
            Timeframe::Seconds(30),
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
            Timeframe::Bars(Period::M30),
        ],
    ),
    (
        "Hours",
        &[
            Timeframe::Bars(Period::H1),
            Timeframe::Bars(Period::H4),
            Timeframe::Bars(Period::H12),
        ],
    ),
    (
        "Days and up",
        &[
            Timeframe::Bars(Period::D1),
            Timeframe::Bars(Period::W1),
            Timeframe::Bars(Period::MN1),
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

impl Timeframe {
    pub const DEFAULT: Self = Self::Bars(Period::M5);

    /// The short name: `Tick`, `15s`, `M5`, `H4`.
    pub fn label(self) -> String {
        match self {
            Self::Ticks => "Tick".to_owned(),
            Self::Seconds(n) => format!("{n}s"),
            Self::Bars(period) => period.label().to_owned(),
        }
    }

    /// The short, stable name written in saved files: `T`, `S15`, `M5`, `H4`, `D1`. It never
    /// changes, so a file saved by one version is read by the next.
    pub fn code(self) -> String {
        match self {
            Self::Ticks => "T".to_owned(),
            Self::Seconds(n) => format!("S{n}"),
            Self::Bars(period) => period.label().to_owned(),
        }
    }

    /// The timeframe a saved code stands for, if it is one this version offers.
    pub fn from_code(code: &str) -> Option<Self> {
        GROUPS
            .iter()
            .flat_map(|(_, items)| items.iter().copied())
            .find(|timeframe| timeframe.code() == code)
    }

    /// The name written out, for tooltips.
    pub fn name(self) -> String {
        match self {
            Self::Ticks => "Tick by tick".to_owned(),
            Self::Seconds(1) => "1 second".to_owned(),
            Self::Seconds(n) => format!("{n} seconds"),
            Self::Bars(period) => match period.minutes() {
                1 => "1 minute".to_owned(),
                m if m < 60 => format!("{m} minutes"),
                60 => "1 hour".to_owned(),
                m if m < 1_440 => format!("{} hours", m / 60),
                1_440 => "1 day".to_owned(),
                10_080 => "1 week".to_owned(),
                _ => "1 month".to_owned(),
            },
        }
    }

    /// The length of one bar in milliseconds, `None` for ticks.
    pub fn bar_ms(self) -> Option<i64> {
        match self {
            Self::Ticks => None,
            Self::Seconds(n) => Some(i64::from(n) * 1_000),
            Self::Bars(period) => Some(period.millis()),
        }
    }

    /// The server period, for the timeframes the server serves.
    pub fn period(self) -> Option<Period> {
        match self {
            Self::Bars(period) => Some(period),
            _ => None,
        }
    }

    /// Whether the bars of this timeframe are built here from ticks.
    pub fn is_tick_built(self) -> bool {
        !matches!(self, Self::Bars(_))
    }

    /// How far back the first load of a tick-built timeframe reaches, and the size of each older
    /// step after it.
    pub fn tick_span_ms(self) -> i64 {
        const MINUTE: i64 = 60_000;
        match self {
            Self::Ticks => 10 * MINUTE,
            Self::Seconds(1) => 60 * MINUTE,
            Self::Seconds(n) => i64::from(n) * 8 * MINUTE,
            Self::Bars(_) => 0,
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
        assert_eq!(Timeframe::from_code("M7"), None);
        assert_eq!(Timeframe::from_code(""), None);
        assert_eq!(Timeframe::from_code("S3"), None, "only the offered seconds");
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
        assert_eq!(Timeframe::Bars(Period::H4).name(), "4 hours");
        assert_eq!(Timeframe::Bars(Period::MN1).name(), "1 month");
    }
}
