//! The two chart types built on volume.
//!
//! The volume of a bar here is the number of ticks it saw (see [`Bar::volume`]), not an amount
//! traded, so it stands for activity.
//!
//! - **Volume candles** keep one candle per period, on the regular time axis, but a candle is as
//!   wide as the activity of its period: a thick candle is a busy one. [`VolumeCandleSettings`]
//!   says how the width follows the volume, how the candles are colored and filled.
//! - **Volume bars** are not periods at all: a bar closes once it has seen a set amount of
//!   volume, however long that took, so a quiet hour is one bar and a busy minute is several.
//!   [`volume_bars`] builds them by walking the price through each source bar and closing a bar
//!   every time the volume set is used up. A bar's volume is spread over the legs of its path
//!   by their length, since the bar does not say when the price was where.

use serde::{Deserialize, Serialize};
use wyck::openapi::market::Bar;

use super::transform::PricePath;

/// Most bars a construction produces, like the bar limit of the chart.
const MAX_OUT: usize = 120_000;

/// How the width of a candle follows its volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidthScale {
    /// Twice the volume, twice the width.
    Linear,
    /// The width follows the square root of the volume: a busy candle stands out without
    /// crushing the quiet ones.
    #[default]
    Sqrt,
    /// The width follows the logarithm of the volume, for volumes that differ by orders of
    /// magnitude.
    Log,
}

impl WidthScale {
    pub const ALL: [Self; 3] = [Self::Linear, Self::Sqrt, Self::Log];

    pub fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Sqrt => "Square root",
            Self::Log => "Logarithmic",
        }
    }

    /// `share` (0 to 1 or more) of the reference volume, put through the curve.
    fn curve(self, share: f64) -> f64 {
        let share = share.max(0.0);
        match self {
            Self::Linear => share,
            Self::Sqrt => share.sqrt(),
            // 0 at no volume, 1 at the reference, and gentle in between.
            Self::Log => (1.0 + share * 9.0).log10(),
        }
    }
}

/// The volume that gets the widest candle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidthReference {
    /// The busiest candle on screen. The widths move as the chart scrolls.
    Max,
    /// The volume 95 percent of the candles on screen stay under, so one spike does not make
    /// the others thin. A candle above it is as wide as the widest.
    #[default]
    Typical,
    /// Twice the average volume on screen.
    Average,
}

impl WidthReference {
    pub const ALL: [Self; 3] = [Self::Max, Self::Typical, Self::Average];

    pub fn label(self) -> &'static str {
        match self {
            Self::Max => "Busiest on screen",
            Self::Typical => "Typical (95%)",
            Self::Average => "Twice the average",
        }
    }
}

/// What decides whether a candle is a rising one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorBy {
    /// The candle's own open and close.
    #[default]
    Body,
    /// The candle's close against the close before it.
    PreviousClose,
}

impl ColorBy {
    pub const ALL: [Self; 2] = [Self::Body, Self::PreviousClose];

    pub fn label(self) -> &'static str {
        match self {
            Self::Body => "Open and close",
            Self::PreviousClose => "Previous close",
        }
    }
}

/// Which candles are drawn empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fill {
    #[default]
    Solid,
    /// Every body is an outline.
    Hollow,
    /// Only the rising bodies are outlines.
    HollowUp,
}

impl Fill {
    pub const ALL: [Self; 3] = [Self::Solid, Self::Hollow, Self::HollowUp];

    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Hollow => "Hollow",
            Self::HollowUp => "Hollow when rising",
        }
    }

    /// Whether a body that rose (or not) is an outline.
    pub fn is_hollow(self, up: bool) -> bool {
        match self {
            Self::Solid => false,
            Self::Hollow => true,
            Self::HollowUp => up,
        }
    }
}

/// The settings of the volume candles. Saved with the chart.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VolumeCandleSettings {
    #[serde(default)]
    pub scale: WidthScale,
    #[serde(default)]
    pub reference: WidthReference,
    /// The width of a candle with no volume, as a share of the room it has.
    #[serde(default = "default_min_width")]
    pub min_width: f32,
    /// The width of the busiest candle, as a share of the room it has.
    #[serde(default = "default_max_width")]
    pub max_width: f32,
    #[serde(default)]
    pub color_by: ColorBy,
    #[serde(default)]
    pub fill: Fill,
    /// A thin edge in the background color around each body, so wide candles that touch stay apart.
    #[serde(default)]
    pub outline: bool,
    /// The width of the wick, in pixels.
    #[serde(default = "default_wick")]
    pub wick_width: f32,
    /// The volume written over each candle wide enough to hold it.
    #[serde(default)]
    pub labels: bool,
}

fn default_min_width() -> f32 {
    0.25
}

