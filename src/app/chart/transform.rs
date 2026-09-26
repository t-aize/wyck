//! Chart types that are not drawn one bar per period: Renko, line break, Kagi, point and figure
//! and range bars. Each one filters the prices it is given into its own sequence of bricks, lines,
//! columns or bars, and the chart then shows that sequence one after the other, like bars.
//!
//! A construction reads a stream of prices ([`Sample`]s), made from the bars by [`samples`]. Which
//! prices of a bar it reads is [`PricePath`]: only the close, or the whole path the price most
//! likely took inside the bar (open, then low and high, then close). Reading the path is what
//! keeps a brick chart of one minute bars close to what a tick chart would show, since a bar that
//! swung across several boxes and came back is not lost. They are the textbook constructions:
//!
//! - **Renko**: a brick every time the price moves a full box past the last brick; turning back
//!   takes `reversal` boxes (two by default: one past the other end of the last brick). Bricks
//!   can carry wicks: the highest and lowest price seen while the brick was forming.
//! - **Line break**: a new line when the price goes past the highest high (or lowest low) of the
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

/// Which prices of a bar a construction reads. A bar only says where the price opened, went and
/// closed, so this is how much of that it trusts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricePath {
    /// Only the close. Fast and safe, but a bar that swung across several boxes and came back
    /// leaves no trace.
    Close,
    /// The open, then the low and the high in the order the bar most likely took them (low
    /// first when it closed above its open), then the close. On an intraday chart this is what
    /// keeps the bricks and columns close to what a tick chart would show.
    #[default]
    Ohlc,
}

impl PricePath {
    pub const ALL: [Self; 2] = [Self::Close, Self::Ohlc];

    /// What a file from before the path could be chosen was built with.
    fn close() -> Self {
        Self::Close
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Close => "Close only",
            Self::Ohlc => "Open, high, low, close",
        }
    }

    /// The prices of `bar` in the order the construction walks them.
    pub fn points(self, bar: &Bar) -> Vec<i64> {
        match self {
            Self::Close => vec![bar.close],
            Self::Ohlc if bar.close >= bar.open => vec![bar.open, bar.low, bar.high, bar.close],
            Self::Ohlc => vec![bar.open, bar.high, bar.low, bar.close],
        }
    }
}

/// One price of the stream a construction reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub time_ms: i64,
    pub price: i64,
    /// The share of the volume of its bar that goes with this price.
    pub volume: i64,
}

