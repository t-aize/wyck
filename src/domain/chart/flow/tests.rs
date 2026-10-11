use super::*;

fn q(time_ms: i64, bid: i64, ask: i64) -> Quote {
    Quote {
        time_ms,
        bid: Some(bid),
        ask: Some(ask),
    }
}

#[test]
fn a_rising_middle_is_a_buy_and_a_falling_one_a_sell() {
    let mut c = Classifier::default();
    assert_eq!(c.push(Some(100), Some(102)), None, "nothing to compare");
    assert_eq!(c.push(Some(101), Some(103)), Some((101, Side::Buy)));
    assert_eq!(c.push(Some(99), Some(101)), Some((99, Side::Sell)));
}

#[test]
fn a_change_of_spread_alone_is_not_aggression() {
    let mut c = Classifier::default();
    c.push(Some(100), Some(102));
    assert_eq!(c.push(Some(99), Some(103)), None, "the middle stayed");
    assert_eq!(c.push(Some(100), Some(102)), None);
}

#[test]
fn a_side_that_did_not_change_is_carried() {
    let mut c = Classifier::default();
    c.push(Some(100), Some(102));
    // Only the ask moved, up: the middle rose by one.
    assert_eq!(c.push(None, Some(104)), Some((100, Side::Buy)));
    // Only the bid moved, down.
    assert_eq!(c.push(Some(97), None), Some((97, Side::Sell)));
}

#[test]
fn without_an_ask_the_bid_alone_decides() {
    let mut c = Classifier::default();
    c.push(Some(100), None);
    assert_eq!(c.push(Some(101), None), Some((101, Side::Buy)));
    assert_eq!(c.push(Some(101), None), None);
}

#[test]
fn nothing_is_counted_before_a_bid_is_known() {
    let mut c = Classifier::default();
    assert_eq!(c.push(None, Some(102)), None);
    assert_eq!(c.push(None, Some(103)), None);
}

#[test]
fn a_quote_belongs_to_the_bar_it_falls_in() {
    let times = [0, 60_000, 120_000];
    assert_eq!(bar_open(&times, Some(60_000), 0), Some(0));
    assert_eq!(bar_open(&times, Some(60_000), 59_999), Some(0));
    assert_eq!(bar_open(&times, Some(60_000), 60_000), Some(60_000));
    assert_eq!(bar_open(&times, Some(60_000), 179_999), Some(120_000));
    assert_eq!(bar_open(&times, Some(60_000), -1), None);
    assert_eq!(bar_open(&[], Some(60_000), 5), None);
}

#[test]
fn a_quote_past_the_newest_bar_goes_to_the_bar_that_opens_next() {
    let times = [0, 60_000];
    assert_eq!(bar_open(&times, Some(60_000), 125_000), Some(120_000));
    assert_eq!(bar_open(&times, Some(60_000), 300_500), Some(300_000));
    assert_eq!(bar_open(&times, None, 300_500), Some(60_000));
}

#[test]
fn a_history_is_counted_per_bar_and_per_price() {
    let times = [0, 1_000];
    let quotes = [
        q(100, 100, 102),
        q(200, 101, 103),  // buy at 101
        q(300, 101, 104),  // spread only in the middle: 101+104 = 205 > 204: buy
        q(1_100, 99, 101), // second bar: sell at 99
        q(1_200, 99, 101), // nothing
    ];
    let mut flow = Flow::default();
    flow.ingest(&times, Some(1_000), &quotes, true);
    let first = flow.bar(0).unwrap();
    assert_eq!(
        first.levels,
        vec![Level {
            price: 101,
            sell: 0,
            buy: 2
        }]
    );
    let second = flow.bar(1_000).unwrap();
    assert_eq!(
        second.levels,
        vec![Level {
            price: 99,
            sell: 1,
            buy: 0
        }]
    );
    assert_eq!(first.totals(), (0, 2));
    assert_eq!(flow.last_ms(), 1_200);
}

#[test]
fn levels_stay_sorted_by_price() {
    let mut bar = BarFlow::new(0);
    for price in [5, 1, 3, 1, 5] {
        bar.add(price, Side::Buy);
    }
    let prices: Vec<i64> = bar.levels.iter().map(|l| l.price).collect();
    assert_eq!(prices, vec![1, 3, 5]);
    assert_eq!(bar.levels[0].buy, 2);
}

#[test]
fn live_quotes_carry_on_from_the_history() {
    let times = [0];
    let mut flow = Flow::default();
    flow.ingest(
        &times,
        Some(60_000),
        &[q(10, 100, 102), q(20, 100, 102)],
        true,
    );
    // The last state was (100, 102): a bid of 101 with the ask carried is a buy.
    flow.push_live(0, 30, Some(101), None);
    assert_eq!(flow.bar(0).unwrap().totals(), (0, 1));
    assert_eq!(flow.last_ms(), 30);
}

