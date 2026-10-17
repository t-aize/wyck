//! Turns the data, the view and the settings into a flat list of drawing commands.
//!
//! Building the list is plain arithmetic (no window needed), so it can be unit tested, and the
//! amount of work is bounded by the size of the screen, never by the size of the data:
//!
//! - Only the points that reach the plot are visited.
//! - Once points are narrower than a pixel they are folded into one column each (their low, high,
//!   first and last), so a chart with a hundred thousand bars costs what a screenful costs.
//!
//! The plot is cut into bands (`geometry`): the prices on top, then one pane per indicator that
//! needs its own scale. Each band has its own [`PriceMap`]; the time axis is shared.

mod axes;
mod bands;
pub(crate) mod cmd;
pub(crate) mod color;
mod ctx;
mod drawings;
mod footprint;
pub(crate) mod geometry;
mod labels;
pub(crate) mod palette;
pub(crate) mod price;
pub(crate) mod scale;
mod series;
mod studies;
mod tpo;

use self::bands::Plan;
pub(crate) use self::cmd::{Align, Cmd, P, font, hsla, rgb_alpha, with_alpha};
use self::ctx::Ctx;
pub(crate) use self::geometry::{Band, Geometry, axis_w};
pub(crate) use self::price::PriceMap;
use self::scale::{geometry, main_map};
use super::data::Series;
use super::display::{Display, effective_kind};
use super::options::CrosshairStyle;
use super::order_flow::flow::Flow;
use super::settings::{ChartKind, ChartSettings};
use super::view::View;
use wyck_core::math::f32_of;
use wyck_drawings::model::Drawing;
use wyck_drawings::model::style::Dash;
use wyck_indicators::spec::Placement;
use wyck_market::Timeframe;

/// The opacity of a grid color the user chose.
const GRID_ALPHA: f32 = 0.3;

/// The colors of a TPO profile that a theme does not have: mid tones that read on light and dark.
pub(crate) const TPO_MARKS: u32 = 0x7f_93b2;
pub(crate) const TPO_POC: u32 = 0xf0_a020;
pub(crate) const TPO_VALUE_AREA: u32 = 0x5b_8def;
pub(crate) const TPO_IB: u32 = 0xb2_7be6;
pub(crate) const TPO_SINGLE: u32 = 0xe5_586a;

/// One unit of the symbol's last decimal, in raw price units.
pub fn quote_unit(digits: u32) -> f64 {
    10f64.powi(5 - i32::try_from(digits.min(5)).unwrap_or(5))
}

/// A horizontal line the chart is told to show on the prices: an order, a position, an alert.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceMark {
    /// Raw price units.
    pub price: f64,
    pub color: u32,
    pub dash: Dash,
    pub width: f32,
    /// Where the line starts, from the left of the plot (a line may start at a time).
    pub from_x: Option<f32>,
    /// Whether the price shows as a tag on the axis.
    pub axis_tag: bool,
}

/// The drawings to show on a chart.
pub struct DrawingView<'a> {
    pub list: &'a [Drawing],
    /// The drawing being made, which follows the pointer.
    pub creating: Option<&'a Drawing>,
    pub selected: Option<u64>,
    /// The drawings selected along with it.
    pub also: &'a [u64],
    /// Whether a drawing shows on this timeframe.
    pub visible: &'a dyn Fn(&Drawing) -> bool,
}

pub struct Frame<'a> {
    /// The prices as held, for the last price.
    pub raw: &'a Series,
    pub display: &'a Display,
    pub settings: &'a ChartSettings,
    pub view: &'a View,
    pub timeframe: Timeframe,
    pub digits: u32,
    pub pip_position: Option<i64>,
    pub origin: P,
    pub w: f64,
    pub h: f64,
    /// Device pixels per logical pixel, for snapping lines to the pixel grid.
    pub scale: f32,
    /// The pointer, from the top left of the chart.
    pub hover: Option<P>,
    /// The pointer of another chart (time and raw price), when the crosshairs are linked.
    pub remote: Option<(i64, f64)>,
    pub ask: Option<i64>,
    pub now_ms: i64,
    pub palette: palette::Palette,
    pub drawings: Option<DrawingView<'a>>,
    pub marks: &'a [PriceMark],
    /// What traded at each price of each bar, for the footprint chart type.
    pub flow: Option<&'a Flow>,
    /// The text written faint behind the prices, when the chart has a watermark.
    pub watermark: Option<String>,
}

impl Frame<'_> {
    fn series(&self) -> &Series {
        self.display.shown(self.raw)
    }

    fn kind(&self) -> ChartKind {
        effective_kind(self.raw, self.settings.kind)
    }

    /// The time between two points, for placing times past the data.
    fn step_ms(&self) -> f64 {
        let nominal = if self.display.is_derived() && self.settings.kind != ChartKind::HeikinAshi {
            None
        } else {
            self.timeframe.bar_ms()
        };
        self.series().step_ms(nominal)
    }
}

