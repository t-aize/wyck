use super::*;

fn closes(prices: &[i64]) -> Vec<Bar> {
    prices
        .iter()
        .enumerate()
        .map(|(i, &p)| Bar {
            time_ms: i as i64 * 60_000,
            open: p,
            high: p,
            low: p,
            close: p,
            volume: 1,
        })
        .collect()
}

/// The stream of a list of single price bars.
fn stream(prices: &[i64]) -> Vec<Sample> {
    samples(&closes(prices), PricePath::Close)
}

fn pairs(bars: &[Bar]) -> Vec<(i64, i64)> {
    bars.iter().map(|b| (b.open, b.close)).collect()
}

#[test]
fn renko_builds_bricks_and_turns_after_two_boxes() {
    let bricks = renko(&stream(&[100, 105, 131, 125, 111, 109, 89]), 10, 2, false);
    // From 100: 110, 120, 130 up; a fall to 111 is not two boxes; 109 is (below 110);
    // 89 adds two more down... from 110 to 100, then 100 to 90.
    assert_eq!(
        pairs(&bricks),
        vec![
            (100, 110),
            (110, 120),
            (120, 130),
            (120, 110),
            (110, 100),
            (100, 90)
        ]
    );
    for brick in &bricks {
        assert_eq!((brick.close - brick.open).abs(), 10);
    }
}

#[test]
fn renko_keeps_the_volume_of_what_made_each_brick() {
    let bricks = renko(&stream(&[100, 101, 102, 115]), 10, 2, false);
    assert_eq!(bricks.len(), 1);
    assert_eq!(bricks[0].volume, 4);
}

#[test]
fn a_renko_that_turns_after_one_box_follows_the_price_closely() {
    let bricks = renko(&stream(&[100, 111, 99, 111]), 10, 1, false);
    // Up to 110, then back one box: a brick from where the last ended, and up again.
    assert_eq!(pairs(&bricks), vec![(100, 110), (110, 100), (100, 110)]);
    for brick in &bricks {
        assert_eq!((brick.close - brick.open).abs(), 10);
    }
}

#[test]
fn a_renko_that_turns_after_three_boxes_leaves_a_gap() {
    let bricks = renko(&stream(&[100, 121, 88]), 10, 3, false);
    // Up to 120, a fall to 88 is three boxes (below 90): the new brick starts one box
    // under where the last ended.
    assert_eq!(pairs(&bricks), vec![(100, 110), (110, 120), (100, 90)]);
}

#[test]
fn wicks_reach_the_extremes_seen_while_the_brick_formed() {
    let bars = [
        Bar {
            time_ms: 0,
            open: 100,
            high: 100,
            low: 100,
            close: 100,
            volume: 1,
        },
        // A bar that dipped to 96 and spiked to 118 before closing at 111.
        Bar {
            time_ms: 60_000,
            open: 101,
            high: 118,
            low: 96,
            close: 111,
            volume: 1,
        },
    ];
    let plain = renko(&samples(&bars, PricePath::Close), 10, 2, true);
    assert_eq!(plain.len(), 1);
    // Reading only the closes, the only extreme seen is the close that made the brick.
    assert_eq!((plain[0].low, plain[0].high), (100, 111));
    let path = renko(&samples(&bars, PricePath::Ohlc), 10, 2, true);
    assert!(path[0].low <= 96, "{path:?}");
    let no_wicks = renko(&samples(&bars, PricePath::Ohlc), 10, 2, false);
    assert_eq!((no_wicks[0].low, no_wicks[0].high), (100, 110));
}

#[test]
fn a_second_brick_from_the_same_price_has_no_wick() {
    let bricks = renko(&stream(&[100, 145]), 10, 2, true);
    assert_eq!(bricks.len(), 4);
    assert!(
        bricks[1..].iter().all(|b| b.high - b.low == 10),
        "{bricks:?}"
    );
}

