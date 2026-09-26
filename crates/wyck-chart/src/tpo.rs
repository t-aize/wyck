//! Time price opportunity charts, also called market profiles.
//!
//! The chart is cut into sessions (a day, a week, a month, or a few hours). Inside a session the
//! time is cut into periods, each one a letter (A, B, C, ...), and the prices into rows. Every
//! period marks the rows its bars covered, so a row holds as many marks as the periods that
//! traded there, and the sessions read as a shape: where the market spent its time is wide, where
//! it only passed through is thin.
//!
//! From that shape [`Profile`] works out what traders read from it:
//!
//! - the **point of control**: the row with the most marks (the one nearest the middle of the
//!   range when several tie);
//! - the **value area**: the rows around it that hold a share of all marks (70 percent by
//!   default). It grows from the point of control by the textbook rule: the two rows above are
//!   compared with the two below, and the side with more marks is added;
//! - the **initial balance**: the range of the first periods of the session;
//! - **single prints**: runs of rows only one period visited, the ones at the ends of the
//!   profile being its tails;
//! - **poor highs and lows**: an end of the profile with more than one mark, where the market
//!   did not reject the price;
//! - where the session opened and closed.
//!
//! Each profile is one element of the series the chart draws (its open, high, low, close and time
//! are the session's), so the time axis, the crosshair and the drawings work like they do for the
//! other chart types that lay out their own elements.

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use wyck_openapi_model::market::Bar;

use super::zone::Zone;

/// Most profiles produced, like the bar limit of the chart.
const MAX_PROFILES: usize = 120_000;
/// Most rows a profile has; past it the rows get taller instead.
const MAX_ROWS: i64 = 800;
/// How many rows a profile aims for when the row height is automatic.
const AUTO_ROWS: i64 = 40;
/// The most letters in a period sequence before it starts over.
pub const LETTERS: usize = 52;

/// How the chart is cut into sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    #[default]
    Day,
    /// From Monday.
    Week,
    Month,
    /// A set number of hours, see [`TpoSettings::session_hours`].
    Hours,
}

impl SessionKind {
    pub const ALL: [Self; 4] = [Self::Day, Self::Week, Self::Month, Self::Hours];

    pub fn label(self) -> &'static str {
        match self {
            Self::Day => "Day",
            Self::Week => "Week",
            Self::Month => "Month",
            Self::Hours => "Hours",
        }
    }
}

/// How each mark is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TpoDisplay {
    /// Letters when the rows are tall enough to hold them, blocks otherwise.
    #[default]
    Auto,
    Letters,
    Blocks,
}

impl TpoDisplay {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Letters, Self::Blocks];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Letters => "Letters",
            Self::Blocks => "Blocks",
        }
    }
}

/// What decides the color of a mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TpoColor {
    /// One color for all.
    #[default]
    Single,
    /// Each period has a color of its own.
    ByPeriod,
    /// Fainter for the early periods of the session, solid for the late ones.
    ByTime,
    /// Solid where the row is busy, faint where it is not.
    ByCount,
}

impl TpoColor {
    pub const ALL: [Self; 4] = [Self::Single, Self::ByPeriod, Self::ByTime, Self::ByCount];

    pub fn label(self) -> &'static str {
        match self {
            Self::Single => "One color",
            Self::ByPeriod => "By period",
            Self::ByTime => "By time",
            Self::ByCount => "By activity",
        }
    }
}

