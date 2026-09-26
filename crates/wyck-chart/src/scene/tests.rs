use super::color::rgb;
use wyck_openapi_model::market::{Bar, Period, Quote, Tick};

use super::cmd::{count, rect_widths, texts};
use super::*;
use crate::flow::Flow;
use crate::options::{ChartColors, CrosshairStyle, ScaleMargin};
use crate::study::{ScriptDrawing, StudyConfig, StudyKind, StudyOutput};
use crate::zone::Zone;

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
        crate::drawing::model::Tool::Text,
        vec![crate::drawing::model::Point {
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
    assert!(
        texts(&build(&fixture.frame()))
            .iter()
            .any(|text| text.contains("SCRIPT_MARKER_UNIQUE"))
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
fn the_time_left_in_a_bar_reads_like_a_clock() {
    assert_eq!(countdown(65_000), "01:05");
    assert_eq!(countdown(3_725_000), "1:02:05");
    assert_eq!(countdown(2 * 86_400_000 + 3_600_000), "2d 01:00");
    assert_eq!(countdown(-5), "00:00");
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
fn tags_on_the_axis_are_moved_apart_the_later_keeping_their_place() {
    let tag = |y: f32| Cmd::Tag {
        text: String::new(),
        x: 0.0,
        y,
        height: 18.0,
        pad: 0.0,
        bg: hsla(rgb(0)),
        fg: hsla(rgb(0)),
        align: Align::Left,
        fixed_width: Some(10.0),
        within: None,
        size: FONT,
        bold: false,
    };
    let mut tags = vec![tag(100.0), tag(105.0), tag(104.0)];
    spread_tags(&mut tags, &Geometry::single(1_000.0, 528.0), 0.0);
    let ys: Vec<f32> = tags
        .iter()
        .map(|t| match t {
            Cmd::Tag { y, .. } => *y,
            _ => 0.0,
        })
        .collect();
    assert_eq!(ys[2], 104.0, "the last one stays");
    let mut sorted = ys.clone();
    sorted.sort_by(f32::total_cmp);
    assert!(sorted.windows(2).all(|w| w[1] - w[0] >= 18.0), "{ys:?}");
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
fn price_axis_uses_the_available_vertical_space() {
    let target = axis_tick_target(300.0);
    let ticks = axis::price_ticks(30_580.0, 30_660.0, target, 1.0);
    assert!(ticks.len() >= 8, "{ticks:?}");
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
    f.settings.footprint.mode = crate::footprint::CellMode::Delta;
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

fn drawn(settings: ChartSettings, edit: impl FnOnce(&mut Frame<'_>)) -> usize {
    let f = Fixture::new(Series::Bars(bars(300)), settings, View::new(8.0));
    let mut frame = f.frame();
    edit(&mut frame);
    count(&build(&frame))
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
    assert!(day_breaks(&frame, 0, 120).is_empty());
}

#[test]
fn the_previous_close_is_the_last_close_of_the_day_before() {
    let day = 86_400_000;
    let bar = |time_ms, close| Bar {
        time_ms,
        open: close,
        high: close,
        low: close,
        close,
        volume: 1,
    };
    let series = Series::Bars(vec![
        bar(day, 10),
        bar(day + 3_600_000, 11),
        bar(2 * day, 20),
        bar(2 * day + 3_600_000, 21),
    ]);
    assert_eq!(previous_close(&series, Zone::Utc), Some(11));
    assert_eq!(
        previous_close(&Series::Bars(vec![bar(day, 10)]), Zone::Utc),
        None
    );
    assert_eq!(previous_close(&Series::Ticks(Vec::new()), Zone::Utc), None);
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

#[test]
fn trading_lines_that_are_hidden_are_not_marks() {
    let settings = super::super::options::TradingLines {
        alerts: false,
        ..super::super::options::TradingLines::default()
    };
    assert!(!settings.shows(super::super::lines::LineId::Alert(3)));
    assert!(settings.shows(super::super::lines::LineId::Order(3)));
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
    settings.volume_candles.reference = crate::volume::WidthReference::Max;
    settings.volume_candles.scale = crate::volume::WidthScale::Linear;
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
    settings.volume_bars.size = crate::volume::VolumeSize::Fixed { volume: 20 };
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
    wicked.transform.renko_box = crate::transform::BoxSize::Fixed { price: 0.0002 };
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
    narrow.transform.renko_box = crate::transform::BoxSize::Fixed { price: 0.0002 };
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
fn kagi_and_point_figure_take_their_own_colors_and_follow_the_rising_ones_otherwise() {
    let plain = Palette::for_chart(&ChartColors::default());
    assert_eq!(plain.kagi_yang, plain.up);
    assert_eq!(plain.kagi_yin, plain.down);
    assert_eq!(plain.pnf_up, plain.up);
    let colors = ChartColors {
        up: Some(0x112233),
        kagi_yin: Some(0x445566),
        pnf_up: Some(0x778899),
        ..ChartColors::default()
    };
    let p = Palette::for_chart(&colors);
    assert_eq!(p.kagi_yang, rgb(0x112233), "follows the rising color");
    assert_eq!(p.kagi_yin, rgb(0x445566));
    assert_eq!(p.pnf_up, rgb(0x778899));
    assert_eq!(p.pnf_down, p.down);
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

/// Three days of five minute bars, so a session has a dozen letters or so per day.
fn tpo_bars() -> Vec<Bar> {
    (0..864)
        .map(|i| {
            let wave = ((i as f64 * 0.05).sin() * 300.0) as i64;
            let base = 100_000 + wave;
            Bar {
                time_ms: 1_767_571_200_000 + i as i64 * 300_000,
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

#[test]
fn a_tpo_chart_makes_one_element_for_each_session() {
    let f = tpo_fixture(|_| {}, 200.0);
    assert_eq!(f.display.tpo.len(), 3);
    assert_eq!(f.display.shown(&f.raw).len(), 3);
    assert!(f.display.is_derived());
}

#[test]
fn profiles_are_written_in_letters_with_their_levels() {
    let f = tpo_fixture(|s| s.tpo.display = crate::tpo::TpoDisplay::Letters, 200.0);
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
            s.tpo.display = crate::tpo::TpoDisplay::Blocks;
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
            let base = 100_000 + ((i as f64 * 0.01).sin() * 900.0) as i64;
            Bar {
                time_ms: 1_767_571_200_000 + i as i64 * 3_600_000,
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
    use crate::tpo::TpoColor;
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
fn point_and_figure_draws_its_x_and_o_on_a_real_price_scale() {
    // The prices are around 100000 raw units, far from the zero the boxes used to be measured
    // from: the boxes then had no height and every column fell back to a bar.
    let mut settings = kind(ChartKind::PointFigure);
    settings.transform.pnf_box = crate::transform::BoxSize::Fixed { price: 0.0015 };
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
