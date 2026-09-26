//! What a chart draws, worked out from the prices it holds and its settings: the series laid out
//! on screen (the prices themselves, or the bricks, lines and columns of a non-time chart type),
//! and the values of every indicator.
//!
//! It is computed once per change of the prices or of the settings, not once per frame: a frame
//! only reads it. The chart owns the prices; [`Display`] owns what is derived from them.

use wyck_openapi_model::market::{Bar, Tick};

use super::data::Series;
use super::settings::{ChartKind, ChartSettings};
use super::study::{self, StudyInput, StudyKind, StudyOutput};
use super::tpo;
use super::transform::{self, KagiLine, PnfColumn};
use super::volume;
use super::zone::Zone;

/// What is drawn when it is not the prices as they are.
#[derive(Debug, Clone, Default)]
pub struct Display {
    /// The series laid out on screen, when the chart type builds its own.
    derived: Option<Series>,
    pub kagi: Vec<KagiLine>,
    pub pnf: Vec<PnfColumn>,
    /// The sessions of the TPO chart type, one for each element of the series.
    pub tpo: Vec<tpo::Profile>,
    /// The box, reversal or range of the chart type, in raw price units (0 when it has none).
    pub box_size: i64,
    /// The volume of a volume bar (0 for the other chart types).
    pub volume_size: i64,
    /// The values of each indicator of the settings, by position (`None` for one that draws
    /// nothing per bar, like the volume profile, or is hidden).
    pub studies: Vec<Option<StudyOutput>>,
}

/// The kind a series is drawn as: ticks have no bars, so they are a line (or the constructions
/// that work from single prices).
pub fn effective_kind(series: &Series, kind: ChartKind) -> ChartKind {
    match (series, kind) {
        (
            Series::Ticks(_),
            ChartKind::Candles
            | ChartKind::Hollow
            | ChartKind::Bars
            | ChartKind::HeikinAshi
            | ChartKind::Footprint
            | ChartKind::VolumeCandles,
        ) => ChartKind::Line,
        _ => kind,
    }
}

/// Bars out of anything: ticks become bars of one price.
fn as_bars(series: &Series) -> std::borrow::Cow<'_, [Bar]> {
    match series {
        Series::Bars(bars) => std::borrow::Cow::Borrowed(bars.as_slice()),
        Series::Ticks(ticks) => std::borrow::Cow::Owned(ticks.iter().map(tick_bar).collect()),
    }
}

fn tick_bar(tick: &Tick) -> Bar {
    Bar {
        time_ms: tick.time_ms,
        open: tick.price,
        high: tick.price,
        low: tick.price,
        close: tick.price,
        volume: 1,
    }
}

/// Heikin Ashi bars: each closes at the average of its bar and opens halfway through the body of
/// the one before.
pub fn heikin_ashi(bars: &[Bar]) -> Vec<Bar> {
    let mut out: Vec<Bar> = Vec::with_capacity(bars.len());
    for bar in bars {
        let close = ((bar.open + bar.high + bar.low + bar.close) as f64 / 4.0).round() as i64;
        let open = out
            .last()
            .map_or((bar.open + bar.close) / 2, |p| (p.open + p.close) / 2);
        out.push(Bar {
            open,
            close,
            high: bar.high.max(open).max(close),
            low: bar.low.min(open).min(close),
            ..*bar
        });
    }
    out
}

impl Display {
    /// Works out what to draw for `raw` with `settings`. `unit` is one quote unit of the symbol
    /// in raw price units (box sizes are whole numbers of it).
    pub fn build(raw: &Series, settings: &ChartSettings, unit: i64) -> Self {
        let mut display = Self::default();
        let kind = effective_kind(raw, settings.kind);
        let t = &settings.transform;
        if kind.is_derived() && !raw.is_empty() {
            let bars = as_bars(raw);
            let derived = match kind {
                ChartKind::HeikinAshi => heikin_ashi(&bars),
                ChartKind::Renko => {
                    display.box_size = t.renko_box.resolve(&bars, unit);
                    transform::renko(
                        &transform::samples(&bars, t.renko_path),
                        display.box_size,
                        i64::from(t.renko_reversal),
                        t.renko_wicks,
                    )
                }
                ChartKind::LineBreak => {
                    display.box_size = i64::from(t.line_break);
                    transform::line_break(
                        &transform::samples(&bars, t.line_break_path),
                        t.line_break as usize,
                    )
                }
                ChartKind::Kagi => {
                    display.box_size = t.kagi_reversal.resolve(&bars, unit);
                    let (lines, meta) =
                        transform::kagi(&transform::samples(&bars, t.kagi_path), display.box_size);
                    display.kagi = meta;
                    lines
                }
                ChartKind::PointFigure => {
                    display.box_size = t.pnf_box.resolve(&bars, unit);
                    let (columns, meta) = transform::point_and_figure(
                        &transform::samples(&bars, t.pnf_path),
                        display.box_size,
                        i64::from(t.pnf_reversal),
                    );
                    display.pnf = meta;
                    columns
                }
                ChartKind::Range => {
                    display.box_size = t.range.resolve(&bars, unit);
                    transform::range_bars(&bars, display.box_size, t.range_path)
                }
                ChartKind::VolumeBars => {
                    let v = &settings.volume_bars;
                    display.volume_size = v.size.resolve(&bars);
                    volume::volume_bars(&bars, display.volume_size, v.path)
                }
                ChartKind::Tpo => {
                    let profiles = tpo::build(&bars, &settings.tpo, settings.zone, unit);
                    let elements = profiles.iter().map(tpo::element).collect();
                    display.tpo = profiles;
                    elements
                }
                _ => Vec::new(),
            };
            display.derived = Some(Series::Bars(derived));
        }
        display.studies = compute_studies(display.shown(raw), settings);
        display
    }