/// The settings of the TPO chart type. Saved with the chart.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TpoSettings {
    #[serde(default)]
    pub session: SessionKind,
    /// The length of a session when it is a number of hours.
    #[serde(default = "default_session_hours")]
    pub session_hours: u32,
    /// When a session starts, in minutes after midnight in the time zone of the chart.
    #[serde(default)]
    pub session_start: u32,
    /// The length of a period, one letter, in minutes. Never shorter than a bar of the chart.
    #[serde(default = "default_period")]
    pub period_minutes: u32,
    /// The height of a row in quote units; 0 picks one that gives about forty rows a session.
    #[serde(default)]
    pub row_units: u32,
    #[serde(default)]
    pub display: TpoDisplay,
    #[serde(default)]
    pub color: TpoColor,
    #[serde(default = "yes")]
    pub value_area: bool,
    /// The share of the marks the value area holds, in percent.
    #[serde(default = "default_value_area")]
    pub value_area_percent: u32,
    #[serde(default = "yes")]
    pub poc: bool,
    /// A line across the profile at the point of control.
    #[serde(default = "yes")]
    pub poc_line: bool,
    #[serde(default)]
    pub midpoint: bool,
    #[serde(default = "yes")]
    pub initial_balance: bool,
    /// How many periods the initial balance lasts.
    #[serde(default = "default_ib")]
    pub ib_periods: u32,
    #[serde(default = "yes")]
    pub single_prints: bool,
    /// The fewest rows in a row of single prints for it to count.
    #[serde(default = "default_singles")]
    pub single_min_rows: u32,
    /// Mark the ends of a profile that did not reject the price.
    #[serde(default = "yes")]
    pub poor_extremes: bool,
    /// Where the session opened and closed.
    #[serde(default = "yes")]
    pub open_close: bool,
    /// The prices of the point of control and of the value area, written beside the profile.
    #[serde(default = "yes")]
    pub labels: bool,
}

fn yes() -> bool {
    true
}

fn default_session_hours() -> u32 {
    4
}

fn default_period() -> u32 {
    30
}

fn default_value_area() -> u32 {
    70
}

fn default_ib() -> u32 {
    2
}

fn default_singles() -> u32 {
    2
}

impl Default for TpoSettings {
    fn default() -> Self {
        Self {
            session: SessionKind::Day,
            session_hours: default_session_hours(),
            session_start: 0,
            period_minutes: default_period(),
            row_units: 0,
            display: TpoDisplay::Auto,
            color: TpoColor::Single,
            value_area: true,
            value_area_percent: default_value_area(),
            poc: true,
            poc_line: true,
            midpoint: false,
            initial_balance: true,
            ib_periods: default_ib(),
            single_prints: true,
            single_min_rows: default_singles(),
            poor_extremes: true,
            open_close: true,
            labels: true,
        }
    }
}

impl TpoSettings {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.session_hours = self.session_hours.clamp(1, 168);
        self.session_start = self.session_start.min(24 * 60 - 1);
        self.period_minutes = self.period_minutes.clamp(1, 24 * 60);
        self.row_units = self.row_units.min(1_000_000);
        self.value_area_percent = self.value_area_percent.clamp(50, 95);
        self.ib_periods = self.ib_periods.clamp(1, 24);
        self.single_min_rows = self.single_min_rows.clamp(1, 20);
        self
    }

    /// The session a time falls in, as a number that only changes from one session to the next.
    fn session_key(&self, zone: Zone, time_ms: i64) -> i64 {
        let t = time_ms - i64::from(self.session_start) * 60_000;
        match self.session {
            SessionKind::Day => zone.day(t),
            // The epoch began on a Thursday: three days later is the first Monday's week.
            SessionKind::Week => (zone.day(t) + 3).div_euclid(7),
            SessionKind::Month => {
                let shifted = zone.shift(t);
                DateTime::from_timestamp_millis(shifted).map_or(0, |d| {
                    use chrono::Datelike;
                    i64::from(d.year()) * 12 + i64::from(d.month0())
                })
            }
            SessionKind::Hours => {
                let span = i64::from(self.session_hours) * 3_600_000;
                zone.shift(t).div_euclid(span)
            }
        }
    }
}

/// A run of rows that only one period visited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinglePrint {
    /// The rows, counted from the lowest of the profile.
    pub first: usize,
    pub last: usize,
    /// Whether it is at the top or the bottom of the profile: an excess, not a gap.
    pub tail: bool,
}

