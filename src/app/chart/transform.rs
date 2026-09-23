//! Chart types that are not drawn one bar per period: Renko, line break, Kagi, point and figure
//! and range bars. Each one filters the prices it is given into its own sequence of bricks, lines,
//! columns or bars, and the chart then shows that sequence one after the other, like bars.
//!
//! Every construction reads the closes (a tick's price, for tick by tick) except range bars, which
//! walk each bar from its open through its low and high to its close, the path the price most
//! likely took. They are the textbook constructions:
//!
//! - **Renko**: a brick every time the price moves a full box past the last brick; turning back
//!   takes two boxes (one past the other end of the last brick).
//! - **Line break**: a new line when the close goes past the highest high (or lowest low) of the
//!   last `n` lines; three lines by default.
//! - **Kagi**: a line that follows the price and turns only after it moves back by the reversal
//!   amount. It thickens (yang) above the last shoulder and thins (yin) below the last waist.
//! - **Point and figure**: columns of X (rising) and O (falling) boxes; a column turns after the
//!   price moves `reversal` boxes the other way, starting one box off the extreme.
//! - **Range bars**: every bar spans exactly the range asked for, however long it takes.
//!
//! Each output element keeps the time of the price that made it, so drawings anchored to times
//! still land near the right place and the time axis still reads.

use serde::{Deserialize, Serialize};
use wyck::openapi::market::Bar;

use super::study::math;

/// Most elements a construction produces, like the bar limit of the chart.
const MAX_OUT: usize = 120_000;

/// How a box (or a reversal, or a range) is sized.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum BoxSize {
    /// The average true range of the last bars, over this many bars. It adapts to the symbol.
    Atr { length: u32 },
    /// A fixed amount of price, in the symbol's own units (1.2345, not raw integers).
    Fixed { price: f64 },
    /// A share of the last price, in percent.
    Percent { percent: f64 },
}

impl Default for BoxSize {
    fn default() -> Self {
        Self::Atr { length: 14 }
    }
}

impl BoxSize {
    /// The size in raw price units for `bars`, never below one quote unit.
    pub fn resolve(self, bars: &[Bar], unit: i64) -> i64 {
        let unit = unit.max(1);
        let raw = match self {
            Self::Atr { length } => {
                let high: Vec<f64> = bars.iter().map(|b| b.high as f64).collect();
                let low: Vec<f64> = bars.iter().map(|b| b.low as f64).collect();
                let close: Vec<f64> = bars.iter().map(|b| b.close as f64).collect();
                math::atr(&high, &low, &close, length.max(1) as usize)
                    .last()
                    .copied()
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .unwrap_or_else(|| {
                        // Too few bars for the average: a share of the whole range instead.
                        let (lo, hi) = bars
                            .iter()
                            .fold((f64::MAX, f64::MIN), |(l, h), b| {
                                (l.min(b.low as f64), h.max(b.high as f64))
                            });
                        ((hi - lo) / 20.0).max(unit as f64)
                    })
            }
            Self::Fixed { price } => price * wyck::openapi::market::PRICE_SCALE as f64,
            Self::Percent { percent } => {
                bars.last().map_or(0.0, |b| b.close as f64) * percent / 100.0
            }
        };
        if !raw.is_finite() {
            return unit;
        }
        // A round number of quote units.
        (((raw / unit as f64).round() as i64).max(1)) * unit
    }
}

/// The settings of the non-time chart types. Saved with the chart.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TransformSettings {
    #[serde(default)]
    pub renko_box: BoxSize,
    #[serde(default = "default_lines")]
    pub line_break: u32,
    #[serde(default)]
    pub kagi_reversal: BoxSize,
    #[serde(default)]
    pub pnf_box: BoxSize,
    #[serde(default = "default_reversal")]
    pub pnf_reversal: u32,
    #[serde(default)]
    pub range: BoxSize,
}

fn default_lines() -> u32 {
    3
}

fn default_reversal() -> u32 {
    3
}

impl Default for TransformSettings {
    fn default() -> Self {
        Self {
            renko_box: BoxSize::default(),
            line_break: default_lines(),
            kagi_reversal: BoxSize::default(),
            pnf_box: BoxSize::default(),
            pnf_reversal: default_reversal(),
            range: BoxSize::default(),
        }
    }
}

