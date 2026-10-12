#![cfg(test)]

use super::*;

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
        |s| s.tpo.display = crate::domain::chart::tpo::TpoDisplay::Letters,
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
            s.tpo.display = crate::domain::chart::tpo::TpoDisplay::Blocks;
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
    use crate::domain::chart::tpo::TpoColor;
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
    settings.transform.pnf_box =
        crate::domain::chart::transform::settings::BoxSize::Fixed { price: 0.0015 };
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