/// One session laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// When the first bar of the session opened, in Unix milliseconds.
    pub start_ms: i64,
    /// The height of a row, in raw price units.
    pub row_size: i64,
    /// The lowest row, as a multiple of `row_size`: its price range starts at `low_row * row_size`.
    pub low_row: i64,
    /// For each row from the lowest, the periods that marked it, in order.
    pub rows: Vec<Vec<u16>>,
    /// How many periods the session lasted, counting the ones with no bar.
    pub periods: usize,
    pub open: i64,
    pub high: i64,
    pub low: i64,
    pub close: i64,
    pub volume: i64,
    /// The row of the point of control.
    pub poc: usize,
    /// The first and last row of the value area.
    pub value_area: (usize, usize),
    /// The range of the first periods, when there is one.
    pub initial_balance: Option<(i64, i64)>,
    pub singles: Vec<SinglePrint>,
    pub poor_high: bool,
    pub poor_low: bool,
    /// The rows of the open and of the close.
    pub open_row: usize,
    pub close_row: usize,
    /// How many marks the profile has.
    pub marks: usize,
}

impl Profile {
    /// The raw price at the bottom of row `row`.
    pub fn row_bottom(&self, row: usize) -> i64 {
        (self.low_row + row as i64) * self.row_size
    }

    /// The raw price at the top of row `row`.
    pub fn row_top(&self, row: usize) -> i64 {
        self.row_bottom(row) + self.row_size
    }

    /// The most marks in one row.
    pub fn widest(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    /// The row a price falls in, kept inside the profile.
    fn row_of(&self, price: i64) -> usize {
        let row = price.div_euclid(self.row_size) - self.low_row;
        usize::try_from(row.max(0))
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1))
    }

    /// The price in the middle of the range of the session.
    pub fn midpoint(&self) -> i64 {
        (self.high + self.low) / 2
    }
}

/// A profile as the one element of the series the chart draws.
pub fn element(profile: &Profile) -> Bar {
    Bar {
        time_ms: profile.start_ms,
        open: profile.open,
        high: profile.high,
        low: profile.low,
        close: profile.close,
        volume: profile.volume,
    }
}

/// The row with the most marks; among equals the one nearest the middle of the profile, then the
/// lowest.
fn point_of_control(rows: &[Vec<u16>]) -> usize {
    let most = rows.iter().map(Vec::len).max().unwrap_or(0);
    let last = rows.len().saturating_sub(1);
    (0..rows.len())
        .filter(|&r| rows[r].len() == most)
        // Twice the distance from the middle, to stay in whole numbers.
        .min_by_key(|&r| ((2 * r).abs_diff(last), r))
        .unwrap_or(0)
}

/// The first and last row of the value area: from the point of control, the two rows above are
/// compared with the two below and the side with more marks is taken, until `percent` of all the
/// marks are held. Equal sides go up.
fn value_area(rows: &[Vec<u16>], poc: usize, percent: u32) -> (usize, usize) {
    let total: usize = rows.iter().map(Vec::len).sum();
    let target = (total * percent as usize).div_ceil(100);
    let count = |r: usize| rows.get(r).map_or(0, Vec::len);
    let (mut low, mut high) = (poc, poc);
    let mut held = count(poc);
    while held < target {
        let can_up = high + 1 < rows.len();
        let can_down = low > 0;
        if !can_up && !can_down {
            break;
        }
        // The up to two rows on each side, and how many marks they hold.
        let (up_rows, down_rows) = ((rows.len() - 1 - high).min(2), low.min(2));
        let up: usize = (1..=up_rows).map(|i| count(high + i)).sum();
        let down: usize = (1..=down_rows).map(|i| count(low - i)).sum();
        if can_up && (!can_down || up >= down) {
            held += up;
            high += up_rows;
        } else {
            held += down;
            low -= down_rows;
        }
    }
    (low, high)
}