impl TransformSettings {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.line_break = self.line_break.clamp(1, 10);
        self.pnf_reversal = self.pnf_reversal.clamp(1, 10);
        for size in [
            &mut self.renko_box,
            &mut self.kagi_reversal,
            &mut self.pnf_box,
            &mut self.range,
        ] {
            *size = match *size {
                BoxSize::Atr { length } => BoxSize::Atr {
                    length: length.clamp(1, 500),
                },
                BoxSize::Fixed { price } if price.is_finite() && price > 0.0 => *size,
                BoxSize::Percent { percent } if percent.is_finite() && percent > 0.0 => {
                    BoxSize::Percent {
                        percent: percent.min(50.0),
                    }
                }
                _ => BoxSize::default(),
            };
        }
        self
    }
}

/// A Kagi line: how thick it starts and where (if anywhere) it changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KagiLine {
    /// Whether the line starts thick (yang).
    pub yang: bool,
    /// The price where it turns thick or thin on the way.
    pub switch_at: Option<i64>,
}

/// A point and figure column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PnfColumn {
    /// A column of X (rising) or of O.
    pub up: bool,
    /// The lowest and highest box, as price levels in box units: box `k` stands for the price
    /// `k * size`, and is drawn centered on it.
    pub bottom: i64,
    pub top: i64,
    pub size: i64,
}

/// What a construction made: bars to lay out, and what the drawing needs beyond them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Derived {
    pub bars: Vec<Bar>,
    pub kagi: Vec<KagiLine>,
    pub pnf: Vec<PnfColumn>,
    /// The box, reversal or range in raw units, for the legend.
    pub size: i64,
}

fn brick(time_ms: i64, open: i64, close: i64, volume: i64) -> Bar {
    Bar {
        time_ms,
        open,
        high: open.max(close),
        low: open.min(close),
        close,
        volume,
    }
}

/// Renko bricks of `size` from the closes of `bars`.
pub fn renko(bars: &[Bar], size: i64) -> Vec<Bar> {
    let size = size.max(1);
    let mut out: Vec<Bar> = Vec::new();
    let Some(first) = bars.first() else {
        return out;
    };
    // The grid starts on a multiple of the box, below the first close.
    let base = first.close.div_euclid(size) * size;
    let mut volume = 0;
    for bar in bars {
        volume += bar.volume;
        let price = bar.close;
        loop {
            if out.len() >= MAX_OUT {
                return out;
            }
            let next = match out.last() {
                None => {
                    if price >= base + size {
                        Some((base, base + size))
                    } else if price <= base - size {
                        Some((base, base - size))
                    } else {
                        None
                    }
                }
                Some(last) => {
                    // Past the top of the last brick (its close when rising, its open when
                    // falling, which makes a turn take two boxes), or past its bottom.
                    let (top, bottom) = (last.high, last.low);
                    if price >= top + size {
                        Some((top, top + size))
                    } else if price <= bottom - size {
                        Some((bottom, bottom - size))
                    } else {
                        None
                    }
                }
            };
            match next {
                Some((open, close)) => {
                    out.push(brick(bar.time_ms, open, close, volume));
                    volume = 0;
                }
                None => break,
            }
        }
    }
    out
}

/// Line break lines: a new line past the extreme of the last `lines` lines.
pub fn line_break(bars: &[Bar], lines: usize) -> Vec<Bar> {
    let lines = lines.max(1);
    let mut out: Vec<Bar> = Vec::new();
    let Some(first) = bars.first() else {
        return out;
    };
    let mut reference = first.close;
    let mut volume = 0;
    for bar in bars.iter().skip(1) {
        volume += bar.volume;
        let price = bar.close;
        let Some(last) = out.last().copied() else {
            if price != reference {
                out.push(brick(bar.time_ms, reference, price, volume));
                volume = 0;
            }
            reference = price;
            continue;
        };
        let up = last.close > last.open;
        let recent = &out[out.len().saturating_sub(lines)..];
        let highest = recent.iter().map(|b| b.high).max().unwrap_or(last.high);
        let lowest = recent.iter().map(|b| b.low).min().unwrap_or(last.low);
        let line = if up {
            if price > last.close {
                Some((last.close, price))
            } else if price < lowest {
                Some((last.open, price))
            } else {
                None
            }
        } else if price < last.close {
            Some((last.close, price))
        } else if price > highest {
            Some((last.open, price))
        } else {
            None
        };
        if let Some((open, close)) = line {
            out.push(brick(bar.time_ms, open, close, volume));
            volume = 0;
            if out.len() >= MAX_OUT {
                break;
            }
        }
    }
    out
}

