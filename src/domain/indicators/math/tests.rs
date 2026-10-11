use super::*;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9 || (a.is_nan() && b.is_nan())
}

fn all_close(a: &[f64], b: &[f64]) {
    assert_eq!(a.len(), b.len());
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        assert!(close(*x, *y), "at {i}: {x} vs {y}");
    }
}

const NAN: f64 = f64::NAN;

#[test]
fn a_simple_average_waits_for_its_period() {
    all_close(
        &sma(&[1.0, 2.0, 3.0, 4.0, 5.0], 3),
        &[NAN, NAN, 2.0, 3.0, 4.0],
    );
    assert!(sma(&[1.0, 2.0], 0).iter().all(|v| v.is_nan()));
}

#[test]
fn a_gap_restarts_an_average() {
    all_close(
        &sma(&[1.0, 2.0, NAN, 4.0, 6.0], 2),
        &[NAN, 1.5, NAN, NAN, 5.0],
    );
}

#[test]
fn an_exponential_average_is_seeded_with_the_simple_one() {
    // Period 3: alpha 0.5, seed (1 + 2 + 3) / 3 = 2.
    all_close(
        &ema(&[1.0, 2.0, 3.0, 4.0, 5.0], 3),
        &[NAN, NAN, 2.0, 3.0, 4.0],
    );
    // Wilder's: alpha 1/3, seed 2, then 2 + (4 - 2) / 3.
    let r = rma(&[1.0, 2.0, 3.0, 4.0], 3);
    assert!(close(r[3], 2.0 + 2.0 / 3.0));
}

#[test]
fn a_weighted_average_favors_the_newest() {
    // (1*1 + 2*2 + 3*3) / 6
    all_close(&wma(&[1.0, 2.0, 3.0], 3), &[NAN, NAN, 14.0 / 6.0]);
    let h = hma(&(1..=40).map(f64::from).collect::<Vec<_>>(), 9);
    // On a straight line the Hull average has no lag at all.
    assert!(close(h[39], 40.0), "{}", h[39]);
}

#[test]
fn the_deviation_is_the_population_one() {
    let d = stdev(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0], 8);
    assert!(close(d[7], 2.0));
}

#[test]
fn rsi_is_bounded_and_saturates_on_a_one_way_move() {
    let up: Vec<f64> = (0..30).map(f64::from).collect();
    let r = rsi(&up, 14);
    assert!(r[13].is_nan());
    assert!(close(r[14], 100.0));
    let flat = vec![5.0; 30];
    assert!(close(rsi(&flat, 14)[20], 50.0));
    let zig: Vec<f64> = (0..60)
        .map(|i| if i % 2 == 0 { 1.0 } else { 2.0 })
        .collect();
    let r = rsi(&zig, 14);
    assert!(r[40] > 40.0 && r[40] < 60.0, "{}", r[40]);
}

#[test]
fn rsi_matches_wilders_worked_example() {
    // Gains and losses of 1 alternating after a rise: first average gain 8/14 over the first
    // 14 changes of the series below, first average loss 6/14.
    let mut prices = vec![10.0];
    for i in 0..14 {
        let last = *prices.last().unwrap();
        prices.push(if i < 8 { last + 1.0 } else { last - 1.0 });
    }
    let r = rsi(&prices, 14);
    let rs: f64 = (8.0 / 14.0) / (6.0 / 14.0);
    assert!(close(r[14], 100.0 - 100.0 / (1.0 + rs)));
}

#[test]
fn macd_is_the_gap_between_two_averages() {
    let prices: Vec<f64> = (0..60).map(|i| 100.0 + f64::from(i)).collect();
    let (line, signal, hist) = macd(&prices, 12, 26, 9);
    // On a straight line both averages lag by a constant: (26 - 12) / 2 = 7.
    assert!(close(line[59], 7.0), "{}", line[59]);
    assert!(close(signal[59], 7.0));
    assert!(close(hist[59], 0.0));
    assert!(line[24].is_nan() && line[25].is_finite());
    assert!(signal[32].is_nan() && signal[33].is_finite());
}

#[test]
fn the_true_range_counts_the_gap_from_the_previous_close() {
    let tr = true_range(&[10.0, 15.0], &[8.0, 13.0], &[9.0, 14.0]);
    assert_eq!(tr, vec![2.0, 6.0]);
}

#[test]
fn stochastic_and_williams_agree() {
    let high = vec![10.0, 12.0, 14.0, 13.0];
    let low = vec![8.0, 9.0, 10.0, 11.0];
    let close_ = vec![9.0, 11.0, 13.0, 12.0];
    let (k, _) = stochastic(&high, &low, &close_, 3, 1, 1);
    let w = williams_r(&high, &low, &close_, 3);
    // Last bar: range 9..14, close 12 -> 60% of the way up, %R -40.
    assert!(close(k[3], 60.0));
    assert!(close(w[3], -40.0));
}

#[test]
fn cci_is_zero_on_the_average() {
    let flat = vec![5.0; 30];
    assert!(close(cci(&flat, 20)[25], 0.0));
}

