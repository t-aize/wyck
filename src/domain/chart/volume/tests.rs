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