/// Where the crosshair is (from the top left of the chart) and which band it is in: under the
/// pointer when it is over this chart, else where another chart's pointer is (on the prices,
/// when its time is on screen).
fn pointer_of(frame: &Frame<'_>, geometry: &Geometry, map: &PriceMap) -> Option<(P, usize)> {
    let plot_w = geometry.plot_w();
    if let Some((x, y)) = frame.hover
        && (0.0..=plot_w).contains(&f64::from(x))
        && let Some(band) = geometry.band_at(f64::from(y))
    {
        return Some(((x, y), band));
    }
    let (time_ms, price) = frame.remote?;
    let series = frame.series();
    let index = series.index_of_time(time_ms, frame.step_ms())?;
    let x = frame.view.x_of(index, series.len(), plot_w);
    if !(0.0..=plot_w).contains(&x) {
        return None;
    }
    // Another symbol's price means nothing here: only its time is shown, as the vertical line.
    let y = if price.is_finite() {
        map.y(price)
    } else {
        -1.0
    };
    let main = geometry.main();
    (main.contains(y) || !price.is_finite()).then_some(((f32_of(x), f32_of(y)), 0))
}

/// Builds every command for one frame: the prices band, one band for each indicator pane, the
/// crosshair, then the axes over them.
pub fn build(frame: &Frame<'_>) -> Vec<Cmd> {
    let series = frame.series();
    let geometry = geometry(frame.settings, frame.w, frame.h);
    let Some(map) = main_map(
        frame.raw,
        frame.display,
        frame.settings,
        frame.view,
        &geometry,
        frame.digits,
    ) else {
        return vec![axes::axes_background(frame, &geometry)];
    };
    let pointer = if frame.settings.crosshair == CrosshairStyle::Off {
        None
    } else {
        pointer_of(frame, &geometry, &map)
    };
    let (ox, oy) = frame.origin;
    let cx = Ctx {
        f: frame,
        series,
        map,
        len: series.len(),
        plot_w: geometry.plot_w(),
        band: geometry.main(),
        ox,
        oy,
    };
    let plan = Plan::new(&cx);

    let mut cmds = vec![bands::prices(&cx, &plan, &geometry, pointer)];
    let panes = bands::panes(&cx, &plan, &geometry, pointer, &mut cmds);
    cmds.extend(bands::crosshair_column(&cx, &geometry, pointer));
    labels::draw(&cx, &plan, &geometry, &panes, pointer, &mut cmds);
    cmds
}

#[cfg(test)]
mod tests {
    use super::axes::*;
    use super::cmd::{count, rect_widths, texts};
    use super::color::rgb;

    use super::palette::*;
    use super::*;
    use crate::options::{ChartColors, CrosshairStyle, ScaleMargin};
    use crate::order_flow::flow::Flow;
    use crate::settings::ScaleMode;
    use crate::view::PriceScale;
    use crate::zone::Zone;
    use wyck_core::math::{count_f64, count_i64, f32_of, f64_to_i64};
    use wyck_indicators::compute::{ScriptDrawing, StudyOutput};
    use wyck_indicators::config::StudyConfig;
    use wyck_indicators::kind::StudyKind;
    use wyck_indicators::spec::ValueFormat;
    use wyck_market::{Bar, Period, Quote, Tick};