/// The runs of rows with a single mark that are at least `min_rows` long.
fn single_prints(rows: &[Vec<u16>], min_rows: usize) -> Vec<SinglePrint> {
    let mut out = Vec::new();
    let mut r = 0;
    while r < rows.len() {
        if rows[r].len() != 1 {
            r += 1;
            continue;
        }
        let first = r;
        while r + 1 < rows.len() && rows[r + 1].len() == 1 {
            r += 1;
        }
        if r + 1 - first >= min_rows {
            out.push(SinglePrint {
                first,
                last: r,
                tail: first == 0 || r == rows.len() - 1,
            });
        }
        r += 1;
    }
    out
}

/// The smallest of 1, 2 and 5 times a power of ten that is at least `value`.
fn nice_ceiling(value: f64) -> i64 {
    if !(value.is_finite() && value > 1.0) {
        return 1;
    }
    let scale = 10f64.powf(value.log10().floor());
    for step in [1.0, 2.0, 5.0, 10.0] {
        if scale * step >= value {
            return (scale * step) as i64;
        }
    }
    (scale * 10.0) as i64
}

/// The height of a row when it is automatic: about [`AUTO_ROWS`] rows for the usual session.
fn auto_row_units(ranges: &mut [i64], unit: i64) -> i64 {
    if ranges.is_empty() {
        return 1;
    }
    ranges.sort_unstable();
    let typical = ranges[ranges.len() / 2].max(unit);
    nice_ceiling(typical as f64 / unit as f64 / AUTO_ROWS as f64)
}

/// The smallest gap between two bars, which is the length of one, in milliseconds.
fn bar_span(bars: &[Bar]) -> i64 {
    bars.windows(2)
        .take(2_000)
        .map(|w| w[1].time_ms - w[0].time_ms)
        .filter(|gap| *gap > 0)
        .min()
        .unwrap_or(60_000)
}

/// Lays out the sessions of `bars`. `unit` is one quote unit of the symbol in raw price units.
pub fn build(bars: &[Bar], settings: &TpoSettings, zone: Zone, unit: i64) -> Vec<Profile> {
    let unit = unit.max(1);
    let mut sessions: Vec<&[Bar]> = Vec::new();
    let mut start = 0;
    for i in 1..=bars.len() {
        let ends = i == bars.len()
            || settings.session_key(zone, bars[i].time_ms)
                != settings.session_key(zone, bars[start].time_ms);
        if ends {
            sessions.push(&bars[start..i]);
            start = i;
            if sessions.len() >= MAX_PROFILES {
                break;
            }
        }
    }
    let period_ms = (i64::from(settings.period_minutes) * 60_000).max(bar_span(bars));
    let mut ranges: Vec<i64> = sessions
        .iter()
        .map(|s| {
            let high = s.iter().map(|b| b.high).max().unwrap_or(0);
            let low = s.iter().map(|b| b.low).min().unwrap_or(0);
            high - low
        })
        .collect();
    let row_units = if settings.row_units > 0 {
        i64::from(settings.row_units)
    } else {
        auto_row_units(&mut ranges, unit)
    };
    sessions
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(|s| profile(s, settings, period_ms, row_units * unit, unit))
        .collect()
}