#[test]
fn a_refill_skips_what_was_already_counted() {
    let times = [0];
    let mut flow = Flow::default();
    flow.ingest(
        &times,
        Some(60_000),
        &[q(10, 100, 102), q(20, 101, 103)],
        true,
    );
    assert_eq!(flow.bar(0).unwrap().totals(), (0, 1));
    // The same quotes again, and one new: only the new one counts.
    flow.ingest(
        &times,
        Some(60_000),
        &[q(10, 100, 102), q(20, 101, 103), q(30, 102, 104)],
        true,
    );
    assert_eq!(flow.bar(0).unwrap().totals(), (0, 2));
}

#[test]
fn an_older_batch_does_not_touch_the_newest_state() {
    let times = [0, 1_000];
    let mut flow = Flow::default();
    flow.ingest(
        &times,
        Some(1_000),
        &[q(1_010, 100, 102), q(1_020, 101, 103)],
        true,
    );
    let last = flow.last_ms();
    flow.ingest(&times, Some(1_000), &[q(10, 50, 52), q(20, 49, 51)], false);
    assert_eq!(flow.last_ms(), last);
    assert_eq!(flow.bar(0).unwrap().totals(), (1, 0));
    assert_eq!(flow.bar(1_000).unwrap().totals(), (0, 1));
}

fn ask(
    times: &[i64],
    covered: Option<i64>,
    len: usize,
    want: usize,
    per_chunk: usize,
    gap_after: Option<i64>,
    now: i64,
) -> Option<Step> {
    let held = Held {
        covered,
        len,
        gap_after,
    };
    next_step(times, |t| *t, held, want, per_chunk, now)
}

fn minutes(n: usize) -> Vec<i64> {
    (0..n as i64).map(|i| i * 60_000).collect()
}

#[test]
fn nothing_held_asks_for_the_newest_bars_first() {
    let times = minutes(500);
    // The screen needs bar 100 but a request covers 40 bars: the newest ones come first.
    let step = ask(&times, None, 0, 100, 40, None, 99_000_000).unwrap();
    assert_eq!((step.start, step.end, step.newest), (460, 500, true));
    assert_eq!(step.from, times[460]);
    assert_eq!(step.to, 99_000_000);
    assert_eq!(step.covers, times[460]);
    // A screen that only needs the newest few takes no more than it needs.
    let step = ask(&times, None, 0, 495, 40, None, 99_000_000).unwrap();
    assert_eq!((step.start, step.end), (495, 500));
}

#[test]
fn older_flow_is_asked_for_until_the_screen_is_covered() {
    let times = minutes(500);
    let mut covered = times[460];
    let mut held = 40;
    let mut asked = Vec::new();
    while let Some(step) = ask(&times, Some(covered), held, 100, 40, None, 0) {
        assert!(!step.newest);
        assert_eq!(step.to, covered - 1, "no gap, no overlap");
        assert!(step.start < step.end && step.end - step.start <= 40);
        asked.push((step.start, step.end));
        held += step.end - step.start;
        covered = step.covers;
    }
    assert_eq!(asked.first(), Some(&(420, 460)));
    assert_eq!(asked.last().map(|s| s.0), Some(100));
    assert_eq!(asked.len(), 9);
}

#[test]
fn nothing_older_is_asked_for_at_the_first_bar_or_past_the_limit() {
    let times = minutes(100);
    assert_eq!(ask(&times, Some(times[0]), 100, 0, 40, None, 0), None);
    assert_eq!(ask(&times, Some(times[50]), 50, 60, 40, None, 0), None);
    assert_eq!(
        ask(&times, Some(times[50]), MAX_FLOW_BARS, 0, 40, None, 0),
        None,
        "the flow is as long as it may be"
    );
    assert_eq!(ask(&[], None, 0, 0, 40, None, 0), None);
}

#[test]
fn a_gap_is_asked_for_from_the_last_quote_to_now() {
    let times = minutes(100);
    let last = times[97] + 1_000;
    let step = ask(&times, Some(times[60]), 40, 0, 40, Some(last), 7_000_000).unwrap();
    assert!(step.newest);
    assert_eq!((step.from, step.to), (last, 7_000_000));
    // It starts at the bar the last quote fell in, so the quotes after it find their bar.
    assert_eq!((step.start, step.end), (97, 100));
    assert_eq!(
        step.covers, times[60],
        "the flow held is not made shorter or longer"
    );
}

#[test]
fn coverage_only_grows_backwards() {
    let mut flow = Flow::default();
    assert!(!flow.is_loaded());
    flow.cover_from(500);
    flow.cover_from(900);
    assert_eq!(flow.covered_from(), Some(500));
    flow.cover_from(100);
    assert_eq!(flow.covered_from(), Some(100));
}

#[test]
fn the_oldest_bars_are_dropped_past_the_limit() {
    let mut flow = Flow::default();
    flow.cover_from(0);
    for i in 0..(MAX_FLOW_BARS as i64 + 10) {
        flow.bar_mut(i * 1_000).add(1, Side::Buy);
    }
    flow.trim();
    assert_eq!(flow.len(), MAX_FLOW_BARS);
    assert_eq!(flow.covered_from(), Some(10_000));
    assert!(flow.bar(0).is_none());
}
