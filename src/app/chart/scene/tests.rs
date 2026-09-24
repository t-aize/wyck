use wyck::openapi::market::{Bar, Period, Quote, Tick};

use super::cmd::{count, texts};
use super::*;
use crate::app::chart::flow::Flow;
use crate::app::chart::study::{StudyConfig, StudyKind};

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
        bg: hsla(gpui::rgb(0)),
        fg: hsla(gpui::rgb(0)),
        align: Align::Left,
        fixed_width: Some(10.0),
        within: None,
        size: FONT,
        bold: false,
    };
    let mut tags = vec![tag(100.0), tag(105.0), tag(104.0)];
    spread_tags(&mut tags, 0.0, 500.0);
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
    f.settings.footprint.mode = crate::app::chart::footprint::CellMode::Delta;
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