fn default_max_width() -> f32 {
    0.95
}

fn default_wick() -> f32 {
    1.0
}

impl Default for VolumeCandleSettings {
    fn default() -> Self {
        Self {
            scale: WidthScale::default(),
            reference: WidthReference::default(),
            min_width: default_min_width(),
            max_width: default_max_width(),
            color_by: ColorBy::default(),
            fill: Fill::default(),
            outline: false,
            wick_width: default_wick(),
            labels: false,
        }
    }
}

impl VolumeCandleSettings {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let fix = |v: f32, default: f32| if v.is_finite() { v } else { default };
        self.min_width = fix(self.min_width, default_min_width()).clamp(0.05, 1.0);
        self.max_width = fix(self.max_width, default_max_width()).clamp(0.1, 1.0);
        if self.max_width < self.min_width {
            self.max_width = self.min_width;
        }
        self.wick_width = fix(self.wick_width, default_wick()).clamp(1.0, 4.0);
        self
    }

    /// The widths, as a share of the room each candle has, for the volumes of the candles on
    /// screen. A candle's width is never past `max_width` nor under `min_width`.
    pub fn widths(&self, volumes: &[i64]) -> Vec<f32> {
        let reference = self.reference_volume(volumes);
        let (min, max) = (f64::from(self.min_width), f64::from(self.max_width));
        volumes
            .iter()
            .map(|&v| {
                let share = if reference > 0.0 {
                    (v.max(0) as f64 / reference).min(1.0)
                } else {
                    0.0
                };
                (min + (max - min) * self.scale.curve(share).min(1.0)) as f32
            })
            .collect()
    }

    /// The volume that gets the widest candle.
    fn reference_volume(&self, volumes: &[i64]) -> f64 {
        if volumes.is_empty() {
            return 0.0;
        }
        match self.reference {
            WidthReference::Max => volumes.iter().copied().max().unwrap_or(0) as f64,
            WidthReference::Average => {
                2.0 * volumes.iter().map(|&v| v.max(0) as f64).sum::<f64>() / volumes.len() as f64
            }
            WidthReference::Typical => {
                let mut sorted: Vec<i64> = volumes.to_vec();
                sorted.sort_unstable();
                let at = ((sorted.len() - 1) as f64 * 0.95).round() as usize;
                sorted[at] as f64
            }
        }
    }
}

/// How much volume a volume bar holds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum VolumeSize {
    /// A set volume.
    Fixed { volume: u32 },
    /// The average volume of a bar of the chart, times this: 1 gives about as many bars as the
    /// chart has periods, and it follows the symbol's activity without being set by hand.
    Average { multiple: f64 },
}

impl Default for VolumeSize {
    fn default() -> Self {
        Self::Average { multiple: 1.0 }
    }
}

impl VolumeSize {
    /// The volume of a bar for `bars`, at least 1.
    pub fn resolve(self, bars: &[Bar]) -> i64 {
        match self {
            Self::Fixed { volume } => i64::from(volume.max(1)),
            Self::Average { multiple } => {
                if bars.is_empty() {
                    return 1;
                }
                let mean =
                    bars.iter().map(|b| b.volume.max(0) as f64).sum::<f64>() / bars.len() as f64;
                let size = (mean * multiple).round();
                if size.is_finite() {
                    (size as i64).max(1)
                } else {
                    1
                }
            }
        }
    }
}

/// The settings of the volume bars. Saved with the chart.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct VolumeBarSettings {
    #[serde(default)]
    pub size: VolumeSize,
    #[serde(default)]
    pub path: PricePath,
}

impl VolumeBarSettings {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.size = match self.size {
            VolumeSize::Fixed { volume } => VolumeSize::Fixed {
                volume: volume.clamp(1, 1_000_000_000),
            },
            VolumeSize::Average { multiple } if multiple.is_finite() && multiple > 0.0 => {
                VolumeSize::Average {
                    multiple: multiple.clamp(0.01, 10_000.0),
                }
            }
            VolumeSize::Average { .. } => VolumeSize::default(),
        };
        self
    }
}

/// Builds the bar in progress and the bars it closes.
struct Builder {
    size: f64,
    out: Vec<Bar>,
    current: Option<Bar>,
    /// The volume the current bar has seen.
    held: f64,
}

impl Builder {
    /// Moves the current bar to `price`, opening it there when there is none.
    fn reach(&mut self, time_ms: i64, price: i64) {
        let bar = self.current.get_or_insert(Bar {
            time_ms,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: 0,
        });
        bar.high = bar.high.max(price);
        bar.low = bar.low.min(price);
        bar.close = price;
    }

