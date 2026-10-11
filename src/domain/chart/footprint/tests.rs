use super::*;

fn level(price: i64, sell: u32, buy: u32) -> Level {
    Level { price, sell, buy }
}

fn settings() -> FootprintSettings {
    FootprintSettings::default()
}

#[test]
fn levels_are_added_into_rows_of_the_size() {
    let levels = [level(10, 1, 2), level(11, 3, 0), level(20, 0, 5)];
    let a = analyze(&levels, 10, &settings()).unwrap();
    assert_eq!(a.rows.len(), 2);
    assert_eq!(
        a.rows[0],
        Row {
            index: 1,
            sell: 4,
            buy: 2
        }
    );
    assert_eq!(a.rows[1].index, 2);
    assert_eq!((a.sell, a.buy), (4, 7));
    assert_eq!(a.delta(), 3);
    assert_eq!(a.volume(), 11);
}

#[test]
fn negative_prices_still_fall_in_the_right_row() {
    let a = analyze(&[level(-1, 1, 1), level(-10, 1, 1)], 10, &settings()).unwrap();
    assert_eq!(a.rows[0].index, -1);
    assert_eq!(a.rows.len(), 1);
}

#[test]
fn nothing_traded_gives_nothing() {
    assert!(analyze(&[], 10, &settings()).is_none());
    assert!(analyze(&[level(5, 0, 0)], 10, &settings()).is_none());
}

#[test]
fn the_point_of_control_is_the_busiest_row() {
    let levels = [level(1, 1, 1), level(2, 5, 4), level(3, 2, 2)];
    let a = analyze(&levels, 1, &settings()).unwrap();
    assert_eq!(a.poc, 1);
    assert_eq!(a.max_volume, 9);
    assert_eq!(a.max_side, 5);
}

#[test]
fn a_tie_goes_to_the_lowest_row() {
    let a = analyze(&[level(1, 2, 2), level(2, 2, 2)], 1, &settings()).unwrap();
    assert_eq!(a.poc, 0);
}

#[test]
fn the_value_area_grows_from_the_point_of_control_by_pairs() {
    // Volumes 1, 2, 10, 3, 1: the pair above (4) beats the pair below (3).
    let volumes = [1, 2, 10, 3, 1];
    let levels: Vec<Level> = volumes
        .iter()
        .enumerate()
        .map(|(i, v)| level(i as i64, 0, *v))
        .collect();
    let mut s = settings();
    s.value_area_percent = 70;
    let a = analyze(&levels, 1, &s).unwrap();
    assert_eq!(a.poc, 2);
    // 10 of 17 is short of 70% (12): the pair above brings it to 14.
    assert_eq!(a.value_area, (2, 4));
    s.value_area_percent = 95;
    let wide = analyze(&levels, 1, &s).unwrap();
    assert_eq!(wide.value_area, (0, 4));
}

#[test]
fn a_single_row_is_its_own_value_area() {
    let a = analyze(&[level(7, 3, 3)], 1, &settings()).unwrap();
    assert_eq!((a.poc, a.value_area), (0, (0, 0)));
}

#[test]
fn diagonal_imbalances_compare_the_ask_with_the_bid_below() {
    // Row 1 buyers (9) against row 0 sellers (2): 9 >= 3 * 2. Row 1 sellers (1) against row 2
    // buyers (5): the sellers lose.
    let levels = [level(0, 2, 0), level(1, 1, 9), level(2, 0, 5)];
    let mut s = settings();
    s.imbalance_min = 3;
    let a = analyze(&levels, 1, &s).unwrap();
    assert_eq!(a.buy_imbalance, vec![false, true, true]);
    assert_eq!(a.sell_imbalance, vec![false, false, false]);
}

