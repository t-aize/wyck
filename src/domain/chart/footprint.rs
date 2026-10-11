//! The footprint chart type: its settings, and the analysis of one bar's flow (the rows it is cut
//! into, the point of control, the value area, the imbalances and the stacks of them).
//!
//! The drawing lives in [`super::scene`]; what is here is plain arithmetic on [`super::flow`]
//! data, so it is unit tested without a window.
//!
//! What the chart offers is what the order flow platforms (ATAS, Sierra Chart, Quantower,
//! TradingView's volume footprint) agree on:
//!
//! - cells as bid x ask, as delta or as total volume, with a heat color;
//! - rows of a chosen height, or picked for the zoom;
//! - the point of control and the value area of each bar;
//! - diagonal imbalances (the ask against the bid one row below, the bid against the ask one
//!   row above), with a ratio and a smallest volume, and stacks of them;
//! - the delta and the volume of each bar under it.

use serde::{Deserialize, Serialize};

use super::flow::Level;

/// What each cell of a bar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellMode {
    /// Sellers on the left, buyers on the right.
    #[default]
    BidAsk,
    /// Buyers minus sellers, one number a row.
    Delta,
    /// Buyers plus sellers, one number a row.
    Volume,
}

impl CellMode {
    pub const ALL: [Self; 3] = [Self::BidAsk, Self::Delta, Self::Volume];

    pub fn label(self) -> &'static str {
        match self {
            Self::BidAsk => "Bid x Ask",
            Self::Delta => "Delta",
            Self::Volume => "Volume",
        }
    }
}

/// What the intensity of a cell's color is compared with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeatScope {
    /// The busiest cell of the same bar: every bar shows its own shape.
    #[default]
    Bar,
    /// The busiest cell on screen: bars can be told apart by how busy they were.
    Visible,
}

impl HeatScope {
    pub const ALL: [Self; 2] = [Self::Bar, Self::Visible];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bar => "Each bar",
            Self::Visible => "Whole screen",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FootprintSettings {
    pub mode: CellMode,
    /// Rows of this many price steps (the last decimal of the symbol); 0 picks them for the zoom.
    pub row_steps: u32,
    /// Whether the cells are colored by how much traded.
    pub heat: bool,
    pub heat_scope: HeatScope,
    /// Whether the numbers show (they hide by themselves when they do not fit).
    pub numbers: bool,
    pub imbalance: bool,
    /// One side must be this many percent of the other (300 is three times).
    pub imbalance_percent: u32,
    /// The side that wins must have traded at least this much.
    pub imbalance_min: u32,
    /// Compare the ask with the bid one row below (and the bid with the ask one row above)
    /// rather than the two sides of the same row.
    pub diagonal: bool,
    pub stacked: bool,
    /// Rows in a row that make a stack.
    pub stack_rows: u32,
    /// Extend a stack to the right until the price comes back to it.
    pub project_stacks: bool,
    pub poc: bool,
    /// Extend the point of control to the right until the price comes back to it.
    pub extend_poc: bool,
    pub value_area: bool,
    /// The share of the volume the value area holds.
    pub value_area_percent: u32,
    /// The volume and the delta of each bar, under it.
    pub summary: bool,
}

impl Default for FootprintSettings {
    fn default() -> Self {
        Self {
            mode: CellMode::BidAsk,
            row_steps: 0,
            heat: true,
            heat_scope: HeatScope::Bar,
            numbers: true,
            imbalance: true,
            imbalance_percent: 300,
            imbalance_min: 3,
            diagonal: true,
            stacked: true,
            stack_rows: 3,
            project_stacks: true,
            poc: true,
            extend_poc: false,
            value_area: true,
            value_area_percent: 70,
            summary: true,
        }
    }
}

impl FootprintSettings {
    /// The settings repaired: every number in range.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.row_steps = self.row_steps.min(10_000);
        self.imbalance_percent = self.imbalance_percent.clamp(110, 2_000);
        self.imbalance_min = self.imbalance_min.clamp(1, 100_000);
        self.stack_rows = self.stack_rows.clamp(2, 12);
        self.value_area_percent = self.value_area_percent.clamp(50, 95);
        self
    }
}