    /// Walks from `from` to `to` while `volume` trades, closing a bar each time `size` is used up.
    /// Returns false once the limit of bars is reached.
    fn leg(&mut self, time_ms: i64, from: i64, to: i64, volume: f64) -> bool {
        self.reach(time_ms, from);
        let (mut from, mut volume) = (from, volume.max(0.0));
        loop {
            let room = self.size - self.held;
            if volume < room {
                self.held += volume;
                self.reach(time_ms, to);
                return true;
            }
            // The price where the bar is full: the share of the leg that fits.
            let share = if volume > 0.0 { room / volume } else { 1.0 };
            let at = from + ((to - from) as f64 * share).round() as i64;
            self.reach(time_ms, at);
            if let Some(mut bar) = self.current.take() {
                bar.volume = self.size.round() as i64;
                self.out.push(bar);
            }
            self.held = 0.0;
            volume -= room;
            from = at;
            if self.out.len() >= MAX_OUT {
                return false;
            }
            self.reach(time_ms, from);
        }
    }
}

/// Volume bars of `size` volume from `bars`, walking each bar along `path`. Each output bar keeps
/// the time of the source bar it opened in.
pub fn volume_bars(bars: &[Bar], size: i64, path: PricePath) -> Vec<Bar> {
    let mut builder = Builder {
        size: size.max(1) as f64,
        out: Vec::new(),
        current: None,
        held: 0.0,
    };
    let mut last_price: Option<i64> = None;
    for bar in bars {
        let points = path.points(bar);
        let volume = bar.volume.max(0) as f64;
        // The path inside the bar carries its volume, split by length. Getting to its first price
        // (a gap from the bar before) carries none.
        let start = last_price.unwrap_or(points[0]);
        if !builder.leg(bar.time_ms, start, points[0], 0.0) {
            return builder.out;
        }
        let lengths: Vec<f64> = points
            .windows(2)
            .map(|w| (w[1] - w[0]).abs() as f64)
            .collect();
        let total: f64 = lengths.iter().sum();
        if points.len() == 1 || total == 0.0 {
            if !builder.leg(bar.time_ms, points[0], points[points.len() - 1], volume) {
                return builder.out;
            }
        } else {
            for (i, w) in points.windows(2).enumerate() {
                let part = volume * lengths[i] / total;
                if !builder.leg(bar.time_ms, w[0], w[1], part) {
                    return builder.out;
                }
            }
        }
        last_price = points.last().copied();
    }
    if let Some(mut bar) = builder.current.take() {
        bar.volume = builder.held.round() as i64;
        builder.out.push(bar);
    }
    builder.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(time_ms: i64, open: i64, high: i64, low: i64, close: i64, volume: i64) -> Bar {
        Bar {
            time_ms,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    #[test]
    fn a_busier_candle_is_never_thinner() {
        let settings = VolumeCandleSettings::default();
        let widths = settings.widths(&[1, 10, 50, 100, 400]);
        assert!(widths.windows(2).all(|w| w[0] <= w[1]), "{widths:?}");
        assert!(
            widths.iter().all(|w| (0.25..=0.95).contains(w)),
            "{widths:?}"
        );
    }

    #[test]
    fn the_widest_candle_gets_the_maximum_and_none_gets_the_minimum() {
        let settings = VolumeCandleSettings {
            reference: WidthReference::Max,
            scale: WidthScale::Linear,
            ..VolumeCandleSettings::default()
        };
        let widths = settings.widths(&[0, 50, 100]);
        assert!((widths[0] - 0.25).abs() < 1e-6);
        assert!((widths[2] - 0.95).abs() < 1e-6);
        assert!((widths[1] - 0.6).abs() < 1e-6);
    }

    #[test]
    fn one_spike_does_not_thin_the_others_with_the_typical_reference() {
        let mut volumes = vec![10; 99];
        volumes.push(100_000);
        let typical = VolumeCandleSettings::default().widths(&volumes);
        let max = VolumeCandleSettings {
            reference: WidthReference::Max,
            ..VolumeCandleSettings::default()
        }
        .widths(&volumes);
        assert!(
            typical[0] > max[0] + 0.3,
            "{} against {}",
            typical[0],
            max[0]
        );
    }

    #[test]
    fn no_volume_at_all_gives_the_minimum_width() {
        let widths = VolumeCandleSettings::default().widths(&[0, 0, 0]);
        assert!(widths.iter().all(|w| (*w - 0.25).abs() < 1e-6));
        assert!(VolumeCandleSettings::default().widths(&[]).is_empty());
    }

    #[test]
    fn the_settings_are_repaired() {
        let settings = VolumeCandleSettings {
            min_width: f32::NAN,
            max_width: 0.1,
            wick_width: 40.0,
            ..VolumeCandleSettings::default()
        }
        .normalized();
        assert_eq!(settings.min_width, 0.25);
        assert!(settings.max_width >= settings.min_width);
        assert_eq!(settings.wick_width, 4.0);
        let bars = VolumeBarSettings {
            size: VolumeSize::Fixed { volume: 0 },
            ..VolumeBarSettings::default()
        }
        .normalized();
        assert_eq!(bars.size, VolumeSize::Fixed { volume: 1 });
        let text = toml::to_string(&bars).unwrap();
        assert_eq!(toml::from_str::<VolumeBarSettings>(&text).unwrap(), bars);
        let candles = VolumeCandleSettings::default();
        let text = toml::to_string(&candles).unwrap();
        assert_eq!(
            toml::from_str::<VolumeCandleSettings>(&text).unwrap(),
            candles
        );
        assert_eq!(
            toml::from_str::<VolumeCandleSettings>("").unwrap(),
            candles,
            "an old file reads with the defaults"
        );
    }

    #[test]
    fn the_size_follows_the_average_volume() {
        let bars = [
            bar(0, 1, 1, 1, 1, 10),
            bar(1, 1, 1, 1, 1, 30),
            bar(2, 1, 1, 1, 1, 20),
        ];
        assert_eq!(VolumeSize::Average { multiple: 1.0 }.resolve(&bars), 20);
        assert_eq!(VolumeSize::Average { multiple: 2.5 }.resolve(&bars), 50);
        assert_eq!(VolumeSize::Fixed { volume: 7 }.resolve(&bars), 7);
        assert_eq!(VolumeSize::Average { multiple: 1.0 }.resolve(&[]), 1);
        assert_eq!(VolumeSize::Average { multiple: 0.0 }.resolve(&bars), 1);
    }

    #[test]
    fn every_closed_volume_bar_holds_exactly_the_volume_and_none_is_lost() {
        let source: Vec<Bar> = (0..200)
            .map(|i| {
                let base = 1_000 + (i % 17) * 9;
                bar(
                    i * 60_000,
                    base,
                    base + 40,
                    base - 30,
                    base + 11,
                    5 + i % 13,
                )
            })
            .collect();
        let total: i64 = source.iter().map(|b| b.volume).sum();
        for path in PricePath::ALL {
            let out = volume_bars(&source, 40, path);
            assert!(out.len() > 5, "{path:?}");
            for closed in &out[..out.len() - 1] {
                assert_eq!(closed.volume, 40, "{path:?}");
                assert!(closed.is_sane(), "{closed:?}");
            }
            let last = out.last().unwrap();
            assert!(last.volume <= 40 && last.is_sane());
            let held: i64 = out.iter().map(|b| b.volume).sum();
            // Each bar rounds its share, so the total is right to within one unit a bar.
            assert!(
                (held - total).abs() <= out.len() as i64,
                "{held} against {total}"
            );
            assert!(out.windows(2).all(|w| w[0].time_ms <= w[1].time_ms));
        }
    }

    #[test]
    fn a_quiet_stretch_is_one_bar_and_a_busy_one_is_several() {
        let mut source = Vec::new();
        for i in 0..10 {
            source.push(bar(i, 100, 101, 99, 100, 1));
        }
        source.push(bar(10, 100, 130, 95, 128, 100));
        let out = volume_bars(&source, 25, PricePath::Ohlc);
        // Ten quiet bars make less than one volume bar; the busy one makes four.
        assert_eq!(out.iter().filter(|b| b.volume == 25).count(), 4);
        assert!(out[0].time_ms == 0);
    }

    #[test]
    fn a_bar_opens_where_the_one_before_closed() {
        let source: Vec<Bar> = (0..60)
            .map(|i| bar(i, 500 + i * 3, 520 + i * 3, 490 + i * 3, 505 + i * 3, 9))
            .collect();
        let out = volume_bars(&source, 30, PricePath::Ohlc);
        for pair in out.windows(2) {
            assert_eq!(pair[1].open, pair[0].close, "{pair:?}");
        }
    }

    #[test]
    fn the_close_path_reads_only_the_closes() {
        let source = [
            bar(0, 100, 200, 50, 100, 10),
            bar(1, 100, 200, 50, 110, 10),
            bar(2, 110, 300, 10, 120, 10),
        ];
        let out = volume_bars(&source, 10, PricePath::Close);
        assert!(out.iter().all(|b| b.high <= 120 && b.low >= 100), "{out:?}");
    }

    #[test]
    fn a_volume_of_one_bar_never_loops() {
        let source = [bar(0, 100, 110, 90, 105, 1_000)];
        let out = volume_bars(&source, 1, PricePath::Ohlc);
        assert!(out.len() <= MAX_OUT);
        assert!(out.len() >= 900);
        assert!(volume_bars(&[], 10, PricePath::Ohlc).is_empty());
    }
}