#[test]
fn the_ends_of_a_bar_have_no_diagonal_partner() {
    // Lots of buying on the lowest row and selling on the highest: nothing to compare with.
    let levels = [level(0, 0, 50), level(1, 50, 0)];
    let a = analyze(&levels, 1, &settings()).unwrap();
    assert!(!a.buy_imbalance[0]);
    assert!(!a.sell_imbalance[1]);
    // Buying on the highest row against little selling below it does have one, and so does
    // selling on the lowest row against little buying above it.
    let levels = [level(0, 5, 0), level(1, 0, 50)];
    assert!(analyze(&levels, 1, &settings()).unwrap().buy_imbalance[1]);
    let levels = [level(0, 50, 0), level(1, 0, 5)];
    assert!(analyze(&levels, 1, &settings()).unwrap().sell_imbalance[0]);
}

#[test]
fn a_gap_between_rows_counts_as_an_empty_row() {
    // Rows 0 and 2 traded, row 1 did not: the buyers of row 2 face an empty row below.
    let levels = [level(0, 9, 0), level(2, 0, 4)];
    let a = analyze(&levels, 1, &settings()).unwrap();
    assert!(a.buy_imbalance[1]);
}

#[test]
fn the_smallest_volume_and_the_ratio_both_have_to_be_met() {
    let buyers_win =
        |levels: &[Level], s: &FootprintSettings| analyze(levels, 1, s).unwrap().buy_imbalance[1];
    let mut s = settings();
    // 6 buyers against 1 seller below: six times, and at least 3.
    let levels = [level(0, 1, 0), level(1, 0, 6)];
    assert!(buyers_win(&levels, &s));
    s.imbalance_min = 7;
    assert!(!buyers_win(&levels, &s), "too little traded");
    s.imbalance_min = 3;
    s.imbalance_percent = 700;
    assert!(!buyers_win(&levels, &s), "not seven times");
    s.imbalance_percent = 600;
    assert!(buyers_win(&levels, &s), "exactly six times counts");
}

#[test]
fn the_same_row_is_compared_when_it_is_not_diagonal() {
    let levels = [level(0, 1, 9), level(1, 8, 2)];
    let mut s = settings();
    s.diagonal = false;
    let a = analyze(&levels, 1, &s).unwrap();
    assert_eq!(a.buy_imbalance, vec![true, false]);
    assert_eq!(a.sell_imbalance, vec![false, true]);
}

#[test]
fn imbalances_can_be_turned_off() {
    let mut s = settings();
    s.imbalance = false;
    let a = analyze(&[level(0, 0, 50), level(1, 0, 50)], 1, &s).unwrap();
    assert!(a.buy_imbalance.iter().all(|b| !b));
    assert!(a.stacks.is_empty());
}

#[test]
fn neighboring_imbalances_on_one_side_make_a_stack() {
    // Sellers of 5 on row 0, then buyers winning on rows 1, 2 and 3.
    let levels = [
        level(0, 5, 0),
        level(1, 1, 20),
        level(2, 5, 20),
        level(3, 0, 20),
        level(4, 2, 0),
    ];
    let mut s = settings();
    s.stack_rows = 3;
    let a = analyze(&levels, 1, &s).unwrap();
    assert_eq!(a.buy_imbalance, vec![false, true, true, true, false]);
    assert_eq!(
        a.stacks,
        vec![Stack {
            buy: true,
            first: 1,
            last: 3
        }]
    );
    s.stack_rows = 4;
    assert!(analyze(&levels, 1, &s).unwrap().stacks.is_empty());
}

#[test]
fn a_gap_ends_a_stack() {
    // Rows 0, 1 and 3 traded: rows 1 and 3 are not neighbors.
    let levels = [level(0, 9, 0), level(1, 0, 30), level(3, 0, 30)];
    let mut s = settings();
    s.stack_rows = 2;
    let a = analyze(&levels, 1, &s).unwrap();
    assert!(a.stacks.is_empty(), "{:?}", a.stacks);
}