#[test]
fn obv_adds_and_takes_away() {
    assert_eq!(
        obv(&[1.0, 2.0, 1.0, 1.0], &[5.0, 3.0, 2.0, 7.0]),
        vec![0.0, 3.0, 1.0, 1.0]
    );
}

#[test]
fn highest_and_lowest_follow_the_window() {
    all_close(&highest(&[1.0, 3.0, 2.0, 0.0], 2), &[NAN, 3.0, 3.0, 2.0]);
    all_close(&lowest(&[1.0, 3.0, 2.0, 0.0], 2), &[NAN, 1.0, 2.0, 0.0]);
    all_close(&momentum(&[1.0, 3.0, 6.0], 1), &[NAN, 2.0, 3.0]);
}

#[test]
fn a_steady_rise_has_a_strong_adx_and_positive_di_on_top() {
    let high: Vec<f64> = (0..80).map(|i| 10.0 + f64::from(i)).collect();
    let low: Vec<f64> = high.iter().map(|h| h - 2.0).collect();
    let close_: Vec<f64> = high.iter().map(|h| h - 1.0).collect();
    let (plus, minus, adx) = dmi(&high, &low, &close_, 14, 14);
    assert!(plus[70] > minus[70]);
    assert!(adx[70] > 90.0, "{}", adx[70]);
}

#[test]
fn the_parabolic_sar_trails_below_a_rise() {
    let high: Vec<f64> = (0..30).map(|i| 10.0 + f64::from(i)).collect();
    let low: Vec<f64> = high.iter().map(|h| h - 1.0).collect();
    let sar = parabolic_sar(&high, &low, 0.02, 0.02, 0.2);
    assert!(sar[29] < low[29]);
    assert!(sar[29] > sar[10]);
}

#[test]
fn vwap_weighs_by_volume_and_starts_over_each_day() {
    let out = vwap(
        &[10.0, 20.0, 30.0, 40.0],
        &[1.0, 3.0, 1.0, 1.0],
        &[0, 0, 1, 1],
    );
    assert!(close(out[0], 10.0));
    assert!(close(out[1], 17.5));
    assert!(close(out[2], 30.0), "{out:?}");
    assert!(close(out[3], 35.0));
}

#[test]
fn a_regression_through_a_straight_line_is_that_line() {
    let line: Vec<f64> = (0..10).map(|i| 3.0 + 2.0 * f64::from(i)).collect();
    let (level, slope) = linear_regression(&line, 5);
    assert!(level[3].is_nan());
    for i in 4..10 {
        assert!(close(level[i], line[i]), "{i}");
        assert!(close(slope[i], 2.0));
    }
}

#[test]
fn sums_totals_and_changes_are_worked_out_by_hand() {
    assert_eq!(sum(&[1.0, 2.0, 3.0, 4.0], 2)[1..], [3.0, 5.0, 7.0]);
    assert_eq!(cumulative(&[1.0, f64::NAN, 2.0, 3.0]), [1.0, 1.0, 3.0, 6.0]);
    let r = roc(&[100.0, 110.0, 99.0], 1);
    assert!(r[0].is_nan());
    assert!(close(r[1], 10.0) && close(r[2], -10.0));
    assert!(close(vwma(&[1.0, 3.0], &[1.0, 3.0], 2)[1], 2.5));
}

#[test]
fn bars_since_counts_from_the_last_time_the_condition_held() {
    let out = bars_since(&[0.0, 1.0, 0.0, 0.0, 1.0, 0.0]);
    assert!(out[0].is_nan());
    assert_eq!(out[1..], [0.0, 1.0, 2.0, 0.0, 1.0]);
}

#[test]
fn the_money_flow_index_stays_between_0_and_100() {
    let typical: Vec<f64> = (0..40)
        .map(|i| 100.0 + (f64::from(i) * 0.5).sin() * 4.0)
        .collect();
    let out = mfi(&typical, &vec![10.0; 40], 14);
    assert!(out[..14].iter().all(|v| v.is_nan()));
    assert!(out[14..].iter().all(|v| (0.0..=100.0).contains(v)));
    let rising: Vec<f64> = (0..30).map(f64::from).collect();
    assert!(close(mfi(&rising, &vec![1.0; 30], 5)[29], 100.0));
}

#[test]
fn supertrend_flips_when_the_close_goes_through_the_line() {
    let close: Vec<f64> = (0..60)
        .map(|i| {
            if i < 30 {
                100.0 + f64::from(i)
            } else {
                130.0 - 2.0 * f64::from(i - 30)
            }
        })
        .collect();
    let high: Vec<f64> = close.iter().map(|c| c + 1.0).collect();
    let low: Vec<f64> = close.iter().map(|c| c - 1.0).collect();
    let (line, direction) = supertrend(&high, &low, &close, 5, 2.0);
    assert_eq!(direction[25], 1.0);
    assert_eq!(direction[59], -1.0);
    assert!(line[25] < close[25] && line[59] > close[59]);
}