    /// The series drawn on screen: the derived one, or the prices themselves.
    pub fn shown<'a>(&'a self, raw: &'a Series) -> &'a Series {
        self.derived.as_ref().unwrap_or(raw)
    }

    /// Whether the series on screen is built here rather than the prices themselves.
    pub fn is_derived(&self) -> bool {
        self.derived.is_some()
    }
}

/// The columns an indicator reads, from a series.
pub fn study_input(series: &Series, zone: Zone) -> StudyInput {
    let bars = as_bars(series);
    let n = bars.len();
    let mut input = StudyInput {
        time: Vec::with_capacity(n),
        open: Vec::with_capacity(n),
        high: Vec::with_capacity(n),
        low: Vec::with_capacity(n),
        close: Vec::with_capacity(n),
        volume: Vec::with_capacity(n),
        day: Vec::with_capacity(n),
    };
    for bar in bars.iter() {
        input.time.push(bar.time_ms);
        input.open.push(bar.open as f64);
        input.high.push(bar.high as f64);
        input.low.push(bar.low as f64);
        input.close.push(bar.close as f64);
        input.volume.push(bar.volume as f64);
        input.day.push(zone.day(bar.time_ms));
    }
    input
}

fn compute_studies(series: &Series, settings: &ChartSettings) -> Vec<Option<StudyOutput>> {
    let wanted = settings
        .studies
        .iter()
        .any(|s| s.visible && s.kind != StudyKind::VolumeProfile && !s.is_script());
    if !wanted || series.is_empty() {
        return vec![None; settings.studies.len()];
    }
    let input = study_input(series, settings.zone);
    settings
        .studies
        .iter()
        .map(|config| {
            (config.visible && config.kind != StudyKind::VolumeProfile && !config.is_script())
                .then(|| study::compute(config, &input))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::StudyConfig;

    fn bars(n: usize) -> Series {
        Series::Bars(
            (0..n)
                .map(|i| {
                    let base = 100_000 + ((i as f64 * 0.3).sin() * 400.0) as i64;
                    Bar {
                        time_ms: i as i64 * 60_000,
                        open: base,
                        high: base + 50,
                        low: base - 50,
                        close: base + 10,
                        volume: 3,
                    }
                })
                .collect(),
        )
    }

    #[test]
    fn plain_kinds_draw_the_prices_as_they_are() {
        let raw = bars(100);
        for kind in [ChartKind::Candles, ChartKind::Line, ChartKind::Baseline] {
            let settings = ChartSettings {
                kind,
                ..ChartSettings::default()
            };
            let display = Display::build(&raw, &settings, 1);
            assert!(!display.is_derived(), "{kind:?}");
            assert_eq!(display.shown(&raw).len(), 100);
        }
    }

    #[test]
    fn every_derived_kind_builds_something_from_bars_and_ticks() {
        let raw = bars(400);
        let ticks = Series::Ticks(
            (0..400)
                .map(|i| Tick {
                    time_ms: i * 1_000,
                    price: 100_000 + ((i as f64 * 0.2).sin() * 300.0) as i64,
                })
                .collect(),
        );
        for kind in ChartKind::ALL.into_iter().filter(|k| k.is_derived()) {
            let settings = ChartSettings {
                kind,
                ..ChartSettings::default()
            };
            let display = Display::build(&raw, &settings, 1);
            assert!(display.is_derived(), "{kind:?}");
            assert!(!display.shown(&raw).is_empty(), "{kind:?}");
            if kind != ChartKind::HeikinAshi {
                let from_ticks = Display::build(&ticks, &settings, 1);
                assert!(!from_ticks.shown(&ticks).is_empty(), "{kind:?} from ticks");
            }
        }
        let kagi = Display::build(
            &raw,
            &ChartSettings {
                kind: ChartKind::Kagi,
                ..ChartSettings::default()
            },
            1,
        );
        assert_eq!(kagi.kagi.len(), kagi.shown(&raw).len());
    }

    #[test]
    fn heikin_ashi_on_ticks_is_a_line() {
        let ticks = Series::Ticks(vec![Tick {
            time_ms: 0,
            price: 1,
        }]);
        assert_eq!(
            effective_kind(&ticks, ChartKind::HeikinAshi),
            ChartKind::Line
        );
        assert_eq!(effective_kind(&ticks, ChartKind::Renko), ChartKind::Renko);
    }

    #[test]
    fn heikin_ashi_averages_the_bar_and_halves_the_last_body() {
        let out = heikin_ashi(&[
            Bar {
                time_ms: 0,
                open: 10,
                high: 20,
                low: 6,
                close: 16,
                volume: 1,
            },
            Bar {
                time_ms: 1,
                open: 16,
                high: 30,
                low: 14,
                close: 28,
                volume: 1,
            },
        ]);
        assert_eq!((out[0].open, out[0].close), (13, 13));
        assert_eq!((out[1].open, out[1].close), (13, 22));
        assert_eq!(out[1].low, 13);
    }

    #[test]
    fn studies_are_computed_by_position_and_hidden_ones_skipped() {
        let raw = bars(200);
        let mut settings = ChartSettings::default();
        settings.studies.push(StudyConfig::new(StudyKind::Sma));
        let mut hidden = StudyConfig::new(StudyKind::Rsi);
        hidden.visible = false;
        settings.studies.push(hidden);
        settings
            .studies
            .push(StudyConfig::new(StudyKind::VolumeProfile));
        let display = Display::build(&raw, &settings, 1);
        assert_eq!(display.studies.len(), 3);
        assert!(display.studies[0].is_some());
        assert!(display.studies[1].is_none());
        assert!(display.studies[2].is_none());
    }
}
