use super::*;

fn bar(time_ms: i64, o: i64, h: i64, l: i64, c: i64, volume: i64) -> Bar {
    Bar {
        time_ms,
        open: o,
        high: h,
        low: l,
        close: c,
        volume,
    }
}

fn tick(time_ms: i64, price: i64) -> Tick {
    Tick { time_ms, price }
}

#[test]
fn ticks_group_into_buckets() {
    let ticks = [
        tick(1_000, 10),
        tick(1_400, 14),
        tick(1_900, 8),
        tick(2_100, 9),
    ];
    let bars = aggregate_ticks(&ticks, 1_000);
    assert_eq!(bars.len(), 2);
    assert_eq!(bars[0], bar(1_000, 10, 14, 8, 8, 3));
    assert_eq!(bars[1], bar(2_000, 9, 9, 9, 9, 1));
}

#[test]
fn buckets_align_before_the_epoch_too() {
    assert_eq!(bucket_start(-1, 1_000), -1_000);
    assert_eq!(bucket_start(1_999, 1_000), 1_000);
}

#[test]
fn a_late_tick_never_breaks_the_order() {
    let mut bars = vec![bar(5_000, 1, 1, 1, 1, 1)];
    assert!(!fold_tick(&mut bars, 1_000, tick(3_000, 5)));
    assert_eq!(bars.len(), 1);
    assert_eq!((bars[0].high, bars[0].close), (5, 5));
    assert!(fold_tick(&mut bars, 1_000, tick(6_000, 2)));
    assert_eq!(bars.len(), 2);
}

#[test]
fn a_tick_only_touches_the_bar_it_belongs_to() {
    let mut bars = vec![bar(60_000, 10, 12, 9, 11, 5)];
    touch_last_bar(&mut bars, 60_000, tick(90_000, 15));
    assert_eq!((bars[0].high, bars[0].close, bars[0].volume), (15, 15, 5));
    touch_last_bar(&mut bars, 60_000, tick(120_000, 99));
    assert_eq!(
        bars[0].high, 15,
        "a tick past the bar waits for its live bar"
    );
    touch_last_bar(&mut bars, 60_000, tick(59_999, 1));
    assert_eq!(bars[0].low, 9, "a tick before the bar is not its own");
}

#[test]
fn a_live_bar_replaces_or_appends() {
    let mut bars = vec![bar(0, 1, 1, 1, 1, 1), bar(60, 1, 1, 1, 1, 1)];
    assert!(!apply_live_bar(&mut bars, bar(60, 1, 3, 1, 2, 4)));
    assert_eq!(bars[1].volume, 4);
    assert!(apply_live_bar(&mut bars, bar(120, 2, 2, 2, 2, 1)));
    assert_eq!(bars.len(), 3);
}

#[test]
fn a_live_bar_before_everything_is_dropped() {
    let mut bars = vec![bar(60, 1, 1, 1, 1, 1)];
    assert!(!apply_live_bar(&mut bars, bar(0, 1, 1, 1, 1, 1)));
    assert_eq!(bars.len(), 1);
}

#[test]
fn merging_keeps_the_bar_with_more_ticks() {
    let mut bars = vec![bar(0, 1, 1, 1, 1, 9), bar(60, 1, 1, 1, 1, 2)];
    merge_bars(
        &mut bars,
        vec![
            bar(0, 1, 2, 1, 2, 3),
            bar(60, 1, 5, 1, 5, 7),
            bar(120, 1, 1, 1, 1, 1),
        ],
    );
    assert_eq!(bars.len(), 3);
    assert_eq!(bars[0].volume, 9);
    assert_eq!(bars[1].volume, 7);
    assert!(bars.windows(2).all(|w| w[0].time_ms < w[1].time_ms));
}

#[test]
fn older_bars_go_in_front_without_duplicates() {
    let mut bars = vec![bar(100, 1, 1, 1, 1, 1)];
    let added = prepend_bars(
        &mut bars,
        vec![
            bar(100, 9, 9, 9, 9, 9),
            bar(50, 1, 1, 1, 1, 1),
            bar(10, 1, 1, 1, 1, 1),
            bar(10, 1, 1, 1, 1, 1),
        ],
    );
    assert_eq!(added, 2);
    let times: Vec<i64> = bars.iter().map(|b| b.time_ms).collect();
    assert_eq!(times, vec![10, 50, 100]);
    assert_eq!(bars[2].open, 1, "the bar already held is not replaced");
}