/// Builds Kagi lines one finished line at a time, keeping the thickness and the last shoulder
/// and waist.
#[derive(Default)]
struct KagiBuilder {
    out: Vec<Bar>,
    meta: Vec<KagiLine>,
    yang: bool,
    shoulder: Option<i64>,
    waist: Option<i64>,
}

impl KagiBuilder {
    fn finish(&mut self, start: i64, end: i64, time: i64, volume: i64) {
        let rising = end > start;
        let starts_yang = self.yang;
        let mut switch_at = None;
        if rising && !self.yang {
            if let Some(shoulder) = self.shoulder.filter(|s| end > *s) {
                self.yang = true;
                switch_at = Some(shoulder);
            }
        } else if !rising
            && self.yang
            && let Some(waist) = self.waist.filter(|w| end < *w)
        {
            self.yang = false;
            switch_at = Some(waist);
        }
        if rising {
            self.shoulder = Some(end);
        } else {
            self.waist = Some(end);
        }
        self.out.push(brick(time, start, end, volume));
        self.meta.push(KagiLine {
            yang: starts_yang,
            switch_at,
        });
    }
}

/// Kagi lines with a reversal of `reversal`. Each bar is one vertical line: open where it starts,
/// close where it ends, and [`KagiLine`] for its thickness.
pub fn kagi(bars: &[Bar], reversal: i64) -> (Vec<Bar>, Vec<KagiLine>) {
    let reversal = reversal.max(1);
    let mut builder = KagiBuilder::default();
    let Some(first) = bars.first() else {
        return (Vec::new(), Vec::new());
    };
    // The line in progress: where it starts, its extreme, its direction (unknown at first).
    let (mut start, mut end, mut up): (i64, i64, Option<bool>) = (first.close, first.close, None);
    let mut start_time = first.time_ms;
    let mut volume = 0;
    for bar in bars.iter().skip(1) {
        volume += bar.volume;
        let price = bar.close;
        match up {
            None => {
                if (price - start).abs() >= reversal {
                    up = Some(price > start);
                    end = price;
                    builder.yang = price > start;
                }
            }
            Some(rising) => {
                let extends = if rising { price > end } else { price < end };
                if extends {
                    end = price;
                } else if (end - price).abs() >= reversal {
                    builder.finish(start, end, start_time, std::mem::take(&mut volume));
                    (start, end, up, start_time) = (end, price, Some(!rising), bar.time_ms);
                }
            }
        }
        if builder.out.len() >= MAX_OUT {
            return (builder.out, builder.meta);
        }
    }
    if up.is_some() {
        // The line still in progress.
        builder.finish(start, end, start_time, volume);
    }
    (builder.out, builder.meta)
}

/// Point and figure columns of `size` with a reversal of `reversal` boxes. A rising column marks
/// every level the price reached going up, a falling one every level it reached going down; a
/// column turns once the price goes `reversal` levels back from its extreme, and the new column
/// starts one level off that extreme.
pub fn point_and_figure(bars: &[Bar], size: i64, reversal: i64) -> (Vec<Bar>, Vec<PnfColumn>) {
    let size = size.max(1);
    let reversal = reversal.max(1);
    let mut columns: Vec<(PnfColumn, i64, i64)> = Vec::new(); // column, time, volume
    let Some(first) = bars.first() else {
        return (Vec::new(), Vec::new());
    };
    let start = first.close.div_euclid(size);
    let mut volume = 0;
    for bar in bars.iter().skip(1) {
        volume += bar.volume;
        let price = bar.close;
        // The highest level reached going up, and the lowest going down.
        let up_level = price.div_euclid(size);
        let down_level = (price + size - 1).div_euclid(size);
        let column = |up: bool, bottom: i64, top: i64| PnfColumn {
            up,
            bottom,
            top,
            size,
        };
        match columns.last_mut() {
            None => {
                if up_level > start {
                    columns.push((column(true, start + 1, up_level), bar.time_ms, volume));
                    volume = 0;
                } else if down_level < start {
                    columns.push((column(false, down_level, start - 1), bar.time_ms, volume));
                    volume = 0;
                }
            }
            Some((last, _, held)) => {
                if last.up {
                    if up_level > last.top {
                        last.top = up_level;
                        *held += std::mem::take(&mut volume);
                    } else if down_level <= last.top - reversal {
                        let next = column(false, down_level, last.top - 1);
                        columns.push((next, bar.time_ms, std::mem::take(&mut volume)));
                    }
                } else if down_level < last.bottom {
                    last.bottom = down_level;
                    *held += std::mem::take(&mut volume);
                } else if up_level >= last.bottom + reversal {
                    let next = column(true, last.bottom + 1, up_level);
                    columns.push((next, bar.time_ms, std::mem::take(&mut volume)));
                }
            }
        }
        if columns.len() >= MAX_OUT {
            break;
        }
    }
    let half = size / 2;
    let out = columns
        .iter()
        .map(|(c, time, volume)| {
            let (low, high) = (c.bottom * size - half, c.top * size + (size - half));
            let (open, close) = if c.up { (low, high) } else { (high, low) };
            Bar {
                time_ms: *time,
                open,
                high,
                low,
                close,
                volume: *volume,
            }
        })
        .collect();
    (out, columns.into_iter().map(|(c, _, _)| c).collect())
}