/// One session, from its bars (in order, never none).
fn profile(
    bars: &[Bar],
    settings: &TpoSettings,
    period_ms: i64,
    wanted_row: i64,
    unit: i64,
) -> Profile {
    let high = bars.iter().map(|b| b.high).max().unwrap_or(0);
    let low = bars.iter().map(|b| b.low).min().unwrap_or(0);
    // Taller rows for a session so wide it would have too many.
    let mut row_size = wanted_row.max(unit);
    if (high - low) / row_size >= MAX_ROWS {
        row_size = ((high - low) / MAX_ROWS / unit + 1) * unit;
    }
    let low_row = low.div_euclid(row_size);
    let top_row = high.div_euclid(row_size);
    let mut rows: Vec<Vec<u16>> =
        vec![Vec::new(); usize::try_from(top_row - low_row + 1).unwrap_or(1)];
    let first_time = bars[0].time_ms;
    let grid = first_time.div_euclid(period_ms) * period_ms;
    let mut periods = 0;
    let (mut ib_high, mut ib_low) = (i64::MIN, i64::MAX);
    let ib_periods = settings.ib_periods as usize;
    for bar in bars {
        let period = usize::try_from((bar.time_ms - grid) / period_ms).unwrap_or(0);
        periods = periods.max(period + 1);
        let mark = (period % LETTERS) as u16;
        let from = usize::try_from(bar.low.div_euclid(row_size) - low_row).unwrap_or(0);
        let to = usize::try_from(bar.high.div_euclid(row_size) - low_row)
            .unwrap_or(0)
            .min(rows.len() - 1);
        for row in &mut rows[from.min(to)..=to] {
            // Bars come in order, so a period is only ever the last one to have marked a row.
            if row.last() != Some(&mark) {
                row.push(mark);
            }
        }
        if period < ib_periods {
            ib_high = ib_high.max(bar.high);
            ib_low = ib_low.min(bar.low);
        }
    }
    let poc = point_of_control(&rows);
    let mut profile = Profile {
        start_ms: first_time,
        row_size,
        low_row,
        periods,
        open: bars[0].open,
        high,
        low,
        close: bars[bars.len() - 1].close,
        volume: bars.iter().map(|b| b.volume).sum(),
        poc,
        value_area: value_area(&rows, poc, settings.value_area_percent),
        initial_balance: (ib_low <= ib_high).then_some((ib_low, ib_high)),
        singles: single_prints(&rows, settings.single_min_rows as usize),
        poor_high: rows.last().is_some_and(|r| r.len() > 1),
        poor_low: rows.first().is_some_and(|r| r.len() > 1),
        open_row: 0,
        close_row: 0,
        marks: rows.iter().map(Vec::len).sum(),
        rows,
    };
    profile.open_row = profile.row_of(profile.open);
    profile.close_row = profile.row_of(profile.close);
    profile
}

/// A time of day as `09:30`, from minutes after midnight.
pub fn format_clock(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60 % 24, minutes % 60)
}