/// Whether a footprint can be built for a timeframe: bars of a day or less. Ticks are single
/// prices, and longer bars would need weeks of quotes.
pub fn supports(timeframe: super::timeframe::Timeframe) -> bool {
    timeframe.bar_ms().is_some_and(|ms| ms <= 86_400_000)
}

/// Pixels a bar is drawn wide when a footprint is first shown: wide enough for its numbers.
pub const DEFAULT_BAR_PX: f64 = 110.0;

/// Rows never get shorter than this many pixels, whatever the settings ask.
pub const MIN_ROW_PX: f64 = 3.0;

/// The height of a row in raw price units. `step` is one price step (the last decimal of the
/// symbol) in raw units, `step_px` how many pixels a step is high on screen, and `want_px` the
/// least a row should be to hold a number. With `fixed` steps set that is what is used, unless
/// it would make rows too short to see.
pub fn row_size(step: i64, fixed: u32, step_px: f64, want_px: f64) -> i64 {
    let step = step.max(1);
    let step_px = if step_px.is_finite() && step_px > 0.0 {
        step_px
    } else {
        1.0
    };
    let steps_for = |px: f64| (px / step_px).ceil().max(1.0);
    if fixed > 0 && f64::from(fixed) * step_px >= MIN_ROW_PX {
        return step.saturating_mul(i64::from(fixed));
    }
    let need = if fixed > 0 {
        steps_for(MIN_ROW_PX)
    } else {
        steps_for(want_px)
    };
    step.saturating_mul(nice(need))
}

/// The whole number of steps a row is at least `n` steps high: exactly `n` while that is small,
/// then the next of 10, 20, 50, 100, 200... so the rows of a wide range stay round numbers.
fn nice(n: f64) -> i64 {
    if n <= 10.0 {
        return n.ceil().max(1.0) as i64;
    }
    let mut base = 1i64;
    loop {
        for m in [1, 2, 5] {
            let v = base.saturating_mul(m);
            if v as f64 >= n || v >= i64::MAX / 10 {
                return v;
            }
        }
        base = base.saturating_mul(10);
    }
}

/// One row of a bar: the levels that fall in it, added up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// The row number: the price divided by the row size, rounded down.
    pub index: i64,
    pub sell: u32,
    pub buy: u32,
}

impl Row {
    pub fn volume(&self) -> u64 {
        u64::from(self.sell) + u64::from(self.buy)
    }

    pub fn delta(&self) -> i64 {
        i64::from(self.buy) - i64::from(self.sell)
    }
}

/// A run of rows with an imbalance on the same side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stack {
    pub buy: bool,
    /// Positions in [`Analysis::rows`], both included.
    pub first: usize,
    pub last: usize,
}

/// One bar's flow cut into rows, with what stands out in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// The rows that traded, from the lowest price up.
    pub rows: Vec<Row>,
    /// The size of a row, raw price units.
    pub size: i64,
    /// The row with the most volume (the position in `rows`).
    pub poc: usize,
    /// The rows of the value area, both included.
    pub value_area: (usize, usize),
    pub sell: u64,
    pub buy: u64,
    /// Whether the buyers (or sellers) of the row at that position won an imbalance.
    pub buy_imbalance: Vec<bool>,
    pub sell_imbalance: Vec<bool>,
    pub stacks: Vec<Stack>,
    /// The busiest single number of a cell, of each side, and the largest delta or volume.
    pub max_side: u32,
    pub max_delta: u64,
    pub max_volume: u64,
}

impl Analysis {
    pub fn delta(&self) -> i64 {
        self.buy as i64 - self.sell as i64
    }

    pub fn volume(&self) -> u64 {
        self.sell + self.buy
    }
}