#[test]
fn the_ohlc_path_finds_the_bricks_a_close_hides() {
    // One bar that fell six boxes and came back to where it opened.
    let bars = [
        Bar {
            time_ms: 0,
            open: 100,
            high: 100,
            low: 100,
            close: 100,
            volume: 4,
        },
        Bar {
            time_ms: 60_000,
            open: 100,
            high: 102,
            low: 40,
            close: 100,
            volume: 8,
        },
    ];
    let close = renko(&samples(&bars, PricePath::Close), 10, 2, false);
    let path = renko(&samples(&bars, PricePath::Ohlc), 10, 2, false);
    assert!(close.is_empty());
    assert!(path.len() >= 6, "{path:?}");
}

#[test]
fn samples_keep_all_the_volume() {
    let bars = [Bar {
        time_ms: 5,
        open: 10,
        high: 20,
        low: 5,
        close: 12,
        volume: 10,
    }];
    let ohlc = samples(&bars, PricePath::Ohlc);
    assert_eq!(
        ohlc.iter().map(|s| s.price).collect::<Vec<_>>(),
        vec![10, 5, 20, 12]
    );
    assert_eq!(ohlc.iter().map(|s| s.volume).sum::<i64>(), 10);
    assert!(ohlc.iter().all(|s| s.time_ms == 5));
    let falling = Bar {
        close: 8,
        ..bars[0]
    };
    assert_eq!(
        samples(&[falling], PricePath::Ohlc)
            .iter()
            .map(|s| s.price)
            .collect::<Vec<_>>(),
        vec![10, 20, 5, 8]
    );
    assert_eq!(samples(&bars, PricePath::Close).len(), 1);
}

#[test]
fn a_line_break_turns_only_past_three_lines() {
    let lines = line_break(&stream(&[10, 11, 12, 13, 12, 11, 9, 14]), 3);
    // Up lines 10-11, 11-12, 12-13; 12 and 11 are inside the last three lines; 9 breaks
    // below their lowest low (10) and the new line starts at the bottom of the last one;
    // 14 breaks above the highest of the last three (13).
    assert_eq!(
        pairs(&lines),
        vec![(10, 11), (11, 12), (12, 13), (12, 9), (12, 14)]
    );
}

#[test]
fn kagi_turns_on_the_reversal_and_thickens_past_the_shoulder() {
    let (lines, meta) = kagi(&stream(&[100, 110, 120, 112, 105, 118, 130, 90]), 10);
    // Up to 120, down to 105, up to 130 (past the 120 shoulder: yang), down to 90.
    assert_eq!(
        pairs(&lines),
        vec![(100, 120), (120, 105), (105, 130), (130, 90)]
    );
    assert!(meta[0].yang);
    // The 105 waist was never set before the second line, so it stays yang.
    assert_eq!(
        meta[3].switch_at,
        Some(105),
        "breaking the waist turns it thin"
    );
}

#[test]
fn point_and_figure_columns_turn_three_boxes_off_the_extreme() {
    let (bars, columns) = point_and_figure(&stream(&[100, 131, 125, 99, 140]), 10, 3);
    assert_eq!(columns.len(), 3, "{columns:?}");
    // From 100 up to 130; 125 is not three boxes back; 99 reaches the 100 level, three
    // boxes under 130, so an O column runs from 120 down to 100; 140 turns it again.
    assert!(columns[0].up);
    assert_eq!((columns[0].bottom, columns[0].top), (11, 13));
    assert!(!columns[1].up);
    assert_eq!((columns[1].bottom, columns[1].top), (10, 12));
    assert!(columns[2].up);
    assert_eq!((columns[2].bottom, columns[2].top), (11, 14));
    assert_eq!(bars.len(), 3);
    assert!(bars.iter().all(|b| b.high > b.low));
}

#[test]
fn point_and_figure_on_the_ohlc_path_reads_the_highs_and_lows() {
    let bars = [
        Bar {
            time_ms: 0,
            open: 100,
            high: 100,
            low: 100,
            close: 100,
            volume: 1,
        },
        // The high reaches 140 but it closes back at 101.
        Bar {
            time_ms: 60_000,
            open: 101,
            high: 140,
            low: 100,
            close: 101,
            volume: 1,
        },
    ];
    let (_, close) = point_and_figure(&samples(&bars, PricePath::Close), 10, 3);
    let (_, path) = point_and_figure(&samples(&bars, PricePath::Ohlc), 10, 3);
    assert!(close.is_empty());
    // Up to the high, and the close back near the start turns the column.
    assert!(path.len() >= 2, "{path:?}");
    assert!(path[0].up && path[0].top >= 14, "{path:?}");
}