#[test]
fn a_gap_refill_replaces_the_gap_and_keeps_newer_live_ticks() {
    let mut ticks = vec![tick(1, 1), tick(2, 2), tick(9, 9), tick(12, 12)];
    splice_ticks(
        &mut ticks,
        vec![tick(3, 3), tick(5, 5), tick(10, 10), tick(11, 11)],
        2,
        10,
    );
    let times: Vec<i64> = ticks.iter().map(|t| t.time_ms).collect();
    assert_eq!(times, vec![1, 2, 3, 5, 10, 12]);
}

#[test]
fn trimming_drops_the_oldest() {
    let mut items = vec![1, 2, 3, 4, 5];
    assert_eq!(trim_front(&mut items, 3), 2);
    assert_eq!(items, vec![3, 4, 5]);
    assert_eq!(trim_front(&mut items, 10), 0);
}

#[test]
fn a_time_maps_to_an_index_and_back() {
    let series = Series::Bars(vec![
        bar(1_000, 1, 1, 1, 1, 1),
        bar(2_000, 1, 1, 1, 1, 1),
        bar(4_000, 1, 1, 1, 1, 1),
    ]);
    let step = 1_000.0;
    assert_eq!(series.index_of_time(1_500, step), Some(0.5));
    assert_eq!(
        series.index_of_time(3_000, step),
        Some(1.5),
        "a gap is spread over its points"
    );
    for time in [-3_000, 1_000, 1_250, 2_000, 3_999, 4_000, 9_000] {
        let index = series.index_of_time(time, step).unwrap();
        let back = series.time_of_index(index, step).unwrap();
        assert!((back - time).abs() <= 1, "{time} -> {index} -> {back}");
    }
    assert_eq!(series.index_of_time(0, step), Some(-1.0));
    assert_eq!(series.index_of_time(6_000, step), Some(4.0));
    assert_eq!(Series::Bars(Vec::new()).index_of_time(1, step), None);
}

#[test]
fn the_step_of_ticks_is_their_average_spacing() {
    let ticks = Series::Ticks(vec![tick(0, 1), tick(100, 1), tick(300, 1)]);
    assert_eq!(ticks.step_ms(None), 150.0);
    assert_eq!(ticks.step_ms(Some(60_000)), 60_000.0);
    assert_eq!(Series::Ticks(Vec::new()).step_ms(None), 1_000.0);
}

#[test]
fn the_price_range_follows_the_wicks_or_the_closes() {
    let series = Series::Bars(vec![bar(0, 5, 9, 1, 6, 1), bar(60, 6, 7, 4, 5, 1)]);
    assert_eq!(series.price_range(0, 2, true), Some((1, 9)));
    assert_eq!(series.price_range(0, 2, false), Some((5, 6)));
    assert_eq!(series.price_range(2, 2, true), None);
    assert_eq!(series.price_range(0, 99, true), Some((1, 9)));
}

#[test]
fn server_bars_are_grouped_and_the_newest_group_follows_live_bars() {
    const H: i64 = 3_600_000;
    let h2 = Timeframe::from_code("H2").unwrap();
    let bar = |t: i64, price: i64| Bar {
        time_ms: t * H,
        open: price,
        high: price + 2,
        low: price - 2,
        close: price + 1,
        volume: 10,
    };
    let hourly = [bar(0, 100), bar(1, 110), bar(2, 90), bar(3, 95), bar(4, 80)];
    let mut grouped = group_bars(&hourly, h2);
    assert_eq!(grouped.len(), 3);
    assert_eq!(grouped[0].time_ms, 0);
    assert_eq!(
        (
            grouped[0].open,
            grouped[0].high,
            grouped[0].low,
            grouped[0].close
        ),
        (100, 112, 98, 111)
    );
    assert_eq!(grouped[0].volume, 20);
    assert_eq!(grouped[2].time_ms, 4 * H);
    let mut tail = last_group(&hourly, h2);
    assert_eq!(tail.len(), 1);
    // The 04:00 bar moves: the group follows without counting it twice.
    assert!(!fold_group(&mut grouped, &mut tail, bar(4, 70), h2));
    assert_eq!(grouped.len(), 3);
    assert_eq!(grouped[2].close, 71);
    assert_eq!(grouped[2].volume, 10);
    // 05:00 joins the 04:00 group, 06:00 starts the next one.
    assert!(!fold_group(&mut grouped, &mut tail, bar(5, 60), h2));
    assert_eq!(
        (grouped[2].open, grouped[2].low, grouped[2].close),
        (70, 58, 61)
    );
    assert!(fold_group(&mut grouped, &mut tail, bar(6, 65), h2));
    assert_eq!(grouped.len(), 4);
    assert_eq!(grouped[3].time_ms, 6 * H);
    // A bar of an older group is ignored.
    assert!(!fold_group(&mut grouped, &mut tail, bar(5, 1), h2));
    assert_eq!(grouped[2].close, 61);
}
