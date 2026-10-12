#![cfg(test)]

use super::*;

#[test]
fn a_volume_candle_is_as_wide_as_its_volume() {
    let mut data = bars(40);
    // Two neighbours, one quiet and one very busy: same room, different widths.
    data[20].volume = 1;
    data[21].volume = 100_000;
    let mut settings = kind(ChartKind::VolumeCandles);
    settings.volume_candles.min_width = 0.2;
    settings.volume_candles.max_width = 1.0;
    settings.volume_candles.reference = crate::domain::chart::volume::WidthReference::Max;
    settings.volume_candles.scale = crate::domain::chart::volume::WidthScale::Linear;
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
    settings.volume_bars.size = crate::domain::chart::volume::VolumeSize::Fixed { volume: 20 };
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
    wicked.transform.renko_box =
        crate::domain::chart::transform::settings::BoxSize::Fixed { price: 0.0002 };
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
    narrow.transform.renko_box =
        crate::domain::chart::transform::settings::BoxSize::Fixed { price: 0.0002 };
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