/// Cuts `levels` (from the lowest price up) into rows of `size` raw units and works out what
/// stands out. `None` when nothing traded.
pub fn analyze(levels: &[Level], size: i64, settings: &FootprintSettings) -> Option<Analysis> {
    let size = size.max(1);
    let mut rows: Vec<Row> = Vec::new();
    for level in levels {
        let index = level.price.div_euclid(size);
        match rows.last_mut() {
            Some(row) if row.index == index => {
                row.sell = row.sell.saturating_add(level.sell);
                row.buy = row.buy.saturating_add(level.buy);
            }
            _ => rows.push(Row {
                index,
                sell: level.sell,
                buy: level.buy,
            }),
        }
    }
    rows.retain(|r| r.volume() > 0);
    if rows.is_empty() {
        return None;
    }
    let sell: u64 = rows.iter().map(|r| u64::from(r.sell)).sum();
    let buy: u64 = rows.iter().map(|r| u64::from(r.buy)).sum();
    let poc = point_of_control(&rows);
    let value_area = value_area(&rows, poc, f64::from(settings.value_area_percent) / 100.0);
    let (buy_imbalance, sell_imbalance) = imbalances(&rows, settings);
    let stacks = if settings.stacked {
        stacks(&rows, &buy_imbalance, &sell_imbalance, settings.stack_rows)
    } else {
        Vec::new()
    };
    Some(Analysis {
        size,
        poc,
        value_area,
        sell,
        buy,
        buy_imbalance,
        sell_imbalance,
        stacks,
        max_side: rows.iter().map(|r| r.sell.max(r.buy)).max().unwrap_or(0),
        max_delta: rows
            .iter()
            .map(|r| r.delta().unsigned_abs())
            .max()
            .unwrap_or(0),
        max_volume: rows.iter().map(Row::volume).max().unwrap_or(0),
        rows,
    })
}

/// The row with the most volume; the first (lowest) when several tie.
fn point_of_control(rows: &[Row]) -> usize {
    let mut best = 0;
    for (i, row) in rows.iter().enumerate() {
        if row.volume() > rows[best].volume() {
            best = i;
        }
    }
    best
}

/// The rows around the point of control that hold `share` (0 to 1) of the volume. As in a market
/// profile, it grows by the pair of rows on one side or the other, whichever holds more, until
/// it holds enough.
fn value_area(rows: &[Row], poc: usize, share: f64) -> (usize, usize) {
    let total: u64 = rows.iter().map(Row::volume).sum();
    let target = (total as f64 * share.clamp(0.0, 1.0)).ceil() as u64;
    let (mut low, mut high) = (poc, poc);
    let mut held = rows[poc].volume();
    let pair = |from: usize, up: bool| -> (u64, usize) {
        // The next two rows (or the one left) in a direction, and how many of them there are.
        let mut sum = 0;
        let mut count = 0;
        for k in 1..=2 {
            let at = if up {
                from.checked_add(k)
            } else {
                from.checked_sub(k)
            };
            if let Some(row) = at.and_then(|i| rows.get(i)) {
                sum += row.volume();
                count += 1;
            }
        }
        (sum, count)
    };
    while held < target {
        let (up, up_n) = pair(high, true);
        let (down, down_n) = pair(low, false);
        match (up_n, down_n) {
            (0, 0) => break,
            (_, 0) => {
                high += up_n;
                held += up;
            }
            (0, _) => {
                low -= down_n;
                held += down;
            }
            _ if up >= down => {
                high += up_n;
                held += up;
            }
            _ => {
                low -= down_n;
                held += down;
            }
        }
    }
    (low, high)
}

/// Whether the winner beats the other side by the ratio, and traded at least the smallest.
fn wins(winner: u32, other: u32, settings: &FootprintSettings) -> bool {
    let ratio = f64::from(settings.imbalance_percent) / 100.0;
    winner >= settings.imbalance_min && f64::from(winner) >= ratio * f64::from(other)
}