/// The stream of prices of `bars` along `path`. The volume of a bar is split evenly between its
/// prices (the last one takes what is left), so nothing is lost.
pub fn samples(bars: &[Bar], path: PricePath) -> Vec<Sample> {
    let mut out = Vec::with_capacity(bars.len() * if path == PricePath::Ohlc { 4 } else { 1 });
    for bar in bars {
        let points = path.points(bar);
        let n = points.len() as i64;
        let share = bar.volume / n;
        for (i, &price) in points.iter().enumerate() {
            let volume = if i + 1 == points.len() {
                bar.volume - share * (n - 1)
            } else {
                share
            };
            out.push(Sample {
                time_ms: bar.time_ms,
                price,
                volume,
            });
        }
    }
    out
}

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
                        let (lo, hi) = bars.iter().fold((f64::MAX, f64::MIN), |(l, h), b| {
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
    #[serde(default = "PricePath::close")]
    pub renko_path: PricePath,
    /// The boxes the price must go back for a brick of the other color: 2 is the textbook Renko,
    /// 1 turns as soon as the price goes back a box.
    #[serde(default = "default_renko_reversal")]
    pub renko_reversal: u32,
    /// Wicks on the bricks: the extremes the price reached while a brick was forming.
    #[serde(default)]
    pub renko_wicks: bool,
    #[serde(default = "default_lines")]
    pub line_break: u32,
    #[serde(default = "PricePath::close")]
    pub line_break_path: PricePath,
    #[serde(default)]
    pub kagi_reversal: BoxSize,
    #[serde(default = "PricePath::close")]
    pub kagi_path: PricePath,
    /// The width of the thick (yang) line, in pixels.
    #[serde(default = "default_thick")]
    pub kagi_thick: f32,
    /// The width of the thin (yin) line, in pixels.
    #[serde(default = "default_thin")]
    pub kagi_thin: f32,
    #[serde(default)]
    pub pnf_box: BoxSize,
    #[serde(default = "default_reversal")]
    pub pnf_reversal: u32,
    #[serde(default = "PricePath::close")]
    pub pnf_path: PricePath,
    /// How much of its box an X or an O fills.
    #[serde(default = "default_glyph")]
    pub pnf_glyph: f32,
    /// The stroke of the X and O, in pixels; 0 follows the size of the boxes.
    #[serde(default)]
    pub pnf_line: f32,
    #[serde(default)]
    pub range: BoxSize,
    #[serde(default = "default_range_path")]
    pub range_path: PricePath,
    /// The width of a brick or a line, as a share of the room it has.
    #[serde(default = "default_brick_width")]
    pub brick_width: f32,
    /// How solid the body of a brick or a line is.
    #[serde(default = "default_brick_opacity")]
    pub brick_opacity: f32,
    /// A line around each brick.
    #[serde(default = "yes")]
    pub brick_border: bool,
}

fn yes() -> bool {
    true
}

fn default_lines() -> u32 {
    3
}

fn default_reversal() -> u32 {
    3
}

fn default_renko_reversal() -> u32 {
    2
}

fn default_thick() -> f32 {
    2.6
}

fn default_thin() -> f32 {
    1.2
}

fn default_glyph() -> f32 {
    0.8
}

fn default_range_path() -> PricePath {
    PricePath::Ohlc
}

fn default_brick_width() -> f32 {
    0.9
}

fn default_brick_opacity() -> f32 {
    0.85
}

impl Default for TransformSettings {
    fn default() -> Self {
        Self {
            renko_box: BoxSize::default(),
            renko_path: PricePath::Ohlc,
            renko_reversal: default_renko_reversal(),
            renko_wicks: false,
            line_break: default_lines(),
            line_break_path: PricePath::Ohlc,
            kagi_reversal: BoxSize::default(),
            kagi_path: PricePath::Ohlc,
            kagi_thick: default_thick(),
            kagi_thin: default_thin(),
            pnf_box: BoxSize::default(),
            pnf_reversal: default_reversal(),
            pnf_path: PricePath::Ohlc,
            pnf_glyph: default_glyph(),
            pnf_line: 0.0,
            range: BoxSize::default(),
            range_path: default_range_path(),
            brick_width: default_brick_width(),
            brick_opacity: default_brick_opacity(),
            brick_border: true,
        }
    }
}

impl TransformSettings {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.line_break = self.line_break.clamp(1, 10);
        self.pnf_reversal = self.pnf_reversal.clamp(1, 10);
        self.renko_reversal = self.renko_reversal.clamp(1, 10);
        let fix = |v: f32, default: f32, low: f32, high: f32| {
            if v.is_finite() {
                v.clamp(low, high)
            } else {
                default
            }
        };
        self.kagi_thick = fix(self.kagi_thick, default_thick(), 0.5, 8.0);
        self.kagi_thin = fix(self.kagi_thin, default_thin(), 0.5, 8.0);
        self.pnf_glyph = fix(self.pnf_glyph, default_glyph(), 0.3, 1.0);
        self.pnf_line = fix(self.pnf_line, 0.0, 0.0, 6.0);
        self.brick_width = fix(self.brick_width, default_brick_width(), 0.2, 1.0);
        self.brick_opacity = fix(self.brick_opacity, default_brick_opacity(), 0.1, 1.0);
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

/// Renko bricks of `size` from `samples`. A brick of the other color needs the price to go back
/// `reversal` boxes from where the last brick ended. With `wicks` a brick's high and low reach the
/// extremes the price saw while it was forming.
pub fn renko(samples: &[Sample], size: i64, reversal: i64, wicks: bool) -> Vec<Bar> {
    let size = size.max(1);
    let reversal = reversal.clamp(1, 10);
    let mut out: Vec<Bar> = Vec::new();
    let Some(first) = samples.first() else {
        return out;
    };
    // The grid starts on a multiple of the box, below the first price.
    let base = first.price.div_euclid(size) * size;
    let mut volume = 0;
    // The extremes since the last brick, for the wicks.
    let (mut high, mut low) = (first.price, first.price);
    for sample in samples {
        volume += sample.volume;
        let price = sample.price;
        high = high.max(price);
        low = low.min(price);
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
                    // Going on the way the last brick went takes one box past where it ended;
                    // turning takes `reversal` boxes back, and the new brick starts one box
                    // less back (so with the textbook two, it starts where the last began).
                    let end = last.close;
                    if last.close > last.open {
                        if price >= end + size {
                            Some((end, end + size))
                        } else if price <= end - reversal * size {
                            Some((end - (reversal - 1) * size, end - reversal * size))
                        } else {
                            None
                        }
                    } else if price <= end - size {
                        Some((end, end - size))
                    } else if price >= end + reversal * size {
                        Some((end + (reversal - 1) * size, end + reversal * size))
                    } else {
                        None
                    }
                }
            };
            let Some((open, close)) = next else {
                break;
            };
            let mut made = brick(sample.time_ms, open, close, volume);
            if wicks {
                made.high = made.high.max(high);
                made.low = made.low.min(low);
            }
            out.push(made);
            volume = 0;
            // Another brick out of the same price has no wick: it did not form slowly.
            (high, low) = (close, close);
        }
        high = high.max(price);
        low = low.min(price);
    }
    out
}

