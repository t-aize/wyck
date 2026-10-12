//! What the user chooses for the constructions: the size of a box and the settings of each one.

use serde::{Deserialize, Serialize};

use super::PricePath;
use crate::domain::indicators::math;
use crate::domain::market::Bar;

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
            Self::Fixed { price } => price * crate::domain::market::PRICE_SCALE as f64,
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
    #[serde(default = "crate::domain::chart::defaults::yes")]
    pub brick_border: bool,
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