/// For every row, whether the buyers won an imbalance and whether the sellers did.
///
/// Diagonally, the buyers of a row are compared with the sellers of the row below (they compete
/// for the same price), and the sellers with the buyers of the row above. The lowest row has
/// nothing below it and the highest nothing above, so the buyers of the first and the sellers of
/// the last never win one. Not diagonally, the two sides of the same row are compared.
fn imbalances(rows: &[Row], settings: &FootprintSettings) -> (Vec<bool>, Vec<bool>) {
    let n = rows.len();
    let mut buys = vec![false; n];
    let mut sells = vec![false; n];
    if !settings.imbalance {
        return (buys, sells);
    }
    let next_to = |i: usize, below: bool| -> Option<&Row> {
        let at = if below { i.checked_sub(1)? } else { i + 1 };
        let row = rows.get(at)?;
        let wanted = if below {
            rows[i].index - 1
        } else {
            rows[i].index + 1
        };
        (row.index == wanted).then_some(row)
    };
    for (i, row) in rows.iter().enumerate() {
        if settings.diagonal {
            // A gap counts as an empty row, except at the ends of the bar.
            let below = if i == 0 {
                None
            } else {
                Some(next_to(i, true).map_or(0, |r| r.sell))
            };
            let above = if i + 1 == n {
                None
            } else {
                Some(next_to(i, false).map_or(0, |r| r.buy))
            };
            buys[i] = below.is_some_and(|other| wins(row.buy, other, settings));
            sells[i] = above.is_some_and(|other| wins(row.sell, other, settings));
        } else {
            buys[i] = wins(row.buy, row.sell, settings);
            sells[i] = wins(row.sell, row.buy, settings);
        }
    }
    (buys, sells)
}

/// The runs of at least `min_rows` neighboring rows where the same side won imbalances.
fn stacks(rows: &[Row], buys: &[bool], sells: &[bool], min_rows: u32) -> Vec<Stack> {
    let mut out = Vec::new();
    for (flags, buy) in [(buys, true), (sells, false)] {
        let mut start: Option<usize> = None;
        for i in 0..=rows.len() {
            let on = i < rows.len()
                && flags[i]
                && start.is_none_or(|s| rows[i].index == rows[i - 1].index + 1 && s <= i);
            match (on, start) {
                (true, None) => start = Some(i),
                (true, Some(_)) => {}
                (false, Some(s)) => {
                    if (i - s) as u32 >= min_rows {
                        out.push(Stack {
                            buy,
                            first: s,
                            last: i - 1,
                        });
                    }
                    // A row that broke the run may start the next one.
                    start = (i < rows.len() && flags[i]).then_some(i);
                }
                (false, None) => {}
            }
        }
    }
    out.sort_by_key(|s| (s.first, s.last));
    out
}

/// How many bars of `ranges` (each a lowest and a highest raw price, in time order) pass before
/// the first one that reaches into `low..=high`: where the price came back to a level, so it
/// stops being worth showing to the right. `None` when none does.
pub fn first_touch(
    mut ranges: impl Iterator<Item = (i64, i64)>,
    low: i64,
    high: i64,
) -> Option<usize> {
    ranges.position(|(lo, hi)| hi >= low && lo <= high)
}

/// A volume as text: whole up to 9999, then `12.3k` and `1.2M`.
pub fn compact(value: u64) -> String {
    if value < 10_000 {
        value.to_string()
    } else if value < 1_000_000 {
        let k = value as f64 / 1_000.0;
        if k < 100.0 {
            format!("{k:.1}k")
        } else {
            format!("{k:.0}k")
        }
    } else {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    }
}

/// A signed volume as text: `+56`, `-1.2k`, `0`.
pub fn signed(value: i64) -> String {
    match value.cmp(&0) {
        std::cmp::Ordering::Greater => format!("+{}", compact(value.unsigned_abs())),
        std::cmp::Ordering::Less => format!("-{}", compact(value.unsigned_abs())),
        std::cmp::Ordering::Equal => "0".to_owned(),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/domain/chart/footprint.rs"]
mod tests;