/// Minutes after midnight from a time of day: `9:30`, `09:30`, `9` (an hour) or `930`.
pub fn parse_clock(text: &str) -> Option<u32> {
    let text = text.trim();
    let (hours, minutes) = match text.split_once(':') {
        Some((h, m)) => (h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().ok()?),
        None => {
            let n = text.parse::<u32>().ok()?;
            if text.len() > 2 {
                (n / 100, n % 100)
            } else {
                (n, 0)
            }
        }
    };
    (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
}

/// The letter of a period: A to Z, then a to z, then over again.
pub fn letter(mark: u16) -> char {
    let m = usize::from(mark) % LETTERS;
    if m < 26 {
        char::from(b'A' + m as u8)
    } else {
        char::from(b'a' + (m - 26) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;
    /// Monday 2026-01-05 00:00 UTC.
    const MONDAY: i64 = 1_767_571_200_000;

    fn bar(time_ms: i64, low: i64, high: i64) -> Bar {
        Bar {
            time_ms,
            open: low,
            high,
            low,
            close: high,
            volume: 1,
        }
    }

    fn settings() -> TpoSettings {
        TpoSettings {
            row_units: 1,
            ..TpoSettings::default()
        }
    }

    /// One bar of a minute per period of thirty, so a bar is a letter.
    fn letters(prices: &[(i64, i64)]) -> Vec<Bar> {
        prices
            .iter()
            .enumerate()
            .map(|(i, (low, high))| bar(MONDAY + i as i64 * 30 * MIN, *low, *high))
            .collect()
    }

    fn utc(bars: &[Bar], settings: &TpoSettings) -> Vec<Profile> {
        build(bars, settings, Zone::Utc, 1)
    }

    #[test]
    fn each_period_marks_the_rows_its_bars_covered() {
        let bars = letters(&[(10, 12), (11, 13), (12, 12)]);
        let profile = &utc(&bars, &settings())[0];
        assert_eq!(profile.periods, 3);
        assert_eq!(profile.low_row, 10);
        // Rows 10 to 13: A covers 10-12, B 11-13, C only 12.
        assert_eq!(
            profile.rows,
            vec![vec![0], vec![0, 1], vec![0, 1, 2], vec![1]]
        );
        assert_eq!(profile.marks, 7);
        assert_eq!((profile.low, profile.high), (10, 13));
    }

    #[test]
    fn several_bars_of_one_period_mark_a_row_once() {
        let bars = vec![
            bar(MONDAY, 10, 12),
            bar(MONDAY + 10 * MIN, 10, 12),
            bar(MONDAY + 20 * MIN, 11, 11),
        ];
        let profile = &utc(&bars, &settings())[0];
        assert_eq!(profile.periods, 1);
        assert!(
            profile.rows.iter().all(|r| r == &vec![0]),
            "{:?}",
            profile.rows
        );
    }

    #[test]
    fn a_period_is_never_shorter_than_a_bar() {
        // Hour bars with a thirty minute period: a bar is a letter, not two.
        let bars: Vec<Bar> = (0..4).map(|i| bar(MONDAY + i * 60 * MIN, 10, 11)).collect();
        let profile = &utc(&bars, &settings())[0];
        assert_eq!(profile.periods, 4);
        assert_eq!(profile.rows[0], vec![0, 1, 2, 3]);
    }

    #[test]
    fn the_point_of_control_is_the_busiest_row_and_ties_go_to_the_middle() {
        let rows = vec![vec![0], vec![0, 1], vec![0, 1, 2], vec![0, 1], vec![0]];
        assert_eq!(point_of_control(&rows), 2);
        // Two rows tie at the top: the one nearer the middle of the profile wins.
        let tie = vec![vec![0, 1], vec![0], vec![0], vec![0, 1], vec![0], vec![0]];
        assert_eq!(point_of_control(&tie), 3);
        // Equally near: the lower.
        let even = vec![vec![0], vec![0, 1], vec![0, 1], vec![0]];
        assert_eq!(point_of_control(&even), 1);
    }

    #[test]
    fn the_value_area_grows_by_pairs_from_the_point_of_control() {
        // Counts 1, 2, 5, 3, 1: total 12, 70% needs 9. From the 5, the two rows above
        // (3 + 1) beat the two below (2 + 1), so 5 + 3 + 1 = 9 is enough.
        let rows: Vec<Vec<u16>> = [1usize, 2, 5, 3, 1]
            .iter()
            .map(|n| (0..*n as u16).collect())
            .collect();
        assert_eq!(point_of_control(&rows), 2);
        assert_eq!(value_area(&rows, 2, 70), (2, 4));
        // All of it.
        assert_eq!(value_area(&rows, 2, 95), (0, 4));
        // Equal sides go up.
        let flat: Vec<Vec<u16>> = [2usize, 2, 4, 2, 2]
            .iter()
            .map(|n| (0..*n as u16).collect())
            .collect();
        assert_eq!(value_area(&flat, 2, 70).1, 4);
    }

    #[test]
    fn the_value_area_stays_inside_the_profile() {
        let one: Vec<Vec<u16>> = vec![vec![0, 1, 2]];
        assert_eq!(value_area(&one, 0, 70), (0, 0));
        let two: Vec<Vec<u16>> = vec![vec![0], vec![0, 1, 2, 3]];
        let (low, high) = value_area(&two, 1, 95);
        assert!(low <= 1 && high <= 1);
        // Near the bottom, with only one row below.
        let near: Vec<Vec<u16>> = vec![vec![0], vec![0, 1, 2], vec![0], vec![0], vec![0]];
        let (low, high) = value_area(&near, 1, 95);
        assert_eq!(low, 0);
        assert!(high <= 4);
    }

    #[test]
    fn single_prints_are_runs_of_one_mark_and_the_ends_are_tails() {
        let rows: Vec<Vec<u16>> = [1usize, 1, 3, 2, 1, 1, 1, 4, 1]
            .iter()
            .map(|n| (0..*n as u16).collect())
            .collect();
        let found = single_prints(&rows, 2);
        assert_eq!(
            found,
            vec![
                SinglePrint {
                    first: 0,
                    last: 1,
                    tail: true
                },
                SinglePrint {
                    first: 4,
                    last: 6,
                    tail: false
                },
            ]
        );
        // A single row does not count when two are asked for, and does with one.
        assert_eq!(single_prints(&rows, 1).len(), 3);
        assert!(single_prints(&rows, 4).is_empty());
    }

    #[test]
    fn poor_ends_are_the_ones_more_than_one_period_touched() {
        let bars = letters(&[(10, 14), (10, 13), (11, 14)]);
        let profile = &utc(&bars, &settings())[0];
        // The high 14 was touched by A and C, the low 10 by A and B.
        assert!(profile.poor_high);
        assert!(profile.poor_low);
        let clean = letters(&[(10, 14), (11, 13), (12, 13)]);
        let profile = &utc(&clean, &settings())[0];
        assert!(!profile.poor_high);
        assert!(!profile.poor_low);
    }

    #[test]
    fn the_initial_balance_is_the_range_of_the_first_periods() {
        let bars = letters(&[(10, 12), (11, 15), (9, 20), (30, 31)]);
        let profile = &utc(&bars, &settings())[0];
        assert_eq!(profile.initial_balance, Some((10, 15)));
        let one = TpoSettings {
            ib_periods: 1,
            ..settings()
        };
        assert_eq!(utc(&bars, &one)[0].initial_balance, Some((10, 12)));
    }

    #[test]
    fn open_and_close_are_where_the_session_started_and_ended() {
        let bars = vec![
            Bar {
                open: 12,
                close: 10,
                ..bar(MONDAY, 10, 14)
            },
            Bar {
                open: 11,
                close: 13,
                ..bar(MONDAY + 30 * MIN, 10, 14)
            },
        ];
        let profile = &utc(&bars, &settings())[0];
        assert_eq!(profile.open, 12);
        assert_eq!(profile.close, 13);
        assert_eq!(profile.open_row, 2);
        assert_eq!(profile.close_row, 3);
    }

    #[test]
    fn a_new_day_is_a_new_profile_and_the_hours_make_shorter_ones() {
        let mut bars = Vec::new();
        for i in 0..96 {
            bars.push(bar(MONDAY + i * 30 * MIN, 10, 12));
        }
        assert_eq!(utc(&bars, &settings()).len(), 2);
        let hours = TpoSettings {
            session: SessionKind::Hours,
            session_hours: 12,
            ..settings()
        };
        assert_eq!(utc(&bars, &hours).len(), 4);
    }

    #[test]
    fn weeks_start_on_monday_and_months_on_the_first() {
        let day = 86_400_000;
        let week = TpoSettings {
            session: SessionKind::Week,
            ..settings()
        };
        // Sunday and Monday are different weeks; Monday to Sunday is one.
        let sunday = MONDAY - day;
        let bars = vec![
            bar(sunday, 1, 2),
            bar(MONDAY, 1, 2),
            bar(MONDAY + 6 * day, 1, 2),
            bar(MONDAY + 7 * day, 1, 2),
        ];
        assert_eq!(utc(&bars, &week).len(), 3);
        let month = TpoSettings {
            session: SessionKind::Month,
            ..settings()
        };
        // January and February of 2026.
        let bars = vec![
            bar(MONDAY, 1, 2),
            bar(MONDAY + 20 * day, 1, 2),
            bar(MONDAY + 30 * day, 1, 2),
        ];
        assert_eq!(utc(&bars, &month).len(), 2);
    }

    #[test]
    fn a_session_can_start_at_another_time_of_day() {
        // Two bars at 21:00 and 23:00 UTC: one day, or two when the day starts at 22:00.
        let bars = vec![
            bar(MONDAY + 21 * 60 * MIN, 1, 2),
            bar(MONDAY + 23 * 60 * MIN, 1, 2),
        ];
        assert_eq!(utc(&bars, &settings()).len(), 1);
        let late = TpoSettings {
            session_start: 22 * 60,
            ..settings()
        };
        assert_eq!(utc(&bars, &late).len(), 2);
    }

    #[test]
    fn the_rows_are_taller_when_asked_and_when_the_session_is_too_wide() {
        let bars = letters(&[(0, 100)]);
        let coarse = TpoSettings {
            row_units: 10,
            ..settings()
        };
        let profile = &utc(&bars, &coarse)[0];
        assert_eq!(profile.row_size, 10);
        assert_eq!(profile.rows.len(), 11);
        let wide = letters(&[(0, 100_000)]);
        let profile = &utc(&wide, &settings())[0];
        assert!(
            profile.rows.len() as i64 <= MAX_ROWS + 1,
            "{}",
            profile.rows.len()
        );
        assert!(profile.row_size > 1);
    }

    #[test]
    fn an_automatic_row_gives_about_forty_rows_and_a_round_height() {
        let bars = letters(&[(0, 2_000), (0, 2_000)]);
        let auto = TpoSettings::default();
        let profile = &utc(&bars, &auto)[0];
        assert!(profile.row_size >= 50, "{}", profile.row_size);
        assert!((profile.rows.len() as i64) < 60, "{}", profile.rows.len());
        assert_eq!(nice_ceiling(0.3), 1);
        assert_eq!(nice_ceiling(3.0), 5);
        assert_eq!(nice_ceiling(51.0), 100);
        assert_eq!(nice_ceiling(f64::NAN), 1);
    }

    #[test]
    fn a_profile_is_one_element_of_the_series() {
        let bars = letters(&[(10, 20), (12, 25)]);
        let profile = &utc(&bars, &settings())[0];
        let e = element(profile);
        assert_eq!((e.time_ms, e.low, e.high), (MONDAY, 10, 25));
        assert_eq!(e.volume, 2);
        assert_eq!(e.close, bars[1].close);
    }

    #[test]
    fn a_time_of_day_is_read_and_written_plainly() {
        assert_eq!(parse_clock("09:30"), Some(570));
        assert_eq!(parse_clock(" 9:30 "), Some(570));
        assert_eq!(parse_clock("22"), Some(1320));
        assert_eq!(parse_clock("930"), Some(570));
        assert_eq!(parse_clock("0"), Some(0));
        assert_eq!(parse_clock("24:00"), None);
        assert_eq!(parse_clock("12:75"), None);
        assert_eq!(parse_clock("noon"), None);
        assert_eq!(parse_clock(""), None);
        assert_eq!(format_clock(570), "09:30");
        assert_eq!(format_clock(0), "00:00");
        assert_eq!(parse_clock(&format_clock(1_439)), Some(1_439));
    }

    #[test]
    fn periods_are_lettered_and_wrap() {
        assert_eq!(letter(0), 'A');
        assert_eq!(letter(25), 'Z');
        assert_eq!(letter(26), 'a');
        assert_eq!(letter(51), 'z');
        assert_eq!(letter(52), 'A');
    }

    #[test]
    fn nothing_in_gives_nothing_out() {
        assert!(utc(&[], &settings()).is_empty());
    }

    #[test]
    fn the_settings_are_repaired_and_saved() {
        let broken = TpoSettings {
            session_hours: 0,
            session_start: 99_999,
            period_minutes: 0,
            value_area_percent: 5,
            ib_periods: 0,
            single_min_rows: 0,
            ..TpoSettings::default()
        }
        .normalized();
        assert_eq!(broken.session_hours, 1);
        assert_eq!(broken.session_start, 24 * 60 - 1);
        assert_eq!(broken.period_minutes, 1);
        assert_eq!(broken.value_area_percent, 50);
        assert_eq!(broken.ib_periods, 1);
        assert_eq!(broken.single_min_rows, 1);
        let text = toml::to_string(&broken).unwrap();
        assert_eq!(toml::from_str::<TpoSettings>(&text).unwrap(), broken);
        assert_eq!(
            toml::from_str::<TpoSettings>("").unwrap(),
            TpoSettings::default(),
            "a file from before the chart type reads with the defaults"
        );
    }
}