#[test]
fn range_bars_all_span_the_range() {
    let source = closes(&[100, 125, 90, 103]);
    let out = range_bars(&source, 10, PricePath::Ohlc);
    assert!(out.len() >= 5, "{out:?}");
    for bar in &out[..out.len() - 1] {
        assert_eq!(bar.high - bar.low, 10, "{bar:?}");
    }
    assert!(out.last().unwrap().high - out.last().unwrap().low <= 10);
    let total: i64 = out.iter().map(|b| b.volume).sum();
    assert_eq!(total, 4, "no volume lost");
}

#[test]
fn range_bars_can_read_only_the_close() {
    let bars = [
        Bar {
            time_ms: 0,
            open: 100,
            high: 100,
            low: 100,
            close: 100,
            volume: 1,
        },
        Bar {
            time_ms: 1,
            open: 100,
            high: 150,
            low: 60,
            close: 101,
            volume: 1,
        },
    ];
    assert!(range_bars(&bars, 10, PricePath::Close).len() <= 1);
    assert!(range_bars(&bars, 10, PricePath::Ohlc).len() >= 8);
}

#[test]
fn box_sizes_resolve_to_whole_quote_units() {
    let bars = closes(&(0..50).map(|i| 100_000 + i * 37).collect::<Vec<_>>());
    let atr = BoxSize::Atr { length: 14 }.resolve(&bars, 10);
    assert!(atr > 0 && atr % 10 == 0, "{atr}");
    assert_eq!(BoxSize::Fixed { price: 0.0005 }.resolve(&bars, 1), 50);
    assert_eq!(BoxSize::Percent { percent: 1.0 }.resolve(&bars, 1), 1_018);
    assert_eq!(BoxSize::Fixed { price: 0.0 }.resolve(&bars, 7), 7);
}

#[test]
fn settings_are_repaired() {
    let settings = TransformSettings {
        line_break: 0,
        pnf_reversal: 99,
        renko_reversal: 0,
        renko_box: BoxSize::Fixed { price: -1.0 },
        kagi_thick: f32::NAN,
        pnf_glyph: 9.0,
        brick_opacity: 0.0,
        ..TransformSettings::default()
    }
    .normalized();
    assert_eq!(settings.line_break, 1);
    assert_eq!(settings.pnf_reversal, 10);
    assert_eq!(settings.renko_reversal, 1);
    assert_eq!(settings.renko_box, BoxSize::default());
    assert_eq!(settings.kagi_thick, 2.6);
    assert_eq!(settings.pnf_glyph, 1.0);
    assert_eq!(settings.brick_opacity, 0.1);
    let text = toml::to_string(&settings).unwrap();
    assert_eq!(
        toml::from_str::<TransformSettings>(&text).unwrap(),
        settings
    );
}

#[test]
fn a_chart_saved_before_the_options_keeps_the_prices_it_was_built_from() {
    // New charts read the whole path, but a file written before there was a choice was built
    // from the closes, and reads the same way now.
    let old: TransformSettings = toml::from_str("line_break = 4\n").unwrap();
    assert_eq!(old.renko_path, PricePath::Close);
    assert_eq!(old.kagi_path, PricePath::Close);
    assert_eq!(old.pnf_path, PricePath::Close);
    assert_eq!(old.line_break_path, PricePath::Close);
    // Range bars always walked the path.
    assert_eq!(old.range_path, PricePath::Ohlc);
    assert_eq!(old.renko_reversal, 2);
    assert!(!old.renko_wicks);
    assert_eq!(old.brick_width, 0.9);
    assert_eq!(TransformSettings::default().renko_path, PricePath::Ohlc);
}

#[test]
fn nothing_in_gives_nothing_out() {
    assert!(renko(&[], 10, 2, true).is_empty());
    assert!(line_break(&[], 3).is_empty());
    assert!(kagi(&[], 10).0.is_empty());
    assert!(point_and_figure(&[], 10, 3).0.is_empty());
    assert!(range_bars(&[], 10, PricePath::Ohlc).is_empty());
}
