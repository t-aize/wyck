#![cfg(test)]

use super::*;

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
    f.settings.footprint.mode = crate::domain::chart::footprint::CellMode::Delta;
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