/// Line break lines: a new line past the extreme of the last `lines` lines.
pub fn line_break(samples: &[Sample], lines: usize) -> Vec<Bar> {
    let lines = lines.max(1);
    let mut out: Vec<Bar> = Vec::new();
    let Some(first) = samples.first() else {
        return out;
    };
    let mut reference = first.price;
    let mut volume = 0;
    for sample in samples.iter().skip(1) {
        volume += sample.volume;
        let price = sample.price;
        let Some(last) = out.last().copied() else {
            if price != reference {
                out.push(brick(sample.time_ms, reference, price, volume));
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
            out.push(brick(sample.time_ms, open, close, volume));
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
pub fn kagi(samples: &[Sample], reversal: i64) -> (Vec<Bar>, Vec<KagiLine>) {
    let reversal = reversal.max(1);
    let mut builder = KagiBuilder::default();
    let Some(first) = samples.first() else {
        return (Vec::new(), Vec::new());
    };
    // The line in progress: where it starts, its extreme, its direction (unknown at first).
    let (mut start, mut end, mut up): (i64, i64, Option<bool>) = (first.price, first.price, None);
    let mut start_time = first.time_ms;
    let mut volume = 0;
    for sample in samples.iter().skip(1) {
        volume += sample.volume;
        let price = sample.price;
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
                    (start, end, up, start_time) = (end, price, Some(!rising), sample.time_ms);
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
pub fn point_and_figure(
    samples: &[Sample],
    size: i64,
    reversal: i64,
) -> (Vec<Bar>, Vec<PnfColumn>) {
    let size = size.max(1);
    let reversal = reversal.max(1);
    let mut columns: Vec<(PnfColumn, i64, i64)> = Vec::new(); // column, time, volume
    let Some(first) = samples.first() else {
        return (Vec::new(), Vec::new());
    };
    let start = first.price.div_euclid(size);
    let mut volume = 0;
    for sample in samples.iter().skip(1) {
        volume += sample.volume;
        let price = sample.price;
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
                    columns.push((column(true, start + 1, up_level), sample.time_ms, volume));
                    volume = 0;
                } else if down_level < start {
                    columns.push((column(false, down_level, start - 1), sample.time_ms, volume));
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
                        columns.push((next, sample.time_ms, std::mem::take(&mut volume)));
                    }
                } else if down_level < last.bottom {
                    last.bottom = down_level;
                    *held += std::mem::take(&mut volume);
                } else if up_level >= last.bottom + reversal {
                    let next = column(true, last.bottom + 1, up_level);
                    columns.push((next, sample.time_ms, std::mem::take(&mut volume)));
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

/// Range bars of `range`, walking each bar along `path` (open, low, high, close for a rising bar,
/// open, high, low, close for a falling one).
pub fn range_bars(bars: &[Bar], range: i64, path: PricePath) -> Vec<Bar> {
    let range = range.max(1);
    let mut out: Vec<Bar> = Vec::new();
    let mut current: Option<Bar> = None;
    let mut last_price: Option<i64> = None;
    for bar in bars {
        let mut volume = bar.volume;
        for target in path.points(bar) {
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

    /// The stream of a list of single price bars.
    fn stream(prices: &[i64]) -> Vec<Sample> {
        samples(&closes(prices), PricePath::Close)
    }

    fn pairs(bars: &[Bar]) -> Vec<(i64, i64)> {
        bars.iter().map(|b| (b.open, b.close)).collect()
    }

    #[test]
    fn renko_builds_bricks_and_turns_after_two_boxes() {
        let bricks = renko(&stream(&[100, 105, 131, 125, 111, 109, 89]), 10, 2, false);
        // From 100: 110, 120, 130 up; a fall to 111 is not two boxes; 109 is (below 110);
        // 89 adds two more down... from 110 to 100, then 100 to 90.
        assert_eq!(
            pairs(&bricks),
            vec![
                (100, 110),
                (110, 120),
                (120, 130),
                (120, 110),
                (110, 100),
                (100, 90)
            ]
        );
        for brick in &bricks {
            assert_eq!((brick.close - brick.open).abs(), 10);
        }
    }

    #[test]
    fn renko_keeps_the_volume_of_what_made_each_brick() {
        let bricks = renko(&stream(&[100, 101, 102, 115]), 10, 2, false);
        assert_eq!(bricks.len(), 1);
        assert_eq!(bricks[0].volume, 4);
    }

    #[test]
    fn a_renko_that_turns_after_one_box_follows_the_price_closely() {
        let bricks = renko(&stream(&[100, 111, 99, 111]), 10, 1, false);
        // Up to 110, then back one box: a brick from where the last ended, and up again.
        assert_eq!(pairs(&bricks), vec![(100, 110), (110, 100), (100, 110)]);
        for brick in &bricks {
            assert_eq!((brick.close - brick.open).abs(), 10);
        }
    }

    #[test]
    fn a_renko_that_turns_after_three_boxes_leaves_a_gap() {
        let bricks = renko(&stream(&[100, 121, 88]), 10, 3, false);
        // Up to 120, a fall to 88 is three boxes (below 90): the new brick starts one box
        // under where the last ended.
        assert_eq!(pairs(&bricks), vec![(100, 110), (110, 120), (100, 90)]);
    }

    #[test]
    fn wicks_reach_the_extremes_seen_while_the_brick_formed() {
        let bars = [
            Bar {
                time_ms: 0,
                open: 100,
                high: 100,
                low: 100,
                close: 100,
                volume: 1,
            },
            // A bar that dipped to 96 and spiked to 118 before closing at 111.
            Bar {
                time_ms: 60_000,
                open: 101,
                high: 118,
                low: 96,
                close: 111,
                volume: 1,
            },
        ];
        let plain = renko(&samples(&bars, PricePath::Close), 10, 2, true);
        assert_eq!(plain.len(), 1);
        // Reading only the closes, the only extreme seen is the close that made the brick.
        assert_eq!((plain[0].low, plain[0].high), (100, 111));
        let path = renko(&samples(&bars, PricePath::Ohlc), 10, 2, true);
        assert!(path[0].low <= 96, "{path:?}");
        let no_wicks = renko(&samples(&bars, PricePath::Ohlc), 10, 2, false);
        assert_eq!((no_wicks[0].low, no_wicks[0].high), (100, 110));
    }

    #[test]
    fn a_second_brick_from_the_same_price_has_no_wick() {
        let bricks = renko(&stream(&[100, 145]), 10, 2, true);
        assert_eq!(bricks.len(), 4);
        assert!(
            bricks[1..].iter().all(|b| b.high - b.low == 10),
            "{bricks:?}"
        );
    }

    #[test]
    fn the_ohlc_path_finds_the_bricks_a_close_hides() {
        // One bar that fell six boxes and came back to where it opened.
        let bars = [
            Bar {
                time_ms: 0,
                open: 100,
                high: 100,
                low: 100,
                close: 100,
                volume: 4,
            },
            Bar {
                time_ms: 60_000,
                open: 100,
                high: 102,
                low: 40,
                close: 100,
                volume: 8,
            },
        ];
        let close = renko(&samples(&bars, PricePath::Close), 10, 2, false);
        let path = renko(&samples(&bars, PricePath::Ohlc), 10, 2, false);
        assert!(close.is_empty());
        assert!(path.len() >= 6, "{path:?}");
    }

    #[test]
    fn samples_keep_all_the_volume() {
        let bars = [Bar {
            time_ms: 5,
            open: 10,
            high: 20,
            low: 5,
            close: 12,
            volume: 10,
        }];
        let ohlc = samples(&bars, PricePath::Ohlc);
        assert_eq!(
            ohlc.iter().map(|s| s.price).collect::<Vec<_>>(),
            vec![10, 5, 20, 12]
        );
        assert_eq!(ohlc.iter().map(|s| s.volume).sum::<i64>(), 10);
        assert!(ohlc.iter().all(|s| s.time_ms == 5));
        let falling = Bar {
            close: 8,
            ..bars[0]
        };
        assert_eq!(
            samples(&[falling], PricePath::Ohlc)
                .iter()
                .map(|s| s.price)
                .collect::<Vec<_>>(),
            vec![10, 20, 5, 8]
        );
        assert_eq!(samples(&bars, PricePath::Close).len(), 1);
    }

    #[test]
    fn a_line_break_turns_only_past_three_lines() {
        let lines = line_break(&stream(&[10, 11, 12, 13, 12, 11, 9, 14]), 3);
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
        let (lines, meta) = kagi(&stream(&[100, 110, 120, 112, 105, 118, 130, 90]), 10);
        // Up to 120, down to 105, up to 130 (past the 120 shoulder: yang), down to 90.
        assert_eq!(
            pairs(&lines),
            vec![(100, 120), (120, 105), (105, 130), (130, 90)]
        );
        assert!(meta[0].yang);
        // The 105 waist was never set before the second line, so it stays yang.
        assert_eq!(
            meta[3].switch_at,
            Some(105),
            "breaking the waist turns it thin"
        );
    }

    #[test]
    fn point_and_figure_columns_turn_three_boxes_off_the_extreme() {
        let (bars, columns) = point_and_figure(&stream(&[100, 131, 125, 99, 140]), 10, 3);
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
    fn point_and_figure_on_the_ohlc_path_reads_the_highs_and_lows() {
        let bars = [
            Bar {
                time_ms: 0,
                open: 100,
                high: 100,
                low: 100,
                close: 100,
                volume: 1,
            },
            // The high reaches 140 but it closes back at 101.
            Bar {
                time_ms: 60_000,
                open: 101,
                high: 140,
                low: 100,
                close: 101,
                volume: 1,
            },
        ];
        let (_, close) = point_and_figure(&samples(&bars, PricePath::Close), 10, 3);
        let (_, path) = point_and_figure(&samples(&bars, PricePath::Ohlc), 10, 3);
        assert!(close.is_empty());
        // Up to the high, and the close back near the start turns the column.
        assert!(path.len() >= 2, "{path:?}");
        assert!(path[0].up && path[0].top >= 14, "{path:?}");
    }

    #[test]
    fn range_bars_all_span_the_range() {
        let source = closes(&[100, 125, 90, 103]);
        let out = range_bars(&source, 10, PricePath::Ohlc);
        assert!(out.len() >= 5, "{out:?}");
        for bar in &out[..out.len() - 1] {
            assert_eq!(bar.high - bar.low, 10, "{bar:?}");
        }
        assert!(out.last().unwrap().high - out.last().unwrap().low <= 10);
        let total: i64 = out.iter().map(|b| b.volume).sum();
        assert_eq!(total, 4, "no volume lost");
    }

    #[test]
    fn range_bars_can_read_only_the_close() {
        let bars = [
            Bar {
                time_ms: 0,
                open: 100,
                high: 100,
                low: 100,
                close: 100,
                volume: 1,
            },
            Bar {
                time_ms: 1,
                open: 100,
                high: 150,
                low: 60,
                close: 101,
                volume: 1,
            },
        ];
        assert!(range_bars(&bars, 10, PricePath::Close).len() <= 1);
        assert!(range_bars(&bars, 10, PricePath::Ohlc).len() >= 8);
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
            renko_reversal: 0,
            renko_box: BoxSize::Fixed { price: -1.0 },
            kagi_thick: f32::NAN,
            pnf_glyph: 9.0,
            brick_opacity: 0.0,
            ..TransformSettings::default()
        }
        .normalized();
        assert_eq!(settings.line_break, 1);
        assert_eq!(settings.pnf_reversal, 10);
        assert_eq!(settings.renko_reversal, 1);
        assert_eq!(settings.renko_box, BoxSize::default());
        assert_eq!(settings.kagi_thick, 2.6);
        assert_eq!(settings.pnf_glyph, 1.0);
        assert_eq!(settings.brick_opacity, 0.1);
        let text = toml::to_string(&settings).unwrap();
        assert_eq!(
            toml::from_str::<TransformSettings>(&text).unwrap(),
            settings
        );
    }

    #[test]
    fn a_chart_saved_before_the_options_keeps_the_prices_it_was_built_from() {
        // New charts read the whole path, but a file written before there was a choice was built
        // from the closes, and reads the same way now.
        let old: TransformSettings = toml::from_str("line_break = 4\n").unwrap();
        assert_eq!(old.renko_path, PricePath::Close);
        assert_eq!(old.kagi_path, PricePath::Close);
        assert_eq!(old.pnf_path, PricePath::Close);
        assert_eq!(old.line_break_path, PricePath::Close);
        // Range bars always walked the path.
        assert_eq!(old.range_path, PricePath::Ohlc);
        assert_eq!(old.renko_reversal, 2);
        assert!(!old.renko_wicks);
        assert_eq!(old.brick_width, 0.9);
        assert_eq!(TransformSettings::default().renko_path, PricePath::Ohlc);
    }

    #[test]
    fn nothing_in_gives_nothing_out() {
        assert!(renko(&[], 10, 2, true).is_empty());
        assert!(line_break(&[], 3).is_empty());
        assert!(kagi(&[], 10).0.is_empty());
        assert!(point_and_figure(&[], 10, 3).0.is_empty());
        assert!(range_bars(&[], 10, PricePath::Ohlc).is_empty());
    }
}