#[test]
fn a_broken_run_can_start_another() {
    let rows = [
        Row {
            index: 0,
            sell: 1,
            buy: 9,
        },
        Row {
            index: 1,
            sell: 1,
            buy: 9,
        },
        Row {
            index: 2,
            sell: 1,
            buy: 1,
        },
        Row {
            index: 3,
            sell: 1,
            buy: 9,
        },
        Row {
            index: 4,
            sell: 1,
            buy: 9,
        },
    ];
    let flags = [true, true, false, true, true];
    let none = [false; 5];
    let found = stacks(&rows, &flags, &none, 2);
    assert_eq!(found.len(), 2);
    assert_eq!((found[0].first, found[0].last), (0, 1));
    assert_eq!((found[1].first, found[1].last), (3, 4));
}

#[test]
fn a_level_is_touched_by_the_first_bar_that_reaches_it() {
    let bars = [(20, 30), (5, 25), (40, 50)];
    let after = || bars.iter().copied();
    assert_eq!(first_touch(after(), 12, 18), Some(1));
    assert_eq!(first_touch(after(), 60, 70), None);
    assert_eq!(first_touch(after().take(1), 12, 18), None, "past the limit");
    assert_eq!(
        first_touch(std::iter::empty(), 0, 100),
        None,
        "nothing after"
    );
}

#[test]
fn rows_are_picked_so_a_number_fits() {
    // A step is 2 pixels high and a row wants 13: 7 steps.
    assert_eq!(row_size(1, 0, 2.0, 13.0), 7);
    assert_eq!(row_size(1, 0, 13.0, 13.0), 1);
    assert_eq!(row_size(1, 0, 6.0, 13.0), 3);
    assert_eq!(
        row_size(10, 0, 6.0, 13.0),
        30,
        "in raw units, of a symbol's step"
    );
    // Past ten steps the row is a round number of them.
    assert_eq!(row_size(1, 0, 1.0, 13.0), 20);
    assert_eq!(row_size(1, 0, 0.5, 13.0), 50);
    assert_eq!(row_size(1, 0, 0.001, 13.0), 20_000);
}

#[test]
fn a_fixed_row_is_honoured_unless_it_is_too_short_to_see() {
    assert_eq!(row_size(1, 4, 5.0, 13.0), 4);
    // 4 steps of half a pixel are 2 pixels: too short, so the row is picked for the zoom.
    assert_eq!(row_size(1, 4, 0.5, 13.0), 6);
    assert_eq!(row_size(1, 0, f64::NAN, 13.0), 20);
}

#[test]
fn settings_are_repaired() {
    let s = FootprintSettings {
        imbalance_percent: 5,
        imbalance_min: 0,
        stack_rows: 99,
        value_area_percent: 5,
        row_steps: u32::MAX,
        ..FootprintSettings::default()
    }
    .normalized();
    assert_eq!(s.imbalance_percent, 110);
    assert_eq!(s.imbalance_min, 1);
    assert_eq!(s.stack_rows, 12);
    assert_eq!(s.value_area_percent, 50);
    assert_eq!(s.row_steps, 10_000);
}

#[test]
fn settings_round_trip_and_read_from_nothing() {
    let s = FootprintSettings {
        mode: CellMode::Delta,
        heat_scope: HeatScope::Visible,
        ..FootprintSettings::default()
    };
    let text = toml::to_string_pretty(&s).unwrap();
    assert_eq!(toml::from_str::<FootprintSettings>(&text).unwrap(), s);
    assert_eq!(
        toml::from_str::<FootprintSettings>("").unwrap(),
        FootprintSettings::default()
    );
}

#[test]
fn volumes_read_short() {
    assert_eq!(compact(0), "0");
    assert_eq!(compact(9_999), "9999");
    assert_eq!(compact(12_345), "12.3k");
    assert_eq!(compact(123_456), "123k");
    assert_eq!(compact(2_500_000), "2.5M");
    assert_eq!(signed(56), "+56");
    assert_eq!(signed(-120_000), "-120k");
    assert_eq!(signed(0), "0");
}