/// Range bars of `range`, walking each bar open, low, high, close (or open, high, low, close for a
/// falling bar).
pub fn range_bars(bars: &[Bar], range: i64) -> Vec<Bar> {
    let range = range.max(1);
    let mut out: Vec<Bar> = Vec::new();
    let mut current: Option<Bar> = None;
    let mut last_price: Option<i64> = None;
    for bar in bars {
        let path = if bar.close >= bar.open {
            [bar.open, bar.low, bar.high, bar.close]
        } else {
            [bar.open, bar.high, bar.low, bar.close]
        };
        let mut volume = bar.volume;
        for target in path {
            let from = last_price.unwrap_or(target);
            let step = if target >= from { 1 } else { -1 };
            let mut price = from;
            loop {
                let bar_now = current.get_or_insert(Bar {
                    time_ms: bar.time_ms,
                    open: price,
                    high: price,
                    low: price,
                    close: price,
                    volume: 0,
                });
                // How far the price can go before the bar spans the full range.
                let limit = if step > 0 {
                    bar_now.low + range
                } else {
                    bar_now.high - range
                };
                let reaches = if step > 0 {
                    target >= limit
                } else {
                    target <= limit
                };
                if reaches {
                    bar_now.close = limit;
                    bar_now.high = bar_now.high.max(limit);
                    bar_now.low = bar_now.low.min(limit);
                    bar_now.volume += std::mem::take(&mut volume);
                    out.push(*bar_now);
                    if out.len() >= MAX_OUT {
                        return out;
                    }
                    current = None;
                    price = limit;
                    if price == target {
                        break;
                    }
                } else {
                    bar_now.close = target;
                    bar_now.high = bar_now.high.max(target);
                    bar_now.low = bar_now.low.min(target);
                    bar_now.volume += std::mem::take(&mut volume);
                    break;
                }
            }
            last_price = Some(target);
        }
    }
    out.extend(current);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closes(prices: &[i64]) -> Vec<Bar> {
        prices
            .iter()
            .enumerate()
            .map(|(i, &p)| Bar {
                time_ms: i as i64 * 60_000,
                open: p,
                high: p,
                low: p,
                close: p,
                volume: 1,
            })
            .collect()
    }

    fn pairs(bars: &[Bar]) -> Vec<(i64, i64)> {
        bars.iter().map(|b| (b.open, b.close)).collect()
    }

    #[test]
    fn renko_builds_bricks_and_turns_after_two_boxes() {
        let bricks = renko(&closes(&[100, 105, 131, 125, 111, 109, 89]), 10);
        // From 100: 110, 120, 130 up; a fall to 111 is not two boxes; 109 is (below 110);
        // 89 adds two more down... from 110 to 100, then 100 to 90.
        assert_eq!(
            pairs(&bricks),
            vec![(100, 110), (110, 120), (120, 130), (120, 110), (110, 100), (100, 90)]
        );
        for brick in &bricks {
            assert_eq!((brick.close - brick.open).abs(), 10);
        }
    }

    #[test]
    fn renko_keeps_the_volume_of_what_made_each_brick() {
        let bricks = renko(&closes(&[100, 101, 102, 115]), 10);
        assert_eq!(bricks.len(), 1);
        assert_eq!(bricks[0].volume, 4);
    }

    #[test]
    fn a_line_break_turns_only_past_three_lines() {
        let lines = line_break(&closes(&[10, 11, 12, 13, 12, 11, 9, 14]), 3);
        // Up lines 10-11, 11-12, 12-13; 12 and 11 are inside the last three lines; 9 breaks
        // below their lowest low (10) and the new line starts at the bottom of the last one;
        // 14 breaks above the highest of the last three (13).
        assert_eq!(
            pairs(&lines),
            vec![(10, 11), (11, 12), (12, 13), (12, 9), (12, 14)]
        );
    }

    #[test]
    fn kagi_turns_on_the_reversal_and_thickens_past_the_shoulder() {
        let (lines, meta) = kagi(&closes(&[100, 110, 120, 112, 105, 118, 130, 90]), 10);
        // Up to 120, down to 105, up to 130 (past the 120 shoulder: yang), down to 90.
        assert_eq!(pairs(&lines), vec![(100, 120), (120, 105), (105, 130), (130, 90)]);
        assert!(meta[0].yang);
        // The 105 waist was never set before the second line, so it stays yang.
        assert_eq!(meta[3].switch_at, Some(105), "breaking the waist turns it thin");
    }

    #[test]
    fn point_and_figure_columns_turn_three_boxes_off_the_extreme() {
        let (bars, columns) = point_and_figure(&closes(&[100, 131, 125, 99, 140]), 10, 3);
        assert_eq!(columns.len(), 3, "{columns:?}");
        // From 100 up to 130; 125 is not three boxes back; 99 reaches the 100 level, three
        // boxes under 130, so an O column runs from 120 down to 100; 140 turns it again.
        assert!(columns[0].up);
        assert_eq!((columns[0].bottom, columns[0].top), (11, 13));
        assert!(!columns[1].up);
        assert_eq!((columns[1].bottom, columns[1].top), (10, 12));
        assert!(columns[2].up);
        assert_eq!((columns[2].bottom, columns[2].top), (11, 14));
        assert_eq!(bars.len(), 3);
        assert!(bars.iter().all(|b| b.high > b.low));
    }

    #[test]
    fn range_bars_all_span_the_range() {
        let source = closes(&[100, 125, 90, 103]);
        let out = range_bars(&source, 10);
        assert!(out.len() >= 5, "{out:?}");
        for bar in &out[..out.len() - 1] {
            assert_eq!(bar.high - bar.low, 10, "{bar:?}");
        }
        assert!(out.last().unwrap().high - out.last().unwrap().low <= 10);
        let total: i64 = out.iter().map(|b| b.volume).sum();
        assert_eq!(total, 4, "no volume lost");
    }

    #[test]
    fn box_sizes_resolve_to_whole_quote_units() {
        let bars = closes(&(0..50).map(|i| 100_000 + i * 37).collect::<Vec<_>>());
        let atr = BoxSize::Atr { length: 14 }.resolve(&bars, 10);
        assert!(atr > 0 && atr % 10 == 0, "{atr}");
        assert_eq!(BoxSize::Fixed { price: 0.0005 }.resolve(&bars, 1), 50);
        assert_eq!(BoxSize::Percent { percent: 1.0 }.resolve(&bars, 1), 1_018);
        assert_eq!(BoxSize::Fixed { price: 0.0 }.resolve(&bars, 7), 7);
    }

    #[test]
    fn settings_are_repaired() {
        let settings = TransformSettings {
            line_break: 0,
            pnf_reversal: 99,
            renko_box: BoxSize::Fixed { price: -1.0 },
            ..TransformSettings::default()
        }
        .normalized();
        assert_eq!(settings.line_break, 1);
        assert_eq!(settings.pnf_reversal, 10);
        assert_eq!(settings.renko_box, BoxSize::default());
        let text = toml::to_string(&settings).unwrap();
        assert_eq!(toml::from_str::<TransformSettings>(&text).unwrap(), settings);
    }

    #[test]
    fn nothing_in_gives_nothing_out() {
        assert!(renko(&[], 10).is_empty());
        assert!(line_break(&[], 3).is_empty());
        assert!(kagi(&[], 10).0.is_empty());
        assert!(point_and_figure(&[], 10, 3).0.is_empty());
        assert!(range_bars(&[], 10).is_empty());
    }
}
