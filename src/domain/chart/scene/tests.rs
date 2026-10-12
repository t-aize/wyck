#![cfg(test)]

mod construction;
mod footprint;
mod tpo;

use super::axes::*;
use super::cmd::{count, rect_widths, texts};
use super::color::rgb;

use super::palette::*;
use super::*;
use crate::domain::chart::flow::Flow;
use crate::domain::chart::options::{ChartColors, CrosshairStyle, ScaleMargin};
use crate::domain::chart::settings::ScaleMode;
use crate::domain::chart::view::PriceScale;
use crate::domain::chart::zone::Zone;
use crate::domain::indicators::compute::{ScriptDrawing, StudyOutput};
use crate::domain::indicators::config::StudyConfig;
use crate::domain::indicators::kind::StudyKind;
use crate::domain::market::{Bar, Period, Quote, Tick};

fn bars(n: usize) -> Vec<Bar> {
    (0..n)
        .map(|i| {
            let base = 100_000 + (i as i64 % 50) * 10;
            Bar {
                time_ms: 1_767_571_200_000 + i as i64 * 60_000,
                open: base,
                high: base + 30,
                low: base - 20,
                close: base + 10,
                volume: 1 + i as i64 % 7,
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
        crate::domain::drawings::model::tool::Tool::Text,
        vec![crate::domain::drawings::model::Point {
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
            time_ms: 1_767_571_200_000 + i as i64 * 100,
            price: 100_000 + ((i as i64 * 7919) % 400),
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
        color: 0x00ff00,
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
                    && g.bands[1].top as f32 <= *y
                    && *y + *height <= g.bands[1].bottom() as f32
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
    let hide = |change: fn(&mut crate::domain::chart::options::PriceLines)| {
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
    assert!(day_breaks(&frame, 0, 120).is_empty());
}

#[test]
fn a_chart_overrides_only_the_colors_it_sets() {
    let theme = Palette::new();
    let colors = ChartColors {
        up: Some(0x00ff00),
        background: Some(0x101010),
        ..ChartColors::default()
    };
    let own = Palette::for_chart(&colors);
    assert_eq!(own.up, rgb(0x00ff00));
    assert_eq!(own.bg, rgb(0x101010));
    assert_eq!(own.down, theme.down);
    let grid = Palette::for_chart(&ChartColors {
        grid: Some(0xffffff),
        ..ChartColors::default()
    });
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