    fn bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let base = 100_000 + (count_i64(i) % 50) * 10;
                Bar {
                    time_ms: 1_767_571_200_000 + count_i64(i) * 60_000,
                    open: base,
                    high: base + 30,
                    low: base - 20,
                    close: base + 10,
                    volume: 1 + count_i64(i) % 7,
                }
            })
            .collect()
    }

    struct Fixture {
        raw: Series,
        display: Display,
        settings: ChartSettings,
        view: View,
        flow: Flow,
    }

    impl Fixture {
        fn new(raw: Series, settings: ChartSettings, view: View) -> Self {
            let display = Display::build(&raw, &settings, 1);
            Self {
                raw,
                display,
                settings,
                view,
                flow: Flow::default(),
            }
        }

        fn frame(&self) -> Frame<'_> {
            Frame {
                raw: &self.raw,
                display: &self.display,
                settings: &self.settings,
                view: &self.view,
                timeframe: Timeframe::DEFAULT,
                digits: 5,
                pip_position: Some(4),
                origin: (0.0, 0.0),
                w: 1_000.0,
                h: 600.0,
                scale: 1.0,
                hover: Some((300.0, 200.0)),
                remote: None,
                ask: Some(100_020),
                now_ms: 1_767_571_200_000,
                palette: Palette::new(),
                drawings: None,
                marks: &[],
                flow: Some(&self.flow),
                watermark: None,
            }
        }
    }

    fn kind(kind: ChartKind) -> ChartSettings {
        ChartSettings {
            kind,
            ..ChartSettings::default()
        }
    }

    fn drawn(settings: ChartSettings, edit: impl FnOnce(&mut Frame<'_>)) -> usize {
        let f = Fixture::new(Series::Bars(bars(300)), settings, View::new(8.0));
        let mut frame = f.frame();
        edit(&mut frame);
        count(&build(&frame))
    }

    #[test]
    fn a_screenful_of_candles_draws() {
        let f = Fixture::new(
            Series::Bars(bars(500)),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        assert!(count(&build(&f.frame())) > 100);
    }

    #[test]
    fn script_drawings_follow_indicator_visibility_and_reach_the_scene() {
        let mut settings = kind(ChartKind::Candles);
        settings.studies.push(StudyConfig::for_script("generated"));
        let mut fixture = Fixture::new(Series::Bars(bars(20)), settings, View::new(8.0));
        let mut drawing = Drawing::new(
            0,
            wyck_drawings::model::tool::Tool::Text,
            vec![wyck_drawings::model::Point {
                t: 1_767_571_200_000,
                p: 100_020.0,
            }],
        );
        drawing.text = "SCRIPT_MARKER_UNIQUE".into();
        fixture.display.studies[0] = Some(StudyOutput {
            drawings: vec![ScriptDrawing {
                key: "marker".into(),
                drawing,
            }],
            ..StudyOutput::default()
        });
        let cmds = build(&fixture.frame());
        assert!(
            texts(&cmds)
                .iter()
                .any(|text| text.contains("SCRIPT_MARKER_UNIQUE"))
        );
        let main = cmds
            .iter()
            .find_map(|cmd| match cmd {
                Cmd::Clip { inner, .. }
                    if inner.iter().any(
                        |cmd| matches!(cmd, Cmd::Text { text, .. } if text == "SCRIPT_MARKER_UNIQUE"),
                    ) =>
                {
                    Some(inner)
                }
                _ => None,
            })
            .unwrap();
        let label = main
            .iter()
            .position(|cmd| matches!(cmd, Cmd::Text { text, .. } if text == "SCRIPT_MARKER_UNIQUE"))
            .unwrap();
        let candle = main.iter().rposition(|cmd| matches!(cmd, Cmd::Rect { fill, .. } if *fill == hsla(fixture.frame().palette.up) || *fill == hsla(fixture.frame().palette.down))).unwrap();
        assert!(
            label < candle,
            "the price series must cover script drawings"
        );
        fixture.settings.studies[0].visible = false;
        assert!(
            !texts(&build(&fixture.frame()))
                .iter()
                .any(|text| text.contains("SCRIPT_MARKER_UNIQUE"))
        );
    }

    #[test]
    fn the_work_does_not_grow_with_the_data() {
        // A hundred thousand bars squeezed into one screen make about as many commands as a
        // thousand: they are folded into pixel columns.
        for chart_kind in ChartKind::ALL {
            let mut view = View::new(0.2);
            view.offset = View::home_offset();
            let small = Fixture::new(Series::Bars(bars(1_000)), kind(chart_kind), view);
            let huge = Fixture::new(Series::Bars(bars(100_000)), kind(chart_kind), view);
            let a = count(&build(&small.frame()));
            let b = count(&build(&huge.frame()));
            assert!(b < 8_000, "{chart_kind:?}: {b} commands");
            assert!(b < a * 8 + 800, "{chart_kind:?}: {a} against {b}");
        }
    }

    #[test]
    fn a_line_over_many_ticks_stays_bounded() {
        let ticks: Vec<Tick> = (0..200_000)
            .map(|i| Tick {
                time_ms: 1_767_571_200_000 + i64::from(i) * 100,
                price: 100_000 + ((i64::from(i) * 7919) % 400),
            })
            .collect();
        let mut view = View::new(0.2);
        view.offset = View::home_offset();
        let f = Fixture::new(Series::Ticks(ticks), kind(ChartKind::Line), view);
        let cmds = build(&f.frame());
        assert!(count(&cmds) < 1_000, "{}", count(&cmds));
    }

    #[test]
    fn an_empty_series_only_draws_the_axes() {
        let f = Fixture::new(
            Series::Bars(Vec::new()),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        assert_eq!(build(&f.frame()).len(), 1);
    }

    #[test]
    fn the_price_map_round_trips_and_keeps_the_data_on_screen() {
        let f = Fixture::new(
            Series::Bars(bars(200)),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        let g = geometry(&f.settings, 1_000.0, 600.0);
        let map = main_map(&f.raw, &f.display, &f.settings, &f.view, &g, 5).unwrap();
        for price in [100_000.0, 100_250.0] {
            assert!((map.price(map.y(price)) - price).abs() < 1e-6);
        }
        assert!(map.y(100_500.0) >= 0.0 && map.y(99_980.0) <= g.plot_h());
    }

    #[test]
    fn a_manual_scale_is_honoured() {
        let mut view = View::new(8.0);
        view.price = PriceScale::Manual {
            lo: 0.0,
            hi: 1_000_000.0,
        };
        let f = Fixture::new(Series::Bars(bars(50)), kind(ChartKind::Candles), view);
        let g = geometry(&f.settings, 1_000.0, 600.0);
        let map = main_map(&f.raw, &f.display, &f.settings, &f.view, &g, 5).unwrap();
        assert_eq!((map.lo, map.hi), (0.0, 1_000_000.0));
    }

    #[test]
    fn another_charts_pointer_draws_a_crosshair_when_its_time_is_on_screen() {
        let f = Fixture::new(
            Series::Bars(bars(200)),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        let mut frame = f.frame();
        frame.hover = None;
        let bare = count(&build(&frame));
        let time = f.raw.time_at(190).unwrap();
        frame.remote = Some((time, 100_030.0));
        let with_pointer = count(&build(&frame));
        assert!(with_pointer > bare, "{with_pointer} against {bare}");
        frame.remote = Some((f.raw.time_at(0).unwrap() - 86_400_000, 100_030.0));
        assert_eq!(count(&build(&frame)), bare);
    }

    #[test]
    fn the_local_pointer_wins_over_a_remote_one() {
        let f = Fixture::new(
            Series::Bars(bars(200)),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        let mut frame = f.frame();
        let local_only = count(&build(&frame));
        frame.remote = Some((f.raw.time_at(150).unwrap(), 100_030.0));
        assert_eq!(count(&build(&frame)), local_only);
    }

    #[test]
    fn quotes_with_fewer_decimals_have_a_bigger_unit() {
        assert_eq!(quote_unit(5), 1.0);
        assert_eq!(quote_unit(3), 100.0);
        assert_eq!(quote_unit(2), 1_000.0);
    }

    #[test]
    fn every_indicator_draws_on_the_prices_or_in_its_pane() {
        let mut settings = ChartSettings::default();
        for kind in StudyKind::ALL {
            settings.studies.push(StudyConfig::new(kind));
        }
        let f = Fixture::new(Series::Bars(bars(600)), settings, View::new(8.0));
        let g = geometry(&f.settings, 1_000.0, 600.0);
        assert_eq!(g.bands.len(), 1 + f.settings.panes().len());
        let bare = Fixture::new(
            Series::Bars(bars(600)),
            ChartSettings::default(),
            View::new(8.0),
        );
        let with = count(&build(&f.frame()));
        let without = count(&build(&bare.frame()));
        assert!(with > without + 500, "{with} against {without}");
    }

    #[test]
    fn a_percent_scale_labels_the_axis_in_percent() {
        let settings = ChartSettings {
            scale: ScaleMode::Percent,
            ..ChartSettings::default()
        };
        let f = Fixture::new(Series::Bars(bars(300)), settings, View::new(8.0));
        let labels = texts(&build(&f.frame()));
        assert!(
            labels.iter().filter(|t| t.ends_with('%')).count() >= 3,
            "{labels:?}"
        );
    }

    #[test]
    fn marks_draw_a_line_and_a_tag() {
        let f = Fixture::new(
            Series::Bars(bars(100)),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        let marks = [PriceMark {
            price: 100_200.0,
            color: 0x00_ff00,
            dash: Dash::Dashed,
            width: 1.0,
            from_x: None,
            axis_tag: true,
        }];
        let mut frame = f.frame();
        let before = texts(&build(&frame)).len();
        frame.marks = &marks;
        let after = texts(&build(&frame));
        // The tag is added; a label of the scale under it gives way.
        assert!(after.len() >= before, "{before} then {after:?}");
        assert!(after.contains(&"1.00200".to_owned()), "{after:?}");
    }

    #[test]
    #[expect(
        clippy::float_cmp_const,
        reason = "the band is clamped to exactly the minimum, a constant returned as is"
    )]
    fn a_compact_atr_pane_keeps_its_last_value_on_the_axis() {
        let mut settings = ChartSettings::default();
        let mut atr = StudyConfig::new(StudyKind::Atr);
        atr.weight = 0.1;
        settings.studies.push(atr);
        let f = Fixture::new(Series::Bars(bars(300)), settings, View::new(8.0));
        let g = geometry(&f.settings, 1_000.0, 600.0);
        assert_eq!(g.bands[1].h, geometry::MIN_BAND);
        let value = f.display.studies[0].as_ref().unwrap().plots[0]
            .values
            .iter()
            .rev()
            .find(|v| v.is_finite())
            .copied()
            .unwrap();
        let label = price::format_value(value, ValueFormat::Price, 5);
        let cmds = build(&f.frame());
        assert!(
            cmds.iter().any(|cmd| match cmd {
                Cmd::Tag {
                    text,
                    y,
                    height,
                    fixed_width: Some(_),
                    ..
                } => {
                    text == &label
                        && f32_of(g.bands[1].top) <= *y
                        && *y + *height <= f32_of(g.bands[1].bottom())
                }
                _ => false,
            }),
            "missing ATR tag: {label}"
        );
    }

    #[test]
    fn the_grid_options_take_lines_out_one_kind_at_a_time() {
        let with = |grid, horizontal, vertical| {
            drawn(
                ChartSettings {
                    grid,
                    grid_horizontal: horizontal,
                    grid_vertical: vertical,
                    ..ChartSettings::default()
                },
                |_| {},
            )
        };
        let both = with(true, true, true);
        let horizontal_only = with(true, true, false);
        let vertical_only = with(true, false, true);
        let none = with(false, true, true);
        assert!(both > horizontal_only && both > vertical_only, "{both}");
        assert!(horizontal_only > none && vertical_only > none);
        assert_eq!(with(true, false, false), none);
    }

    #[test]
    fn a_crosshair_that_is_off_draws_neither_lines_nor_tags() {
        let with = |crosshair| {
            drawn(
                ChartSettings {
                    crosshair,
                    ..ChartSettings::default()
                },
                |_| {},
            )
        };
        let dashed = with(CrosshairStyle::Dashed);
        assert_eq!(with(CrosshairStyle::Solid), dashed);
        let off = with(CrosshairStyle::Off);
        // Two lines and two tags. A tag may also cover one scale label.
        assert!((3..=4).contains(&(dashed - off)));
        assert_eq!(CrosshairStyle::Solid.dash(), None);
        assert!(CrosshairStyle::Dotted.dash().is_some());
    }

    #[test]
    fn the_price_lines_and_tags_can_each_be_hidden() {
        let f = Fixture::new(
            Series::Bars(bars(300)),
            ChartSettings::default(),
            View::new(8.0),
        );
        let mut frame = f.frame();
        // The bar that is current at the moment of the frame, so the countdown shows.
        frame.now_ms = f.raw.last_time().unwrap() + 1_000;
        let all = count(&build(&frame));
        let hide = |change: fn(&mut crate::options::PriceLines)| {
            let mut settings = ChartSettings::default();
            change(&mut settings.price_lines);
            let f = Fixture::new(Series::Bars(bars(300)), settings, View::new(8.0));
            let mut frame = f.frame();
            frame.now_ms = f.raw.last_time().unwrap() + 1_000;
            count(&build(&frame))
        };
        assert_eq!(all - hide(|p| p.last_line = false), 1);
        assert_eq!(all - hide(|p| p.last_tag = false), 1);
        assert_eq!(all - hide(|p| p.countdown = false), 1);
        assert_eq!(all - hide(|p| p.ask_line = false), 1);
    }

    #[test]
    fn the_watermark_is_written_behind_the_prices() {
        let f = Fixture::new(
            Series::Bars(bars(50)),
            ChartSettings::default(),
            View::new(8.0),
        );
        let mut frame = f.frame();
        assert!(!texts(&build(&frame)).iter().any(|t| t == "EURUSD, M5"));
        frame.watermark = Some("EURUSD, M5".to_owned());
        assert!(texts(&build(&frame)).iter().any(|t| t == "EURUSD, M5"));
    }

    #[test]
    fn day_separators_only_show_when_asked_and_on_intraday_bars() {
        let hourly: Vec<Bar> = (0..120)
            .map(|i| Bar {
                time_ms: 1_767_571_200_000 + i * 3_600_000,
                open: 100_000,
                high: 100_030,
                low: 99_980,
                close: 100_010,
                volume: 1,
            })
            .collect();
        let build_with = |day_breaks: bool| {
            let mut settings = ChartSettings::default();
            settings.price_lines.day_breaks = day_breaks;
            let f = Fixture::new(Series::Bars(hourly.clone()), settings, View::new(8.0));
            let mut frame = f.frame();
            frame.timeframe = Timeframe::Bars(Period::H1);
            count(&build(&frame))
        };
        // 120 hourly bars are five days: at least four changes of the day are on screen.
        assert!(build_with(true) >= build_with(false) + 4);
        let f = Fixture::new(
            Series::Bars(hourly),
            ChartSettings::default(),
            View::new(8.0),
        );
        let mut frame = f.frame();
        frame.timeframe = Timeframe::Bars(Period::D1);
        assert_eq!(day_breaks(&frame, 0, 120), [] as [usize; 0]);
    }

    #[test]
    fn a_chart_overrides_only_the_colors_it_sets() {
        let theme = Palette::new();
        let colors = ChartColors {
            up: Some(0x00_ff00),
            background: Some(0x10_1010),
            ..ChartColors::default()
        };
        let own = Palette::for_chart_with_base(&colors, Palette::new());
        assert_eq!(own.up, rgb(0x00_ff00));
        assert_eq!(own.bg, rgb(0x10_1010));
        assert_eq!(own.down, theme.down);
        let grid = Palette::for_chart_with_base(
            &ChartColors {
                grid: Some(0xff_ffff),
                ..ChartColors::default()
            },
            Palette::new(),
        );
        assert!(grid.grid.a < 0.5, "a chosen grid color is drawn faint");
        assert_eq!(own.line, theme.line);
    }

    #[test]
    fn the_margins_widen_the_automatic_price_scale() {
        let range = |margin| {
            let settings = ChartSettings {
                margin,
                ..ChartSettings::default()
            };
            let f = Fixture::new(Series::Bars(bars(300)), settings, View::new(8.0));
            let geometry = geometry(&f.settings, 1_000.0, 600.0);
            let map = main_map(&f.raw, &f.display, &f.settings, &f.view, &geometry, 5).unwrap();
            map.hi - map.lo
        };
        assert!(range(ScaleMargin::Tight) < range(ScaleMargin::Normal));
        assert!(range(ScaleMargin::Normal) < range(ScaleMargin::Loose));
    }

    #[test]
    fn a_volume_candle_is_as_wide_as_its_volume() {
        let mut data = bars(40);
        // Two neighbours, one quiet and one very busy: same room, different widths.
        data[20].volume = 1;
        data[21].volume = 100_000;
        let mut settings = kind(ChartKind::VolumeCandles);
        settings.volume_candles.min_width = 0.2;
        settings.volume_candles.max_width = 1.0;
        settings.volume_candles.reference = crate::order_flow::volume::WidthReference::Max;
        settings.volume_candles.scale = crate::order_flow::volume::WidthScale::Linear;
        let f = Fixture::new(Series::Bars(data), settings, View::new(20.0));
        let widths = rect_widths(&build(&f.frame()));
        let (mut thin, mut wide) = (f32::MAX, 0.0_f32);
        for w in widths.iter().copied().filter(|w| *w >= 3.0 && *w <= 20.5) {
            thin = thin.min(w);
            wide = wide.max(w);
        }
        assert!(wide >= 19.0, "the busiest fills its room: {wide}");
        assert!(thin <= 6.0, "the quietest is thin: {thin}");
    }

    #[test]
    fn volume_candles_can_write_the_volume_over_wide_candles() {
        let mut settings = kind(ChartKind::VolumeCandles);
        settings.volume_candles.labels = true;
        let f = Fixture::new(Series::Bars(bars(30)), settings, View::new(40.0));
        let text = texts(&build(&f.frame()));
        assert!(
            text.iter()
                .any(|t| t.parse::<i64>().is_ok_and(|v| (1..=7).contains(&v))),
            "{text:?}"
        );
    }

    #[test]
    fn volume_bars_draw_and_make_a_different_number_of_bars_than_the_source() {
        let mut settings = kind(ChartKind::VolumeBars);
        settings.volume_bars.size = crate::order_flow::volume::VolumeSize::Fixed { volume: 20 };
        let f = Fixture::new(Series::Bars(bars(200)), settings, View::new(8.0));
        assert!(f.display.is_derived());
        assert_eq!(f.display.volume_size, 20);
        assert_ne!(f.display.shown(&f.raw).len(), 200);
        assert!(count(&build(&f.frame())) > 50);
    }

    #[test]
    fn renko_wicks_add_a_line_to_each_brick() {
        let source = bars(400);
        let plain = kind(ChartKind::Renko);
        let mut wicked = kind(ChartKind::Renko);
        wicked.transform.renko_wicks = true;
        wicked.transform.renko_box = crate::transform::settings::BoxSize::Fixed { price: 0.0002 };
        let mut plain = plain;
        plain.transform.renko_box = wicked.transform.renko_box;
        let a = Fixture::new(Series::Bars(source.clone()), plain, View::new(10.0));
        let b = Fixture::new(Series::Bars(source), wicked, View::new(10.0));
        assert!(count(&build(&b.frame())) > count(&build(&a.frame())));
    }

    #[test]
    fn bricks_can_drop_their_border_and_change_their_width() {
        let mut narrow = kind(ChartKind::Renko);
        narrow.transform.brick_width = 0.4;
        narrow.transform.brick_border = false;
        narrow.transform.renko_box = crate::transform::settings::BoxSize::Fixed { price: 0.0002 };
        let mut wide = narrow.clone();
        wide.transform.brick_width = 1.0;
        let a = Fixture::new(Series::Bars(bars(300)), narrow, View::new(20.0));
        let b = Fixture::new(Series::Bars(bars(300)), wide, View::new(20.0));
        let widest = |f: &Fixture| {
            rect_widths(&build(&f.frame()))
                .into_iter()
                .filter(|w| *w <= 21.0)
                .fold(0.0_f32, f32::max)
        };
        assert!(
            widest(&a) < widest(&b),
            "{} against {}",
            widest(&a),
            widest(&b)
        );
    }

    #[test]
    fn every_price_based_type_draws_from_the_whole_path_by_default() {
        for chart_kind in [
            ChartKind::Renko,
            ChartKind::LineBreak,
            ChartKind::Kagi,
            ChartKind::PointFigure,
            ChartKind::Range,
        ] {
            let f = Fixture::new(Series::Bars(bars(300)), kind(chart_kind), View::new(8.0));
            assert!(f.display.is_derived(), "{chart_kind:?}");
            assert!(count(&build(&f.frame())) > 20, "{chart_kind:?}");
        }
    }

    /// The flow of some bars: the price walks from the low to the high (buyers) and back (sellers).
    fn flow_of(bars: &[Bar]) -> Flow {
        let times: Vec<i64> = bars.iter().map(|b| b.time_ms).collect();
        let mut quotes = Vec::new();
        for bar in bars {
            let mut time = bar.time_ms;
            let up: Vec<i64> = (bar.low..=bar.high).step_by(5).collect();
            for price in up.iter().chain(up.iter().rev()) {
                time += 10;
                quotes.push(Quote {
                    time_ms: time,
                    bid: Some(*price),
                    ask: Some(*price + 2),
                });
            }
        }
        let mut flow = Flow::default();
        flow.ingest(&times, Some(60_000), &quotes, true);
        flow.cover_from(times[0]);
        flow
    }

    fn footprint_fixture(n: usize, bar_px: f64) -> Fixture {
        let data = bars(n);
        let mut f = Fixture::new(
            Series::Bars(data.clone()),
            kind(ChartKind::Footprint),
            View::new(bar_px),
        );
        f.flow = flow_of(&data);
        f
    }

    #[test]
    fn a_footprint_shows_numbers_and_the_summary_of_each_bar() {
        let f = footprint_fixture(60, 110.0);
        let labels = texts(&build(&f.frame()));
        assert!(labels.contains(&"Delta".to_owned()), "{labels:?}");
        assert!(labels.contains(&"Vol".to_owned()));
        let numbers = labels
            .iter()
            .filter(|t| t.parse::<u32>().is_ok_and(|n| n > 0))
            .count();
        assert!(numbers > 20, "{numbers} numbers in {labels:?}");
    }

    #[test]
    fn the_cells_can_show_the_delta_or_the_volume() {
        let mut f = footprint_fixture(60, 110.0);
        f.settings.footprint.mode = crate::order_flow::footprint::CellMode::Delta;
        let labels = texts(&build(&f.frame()));
        assert!(
            labels
                .iter()
                .any(|t| t.starts_with('+') || t.starts_with('-')),
            "{labels:?}"
        );
        f.settings.footprint.summary = false;
        let labels = texts(&build(&f.frame()));
        assert!(!labels.contains(&"Delta".to_owned()), "the summary is off");
    }

    #[test]
    fn the_numbers_can_be_turned_off_and_the_cells_still_draw() {
        let mut f = footprint_fixture(60, 110.0);
        let with = count(&build(&f.frame()));
        f.settings.footprint.numbers = false;
        let cmds = build(&f.frame());
        let numbers = texts(&cmds)
            .iter()
            .filter(|t| t.parse::<u32>().is_ok_and(|n| n > 0 && n < 1_000))
            .count();
        assert!(numbers < 10, "{numbers}");
        assert!(count(&cmds) > 50 && count(&cmds) < with);
    }

    #[test]
    fn narrow_bars_are_plain_candles() {
        let f = footprint_fixture(300, 8.0);
        let cmds = build(&f.frame());
        assert!(!texts(&cmds).contains(&"Delta".to_owned()));
        assert!(count(&cmds) > 100);
    }

    #[test]
    fn a_timeframe_a_footprint_cannot_be_built_for_gives_candles() {
        let f = footprint_fixture(60, 110.0);
        let mut frame = f.frame();
        frame.timeframe = Timeframe::Bars(Period::W1);
        assert!(!texts(&build(&frame)).contains(&"Delta".to_owned()));
    }

    #[test]
    fn bars_without_flow_are_still_drawn() {
        let mut f = footprint_fixture(60, 110.0);
        f.flow = Flow::default();
        let cmds = build(&f.frame());
        // A wick and a body for each of the bars on screen.
        assert!(count(&cmds) > 16, "{}", count(&cmds));
    }

    #[test]
    fn a_footprint_frame_stays_bounded_at_any_zoom() {
        for bar_px in [24.0, 60.0, 110.0, 260.0] {
            let f = footprint_fixture(500, bar_px);
            let n = count(&build(&f.frame()));
            assert!(n < 12_000, "{bar_px}px: {n} commands");
        }
    }

    #[test]
    fn the_prices_leave_room_for_the_summary_under_them() {
        let f = footprint_fixture(60, 110.0);
        let g = geometry(&f.settings, 1_000.0, 600.0);
        let with = main_map(&f.raw, &f.display, &f.settings, &f.view, &g, 5).unwrap();
        let mut off = f.settings.clone();
        off.footprint.summary = false;
        let without = main_map(&f.raw, &f.display, &off, &f.view, &g, 5).unwrap();
        assert!(with.bottom < without.bottom - 20.0);
    }

    /// Three days of five minute bars, so a session has a dozen letters or so per day.
    fn tpo_bars() -> Vec<Bar> {
        (0..864)
            .map(|i| {
                let wave = f64_to_i64((f64::from(i) * 0.05).sin() * 300.0);
                let base = 100_000 + wave;
                Bar {
                    time_ms: 1_767_571_200_000 + i64::from(i) * 300_000,
                    open: base,
                    high: base + 60,
                    low: base - 60,
                    close: base + 10,
                    volume: 5,
                }
            })
            .collect()
    }

    fn tpo_fixture(edit: impl FnOnce(&mut ChartSettings), bar_px: f64) -> Fixture {
        let mut settings = kind(ChartKind::Tpo);
        settings.zone = Zone::Utc;
        settings.tpo.row_units = 20;
        edit(&mut settings);
        // A little air after the last profile, not the six points a narrow view keeps.
        let mut view = View::new(bar_px);
        view.offset = 1.0;
        Fixture::new(Series::Bars(tpo_bars()), settings, view)
    }

    /// How many strokes of `points` or more points a frame has (the ring of an O has twenty one).
    fn long_strokes(cmds: &[Cmd], points: usize) -> usize {
        cmds.iter()
            .map(|c| match c {
                Cmd::Stroke { points: p, .. } if p.len() >= points => 1,
                Cmd::Clip { inner, .. } => long_strokes(inner, points),
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn a_tpo_chart_makes_one_element_for_each_session() {
        let f = tpo_fixture(|_| {}, 200.0);
        assert_eq!(f.display.tpo.len(), 3);
        assert_eq!(f.display.shown(&f.raw).len(), 3);
        assert!(f.display.is_derived());
    }

    #[test]
    fn profiles_are_written_in_letters_with_their_levels() {
        let f = tpo_fixture(
            |s| s.tpo.display = crate::order_flow::tpo::TpoDisplay::Letters,
            200.0,
        );
        let text = texts(&build(&f.frame()));
        assert!(text.iter().any(|t| t == "A"), "{text:?}");
        assert!(text.iter().any(|t| t == "B"), "{text:?}");
        assert!(text.iter().any(|t| t.starts_with("POC ")), "{text:?}");
        assert!(text.iter().any(|t| t.starts_with("VAH ")), "{text:?}");
        assert!(text.iter().any(|t| t.starts_with("VAL ")), "{text:?}");
    }

    #[test]
    fn blocks_have_no_letters_and_the_levels_can_be_turned_off() {
        let f = tpo_fixture(
            |s| {
                s.tpo.display = crate::order_flow::tpo::TpoDisplay::Blocks;
                s.tpo.poc = false;
                s.tpo.value_area = false;
            },
            200.0,
        );
        let text = texts(&build(&f.frame()));
        assert!(!text.iter().any(|t| t == "A" || t == "B"), "{text:?}");
        assert!(
            !text
                .iter()
                .any(|t| t.starts_with("POC ") || t.starts_with("VAH "))
        );
        let plain = tpo_fixture(|s| s.tpo.labels = false, 200.0);
        let text = texts(&build(&plain.frame()));
        assert!(!text.iter().any(|t| t.starts_with("POC ")), "{text:?}");
    }

    #[test]
    fn the_profile_extras_add_what_they_draw() {
        let bare = tpo_fixture(
            |s| {
                s.tpo.value_area = false;
                s.tpo.poc_line = false;
                s.tpo.initial_balance = false;
                s.tpo.single_prints = false;
                s.tpo.poor_extremes = false;
                s.tpo.open_close = false;
                s.tpo.labels = false;
            },
            200.0,
        );
        let full = tpo_fixture(|s| s.tpo.midpoint = true, 200.0);
        assert!(count(&build(&full.frame())) > count(&build(&bare.frame())));
    }

    #[test]
    fn zoomed_out_the_sessions_are_bars_and_the_work_stays_bounded() {
        let f = tpo_fixture(|_| {}, 4.0);
        let text = texts(&build(&f.frame()));
        assert!(!text.iter().any(|t| t == "A"), "{text:?}");
        let mut view = View::new(0.2);
        view.offset = View::home_offset();
        let many: Vec<Bar> = (0..60_000)
            .map(|i| {
                let base = 100_000 + f64_to_i64((f64::from(i) * 0.01).sin() * 900.0);
                Bar {
                    time_ms: 1_767_571_200_000 + i64::from(i) * 3_600_000,
                    open: base,
                    high: base + 50,
                    low: base - 50,
                    close: base + 5,
                    volume: 3,
                }
            })
            .collect();
        let mut settings = kind(ChartKind::Tpo);
        settings.zone = Zone::Utc;
        let huge = Fixture::new(Series::Bars(many), settings, view);
        assert!(count(&build(&huge.frame())) < 8_000);
    }

    #[test]
    fn every_color_mode_draws() {
        use crate::order_flow::tpo::TpoColor;
        for mode in TpoColor::ALL {
            let f = tpo_fixture(|s| s.tpo.color = mode, 200.0);
            assert!(count(&build(&f.frame())) > 100, "{mode:?}");
        }
    }

    #[test]
    fn the_automatic_display_keeps_letters_for_columns_wide_enough_to_read() {
        // With few periods in a session the columns are wide, and the letters show by themselves.
        let f = tpo_fixture(|s| s.tpo.period_minutes = 240, 200.0);
        let text = texts(&build(&f.frame()));
        assert!(text.iter().any(|t| t == "A"), "{text:?}");
        let crowded = tpo_fixture(|s| s.tpo.period_minutes = 5, 200.0);
        let text = texts(&build(&crowded.frame()));
        assert!(!text.iter().any(|t| t == "A"), "{text:?}");
    }

    #[test]
    fn point_and_figure_draws_its_x_and_o_on_a_real_price_scale() {
        // The prices are around 100000 raw units, far from the zero the boxes used to be measured
        // from: the boxes then had no height and every column fell back to a bar.
        let mut settings = kind(ChartKind::PointFigure);
        settings.transform.pnf_box = crate::transform::settings::BoxSize::Fixed { price: 0.0015 };
        let mut view = View::new(22.0);
        view.offset = 1.0;
        let f = Fixture::new(Series::Bars(bars(500)), settings, view);
        let mut frame = f.frame();
        frame.hover = None;
        assert!(
            long_strokes(&build(&frame), 10) > 0,
            "no O drawn: the columns fell back to bars"
        );
    }

    /// Whether every number of the commands is a number: a point that is not would cut a line out of
    /// a frame, or spoil the whole path.
    fn all_finite(cmds: &[Cmd]) -> bool {
        cmds.iter().all(|cmd| match cmd {
            Cmd::Rect { x, y, w, h, .. } => [x, y, w, h].iter().all(|v| v.is_finite()),
            Cmd::Stroke { points, .. } | Cmd::Fill { points, .. } => {
                points.iter().all(|p| p.0.is_finite() && p.1.is_finite())
            }
            Cmd::Text { x, y, .. } | Cmd::Tag { x, y, .. } => x.is_finite() && y.is_finite(),
            Cmd::FittedText { x, y, w, h, .. } => [x, y, w, h].iter().all(|v| v.is_finite()),
            Cmd::Clip { x, y, w, h, inner } => {
                [x, y, w, h].iter().all(|v| v.is_finite()) && all_finite(inner)
            }
        })
    }

    /// A drawing of `tool` over the bars of the fixture, spread so that its points are not on one
    /// line.
    fn sample_drawing(tool: wyck_drawings::model::tool::Tool) -> Drawing {
        use wyck_drawings::model::Point;
        let points = (0..tool.anchors().max(2))
            .map(|i| Point {
                t: 1_767_571_200_000 + (210 + count_i64(i) * 9) * 60_000,
                p: 100_000.0 + 60.0 + ((count_f64(i) * 37.0) % 90.0),
            })
            .collect();
        Drawing::new(1, tool, points)
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "`bar_px` and `pan` come from the lists of literals the test loops over, so the comparison is exact"
    )]
    fn every_drawing_tool_draws_finite_shapes_at_any_zoom_and_position() {
        use wyck_drawings::model::tool::Tool;
        let mut shown_at_start = Vec::new();
        for tool in Tool::ALL {
            let drawing = sample_drawing(tool);
            let list = [drawing];
            let visible = |_: &Drawing| true;
            for (bar_px, pan) in [
                (8.0, 0.0),
                (0.4, 0.0),
                (2.0, 3_000.0),
                (60.0, -2_000.0),
                (240.0, 0.0),
                (900.0, 40_000.0),
            ] {
                let mut fixture = Fixture::new(
                    Series::Bars(bars(300)),
                    kind(ChartKind::Candles),
                    View::new(bar_px),
                );
                fixture.view.pan(pan, 300, 924.0);
                let bare = build(&fixture.frame());
                let mut frame = fixture.frame();
                frame.drawings = Some(DrawingView {
                    list: &list,
                    creating: None,
                    selected: None,
                    also: &[],
                    visible: &visible,
                });
                let cmds = build(&frame);
                assert!(
                    all_finite(&cmds),
                    "{tool:?} at {bar_px} px a bar, panned {pan}: a number is not one"
                );
                if bar_px == 8.0 && pan == 0.0 && count(&cmds) <= count(&bare) {
                    shown_at_start.push(format!("{tool:?}"));
                }
            }
        }
        assert!(
            shown_at_start.is_empty(),
            "tools that draw nothing over the bars of the chart: {shown_at_start:?}"
        );
    }
}
